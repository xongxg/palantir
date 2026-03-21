use anyhow::Result;
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

// ── Row types ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ProjectRow {
    pub id: String,
    pub name: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone)]
pub struct ConnectorRow {
    pub id: String,
    pub project_id: String,
    pub path: String,
    pub ns: String,
    pub schema_name: String,
    pub headers: Option<String>,        // JSON array of column names
    pub samples: Option<String>,        // JSON array of sample rows (first 5)
    pub mapping_config: Option<String>, // JSON: {entity_type, id_field, columns:[...]}
}

#[derive(Debug, Clone)]
pub struct EntityRow {
    pub id: String,
    pub project_id: String,
    pub entity_type: String,
    pub ddd_concept: String,
    pub label: String,
    pub properties: String, // JSON
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BuildRow {
    pub id: String,
    pub project_id: String,
    pub created_at: String,
    pub entities: i64,
    pub relationships: i64,
    pub bounded_contexts: i64,
    pub applied_events: i64,
}

pub struct RelRow {
    pub project_id: String,
    pub from_id: String,
    pub to_id: String,
    pub kind: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct EntityTypeRow {
    pub id: String,
    pub name: String,
    pub display_name: String,
    pub color: String,
    pub icon: String,
    pub created_at: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct EntityFieldRow {
    pub id: String,
    pub entity_type_id: String,
    pub name: String,
    pub data_type: String,
    pub is_required: bool,
    pub classification: String,
    pub sort_order: i64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct OntologyObjectRow {
    pub id: String,
    pub entity_type_id: String,
    pub entity_type_name: String,
    pub label: String,
    pub fields: String, // JSON
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct OntologyLinkRow {
    pub id: String,
    pub from_id: String,
    pub to_id: String,
    pub rel_type: String,
    pub created_at: String,
}

// ── Ingest row types ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FoldRow {
    pub id: String,
    pub project_id: String,
    pub name: String,
    pub description: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DataSourceRow {
    pub id: String,
    pub fold_id: String,
    pub name: String,
    pub source_type: String,
    pub config: String,      // JSON
    pub status: String,
    pub write_lock: Option<String>,
    pub last_sync_at: Option<String>,
    pub record_count: Option<i64>,
    pub created_at: String,
    pub deprecated: bool,
    pub deleted_at: Option<String>,
    pub group_id: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SyncRunRow {
    pub id: String,
    pub source_id: String,
    pub status: String,
    pub total_records: Option<i64>,
    pub processed: i64,
    pub current_item: Option<String>,
    pub error_message: Option<String>,
    pub error_type: Option<String>,
    pub started_at: String,
    pub finished_at: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DatasetRow {
    pub id: String,
    pub source_id: String,
    pub name: String,
    pub entity_type_id: Option<String>,
    pub current_version: i64,
    pub created_at: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DatasetVersionRow {
    pub id: String,
    pub dataset_id: String,
    pub version: i64,
    pub sync_run_id: String,
    pub status: String,
    pub schema_json: String,
    pub schema_change: Option<String>,
    pub total_rows: i64,
    pub is_current: bool,
    pub created_at: String,
    pub manifest_path: Option<String>,
}

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
        let _ = sqlx::query(
            "CREATE UNIQUE INDEX IF NOT EXISTS idx_source_name ON data_sources(fold_id, name)",
        ).execute(&self.pool).await;

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
    ) -> Result<EntityTypeRow> {
        let id = Uuid::new_v4().to_string();
        let now = Self::now_str();
        sqlx::query(
            "INSERT INTO entity_types (id, name, display_name, color, icon, created_at)
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(name)
        .bind(display_name)
        .bind(color)
        .bind(icon)
        .bind(&now)
        .execute(&self.pool)
        .await?;
        Ok(EntityTypeRow {
            id,
            name: name.to_string(),
            display_name: display_name.to_string(),
            color: color.to_string(),
            icon: icon.to_string(),
            created_at: now,
        })
    }

    pub async fn list_entity_types(&self) -> Result<Vec<EntityTypeRow>> {
        let rows = sqlx::query(
            "SELECT id, name, display_name, color, icon, created_at
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
                created_at: r.get("created_at"),
            })
            .collect())
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

    pub async fn create_ontology_object_with_lineage(
        &self,
        entity_type_id: &str,
        entity_type_name: &str,
        label: &str,
        fields_json: &str,
        dataset_id: &str,
        sync_run_id: &str,
    ) -> Result<OntologyObjectRow> {
        let id = Uuid::new_v4().to_string();
        let now = Self::now_str();
        sqlx::query(
            "INSERT INTO ontology_objects
             (id, entity_type_id, entity_type_name, label, fields, dataset_id, sync_run_id, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&id).bind(entity_type_id).bind(entity_type_name).bind(label).bind(fields_json)
        .bind(dataset_id).bind(sync_run_id).bind(&now).bind(&now)
        .execute(&self.pool)
        .await?;
        Ok(OntologyObjectRow {
            id, entity_type_id: entity_type_id.to_string(),
            entity_type_name: entity_type_name.to_string(),
            label: label.to_string(), fields: fields_json.to_string(),
            created_at: now.clone(), updated_at: now,
        })
    }

    pub async fn create_ontology_object(
        &self,
        entity_type_id: &str,
        entity_type_name: &str,
        label: &str,
        fields_json: &str,
    ) -> Result<OntologyObjectRow> {
        let id = Uuid::new_v4().to_string();
        let now = Self::now_str();
        sqlx::query(
            "INSERT INTO ontology_objects
             (id, entity_type_id, entity_type_name, label, fields, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(entity_type_id)
        .bind(entity_type_name)
        .bind(label)
        .bind(fields_json)
        .bind(&now)
        .bind(&now)
        .execute(&self.pool)
        .await?;
        Ok(OntologyObjectRow {
            id,
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
    ) -> Result<DataSourceRow> {
        let id = Uuid::new_v4().to_string();
        let now = Self::now_str();
        sqlx::query(
            "INSERT INTO data_sources (id, fold_id, name, source_type, config, status, created_at, group_id)
             VALUES (?, ?, ?, ?, ?, 'idle', ?, ?)",
        )
        .bind(&id).bind(fold_id).bind(name).bind(source_type).bind(config).bind(&now).bind(group_id)
        .execute(&self.pool)
        .await?;
        Ok(DataSourceRow { id, fold_id: fold_id.to_string(), name: name.to_string(),
            source_type: source_type.to_string(), config: config.to_string(),
            status: "idle".to_string(), write_lock: None, last_sync_at: None,
            record_count: None, created_at: now, deprecated: false, deleted_at: None,
            group_id: group_id.map(|s| s.to_string()) })
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
        &self, id: &str, name: &str, source_type: &str, config: &str,
    ) -> Result<()> {
        sqlx::query(
            "UPDATE data_sources SET name = ?, source_type = ?, config = ?, status = 'idle', write_lock = NULL WHERE id = ?",
        )
        .bind(name).bind(source_type).bind(config).bind(id).execute(&self.pool).await?;
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
             FROM ontology_objects WHERE dataset_id = ? ORDER BY created_at ASC LIMIT ? OFFSET ?",
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
            "SELECT COUNT(*) FROM ontology_objects WHERE dataset_id = ?",
        )
        .bind(dataset_id)
        .fetch_one(&self.pool)
        .await?;
        Ok(count)
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
