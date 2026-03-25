use anyhow::Result;
use sqlx::{Row, SqlitePool};
use uuid::Uuid;
use palantir_meta_store::{
    ProjectRow, ConnectorRow, EntityRow, BuildRow, RelRow,
    EntityTypeRow, EntityFieldRow, OntologyObjectRow, OntologyLinkRow,
    LinkTypeMappingInput, FoldRow, DataSourceRow, SyncRunRow, DatasetRow, DatasetVersionRow,
};

// ── Db ────────────────────────────────────────────────────────────────────────

pub struct Db {
    pool: SqlitePool,
}

impl Db {
    pub async fn open(path: &str) -> Result<Self> {
        // mode=rwc creates the file if it doesn't exist
        let url = format!("sqlite://{}?mode=rwc", path);
        let pool = SqlitePool::connect(&url).await?;
        // Enable WAL mode: allows concurrent readers + writer (critical when
        // a SqlAdapter reads the same .db file while we write ontology objects)
        sqlx::query("PRAGMA journal_mode=WAL").execute(&pool).await?;
        let db = Self { pool };
        db.migrate().await?;
        Ok(db)
    }

    async fn migrate(&self) -> Result<()> {
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS projects (
                id         TEXT PRIMARY KEY,
                name       TEXT NOT NULL UNIQUE,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            )",
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS connectors (
                id          TEXT PRIMARY KEY,
                project_id  TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
                path        TEXT NOT NULL,
                ns          TEXT NOT NULL,
                schema_name TEXT NOT NULL
            )",
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS live_entities (
                id          TEXT NOT NULL,
                project_id  TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
                entity_type TEXT NOT NULL,
                ddd_concept TEXT NOT NULL,
                label       TEXT NOT NULL,
                properties  TEXT NOT NULL,
                PRIMARY KEY (id, project_id)
            )",
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS live_relationships (
                project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
                from_id    TEXT NOT NULL,
                to_id      TEXT NOT NULL,
                kind       TEXT NOT NULL,
                PRIMARY KEY (project_id, from_id, to_id, kind)
            )",
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS builds (
                id                TEXT PRIMARY KEY,
                project_id        TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
                created_at        TEXT NOT NULL,
                entities          INTEGER NOT NULL DEFAULT 0,
                relationships     INTEGER NOT NULL DEFAULT 0,
                bounded_contexts  INTEGER NOT NULL DEFAULT 0,
                applied_events    INTEGER NOT NULL DEFAULT 0
            )",
        )
        .execute(&self.pool)
        .await?;

        // Add metadata columns to connectors (idempotent — ignore if already exist)
        let _ = sqlx::query("ALTER TABLE connectors ADD COLUMN headers        TEXT DEFAULT NULL")
            .execute(&self.pool)
            .await;
        let _ = sqlx::query("ALTER TABLE connectors ADD COLUMN samples        TEXT DEFAULT NULL")
            .execute(&self.pool)
            .await;
        let _ = sqlx::query("ALTER TABLE connectors ADD COLUMN mapping_config TEXT DEFAULT NULL")
            .execute(&self.pool)
            .await;

        // ── Ontology TBox (EntityType schema) ────────────────────────────────
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS entity_types (
                id           TEXT PRIMARY KEY,
                name         TEXT NOT NULL UNIQUE,
                display_name TEXT NOT NULL,
                color        TEXT NOT NULL DEFAULT '#6366f1',
                icon         TEXT NOT NULL DEFAULT '●',
                created_at   TEXT NOT NULL
            )",
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS entity_fields (
                id             TEXT PRIMARY KEY,
                entity_type_id TEXT NOT NULL REFERENCES entity_types(id) ON DELETE CASCADE,
                name           TEXT NOT NULL,
                data_type      TEXT NOT NULL DEFAULT 'string',
                is_required    INTEGER NOT NULL DEFAULT 0,
                classification TEXT NOT NULL DEFAULT 'Internal',
                sort_order     INTEGER NOT NULL DEFAULT 0,
                UNIQUE(entity_type_id, name)
            )",
        )
        .execute(&self.pool)
        .await?;

        // ── Ontology ABox (Objects & Links) ──────────────────────────────────
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS ontology_objects (
                id               TEXT PRIMARY KEY,
                entity_type_id   TEXT NOT NULL REFERENCES entity_types(id),
                entity_type_name TEXT NOT NULL,
                external_id      TEXT,
                label            TEXT NOT NULL,
                fields           TEXT NOT NULL DEFAULT '{}',
                created_at       TEXT NOT NULL,
                updated_at       TEXT NOT NULL
            )",
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS ontology_links (
                id         TEXT PRIMARY KEY,
                from_id    TEXT NOT NULL REFERENCES ontology_objects(id) ON DELETE CASCADE,
                to_id      TEXT NOT NULL REFERENCES ontology_objects(id) ON DELETE CASCADE,
                rel_type   TEXT NOT NULL,
                created_at TEXT NOT NULL,
                UNIQUE(from_id, to_id, rel_type)
            )",
        )
        .execute(&self.pool)
        .await?;

        // Enable foreign key enforcement
        sqlx::query("PRAGMA foreign_keys = ON")
            .execute(&self.pool)
            .await?;

        // ── Ingest workflow tables ─────────────────────────────────────────────
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS folds (
                id          TEXT PRIMARY KEY,
                project_id  TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
                name        TEXT NOT NULL,
                description TEXT,
                created_at  TEXT NOT NULL,
                UNIQUE(project_id, name)
            )",
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS data_sources (
                id           TEXT PRIMARY KEY,
                fold_id      TEXT NOT NULL REFERENCES folds(id) ON DELETE CASCADE,
                name         TEXT NOT NULL,
                source_type  TEXT NOT NULL,
                config       TEXT NOT NULL DEFAULT '{}',
                status       TEXT NOT NULL DEFAULT 'idle',
                write_lock   TEXT,
                last_sync_at TEXT,
                record_count INTEGER,
                created_at   TEXT NOT NULL
            )",
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS sync_runs (
                id            TEXT PRIMARY KEY,
                source_id     TEXT NOT NULL REFERENCES data_sources(id) ON DELETE CASCADE,
                status        TEXT NOT NULL DEFAULT 'pending',
                total_records INTEGER,
                processed     INTEGER NOT NULL DEFAULT 0,
                current_item  TEXT,
                error_message TEXT,
                error_type    TEXT,
                started_at    TEXT NOT NULL,
                finished_at   TEXT
            )",
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS datasets (
                id              TEXT PRIMARY KEY,
                source_id       TEXT NOT NULL REFERENCES data_sources(id) ON DELETE CASCADE,
                name            TEXT NOT NULL,
                entity_type_id  TEXT REFERENCES entity_types(id),
                current_version INTEGER NOT NULL DEFAULT 0,
                created_at      TEXT NOT NULL
            )",
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS dataset_versions (
                id            TEXT PRIMARY KEY,
                dataset_id    TEXT NOT NULL REFERENCES datasets(id) ON DELETE CASCADE,
                version       INTEGER NOT NULL,
                sync_run_id   TEXT NOT NULL REFERENCES sync_runs(id),
                status        TEXT NOT NULL DEFAULT 'pending',
                schema_json   TEXT NOT NULL DEFAULT '{}',
                schema_change TEXT,
                total_rows    INTEGER NOT NULL DEFAULT 0,
                is_current    INTEGER NOT NULL DEFAULT 0,
                created_at    TEXT NOT NULL,
                UNIQUE(dataset_id, version)
            )",
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_dv_current ON dataset_versions(dataset_id, is_current)",
        )
        .execute(&self.pool)
        .await?;

        // OntologyObject lineage columns (idempotent)
        let _ = sqlx::query("ALTER TABLE ontology_objects ADD COLUMN dataset_id  TEXT")
            .execute(&self.pool)
            .await;
        let _ = sqlx::query("ALTER TABLE ontology_objects ADD COLUMN sync_run_id TEXT")
            .execute(&self.pool)
            .await;
        // Multi-source provenance: JSON array of all dataset_ids that contributed to this object
        let _ = sqlx::query("ALTER TABLE ontology_objects ADD COLUMN source_ids TEXT NOT NULL DEFAULT '[]'")
            .execute(&self.pool)
            .await;

        // Upsert index: dedup by (entity_type_id, external_id).
        // NULL external_id rows are never deduplicated (SQLite NULL != NULL).
        let _ = sqlx::query(
            "CREATE UNIQUE INDEX IF NOT EXISTS idx_oo_upsert \
             ON ontology_objects(entity_type_id, external_id)",
        )
        .execute(&self.pool)
        .await;

        // Mapping persistence: remember dataset→entity_type mapping + field_mapping for re-sync
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS object_type_mappings (
                id              TEXT PRIMARY KEY,
                dataset_id      TEXT NOT NULL UNIQUE REFERENCES datasets(id) ON DELETE CASCADE,
                entity_type_id  TEXT NOT NULL REFERENCES entity_types(id),
                primary_key_col TEXT NOT NULL DEFAULT '',
                field_mapping   TEXT NOT NULL DEFAULT '{}',
                created_at      TEXT NOT NULL,
                updated_at      TEXT NOT NULL
            )",
        )
        .execute(&self.pool)
        .await?;

        // Link type mappings: FK col → target entity type, drives resolve_links after promote
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS link_type_mappings (
                id                  TEXT PRIMARY KEY,
                dataset_id          TEXT NOT NULL REFERENCES datasets(id) ON DELETE CASCADE,
                from_fk_col         TEXT NOT NULL,
                to_entity_type_id   TEXT NOT NULL REFERENCES entity_types(id),
                rel_type            TEXT NOT NULL DEFAULT 'HAS',
                created_at          TEXT NOT NULL
            )",
        )
        .execute(&self.pool)
        .await?;

        // DatasetVersion: manifest_path column added in Iter-1 (idempotent)
        let _ = sqlx::query("ALTER TABLE dataset_versions ADD COLUMN manifest_path TEXT")
            .execute(&self.pool)
            .await;

        // DatasetVersion: schema_change column added in Iter-3 (idempotent)
        let _ = sqlx::query("ALTER TABLE dataset_versions ADD COLUMN schema_change TEXT")
            .execute(&self.pool)
            .await;

        // DataSource: soft-delete + deprecation (idempotent)
        let _ = sqlx::query("ALTER TABLE data_sources ADD COLUMN deprecated INTEGER NOT NULL DEFAULT 0")
            .execute(&self.pool).await;
        let _ = sqlx::query("ALTER TABLE data_sources ADD COLUMN deleted_at TEXT")
            .execute(&self.pool).await;
        let _ = sqlx::query("ALTER TABLE data_sources ADD COLUMN group_id TEXT")
            .execute(&self.pool).await;
        // DataSource: sync_mode column — snapshot | append | upsert (idempotent)
        let _ = sqlx::query("ALTER TABLE data_sources ADD COLUMN sync_mode TEXT NOT NULL DEFAULT 'snapshot'")
            .execute(&self.pool).await;
        let _ = sqlx::query(
            "CREATE UNIQUE INDEX IF NOT EXISTS idx_source_name ON data_sources(fold_id, name)",
        ).execute(&self.pool).await;
        // Dataset-level sync_mode in object_type_mappings (idempotent)
        let _ = sqlx::query("ALTER TABLE object_type_mappings ADD COLUMN sync_mode TEXT NOT NULL DEFAULT 'snapshot'")
            .execute(&self.pool).await;
        // ET → Fold association (idempotent)
        let _ = sqlx::query("ALTER TABLE entity_types ADD COLUMN fold_id TEXT REFERENCES folds(id)")
            .execute(&self.pool).await;
        // DDD role (idempotent)
        let _ = sqlx::query("ALTER TABLE entity_types ADD COLUMN ddd_role TEXT NOT NULL DEFAULT 'entity'")
            .execute(&self.pool).await;
        // ET namespace = fold name prefix, stored for display (idempotent)
        let _ = sqlx::query("ALTER TABLE entity_types ADD COLUMN namespace TEXT")
            .execute(&self.pool).await;

        // ── Bounded Context tables (P0) ────────────────────────────────────────
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS bounded_contexts (
                id            TEXT PRIMARY KEY,
                fold_id       TEXT NOT NULL REFERENCES folds(id) ON DELETE CASCADE,
                name          TEXT NOT NULL,
                color         TEXT NOT NULL DEFAULT '#6366f1',
                auto_detected INTEGER NOT NULL DEFAULT 1,
                created_at    TEXT NOT NULL,
                UNIQUE(fold_id, name)
            )",
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS bc_relationships (
                id                TEXT PRIMARY KEY,
                from_bc_id        TEXT NOT NULL REFERENCES bounded_contexts(id) ON DELETE CASCADE,
                to_bc_id          TEXT NOT NULL REFERENCES bounded_contexts(id) ON DELETE CASCADE,
                relationship_type TEXT NOT NULL DEFAULT 'shared_kernel',
                notes             TEXT,
                created_at        TEXT NOT NULL,
                UNIQUE(from_bc_id, to_bc_id, relationship_type)
            )",
        )
        .execute(&self.pool)
        .await?;

        // ET → BC assignment (idempotent)
        let _ = sqlx::query("ALTER TABLE entity_types ADD COLUMN bc_id TEXT REFERENCES bounded_contexts(id)")
            .execute(&self.pool).await;

        // Fold type: 'normal' | 'shared_kernel' (idempotent)
        let _ = sqlx::query("ALTER TABLE folds ADD COLUMN fold_type TEXT NOT NULL DEFAULT 'normal'")
            .execute(&self.pool).await;

        // Link type → BC relationship governance binding (idempotent)
        let _ = sqlx::query("ALTER TABLE link_type_mappings ADD COLUMN bc_relationship_id TEXT REFERENCES bc_relationships(id)")
            .execute(&self.pool).await;

        // Platform config table: stores platform-wide settings (e.g. storage backend)
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS platform_config (
                key        TEXT PRIMARY KEY,
                value      TEXT NOT NULL,
                updated_at TEXT NOT NULL
            )",
        )
        .execute(&self.pool)
        .await?;

        // Seed a "default" entity type used when syncing without an explicit mapping.
        // INSERT OR IGNORE so it is idempotent.
        sqlx::query(
            "INSERT OR IGNORE INTO entity_types (id, name, display_name, color, icon, created_at)
             VALUES ('default', 'default', '未分类', '#6366f1', '📦',
                     strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))",
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    fn now_str() -> String {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs().to_string())
            .unwrap_or_else(|_| "0".into())
    }

    // ── Projects ──────────────────────────────────────────────────────────────

    pub async fn create_project(&self, name: &str) -> Result<ProjectRow> {
        let id = Uuid::new_v4().to_string();
        let now = Self::now_str();
        sqlx::query("INSERT INTO projects (id, name, created_at, updated_at) VALUES (?, ?, ?, ?)")
            .bind(&id)
            .bind(name)
            .bind(&now)
            .bind(&now)
            .execute(&self.pool)
            .await?;
        Ok(ProjectRow {
            id,
            name: name.to_string(),
            created_at: now.clone(),
            updated_at: now,
        })
    }

    pub async fn list_projects(&self) -> Result<Vec<ProjectRow>> {
        let rows = sqlx::query(
            "SELECT id, name, created_at, updated_at FROM projects ORDER BY updated_at DESC",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| ProjectRow {
                id: r.get("id"),
                name: r.get("name"),
                created_at: r.get("created_at"),
                updated_at: r.get("updated_at"),
            })
            .collect())
    }

    pub async fn get_project(&self, id: &str) -> Result<Option<ProjectRow>> {
        let row = sqlx::query("SELECT id, name, created_at, updated_at FROM projects WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?;
        Ok(row.map(|r| ProjectRow {
            id: r.get("id"),
            name: r.get("name"),
            created_at: r.get("created_at"),
            updated_at: r.get("updated_at"),
        }))
    }

    pub async fn rename_project(&self, id: &str, name: &str) -> Result<()> {
        sqlx::query("UPDATE projects SET name = ?, updated_at = ? WHERE id = ?")
            .bind(name)
            .bind(Self::now_str())
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn delete_project(&self, id: &str) -> Result<()> {
        sqlx::query("DELETE FROM projects WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// (fold_count, last_sync_at, aggregated_status)
    pub async fn project_stats(&self, project_id: &str) -> Result<(i64, Option<String>, String)> {
        let fold_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM folds WHERE project_id = ?",
        )
        .bind(project_id)
        .fetch_one(&self.pool)
        .await
        .unwrap_or(0);

        let last_sync: Option<String> = sqlx::query_scalar(
            "SELECT MAX(s.last_sync_at) FROM data_sources s
             JOIN folds f ON s.fold_id = f.id
             WHERE f.project_id = ?",
        )
        .bind(project_id)
        .fetch_one(&self.pool)
        .await
        .unwrap_or(None);

        let row = sqlx::query(
            "SELECT
               COALESCE(SUM(CASE WHEN s.status='syncing' THEN 1 ELSE 0 END), 0) AS n_syncing,
               COALESCE(SUM(CASE WHEN s.status='error'   THEN 1 ELSE 0 END), 0) AS n_error,
               COALESCE(SUM(CASE WHEN s.status='synced'  THEN 1 ELSE 0 END), 0) AS n_synced,
               COUNT(s.id)                                                        AS n_total
             FROM data_sources s JOIN folds f ON s.fold_id = f.id
             WHERE f.project_id = ?",
        )
        .bind(project_id)
        .fetch_one(&self.pool)
        .await;

        let status = if let Ok(r) = row {
            let n_syncing: i64 = r.try_get("n_syncing").unwrap_or(0);
            let n_error:   i64 = r.try_get("n_error").unwrap_or(0);
            let n_synced:  i64 = r.try_get("n_synced").unwrap_or(0);
            let n_total:   i64 = r.try_get("n_total").unwrap_or(0);
            if n_total == 0       { "idle" }
            else if n_syncing > 0 { "syncing" }
            else if n_error > 0   { "error" }
            else if n_synced > 0  { "synced" }
            else                  { "idle" }
        } else { "idle" };

        Ok((fold_count, last_sync, status.to_string()))
    }

    pub async fn touch_project(&self, id: &str) -> Result<()> {
        let now = Self::now_str();
        sqlx::query("UPDATE projects SET updated_at = ? WHERE id = ?")
            .bind(&now)
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    // ── Connectors ────────────────────────────────────────────────────────────

    pub async fn save_connector(&self, c: &ConnectorRow) -> Result<()> {
        sqlx::query(
            "INSERT OR REPLACE INTO connectors
             (id, project_id, path, ns, schema_name, headers, samples, mapping_config)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&c.id)
        .bind(&c.project_id)
        .bind(&c.path)
        .bind(&c.ns)
        .bind(&c.schema_name)
        .bind(&c.headers)
        .bind(&c.samples)
        .bind(&c.mapping_config)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn update_connector_metadata(
        &self,
        id: &str,
        headers: &str,
        samples: &str,
    ) -> Result<()> {
        sqlx::query("UPDATE connectors SET headers = ?, samples = ? WHERE id = ?")
            .bind(headers)
            .bind(samples)
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn save_connector_mapping(&self, id: &str, config_json: &str) -> Result<()> {
        sqlx::query("UPDATE connectors SET mapping_config = ? WHERE id = ?")
            .bind(config_json)
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn load_connectors(&self, project_id: &str) -> Result<Vec<ConnectorRow>> {
        let rows = sqlx::query(
            "SELECT id, project_id, path, ns, schema_name, headers, samples, mapping_config
             FROM connectors WHERE project_id = ?",
        )
        .bind(project_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| ConnectorRow {
                id: r.get("id"),
                project_id: r.get("project_id"),
                path: r.get("path"),
                ns: r.get("ns"),
                schema_name: r.get("schema_name"),
                headers: r.get("headers"),
                samples: r.get("samples"),
                mapping_config: r.get("mapping_config"),
            })
            .collect())
    }

    // ── Graph ─────────────────────────────────────────────────────────────────

    pub async fn upsert_entity(&self, e: &EntityRow) -> Result<()> {
        sqlx::query(
            "INSERT OR REPLACE INTO live_entities
             (id, project_id, entity_type, ddd_concept, label, properties)
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(&e.id)
        .bind(&e.project_id)
        .bind(&e.entity_type)
        .bind(&e.ddd_concept)
        .bind(&e.label)
        .bind(&e.properties)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn upsert_relationship(&self, r: &RelRow) -> Result<()> {
        sqlx::query(
            "INSERT OR IGNORE INTO live_relationships (project_id, from_id, to_id, kind)
             VALUES (?, ?, ?, ?)",
        )
        .bind(&r.project_id)
        .bind(&r.from_id)
        .bind(&r.to_id)
        .bind(&r.kind)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn load_entities(&self, project_id: &str) -> Result<Vec<EntityRow>> {
        let rows = sqlx::query(
            "SELECT id, project_id, entity_type, ddd_concept, label, properties
             FROM live_entities WHERE project_id = ?",
        )
        .bind(project_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| EntityRow {
                id: r.get("id"),
                project_id: r.get("project_id"),
                entity_type: r.get("entity_type"),
                ddd_concept: r.get("ddd_concept"),
                label: r.get("label"),
                properties: r.get("properties"),
            })
            .collect())
    }

    pub async fn load_relationships(&self, project_id: &str) -> Result<Vec<RelRow>> {
        let rows = sqlx::query(
            "SELECT project_id, from_id, to_id, kind
             FROM live_relationships WHERE project_id = ?",
        )
        .bind(project_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| RelRow {
                project_id: r.get("project_id"),
                from_id: r.get("from_id"),
                to_id: r.get("to_id"),
                kind: r.get("kind"),
            })
            .collect())
    }

    pub async fn delete_connector(&self, id: &str) -> Result<()> {
        sqlx::query("DELETE FROM connectors WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    // ── Builds ────────────────────────────────────────────────────────────────

    pub async fn save_build(&self, b: &BuildRow) -> Result<()> {
        sqlx::query(
            "INSERT INTO builds (id, project_id, created_at, entities, relationships, bounded_contexts, applied_events)
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&b.id)
        .bind(&b.project_id)
        .bind(&b.created_at)
        .bind(b.entities)
        .bind(b.relationships)
        .bind(b.bounded_contexts)
        .bind(b.applied_events)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn list_builds(&self, project_id: &str) -> Result<Vec<BuildRow>> {
        let rows = sqlx::query(
            "SELECT id, project_id, created_at, entities, relationships, bounded_contexts, applied_events
             FROM builds WHERE project_id = ? ORDER BY created_at DESC LIMIT 20",
        )
        .bind(project_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| BuildRow {
                id: r.get("id"),
                project_id: r.get("project_id"),
                created_at: r.get("created_at"),
                entities: r.get("entities"),
                relationships: r.get("relationships"),
                bounded_contexts: r.get("bounded_contexts"),
                applied_events: r.get("applied_events"),
            })
            .collect())
    }

    // ── Ontology TBox ─────────────────────────────────────────────────────

    pub async fn create_entity_type(
        &self,
        name: &str,
        display_name: &str,
        color: &str,
        icon: &str,
        fold_id: Option<&str>,
        ddd_role: &str,
        namespace: Option<&str>,
    ) -> Result<EntityTypeRow> {
        let id = Uuid::new_v4().to_string();
        let now = Self::now_str();
        sqlx::query(
            "INSERT INTO entity_types (id, name, display_name, color, icon, fold_id, ddd_role, namespace, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(name)
        .bind(display_name)
        .bind(color)
        .bind(icon)
        .bind(fold_id)
        .bind(ddd_role)
        .bind(namespace)
        .bind(&now)
        .execute(&self.pool)
        .await?;
        Ok(EntityTypeRow {
            id,
            name: name.to_string(),
            display_name: display_name.to_string(),
            color: color.to_string(),
            icon: icon.to_string(),
            fold_id: fold_id.map(|s| s.to_string()),
            namespace: namespace.map(|s| s.to_string()),
            ddd_role: ddd_role.to_string(),
            created_at: now,
        })
    }

    pub async fn list_entity_types(&self) -> Result<Vec<EntityTypeRow>> {
        let rows = sqlx::query(
            "SELECT id, name, display_name, color, icon, fold_id, namespace,
                    COALESCE(ddd_role, 'entity') as ddd_role, created_at
             FROM entity_types ORDER BY created_at ASC",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| EntityTypeRow {
                id: r.get("id"),
                name: r.get("name"),
                display_name: r.get("display_name"),
                color: r.get("color"),
                icon: r.get("icon"),
                fold_id: r.get("fold_id"),
                namespace: r.get("namespace"),
                ddd_role: r.get("ddd_role"),
                created_at: r.get("created_at"),
            })
            .collect())
    }

    /// List entity types belonging to a specific fold.
    pub async fn list_entity_types_for_fold(&self, fold_id: &str) -> Result<Vec<EntityTypeRow>> {
        let rows = sqlx::query(
            "SELECT id, name, display_name, color, icon, fold_id, namespace,
                    COALESCE(ddd_role, 'entity') as ddd_role, created_at
             FROM entity_types WHERE fold_id = ? ORDER BY created_at ASC",
        )
        .bind(fold_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| EntityTypeRow {
                id: r.get("id"),
                name: r.get("name"),
                display_name: r.get("display_name"),
                color: r.get("color"),
                icon: r.get("icon"),
                fold_id: r.get("fold_id"),
                namespace: r.get("namespace"),
                ddd_role: r.get("ddd_role"),
                created_at: r.get("created_at"),
            })
            .collect())
    }

    pub async fn update_entity_type_ddd_role(&self, et_id: &str, ddd_role: &str) -> Result<()> {
        sqlx::query("UPDATE entity_types SET ddd_role = ? WHERE id = ?")
            .bind(ddd_role)
            .bind(et_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn update_entity_type_fold(&self, et_id: &str, fold_id: Option<&str>) -> Result<()> {
        sqlx::query("UPDATE entity_types SET fold_id = ? WHERE id = ?")
            .bind(fold_id)
            .bind(et_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn delete_entity_type(&self, id: &str) -> Result<()> {
        sqlx::query("DELETE FROM entity_types WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn add_entity_field(
        &self,
        entity_type_id: &str,
        name: &str,
        data_type: &str,
        is_required: bool,
        classification: &str,
        sort_order: i64,
    ) -> Result<EntityFieldRow> {
        let id = Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO entity_fields
             (id, entity_type_id, name, data_type, is_required, classification, sort_order)
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(entity_type_id)
        .bind(name)
        .bind(data_type)
        .bind(is_required as i64)
        .bind(classification)
        .bind(sort_order)
        .execute(&self.pool)
        .await?;
        Ok(EntityFieldRow {
            id,
            entity_type_id: entity_type_id.to_string(),
            name: name.to_string(),
            data_type: data_type.to_string(),
            is_required,
            classification: classification.to_string(),
            sort_order,
        })
    }

    pub async fn list_entity_fields(&self, entity_type_id: &str) -> Result<Vec<EntityFieldRow>> {
        let rows = sqlx::query(
            "SELECT id, entity_type_id, name, data_type, is_required, classification, sort_order
             FROM entity_fields WHERE entity_type_id = ? ORDER BY sort_order ASC",
        )
        .bind(entity_type_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| EntityFieldRow {
                id: r.get("id"),
                entity_type_id: r.get("entity_type_id"),
                name: r.get("name"),
                data_type: r.get("data_type"),
                is_required: r.get::<i64, _>("is_required") != 0,
                classification: r.get("classification"),
                sort_order: r.get("sort_order"),
            })
            .collect())
    }

    pub async fn delete_entity_field(&self, id: &str) -> Result<()> {
        sqlx::query("DELETE FROM entity_fields WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    // ── Ontology ABox ─────────────────────────────────────────────────────

    /// Expose pool for raw queries (used by sync background tasks)
    pub fn pool(&self) -> &SqlitePool { &self.pool }

    /// Sync path: always insert a new row (external_id = None → no dedup).
    pub async fn create_ontology_object_with_lineage(
        &self,
        entity_type_id: &str,
        entity_type_name: &str,
        label: &str,
        fields_json: &str,
        dataset_id: &str,
        sync_run_id: &str,
    ) -> Result<OntologyObjectRow> {
        self.upsert_ontology_object(
            entity_type_id, entity_type_name, None,
            label, fields_json, dataset_id, sync_run_id,
        ).await?;
        // Return a minimal row (callers only use it for error checking)
        let now = Self::now_str();
        Ok(OntologyObjectRow {
            id: String::new(),
            entity_type_id: entity_type_id.to_string(),
            entity_type_name: entity_type_name.to_string(),
            label: label.to_string(),
            fields: fields_json.to_string(),
            created_at: now.clone(),
            updated_at: now,
        })
    }

    /// connections_sync path: insert without lineage (dataset_id / run_id unknown).
    pub async fn create_ontology_object(
        &self,
        entity_type_id: &str,
        entity_type_name: &str,
        label: &str,
        fields_json: &str,
    ) -> Result<OntologyObjectRow> {
        self.upsert_ontology_object(
            entity_type_id, entity_type_name, None,
            label, fields_json, "", "",
        ).await?;
        let now = Self::now_str();
        Ok(OntologyObjectRow {
            id: String::new(),
            entity_type_id: entity_type_id.to_string(),
            entity_type_name: entity_type_name.to_string(),
            label: label.to_string(),
            fields: fields_json.to_string(),
            created_at: now.clone(),
            updated_at: now,
        })
    }

    pub async fn list_ontology_objects(
        &self,
        entity_type_id: Option<&str>,
    ) -> Result<Vec<OntologyObjectRow>> {
        let rows = if let Some(et) = entity_type_id {
            sqlx::query(
                "SELECT id, entity_type_id, entity_type_name, label, fields, created_at, updated_at
                 FROM ontology_objects WHERE entity_type_id = ? ORDER BY created_at DESC",
            )
            .bind(et)
            .fetch_all(&self.pool)
            .await?
        } else {
            sqlx::query(
                "SELECT id, entity_type_id, entity_type_name, label, fields, created_at, updated_at
                 FROM ontology_objects ORDER BY created_at DESC",
            )
            .fetch_all(&self.pool)
            .await?
        };
        Ok(rows
            .into_iter()
            .map(|r| OntologyObjectRow {
                id: r.get("id"),
                entity_type_id: r.get("entity_type_id"),
                entity_type_name: r.get("entity_type_name"),
                label: r.get("label"),
                fields: r.get("fields"),
                created_at: r.get("created_at"),
                updated_at: r.get("updated_at"),
            })
            .collect())
    }

    pub async fn get_ontology_object(&self, id: &str) -> Result<Option<OntologyObjectRow>> {
        let row = sqlx::query(
            "SELECT id, entity_type_id, entity_type_name, label, fields, created_at, updated_at
             FROM ontology_objects WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|r| OntologyObjectRow {
            id: r.get("id"),
            entity_type_id: r.get("entity_type_id"),
            entity_type_name: r.get("entity_type_name"),
            label: r.get("label"),
            fields: r.get("fields"),
            created_at: r.get("created_at"),
            updated_at: r.get("updated_at"),
        }))
    }

    pub async fn update_ontology_object(
        &self,
        id: &str,
        label: &str,
        fields_json: &str,
    ) -> Result<()> {
        let now = Self::now_str();
        sqlx::query(
            "UPDATE ontology_objects SET label = ?, fields = ?, updated_at = ? WHERE id = ?",
        )
        .bind(label)
        .bind(fields_json)
        .bind(&now)
        .bind(id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn delete_ontology_object(&self, id: &str) -> Result<()> {
        sqlx::query("DELETE FROM ontology_objects WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn create_link(
        &self,
        from_id: &str,
        to_id: &str,
        rel_type: &str,
    ) -> Result<OntologyLinkRow> {
        let id = Uuid::new_v4().to_string();
        let now = Self::now_str();
        sqlx::query(
            "INSERT OR IGNORE INTO ontology_links (id, from_id, to_id, rel_type, created_at)
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(from_id)
        .bind(to_id)
        .bind(rel_type)
        .bind(&now)
        .execute(&self.pool)
        .await?;
        Ok(OntologyLinkRow {
            id,
            from_id: from_id.to_string(),
            to_id: to_id.to_string(),
            rel_type: rel_type.to_string(),
            created_at: now,
        })
    }

    pub async fn list_links_for_object(&self, object_id: &str) -> Result<Vec<OntologyLinkRow>> {
        let rows = sqlx::query(
            "SELECT id, from_id, to_id, rel_type, created_at
             FROM ontology_links WHERE from_id = ? OR to_id = ?",
        )
        .bind(object_id)
        .bind(object_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| OntologyLinkRow {
                id: r.get("id"),
                from_id: r.get("from_id"),
                to_id: r.get("to_id"),
                rel_type: r.get("rel_type"),
                created_at: r.get("created_at"),
            })
            .collect())
    }

    /// Like list_links_for_object but also returns the other object's label,
    /// entity_type_name, and entity_type_id for display purposes.
    pub async fn list_links_for_object_enriched(
        &self,
        object_id: &str,
    ) -> Result<Vec<serde_json::Value>> {
        let rows = sqlx::query(
            "SELECT l.id, l.from_id, l.to_id, l.rel_type,
                    o.label        AS other_label,
                    o.entity_type_id   AS other_et_id,
                    o.entity_type_name AS other_et_name
             FROM ontology_links l
             JOIN ontology_objects o ON o.id = CASE
               WHEN l.from_id = ? THEN l.to_id
               ELSE l.from_id
             END
             WHERE l.from_id = ? OR l.to_id = ?",
        )
        .bind(object_id)
        .bind(object_id)
        .bind(object_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| {
                serde_json::json!({
                    "id":             r.get::<String, _>("id"),
                    "from_id":        r.get::<String, _>("from_id"),
                    "to_id":          r.get::<String, _>("to_id"),
                    "rel_type":       r.get::<String, _>("rel_type"),
                    "other_id":       if r.get::<String, _>("from_id") == object_id {
                                          r.get::<String, _>("to_id")
                                      } else {
                                          r.get::<String, _>("from_id")
                                      },
                    "other_label":    r.get::<String, _>("other_label"),
                    "other_et_id":    r.get::<String, _>("other_et_id"),
                    "other_et_name":  r.get::<String, _>("other_et_name"),
                })
            })
            .collect())
    }

    pub async fn delete_link(&self, id: &str) -> Result<()> {
        sqlx::query("DELETE FROM ontology_links WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn get_ontology_graph(
        &self,
    ) -> Result<(Vec<OntologyObjectRow>, Vec<OntologyLinkRow>)> {
        let objects = self.list_ontology_objects(None).await?;
        let links = sqlx::query(
            "SELECT id, from_id, to_id, rel_type, created_at FROM ontology_links",
        )
        .fetch_all(&self.pool)
        .await?
        .into_iter()
        .map(|r| OntologyLinkRow {
            id: r.get("id"),
            from_id: r.get("from_id"),
            to_id: r.get("to_id"),
            rel_type: r.get("rel_type"),
            created_at: r.get("created_at"),
        })
        .collect();
        Ok((objects, links))
    }

    // ── Folds ─────────────────────────────────────────────────────────────────

    pub async fn create_fold(
        &self,
        project_id: &str,
        name: &str,
        description: Option<&str>,
    ) -> Result<FoldRow> {
        let id = Uuid::new_v4().to_string();
        let now = Self::now_str();
        sqlx::query(
            "INSERT INTO folds (id, project_id, name, description, created_at)
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(project_id)
        .bind(name)
        .bind(description)
        .bind(&now)
        .execute(&self.pool)
        .await?;
        Ok(FoldRow { id, project_id: project_id.to_string(), name: name.to_string(), description: description.map(|s| s.to_string()), created_at: now })
    }

    pub async fn list_folds(&self, project_id: &str) -> Result<Vec<FoldRow>> {
        let rows = sqlx::query(
            "SELECT id, project_id, name, description, created_at FROM folds
             WHERE project_id = ? ORDER BY created_at ASC",
        )
        .bind(project_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(|r| FoldRow {
            id: r.get("id"), project_id: r.get("project_id"),
            name: r.get("name"), description: r.get("description"), created_at: r.get("created_at"),
        }).collect())
    }

    pub async fn get_fold(&self, id: &str) -> Result<Option<FoldRow>> {
        let row = sqlx::query(
            "SELECT id, project_id, name, description, created_at FROM folds WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|r| FoldRow {
            id: r.get("id"), project_id: r.get("project_id"),
            name: r.get("name"), description: r.get("description"), created_at: r.get("created_at"),
        }))
    }

    pub async fn delete_fold(&self, id: &str) -> Result<()> {
        sqlx::query("DELETE FROM folds WHERE id = ?").bind(id).execute(&self.pool).await?;
        Ok(())
    }

    /// 返回 (source_count, dataset_count, aggregated_status)
    /// status 聚合规则：有 syncing → "syncing"；有 error → "error"；有 synced → "synced"；否则 "idle"
    pub async fn fold_stats(&self, fold_id: &str) -> Result<(i64, i64, String)> {
        let row = sqlx::query(
            "SELECT
               COUNT(*)                                                          AS src_cnt,
               COALESCE(SUM(CASE WHEN status='syncing' THEN 1 ELSE 0 END), 0)   AS n_syncing,
               COALESCE(SUM(CASE WHEN status='error'   THEN 1 ELSE 0 END), 0)   AS n_error,
               COALESCE(SUM(CASE WHEN status='synced'  THEN 1 ELSE 0 END), 0)   AS n_synced
             FROM data_sources WHERE fold_id = ?",
        )
        .bind(fold_id)
        .fetch_one(&self.pool)
        .await?;

        let src_cnt: i64 = row.try_get("src_cnt").unwrap_or(0);
        let n_syncing: i64 = row.try_get("n_syncing").unwrap_or(0);
        let n_error:   i64 = row.try_get("n_error").unwrap_or(0);
        let n_synced:  i64 = row.try_get("n_synced").unwrap_or(0);

        let status = if n_syncing > 0 { "syncing" }
                     else if n_error > 0 { "error" }
                     else if n_synced > 0 { "synced" }
                     else { "idle" };

        let ds_cnt: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM datasets ds
             JOIN data_sources s ON ds.source_id = s.id
             WHERE s.fold_id = ?",
        )
        .bind(fold_id)
        .fetch_one(&self.pool)
        .await
        .unwrap_or(0);

        Ok((src_cnt, ds_cnt, status.to_string()))
    }

    // ── DataSources ───────────────────────────────────────────────────────────

    pub async fn create_data_source(
        &self,
        fold_id: &str,
        name: &str,
        source_type: &str,
        config: &str,
        group_id: Option<&str>,
        sync_mode: &str,
    ) -> Result<DataSourceRow> {
        let id = Uuid::new_v4().to_string();
        let now = Self::now_str();
        sqlx::query(
            "INSERT INTO data_sources (id, fold_id, name, source_type, config, status, deprecated, created_at, group_id, sync_mode)
             VALUES (?, ?, ?, ?, ?, 'idle', 1, ?, ?, ?)",
        )
        .bind(&id).bind(fold_id).bind(name).bind(source_type).bind(config).bind(&now).bind(group_id).bind(sync_mode)
        .execute(&self.pool)
        .await?;
        Ok(DataSourceRow { id, fold_id: fold_id.to_string(), name: name.to_string(),
            source_type: source_type.to_string(), config: config.to_string(),
            status: "idle".to_string(), write_lock: None, last_sync_at: None,
            record_count: None, created_at: now, deprecated: true, deleted_at: None,
            group_id: group_id.map(|s| s.to_string()),
            sync_mode: sync_mode.to_string() })
    }

    pub async fn list_all_sources(&self) -> Result<Vec<DataSourceRow>> {
        let rows = sqlx::query(
            "SELECT id, fold_id, name, source_type, config, status, write_lock,
                    last_sync_at, record_count, created_at, deprecated, deleted_at, group_id
             FROM data_sources WHERE deleted_at IS NULL ORDER BY created_at DESC",
        )
        .fetch_all(&self.pool).await?;
        Ok(rows.into_iter().map(Self::map_source_row).collect())
    }

    pub async fn list_data_sources(&self, fold_id: &str) -> Result<Vec<DataSourceRow>> {
        let rows = sqlx::query(
            "SELECT id, fold_id, name, source_type, config, status, write_lock,
                    last_sync_at, record_count, created_at, deprecated, deleted_at, group_id
             FROM data_sources WHERE fold_id = ? AND deleted_at IS NULL ORDER BY created_at ASC",
        )
        .bind(fold_id).fetch_all(&self.pool).await?;
        Ok(rows.into_iter().map(Self::map_source_row).collect())
    }

    pub async fn get_data_source(&self, id: &str) -> Result<Option<DataSourceRow>> {
        let row = sqlx::query(
            "SELECT id, fold_id, name, source_type, config, status, write_lock,
                    last_sync_at, record_count, created_at, deprecated, deleted_at, group_id
             FROM data_sources WHERE id = ?",
        )
        .bind(id).fetch_optional(&self.pool).await?;
        Ok(row.map(Self::map_source_row))
    }

    pub async fn update_data_source(
        &self, id: &str, name: &str, source_type: &str, config: &str, sync_mode: &str,
    ) -> Result<()> {
        sqlx::query(
            "UPDATE data_sources SET name = ?, source_type = ?, config = ?, sync_mode = ?, status = 'idle', write_lock = NULL WHERE id = ?",
        )
        .bind(name).bind(source_type).bind(config).bind(sync_mode).bind(id).execute(&self.pool).await?;
        Ok(())
    }

    pub async fn set_source_status(&self, id: &str, status: &str) -> Result<()> {
        sqlx::query("UPDATE data_sources SET status = ? WHERE id = ?")
            .bind(status).bind(id).execute(&self.pool).await?;
        Ok(())
    }

    /// Atomically acquire write_lock (CAS). Returns true if lock was acquired.
    pub async fn acquire_write_lock(&self, source_id: &str, run_id: &str) -> Result<bool> {
        let res = sqlx::query(
            "UPDATE data_sources SET write_lock = ?, status = 'syncing'
             WHERE id = ? AND write_lock IS NULL",
        )
        .bind(run_id).bind(source_id).execute(&self.pool).await?;
        Ok(res.rows_affected() > 0)
    }

    pub async fn release_write_lock(&self, source_id: &str, status: &str, record_count: Option<i64>) -> Result<()> {
        let now = Self::now_str();
        sqlx::query(
            "UPDATE data_sources SET write_lock = NULL, status = ?, last_sync_at = ?, record_count = ? WHERE id = ?",
        )
        .bind(status).bind(&now).bind(record_count).bind(source_id).execute(&self.pool).await?;
        Ok(())
    }

    /// 软删除：设置 deleted_at，不物理删除
    pub async fn delete_data_source(&self, id: &str) -> Result<()> {
        let now = Self::now_str();
        sqlx::query("UPDATE data_sources SET deleted_at = ? WHERE id = ?")
            .bind(&now).bind(id).execute(&self.pool).await?;
        Ok(())
    }

    pub async fn deprecate_data_source(&self, id: &str) -> Result<()> {
        sqlx::query("UPDATE data_sources SET deprecated = 1 WHERE id = ?")
            .bind(id).execute(&self.pool).await?;
        Ok(())
    }

    pub async fn activate_data_source(&self, id: &str) -> Result<()> {
        sqlx::query("UPDATE data_sources SET deprecated = 0 WHERE id = ?")
            .bind(id).execute(&self.pool).await?;
        Ok(())
    }

    fn map_source_row(r: sqlx::sqlite::SqliteRow) -> DataSourceRow {
        use sqlx::Row;
        DataSourceRow {
            id: r.get("id"), fold_id: r.get("fold_id"), name: r.get("name"),
            source_type: r.get("source_type"), config: r.get("config"),
            status: r.get("status"), write_lock: r.get("write_lock"),
            last_sync_at: r.get("last_sync_at"), record_count: r.get("record_count"),
            created_at: r.get("created_at"),
            deprecated: r.get::<i64, _>("deprecated") != 0,
            deleted_at: r.get("deleted_at"),
            group_id: r.get("group_id"),
            sync_mode: r.try_get("sync_mode").unwrap_or_else(|_| "snapshot".to_string()),
        }
    }

    // ── SyncRuns ──────────────────────────────────────────────────────────────

    pub async fn create_sync_run(&self, source_id: &str) -> Result<SyncRunRow> {
        let id = Uuid::new_v4().to_string();
        let now = Self::now_str();
        sqlx::query(
            "INSERT INTO sync_runs (id, source_id, status, processed, started_at)
             VALUES (?, ?, 'pending', 0, ?)",
        )
        .bind(&id).bind(source_id).bind(&now).execute(&self.pool).await?;
        Ok(SyncRunRow { id, source_id: source_id.to_string(), status: "pending".to_string(),
            total_records: None, processed: 0, current_item: None,
            error_message: None, error_type: None, started_at: now, finished_at: None })
    }

    pub async fn get_sync_run(&self, id: &str) -> Result<Option<SyncRunRow>> {
        let row = sqlx::query(
            "SELECT id, source_id, status, total_records, processed, current_item,
                    error_message, error_type, started_at, finished_at
             FROM sync_runs WHERE id = ?",
        )
        .bind(id).fetch_optional(&self.pool).await?;
        Ok(row.map(Self::map_run_row))
    }

    pub async fn list_sync_runs(&self, source_id: &str) -> Result<Vec<SyncRunRow>> {
        let rows = sqlx::query(
            "SELECT id, source_id, status, total_records, processed, current_item,
                    error_message, error_type, started_at, finished_at
             FROM sync_runs WHERE source_id = ? ORDER BY started_at DESC LIMIT 20",
        )
        .bind(source_id).fetch_all(&self.pool).await?;
        Ok(rows.into_iter().map(Self::map_run_row).collect())
    }

    pub async fn update_sync_run_progress(
        &self, id: &str, processed: i64, total: Option<i64>, current: Option<&str>,
    ) -> Result<()> {
        sqlx::query(
            "UPDATE sync_runs SET processed = ?, total_records = ?, current_item = ? WHERE id = ?",
        )
        .bind(processed).bind(total).bind(current).bind(id).execute(&self.pool).await?;
        Ok(())
    }

    pub async fn set_sync_run_status(&self, id: &str, status: &str) -> Result<()> {
        sqlx::query("UPDATE sync_runs SET status = ? WHERE id = ?")
            .bind(status).bind(id).execute(&self.pool).await?;
        Ok(())
    }

    pub async fn finish_sync_run(
        &self, id: &str, status: &str, error: Option<&str>, error_type: Option<&str>,
    ) -> Result<()> {
        let now = Self::now_str();
        sqlx::query(
            "UPDATE sync_runs SET status = ?, error_message = ?, error_type = ?, finished_at = ? WHERE id = ?",
        )
        .bind(status).bind(error).bind(error_type).bind(&now).bind(id).execute(&self.pool).await?;
        Ok(())
    }

    fn map_run_row(r: sqlx::sqlite::SqliteRow) -> SyncRunRow {
        use sqlx::Row;
        SyncRunRow {
            id: r.get("id"), source_id: r.get("source_id"), status: r.get("status"),
            total_records: r.get("total_records"), processed: r.get("processed"),
            current_item: r.get("current_item"), error_message: r.get("error_message"),
            error_type: r.get("error_type"), started_at: r.get("started_at"),
            finished_at: r.get("finished_at"),
        }
    }

    // ── Datasets ──────────────────────────────────────────────────────────────

    pub async fn create_dataset(&self, source_id: &str, name: &str) -> Result<DatasetRow> {
        let id = Uuid::new_v4().to_string();
        let now = Self::now_str();
        sqlx::query(
            "INSERT INTO datasets (id, source_id, name, current_version, created_at)
             VALUES (?, ?, ?, 0, ?)",
        )
        .bind(&id).bind(source_id).bind(name).bind(&now).execute(&self.pool).await?;
        Ok(DatasetRow { id, source_id: source_id.to_string(), name: name.to_string(),
            entity_type_id: None, current_version: 0, created_at: now })
    }

    /// List all datasets across all sources, with record count (raw only).
    pub async fn list_all_datasets(&self) -> Result<Vec<serde_json::Value>> {
        let rows = sqlx::query(
            "SELECT d.id, d.source_id, d.name, d.entity_type_id, d.current_version, d.created_at,
                    s.fold_id, s.name AS source_name,
                    f.name AS fold_name,
                    COALESCE(
                        (SELECT dv.total_rows FROM dataset_versions dv
                         WHERE dv.dataset_id = d.id AND dv.is_current = 1 LIMIT 1),
                        0
                    ) AS record_count
             FROM datasets d
             LEFT JOIN data_sources s ON s.id = d.source_id
             LEFT JOIN folds f ON f.id = s.fold_id
             ORDER BY d.created_at DESC",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(|r| serde_json::json!({
            "id":              r.get::<String, _>("id"),
            "source_id":       r.get::<String, _>("source_id"),
            "source_name":     r.get::<Option<String>, _>("source_name"),
            "name":            r.get::<String, _>("name"),
            "entity_type_id":  r.get::<Option<String>, _>("entity_type_id"),
            "current_version": r.get::<i64, _>("current_version"),
            "created_at":      r.get::<String, _>("created_at"),
            "record_count":    r.get::<i64, _>("record_count"),
            "fold_id":         r.get::<Option<String>, _>("fold_id"),
            "fold_name":       r.get::<Option<String>, _>("fold_name"),
        })).collect())
    }

    pub async fn list_datasets(&self, source_id: &str) -> Result<Vec<DatasetRow>> {
        let rows = sqlx::query(
            "SELECT id, source_id, name, entity_type_id, current_version, created_at
             FROM datasets WHERE source_id = ? ORDER BY created_at DESC",
        )
        .bind(source_id).fetch_all(&self.pool).await?;
        Ok(rows.into_iter().map(|r| DatasetRow {
            id: r.get("id"), source_id: r.get("source_id"), name: r.get("name"),
            entity_type_id: r.get("entity_type_id"), current_version: r.get("current_version"),
            created_at: r.get("created_at"),
        }).collect())
    }

    /// Like list_datasets but includes record_count from dataset_versions.total_rows (S3-first).
    pub async fn list_datasets_with_count(&self, source_id: &str) -> Result<Vec<serde_json::Value>> {
        let rows = sqlx::query(
            "SELECT d.id, d.source_id, d.name, d.entity_type_id, d.current_version, d.created_at,
                    COALESCE(
                        (SELECT dv.total_rows FROM dataset_versions dv
                         WHERE dv.dataset_id = d.id AND dv.is_current = 1 LIMIT 1),
                        0
                    ) AS record_count
             FROM datasets d WHERE d.source_id = ? ORDER BY d.created_at DESC",
        )
        .bind(source_id).fetch_all(&self.pool).await?;
        Ok(rows.into_iter().map(|r| serde_json::json!({
            "id":              r.get::<String, _>("id"),
            "source_id":       r.get::<String, _>("source_id"),
            "name":            r.get::<String, _>("name"),
            "entity_type_id":  r.get::<Option<String>, _>("entity_type_id"),
            "current_version": r.get::<i64, _>("current_version"),
            "created_at":      r.get::<String, _>("created_at"),
            "record_count":    r.get::<i64, _>("record_count"),
        })).collect())
    }

    pub async fn get_dataset(&self, id: &str) -> Result<Option<DatasetRow>> {
        let row = sqlx::query(
            "SELECT id, source_id, name, entity_type_id, current_version, created_at
             FROM datasets WHERE id = ?",
        )
        .bind(id).fetch_optional(&self.pool).await?;
        Ok(row.map(|r| DatasetRow {
            id: r.get("id"), source_id: r.get("source_id"), name: r.get("name"),
            entity_type_id: r.get("entity_type_id"), current_version: r.get("current_version"),
            created_at: r.get("created_at"),
        }))
    }

    // ── DatasetVersions ───────────────────────────────────────────────────────

    pub async fn create_dataset_version(
        &self, dataset_id: &str, sync_run_id: &str,
    ) -> Result<DatasetVersionRow> {
        // Get next version number
        let next: i64 = sqlx::query_scalar(
            "SELECT COALESCE(MAX(version), 0) + 1 FROM dataset_versions WHERE dataset_id = ?",
        )
        .bind(dataset_id).fetch_one(&self.pool).await?;

        let id = Uuid::new_v4().to_string();
        let now = Self::now_str();
        sqlx::query(
            "INSERT INTO dataset_versions
             (id, dataset_id, version, sync_run_id, status, total_rows, is_current, created_at)
             VALUES (?, ?, ?, ?, 'pending', 0, 0, ?)",
        )
        .bind(&id).bind(dataset_id).bind(next).bind(sync_run_id).bind(&now)
        .execute(&self.pool).await?;

        Ok(DatasetVersionRow { id, dataset_id: dataset_id.to_string(), version: next,
            sync_run_id: sync_run_id.to_string(), status: "pending".to_string(),
            schema_json: "{}".to_string(), schema_change: None,
            total_rows: 0, is_current: false, created_at: now,
            manifest_path: None })
    }

    pub async fn commit_dataset_version(
        &self, version_id: &str, dataset_id: &str, total_rows: i64, schema_json: &str,
    ) -> Result<()> {
        // Clear previous current
        sqlx::query("UPDATE dataset_versions SET is_current = 0 WHERE dataset_id = ?")
            .bind(dataset_id).execute(&self.pool).await?;
        // Commit this version
        sqlx::query(
            "UPDATE dataset_versions
             SET status = 'committed', is_current = 1, total_rows = ?, schema_json = ?
             WHERE id = ?",
        )
        .bind(total_rows).bind(schema_json).bind(version_id).execute(&self.pool).await?;
        // Update dataset current_version
        let version: i64 = sqlx::query_scalar(
            "SELECT version FROM dataset_versions WHERE id = ?",
        )
        .bind(version_id).fetch_one(&self.pool).await?;
        sqlx::query("UPDATE datasets SET current_version = ? WHERE id = ?")
            .bind(version).bind(dataset_id).execute(&self.pool).await?;
        Ok(())
    }

    pub async fn abort_dataset_version(&self, version_id: &str) -> Result<()> {
        sqlx::query("UPDATE dataset_versions SET status = 'aborted' WHERE id = ?")
            .bind(version_id).execute(&self.pool).await?;
        Ok(())
    }

    // ── Platform config ───────────────────────────────────────────────────────

    pub async fn get_platform_config(&self, key: &str) -> Result<Option<String>> {
        let val: Option<String> = sqlx::query_scalar(
            "SELECT value FROM platform_config WHERE key = ?",
        )
        .bind(key)
        .fetch_optional(&self.pool)
        .await?;
        Ok(val)
    }

    pub async fn set_platform_config(&self, key: &str, value: &str) -> Result<()> {
        let now = Self::now_str();
        sqlx::query(
            "INSERT INTO platform_config (key, value, updated_at) VALUES (?, ?, ?)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
        )
        .bind(key).bind(value).bind(now)
        .execute(&self.pool).await?;
        Ok(())
    }

    pub async fn get_storage_config(&self) -> Result<serde_json::Value> {
        let keys = ["storage.endpoint", "storage.bucket", "storage.access_key",
                    "storage.secret_key", "storage.region"];
        let mut map = serde_json::Map::new();
        for key in &keys {
            if let Some(v) = self.get_platform_config(key).await? {
                let short = key.strip_prefix("storage.").unwrap_or(key);
                map.insert(short.to_string(), serde_json::Value::String(v));
            }
        }
        Ok(serde_json::Value::Object(map))
    }

    pub async fn set_storage_config(&self, cfg: &serde_json::Value) -> Result<()> {
        let fields = [
            ("endpoint",   "storage.endpoint"),
            ("bucket",     "storage.bucket"),
            ("access_key", "storage.access_key"),
            ("secret_key", "storage.secret_key"),
            ("region",     "storage.region"),
        ];
        for (json_key, db_key) in &fields {
            if let Some(v) = cfg[json_key].as_str() {
                self.set_platform_config(db_key, v).await?;
            }
        }
        Ok(())
    }

    /// Update the manifest_path after platform storage write (Iter-1).
    pub async fn update_version_manifest_path(
        &self, version_id: &str, manifest_path: &str,
    ) -> Result<()> {
        sqlx::query("UPDATE dataset_versions SET manifest_path = ? WHERE id = ?")
            .bind(manifest_path).bind(version_id).execute(&self.pool).await?;
        Ok(())
    }

    pub async fn list_dataset_versions(&self, dataset_id: &str) -> Result<Vec<DatasetVersionRow>> {
        let rows = sqlx::query(
            "SELECT id, dataset_id, version, sync_run_id, status, schema_json, schema_change,
                    total_rows, is_current, created_at, manifest_path
             FROM dataset_versions WHERE dataset_id = ? ORDER BY version DESC",
        )
        .bind(dataset_id).fetch_all(&self.pool).await?;
        Ok(rows.into_iter().map(Self::map_version_row).collect())
    }

    pub async fn rollback_dataset_version(
        &self, dataset_id: &str, version: i64,
    ) -> Result<bool> {
        // Check target version exists and is committed
        let exists: Option<String> = sqlx::query_scalar(
            "SELECT id FROM dataset_versions
             WHERE dataset_id = ? AND version = ? AND status = 'committed'",
        )
        .bind(dataset_id).bind(version).fetch_optional(&self.pool).await?;
        if exists.is_none() { return Ok(false); }

        sqlx::query("UPDATE dataset_versions SET is_current = 0 WHERE dataset_id = ?")
            .bind(dataset_id).execute(&self.pool).await?;
        sqlx::query(
            "UPDATE dataset_versions SET is_current = 1 WHERE dataset_id = ? AND version = ?",
        )
        .bind(dataset_id).bind(version).execute(&self.pool).await?;
        sqlx::query("UPDATE datasets SET current_version = ? WHERE id = ?")
            .bind(version).bind(dataset_id).execute(&self.pool).await?;
        Ok(true)
    }

    // ── Iter-3: Schema evolution, GC, Rollback ────────────────────────────────

    /// Get the previous committed version's schema_json for schema diff.
    pub async fn get_prev_committed_schema(
        &self, dataset_id: &str, current_version: i64,
    ) -> Result<Option<String>> {
        let schema: Option<String> = sqlx::query_scalar(
            "SELECT schema_json FROM dataset_versions
             WHERE dataset_id = ? AND version < ? AND status = 'committed'
             ORDER BY version DESC LIMIT 1",
        )
        .bind(dataset_id).bind(current_version)
        .fetch_optional(&self.pool).await?;
        Ok(schema)
    }

    /// Store schema_change classification in a committed version.
    pub async fn set_version_schema_change(
        &self, version_id: &str, schema_change: &str,
    ) -> Result<()> {
        sqlx::query("UPDATE dataset_versions SET schema_change = ? WHERE id = ?")
            .bind(schema_change).bind(version_id).execute(&self.pool).await?;
        Ok(())
    }

    /// Delete all OntologyObjects belonging to a dataset (used before re-materialization).
    pub async fn delete_dataset_objects(&self, dataset_id: &str) -> Result<u64> {
        let result = sqlx::query(
            "DELETE FROM ontology_objects WHERE dataset_id = ?",
        )
        .bind(dataset_id).execute(&self.pool).await?;
        Ok(result.rows_affected())
    }

    /// Return committed versions older than keep_n, ordered oldest first.
    /// Returns (id, version, manifest_path) tuples.
    pub async fn old_dataset_versions(
        &self, dataset_id: &str, keep_n: i64,
    ) -> Result<Vec<(String, i64, Option<String>)>> {
        let rows = sqlx::query(
            "SELECT id, version, manifest_path FROM dataset_versions
             WHERE dataset_id = ? AND status = 'committed'
             ORDER BY version DESC LIMIT -1 OFFSET ?",
        )
        .bind(dataset_id).bind(keep_n)
        .fetch_all(&self.pool).await?;
        Ok(rows.into_iter().map(|r| {
            use sqlx::Row;
            (r.get::<String,_>("id"), r.get::<i64,_>("version"), r.try_get("manifest_path").ok().flatten())
        }).collect())
    }

    /// Mark a version as GC'd and remove its manifest_path reference.
    pub async fn gc_version(&self, version_id: &str) -> Result<()> {
        sqlx::query(
            "UPDATE dataset_versions SET status = 'gc', manifest_path = NULL WHERE id = ?",
        )
        .bind(version_id).execute(&self.pool).await?;
        Ok(())
    }

    /// Get the currently active (is_current=1) version for a dataset.
    pub async fn get_current_dataset_version(&self, dataset_id: &str) -> Result<Option<DatasetVersionRow>> {
        let row = sqlx::query(
            "SELECT id, dataset_id, version, sync_run_id, status, schema_json, schema_change,
                    total_rows, is_current, created_at, manifest_path
             FROM dataset_versions WHERE dataset_id = ? AND is_current = 1 LIMIT 1",
        )
        .bind(dataset_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(Self::map_version_row))
    }

    fn map_version_row(r: sqlx::sqlite::SqliteRow) -> DatasetVersionRow {
        use sqlx::Row;
        DatasetVersionRow {
            id: r.get("id"), dataset_id: r.get("dataset_id"), version: r.get("version"),
            sync_run_id: r.get("sync_run_id"), status: r.get("status"),
            schema_json: r.get("schema_json"), schema_change: r.get("schema_change"),
            total_rows: r.get("total_rows"),
            is_current: r.get::<i64, _>("is_current") != 0,
            created_at: r.get("created_at"),
            manifest_path: r.try_get("manifest_path").ok().flatten(),
        }
    }

    pub async fn list_dataset_records(
        &self,
        dataset_id: &str,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<OntologyObjectRow>> {
        let rows = sqlx::query(
            "SELECT id, entity_type_id, entity_type_name, label, fields, created_at, updated_at
             FROM ontology_objects WHERE dataset_id = ? AND sync_run_id != 'promote'
             ORDER BY created_at ASC LIMIT ? OFFSET ?",
        )
        .bind(dataset_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(|r| OntologyObjectRow {
            id: r.get("id"),
            entity_type_id: r.get("entity_type_id"),
            entity_type_name: r.get("entity_type_name"),
            label: r.get("label"),
            fields: r.get("fields"),
            created_at: r.get("created_at"),
            updated_at: r.get("updated_at"),
        }).collect())
    }

    pub async fn count_dataset_records(&self, dataset_id: &str) -> Result<i64> {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM ontology_objects WHERE dataset_id = ? AND sync_run_id != 'promote'",
        )
        .bind(dataset_id)
        .fetch_one(&self.pool)
        .await?;
        Ok(count)
    }

    /// Upsert a single ontology object.
    /// If `external_id` is Some, deduplicates by (entity_type_id, external_id).
    /// If `external_id` is None, always inserts a new row.
    pub async fn upsert_ontology_object(
        &self,
        entity_type_id: &str,
        entity_type_name: &str,
        external_id: Option<&str>,
        label: &str,
        fields: &str,
        dataset_id: &str,
        run_id: &str,
    ) -> Result<()> {
        let now = Self::now_str();
        let id = Uuid::new_v4().to_string();
        // ON CONFLICT (entity_type_id, external_id):
        //   - fields: json_patch merges new fields over existing (new values win, null removes)
        //   - source_ids: append dataset_id if not already present (provenance tracking)
        //   - label: update from latest source
        sqlx::query(
            "INSERT INTO ontology_objects
                (id, entity_type_id, entity_type_name, external_id, label, fields,
                 dataset_id, sync_run_id, source_ids, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, json_array(?), ?, ?)
             ON CONFLICT(entity_type_id, external_id) DO UPDATE SET
                label      = excluded.label,
                fields     = json_patch(fields, excluded.fields),
                source_ids = CASE
                    WHEN instr(source_ids, ?) > 0 THEN source_ids
                    ELSE json_insert(source_ids, '$[#]', ?)
                END,
                dataset_id = excluded.dataset_id,
                updated_at = excluded.updated_at",
        )
        .bind(&id)
        .bind(entity_type_id)
        .bind(entity_type_name)
        .bind(external_id)
        .bind(label)
        .bind(fields)
        .bind(dataset_id)
        .bind(run_id)
        .bind(dataset_id)   // initial source_ids = [dataset_id]
        .bind(&now)
        .bind(&now)
        .bind(dataset_id)   // CASE WHEN instr check
        .bind(dataset_id)   // json_insert append
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    // ── Object Type Mappings ───────────────────────────────────────────────────

    pub async fn save_object_type_mapping(
        &self,
        dataset_id: &str,
        entity_type_id: &str,
        primary_key_col: &str,
        field_mapping: &str,
        sync_mode: &str,
    ) -> Result<()> {
        let now = Self::now_str();
        let id = Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO object_type_mappings
                (id, dataset_id, entity_type_id, primary_key_col, field_mapping, sync_mode, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(dataset_id) DO UPDATE SET
                entity_type_id  = excluded.entity_type_id,
                primary_key_col = excluded.primary_key_col,
                field_mapping   = excluded.field_mapping,
                sync_mode       = excluded.sync_mode,
                updated_at      = excluded.updated_at",
        )
        .bind(&id)
        .bind(dataset_id)
        .bind(entity_type_id)
        .bind(primary_key_col)
        .bind(field_mapping)
        .bind(sync_mode)
        .bind(&now)
        .bind(&now)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn update_dataset_entity_type(&self, dataset_id: &str, entity_type_id: &str) -> Result<()> {
        sqlx::query("UPDATE datasets SET entity_type_id = ? WHERE id = ?")
            .bind(entity_type_id)
            .bind(dataset_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn update_dataset_sync_mode(&self, dataset_id: &str, sync_mode: &str) -> Result<()> {
        sqlx::query(
            "UPDATE object_type_mappings SET sync_mode = ? WHERE dataset_id = ?",
        )
        .bind(sync_mode)
        .bind(dataset_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn list_mapped_dataset_ids(&self) -> Result<Vec<String>> {
        let rows = sqlx::query(
            "SELECT DISTINCT dataset_id FROM object_type_mappings WHERE entity_type_id IS NOT NULL AND entity_type_id != ''",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.iter().map(|r| r.get::<String, _>("dataset_id")).collect())
    }

    pub async fn get_object_type_mapping(
        &self,
        dataset_id: &str,
    ) -> Result<Option<serde_json::Value>> {
        let row = sqlx::query(
            "SELECT entity_type_id, primary_key_col, field_mapping, sync_mode, updated_at
             FROM object_type_mappings WHERE dataset_id = ?",
        )
        .bind(dataset_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|r| {
            serde_json::json!({
                "entity_type_id":  r.get::<String, _>("entity_type_id"),
                "primary_key_col": r.get::<String, _>("primary_key_col"),
                "field_mapping":   r.get::<String, _>("field_mapping"),
                "sync_mode":       r.try_get::<String, _>("sync_mode").unwrap_or_else(|_| "snapshot".to_string()),
                "updated_at":      r.get::<String, _>("updated_at"),
            })
        }))
    }

    pub async fn delete_ontology_objects_by_dataset(&self, dataset_id: &str) -> Result<()> {
        // Only fully delete objects that have no other contributing sources.
        // Objects shared across multiple sources just lose this dataset from source_ids.
        sqlx::query(
            "DELETE FROM ontology_objects
             WHERE dataset_id = ?
               AND (source_ids IS NULL
                    OR source_ids = '[]'
                    OR source_ids = json_array(?))",
        )
        .bind(dataset_id)
        .bind(dataset_id)
        .execute(&self.pool)
        .await?;

        // For multi-source objects: remove this dataset_id from source_ids array
        // SQLite doesn't have json_remove by value, so we re-build the array
        sqlx::query(
            "UPDATE ontology_objects
             SET source_ids = (
                SELECT json_group_array(value)
                FROM json_each(source_ids)
                WHERE value != ?
             ),
             updated_at = ?
             WHERE instr(source_ids, ?) > 0",
        )
        .bind(dataset_id)
        .bind(Self::now_str())
        .bind(dataset_id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Save (upsert) link type mappings for a dataset (replaces all existing entries).
    pub async fn save_link_type_mappings(
        &self,
        dataset_id: &str,
        links: &[LinkTypeMappingInput],
    ) -> Result<()> {
        let now = Self::now_str();
        sqlx::query("DELETE FROM link_type_mappings WHERE dataset_id = ?")
            .bind(dataset_id)
            .execute(&self.pool)
            .await?;
        for link in links {
            let id = Uuid::new_v4().to_string();
            sqlx::query(
                "INSERT INTO link_type_mappings (id, dataset_id, from_fk_col, to_entity_type_id, rel_type, created_at)
                 VALUES (?, ?, ?, ?, ?, ?)",
            )
            .bind(&id)
            .bind(dataset_id)
            .bind(&link.from_fk_col)
            .bind(&link.to_entity_type_id)
            .bind(&link.rel_type)
            .bind(&now)
            .execute(&self.pool)
            .await?;
        }
        Ok(())
    }

    pub async fn get_link_type_mappings(&self, dataset_id: &str) -> Result<Vec<serde_json::Value>> {
        let rows = sqlx::query(
            "SELECT from_fk_col, to_entity_type_id, rel_type FROM link_type_mappings WHERE dataset_id = ?",
        )
        .bind(dataset_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(|r| serde_json::json!({
            "from_fk_col":       r.get::<String, _>("from_fk_col"),
            "to_entity_type_id": r.get::<String, _>("to_entity_type_id"),
            "rel_type":          r.get::<String, _>("rel_type"),
        })).collect())
    }

    /// Return all schema-level relationships: (from_et_id, from_et_name, fk_col, rel_type, to_et_id, to_et_name)
    pub async fn list_schema_links(&self) -> Result<Vec<serde_json::Value>> {
        let rows = sqlx::query(
            "SELECT ltm.from_fk_col, ltm.to_entity_type_id, ltm.rel_type,
                    otm.entity_type_id AS from_entity_type_id,
                    et_from.display_name AS from_et_name,
                    et_to.display_name  AS to_et_name
             FROM link_type_mappings ltm
             JOIN object_type_mappings otm ON otm.dataset_id = ltm.dataset_id
             JOIN entity_types et_from ON et_from.id = otm.entity_type_id
             JOIN entity_types et_to   ON et_to.id  = ltm.to_entity_type_id",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(|r| serde_json::json!({
            "from_entity_type_id": r.get::<String, _>("from_entity_type_id"),
            "from_et_name":        r.get::<String, _>("from_et_name"),
            "fk_col":              r.get::<String, _>("from_fk_col"),
            "rel_type":            r.get::<String, _>("rel_type"),
            "to_entity_type_id":   r.get::<String, _>("to_entity_type_id"),
            "to_et_name":          r.get::<String, _>("to_et_name"),
        })).collect())
    }

    /// After promote, resolve FK columns → ontology_links using link_type_mappings.
    pub async fn resolve_links_for_dataset(&self, dataset_id: &str) -> Result<usize> {
        let mappings = self.get_link_type_mappings(dataset_id).await?;
        let now = Self::now_str();
        let mut total = 0usize;

        for m in &mappings {
            let fk_col = m["from_fk_col"].as_str().unwrap_or_default();
            let to_et  = m["to_entity_type_id"].as_str().unwrap_or_default();
            let rel    = m["rel_type"].as_str().unwrap_or("HAS");

            // Find source objects promoted from this dataset (both manual and auto-promote)
            let src_rows = sqlx::query(
                "SELECT id, fields FROM ontology_objects
                 WHERE dataset_id = ? AND sync_run_id IN ('promote', 'auto-promote')",
            )
            .bind(dataset_id)
            .fetch_all(&self.pool)
            .await?;

            for src in &src_rows {
                let src_id: String = src.get("id");
                let fields_str: String = src.get("fields");
                let fields: serde_json::Value = serde_json::from_str(&fields_str).unwrap_or_default();
                let fk_val = match fields.get(fk_col).and_then(|v| v.as_str()) {
                    Some(v) => v.to_string(),
                    None => continue,
                };

                // Look up target object by external_id (both manual and auto-promote)
                let tgt: Option<String> = sqlx::query_scalar(
                    "SELECT id FROM ontology_objects
                     WHERE entity_type_id = ? AND external_id = ? AND sync_run_id IN ('promote', 'auto-promote')",
                )
                .bind(to_et)
                .bind(&fk_val)
                .fetch_optional(&self.pool)
                .await?;

                if let Some(tgt_id) = tgt {
                    let link_id = Uuid::new_v4().to_string();
                    let _ = sqlx::query(
                        "INSERT OR IGNORE INTO ontology_links
                         (id, from_id, to_id, rel_type, created_at)
                         VALUES (?, ?, ?, ?, ?)",
                    )
                    .bind(&link_id)
                    .bind(&src_id)
                    .bind(&tgt_id)
                    .bind(rel)
                    .bind(&now)
                    .execute(&self.pool)
                    .await;
                    total += 1;
                }
            }
        }
        Ok(total)
    }

    // ── Bounded Contexts ──────────────────────────────────────────────────────

    pub async fn create_bounded_context(
        &self, fold_id: &str, name: &str, color: &str, auto_detected: bool,
    ) -> Result<serde_json::Value> {
        let id = Uuid::new_v4().to_string();
        let now = Self::now_str();
        sqlx::query(
            "INSERT INTO bounded_contexts (id, fold_id, name, color, auto_detected, created_at)
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(fold_id)
        .bind(name)
        .bind(color)
        .bind(auto_detected as i64)
        .bind(&now)
        .execute(&self.pool)
        .await?;
        Ok(serde_json::json!({
            "id": id, "fold_id": fold_id, "name": name,
            "color": color, "auto_detected": auto_detected, "created_at": now,
        }))
    }

    pub async fn list_bounded_contexts(&self, fold_id: &str) -> Result<Vec<serde_json::Value>> {
        let rows = sqlx::query(
            "SELECT id, fold_id, name, color, auto_detected, created_at
             FROM bounded_contexts WHERE fold_id = ? ORDER BY created_at",
        )
        .bind(fold_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(|r| serde_json::json!({
            "id":            r.get::<String, _>("id"),
            "fold_id":       r.get::<String, _>("fold_id"),
            "name":          r.get::<String, _>("name"),
            "color":         r.get::<String, _>("color"),
            "auto_detected": r.get::<i64, _>("auto_detected") != 0,
            "created_at":    r.get::<String, _>("created_at"),
        })).collect())
    }

    pub async fn update_bounded_context(
        &self, bc_id: &str, name: Option<&str>, color: Option<&str>,
    ) -> Result<()> {
        if let Some(n) = name {
            sqlx::query("UPDATE bounded_contexts SET name = ? WHERE id = ?")
                .bind(n).bind(bc_id).execute(&self.pool).await?;
        }
        if let Some(c) = color {
            sqlx::query("UPDATE bounded_contexts SET color = ? WHERE id = ?")
                .bind(c).bind(bc_id).execute(&self.pool).await?;
        }
        Ok(())
    }

    pub async fn delete_bounded_context(&self, bc_id: &str) -> Result<()> {
        // Detach all ETs that belong to this BC before deleting
        sqlx::query("UPDATE entity_types SET bc_id = NULL WHERE bc_id = ?")
            .bind(bc_id).execute(&self.pool).await?;
        sqlx::query("DELETE FROM bounded_contexts WHERE id = ?")
            .bind(bc_id).execute(&self.pool).await?;
        Ok(())
    }

    /// Assign or unassign an ET to a BC (NULL = revert to fold-level)
    pub async fn set_entity_type_bc(&self, et_id: &str, bc_id: Option<&str>) -> Result<()> {
        sqlx::query("UPDATE entity_types SET bc_id = ? WHERE id = ?")
            .bind(bc_id)
            .bind(et_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Bulk-upsert child-BC inference result for a fold.
    /// Clears auto-detected BCs for the fold and inserts the new set.
    pub async fn upsert_inferred_bcs(
        &self,
        fold_id: &str,
        bcs: &[(String, String, Vec<String>)], // (name, color, et_ids)
    ) -> Result<Vec<serde_json::Value>> {
        // Remove stale auto-detected BCs (user-confirmed ones are preserved)
        sqlx::query("DELETE FROM bounded_contexts WHERE fold_id = ? AND auto_detected = 1")
            .bind(fold_id).execute(&self.pool).await?;

        let now = Self::now_str();
        let mut created = Vec::new();
        for (name, color, et_ids) in bcs {
            let bc_id = Uuid::new_v4().to_string();
            sqlx::query(
                "INSERT OR IGNORE INTO bounded_contexts (id, fold_id, name, color, auto_detected, created_at)
                 VALUES (?, ?, ?, ?, 1, ?)",
            )
            .bind(&bc_id).bind(fold_id).bind(name).bind(color).bind(&now)
            .execute(&self.pool).await?;

            for et_id in et_ids {
                let _ = sqlx::query("UPDATE entity_types SET bc_id = ? WHERE id = ?")
                    .bind(&bc_id).bind(et_id).execute(&self.pool).await;
            }
            created.push(serde_json::json!({
                "id": bc_id, "fold_id": fold_id, "name": name,
                "color": color, "auto_detected": true, "et_ids": et_ids,
            }));
        }
        Ok(created)
    }

    // ── BC Relationships ───────────────────────────────────────────────────────

    pub async fn create_bc_relationship(
        &self,
        from_bc_id: &str, to_bc_id: &str,
        relationship_type: &str, notes: Option<&str>,
    ) -> Result<serde_json::Value> {
        let id = Uuid::new_v4().to_string();
        let now = Self::now_str();
        sqlx::query(
            "INSERT OR IGNORE INTO bc_relationships
             (id, from_bc_id, to_bc_id, relationship_type, notes, created_at)
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(&id).bind(from_bc_id).bind(to_bc_id)
        .bind(relationship_type).bind(notes).bind(&now)
        .execute(&self.pool).await?;
        Ok(serde_json::json!({
            "id": id, "from_bc_id": from_bc_id, "to_bc_id": to_bc_id,
            "relationship_type": relationship_type, "notes": notes, "created_at": now,
        }))
    }

    pub async fn list_bc_relationships(&self, bc_id: &str) -> Result<Vec<serde_json::Value>> {
        let rows = sqlx::query(
            "SELECT r.id, r.from_bc_id, r.to_bc_id, r.relationship_type, r.notes, r.created_at,
                    fb.name AS from_name, tb.name AS to_name
             FROM bc_relationships r
             JOIN bounded_contexts fb ON fb.id = r.from_bc_id
             JOIN bounded_contexts tb ON tb.id = r.to_bc_id
             WHERE r.from_bc_id = ? OR r.to_bc_id = ?
             ORDER BY r.created_at",
        )
        .bind(bc_id).bind(bc_id)
        .fetch_all(&self.pool).await?;
        Ok(rows.into_iter().map(|r| serde_json::json!({
            "id":                r.get::<String, _>("id"),
            "from_bc_id":        r.get::<String, _>("from_bc_id"),
            "from_name":         r.get::<String, _>("from_name"),
            "to_bc_id":          r.get::<String, _>("to_bc_id"),
            "to_name":           r.get::<String, _>("to_name"),
            "relationship_type": r.get::<String, _>("relationship_type"),
            "notes":             r.get::<Option<String>, _>("notes"),
            "created_at":        r.get::<String, _>("created_at"),
        })).collect())
    }

    pub async fn delete_bc_relationship(&self, rel_id: &str) -> Result<()> {
        sqlx::query("DELETE FROM bc_relationships WHERE id = ?")
            .bind(rel_id).execute(&self.pool).await?;
        Ok(())
    }

    pub async fn clear_project_graph(&self, project_id: &str) -> Result<()> {
        sqlx::query("DELETE FROM live_entities WHERE project_id = ?")
            .bind(project_id)
            .execute(&self.pool)
            .await?;
        sqlx::query("DELETE FROM live_relationships WHERE project_id = ?")
            .bind(project_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}
