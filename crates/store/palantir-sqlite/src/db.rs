use anyhow::Result;
use sqlx::{Row, SqlitePool};
use uuid::Uuid;
use palantir_meta_store::{
    ProjectRow, ConnectorRow, EntityRow, BuildRow, RelRow,
    EntityTypeRow, EntityFieldRow, OntologyObjectRow, OntologyLinkRow,
    LinkTypeMappingInput, FoldRow, DataSourceRow, SyncRunRow, DatasetRow, DatasetVersionRow,
    BoundedContextRow, BcRelationshipRow, InterfaceRow, SchemaMigrationRow, BreakingChangeInfo,
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

        // ── P0: Schema migration history ──────────────────────────────────────
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS schema_migrations (
                id             TEXT PRIMARY KEY,
                et_id          TEXT NOT NULL REFERENCES entity_types(id) ON DELETE CASCADE,
                field_name     TEXT NOT NULL,
                change_type    TEXT NOT NULL,
                old_value      TEXT,
                new_value      TEXT,
                strategy       TEXT NOT NULL DEFAULT 'drop',
                affected_count INTEGER NOT NULL DEFAULT 0,
                applied_by     TEXT,
                applied_at     TEXT NOT NULL
            )",
        )
        .execute(&self.pool)
        .await?;

        // ── P1a: ET status lifecycle (idempotent) ──────────────────────────────
        let _ = sqlx::query(
            "ALTER TABLE entity_types ADD COLUMN status TEXT NOT NULL DEFAULT 'active'",
        )
        .execute(&self.pool)
        .await;

        // ddd_role_locked = true means the user explicitly set the role via UI;
        // false (default) means it was auto-inferred and can be overwritten.
        let _ = sqlx::query(
            "ALTER TABLE entity_types ADD COLUMN ddd_role_locked INTEGER NOT NULL DEFAULT 0",
        )
        .execute(&self.pool)
        .await;

        // source: 'manual' = user created, 'inferred' = derived from DataSource import
        let _ = sqlx::query(
            "ALTER TABLE entity_types ADD COLUMN source TEXT NOT NULL DEFAULT 'manual'",
        )
        .execute(&self.pool)
        .await;

        // ── P2c: System Interface tables ───────────────────────────────────────
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS interfaces (
                id          TEXT PRIMARY KEY,
                name        TEXT NOT NULL UNIQUE,
                description TEXT,
                is_builtin  INTEGER NOT NULL DEFAULT 0,
                created_at  TEXT NOT NULL
            )",
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS interface_fields (
                interface_id TEXT NOT NULL REFERENCES interfaces(id) ON DELETE CASCADE,
                field_name   TEXT NOT NULL,
                field_type   TEXT NOT NULL,
                required     INTEGER NOT NULL DEFAULT 1,
                description  TEXT,
                PRIMARY KEY (interface_id, field_name)
            )",
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS entity_type_interfaces (
                et_id        TEXT NOT NULL REFERENCES entity_types(id) ON DELETE CASCADE,
                interface_id TEXT NOT NULL REFERENCES interfaces(id) ON DELETE CASCADE,
                PRIMARY KEY (et_id, interface_id)
            )",
        )
        .execute(&self.pool)
        .await?;

        // ── 状态机声明层（Phase 2）───────────────────────────────────────────────
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS state_definitions (
                id           TEXT PRIMARY KEY,
                target_et_id TEXT NOT NULL REFERENCES entity_types(id) ON DELETE CASCADE,
                name         TEXT NOT NULL,
                display_name TEXT NOT NULL,
                color        TEXT NOT NULL DEFAULT '#6366f1',
                description  TEXT,
                is_initial   INTEGER NOT NULL DEFAULT 0,
                is_terminal  INTEGER NOT NULL DEFAULT 0,
                created_at   TEXT NOT NULL,
                UNIQUE(target_et_id, name)
            )",
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS state_transitions (
                id            TEXT PRIMARY KEY,
                target_et_id  TEXT NOT NULL REFERENCES entity_types(id) ON DELETE CASCADE,
                from_state_id TEXT NOT NULL REFERENCES state_definitions(id) ON DELETE CASCADE,
                to_state_id   TEXT NOT NULL REFERENCES state_definitions(id) ON DELETE CASCADE,
                created_at    TEXT NOT NULL,
                UNIQUE(target_et_id, from_state_id, to_state_id)
            )",
        )
        .execute(&self.pool)
        .await?;

        // ── ActionType 声明层（Phase 2）─────────────────────────────────────────
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS action_types (
                id               TEXT PRIMARY KEY,
                name             TEXT NOT NULL,
                display_name     TEXT NOT NULL,
                target_et_id     TEXT REFERENCES entity_types(id) ON DELETE CASCADE,
                level            TEXT NOT NULL DEFAULT 'object',
                from_states      TEXT NOT NULL DEFAULT '[]',
                to_state         TEXT,
                params           TEXT NOT NULL DEFAULT '[]',
                trigger          TEXT NOT NULL DEFAULT 'manual',
                allowed_personas TEXT NOT NULL DEFAULT '[]',
                bc_id            TEXT REFERENCES bounded_contexts(id) ON DELETE SET NULL,
                saga_def_id      TEXT,
                status           TEXT NOT NULL DEFAULT 'draft',
                created_at       TEXT NOT NULL
            )",
        )
        .execute(&self.pool)
        .await?;

        // ── SagaDefinition stub（Phase 3 执行引擎接入时填充）──────────────────
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS saga_definitions (
                id         TEXT PRIMARY KEY,
                name       TEXT NOT NULL,
                steps      TEXT NOT NULL DEFAULT '[]',
                created_at TEXT NOT NULL
            )",
        )
        .execute(&self.pool)
        .await?;

        // ── Phase 3: object 当前状态 ──────────────────────────────────────────
        sqlx::query(
            "ALTER TABLE ontology_objects ADD COLUMN current_state_id TEXT
             REFERENCES state_definitions(id) ON DELETE SET NULL",
        )
        .execute(&self.pool)
        .await
        .ok(); // ignore if column already exists

        // ── Phase 3: action 执行记录 ──────────────────────────────────────────
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS action_executions (
                id               TEXT PRIMARY KEY,
                action_type_id   TEXT NOT NULL REFERENCES action_types(id) ON DELETE CASCADE,
                object_id        TEXT NOT NULL REFERENCES ontology_objects(id) ON DELETE CASCADE,
                from_state_id    TEXT REFERENCES state_definitions(id) ON DELETE SET NULL,
                to_state_id      TEXT REFERENCES state_definitions(id) ON DELETE SET NULL,
                executor_persona TEXT,
                params           TEXT NOT NULL DEFAULT '{}',
                result           TEXT,
                status           TEXT NOT NULL DEFAULT 'ok',
                executed_at      TEXT NOT NULL
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

        // Seed built-in System Interfaces (idempotent via INSERT OR IGNORE)
        self.seed_builtin_interfaces().await?;

        Ok(())
    }

    async fn seed_builtin_interfaces(&self) -> Result<()> {
        let now = Self::now_iso();
        let builtins: &[(&str, &str, &str, &[(&str, &str, bool, &str)])] = &[
            ("iface_auditable", "Auditable", "记录创建/更新时间和操作者", &[
                ("created_at", "datetime", true, "创建时间"),
                ("updated_at", "datetime", true, "最后更新时间"),
                ("updated_by", "string",   false, "最后更新者"),
            ]),
            ("iface_identifiable", "Identifiable", "业务标识与展示名", &[
                ("id",          "string", true,  "业务标识"),
                ("name",        "string", true,  "展示名"),
                ("external_id", "string", false, "外部系统 ID"),
            ]),
            ("iface_versioned", "Versioned", "版本与有效期", &[
                ("version",    "integer",  true,  "版本号"),
                ("valid_from", "datetime", false, "有效开始时间"),
                ("valid_to",   "datetime", false, "有效结束时间"),
            ]),
            ("iface_locatable", "Locatable", "地理位置信息", &[
                ("latitude",  "float",  false, "纬度"),
                ("longitude", "float",  false, "经度"),
                ("address",   "string", false, "地址"),
            ]),
        ];

        for (id, name, desc, fields) in builtins {
            sqlx::query(
                "INSERT OR IGNORE INTO interfaces (id, name, description, is_builtin, created_at)
                 VALUES (?, ?, ?, 1, ?)",
            )
            .bind(id)
            .bind(name)
            .bind(desc)
            .bind(&now)
            .execute(&self.pool)
            .await?;

            for (fname, ftype, req, fdesc) in *fields {
                sqlx::query(
                    "INSERT OR IGNORE INTO interface_fields
                     (interface_id, field_name, field_type, required, description)
                     VALUES (?, ?, ?, ?, ?)",
                )
                .bind(id)
                .bind(fname)
                .bind(ftype)
                .bind(if *req { 1i64 } else { 0i64 })
                .bind(fdesc)
                .execute(&self.pool)
                .await?;
            }
        }
        Ok(())
    }

    fn now_str() -> String {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs().to_string())
            .unwrap_or_else(|_| "0".into())
    }

    fn now_iso() -> String {
        use std::time::{SystemTime, UNIX_EPOCH};
        let secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        // Format as ISO-8601 UTC using simple arithmetic (no chrono dep needed)
        let s = secs % 60;
        let m = (secs / 60) % 60;
        let h = (secs / 3600) % 24;
        let days = secs / 86400;
        // days since 1970-01-01
        let (y, mo, d) = Self::days_to_ymd(days);
        format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z", y, mo, d, h, m, s)
    }

    fn days_to_ymd(mut days: u64) -> (u64, u64, u64) {
        let mut y = 1970u64;
        loop {
            let leap = (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
            let ydays = if leap { 366 } else { 365 };
            if days < ydays { break; }
            days -= ydays;
            y += 1;
        }
        let leap = (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
        let mdays = [31u64, if leap { 29 } else { 28 }, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
        let mut mo = 1u64;
        for md in &mdays {
            if days < *md { break; }
            days -= md;
            mo += 1;
        }
        (y, mo, days + 1)
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
        source: &str, // 'manual' | 'inferred'
    ) -> Result<EntityTypeRow> {
        let id = Uuid::new_v4().to_string();
        let now = Self::now_str();
        sqlx::query(
            "INSERT INTO entity_types (id, name, display_name, color, icon, fold_id, ddd_role, namespace, source, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(name)
        .bind(display_name)
        .bind(color)
        .bind(icon)
        .bind(fold_id)
        .bind(ddd_role)
        .bind(namespace)
        .bind(source)
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
            bc_id: None,
            namespace: namespace.map(|s| s.to_string()),
            ddd_role: ddd_role.to_string(),
            status: "active".to_string(),
            created_at: now,
        })
    }

    pub async fn list_entity_types(&self) -> Result<Vec<EntityTypeRow>> {
        let rows = sqlx::query(
            // Show ET if: manually created OR has at least one active (non-deleted) DataSource
            "SELECT et.id, et.name, et.display_name, et.color, et.icon, et.fold_id, et.bc_id, et.namespace,
                    COALESCE(et.ddd_role, 'entity') as ddd_role,
                    COALESCE(et.status, 'active') as status, et.created_at
             FROM entity_types et
             WHERE et.source = 'manual'
                OR EXISTS (
                    SELECT 1 FROM object_type_mappings otm
                    JOIN datasets d ON d.id = otm.dataset_id
                    JOIN data_sources ds ON ds.id = d.source_id
                    WHERE otm.entity_type_id = et.id
                      AND ds.deleted_at IS NULL
                )
             ORDER BY et.created_at ASC",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(Self::row_to_et).collect())
    }

    /// List entity types belonging to a specific fold.
    pub async fn list_entity_types_for_fold(&self, fold_id: &str) -> Result<Vec<EntityTypeRow>> {
        let rows = sqlx::query(
            "SELECT et.id, et.name, et.display_name, et.color, et.icon, et.fold_id, et.bc_id, et.namespace,
                    COALESCE(et.ddd_role, 'entity') as ddd_role,
                    COALESCE(et.status, 'active') as status, et.created_at
             FROM entity_types et
             WHERE et.fold_id = ?
               AND (
                   et.source = 'manual'
                   OR EXISTS (
                       SELECT 1 FROM object_type_mappings otm
                       JOIN datasets d ON d.id = otm.dataset_id
                       JOIN data_sources ds ON ds.id = d.source_id
                       WHERE otm.entity_type_id = et.id
                         AND ds.deleted_at IS NULL
                   )
               )
             ORDER BY et.created_at ASC",
        )
        .bind(fold_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(Self::row_to_et).collect())
    }

    fn row_to_et(r: sqlx::sqlite::SqliteRow) -> EntityTypeRow {
        use sqlx::Row;
        EntityTypeRow {
            id: r.get("id"),
            name: r.get("name"),
            display_name: r.get("display_name"),
            color: r.get("color"),
            icon: r.get("icon"),
            fold_id: r.get("fold_id"),
            bc_id: r.get("bc_id"),
            namespace: r.get("namespace"),
            ddd_role: r.get("ddd_role"),
            status: r.get("status"),
            created_at: r.get("created_at"),
        }
    }

    pub async fn update_entity_type_ddd_role(&self, et_id: &str, ddd_role: &str) -> Result<()> {
        sqlx::query("UPDATE entity_types SET ddd_role = ?, ddd_role_locked = 1 WHERE id = ?")
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

    pub async fn update_entity_type_bc(&self, et_id: &str, bc_id: Option<&str>) -> Result<()> {
        sqlx::query("UPDATE entity_types SET bc_id = ? WHERE id = ?")
            .bind(bc_id)
            .bind(et_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Returns count of ontology_objects for this entity type.
    pub async fn count_objects_for_et(&self, et_id: &str) -> Result<i64> {
        let n: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM ontology_objects WHERE entity_type_id = ?",
        )
        .bind(et_id)
        .fetch_one(&self.pool)
        .await?;
        Ok(n)
    }

    /// P1a: set ET status. Returns count of datasets currently mapped to this ET.
    pub async fn set_entity_type_status(&self, et_id: &str, status: &str) -> Result<i64> {
        sqlx::query("UPDATE entity_types SET status = ? WHERE id = ?")
            .bind(status)
            .bind(et_id)
            .execute(&self.pool)
            .await?;
        // Count datasets that reference this ET
        let affected: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM object_type_mappings WHERE entity_type_id = ?",
        )
        .bind(et_id)
        .fetch_one(&self.pool)
        .await?;
        // Record in schema_migrations
        let now = Self::now_iso();
        let mid = Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO schema_migrations
             (id, et_id, field_name, change_type, old_value, new_value, strategy, affected_count, applied_at)
             VALUES (?, ?, '', 'status_change', NULL, ?, 'drop', ?, ?)",
        )
        .bind(&mid)
        .bind(et_id)
        .bind(status)
        .bind(affected)
        .bind(&now)
        .execute(&self.pool)
        .await?;
        Ok(affected)
    }

    // ── P0: Breaking change detection + migration ──────────────────────────

    /// Check if changing field type would be a breaking change.
    pub async fn check_field_type_change(&self, field_id: &str, new_type: &str) -> Result<BreakingChangeInfo> {
        use palantir_meta_store::BreakingChangeInfo;
        let row = sqlx::query(
            "SELECT ef.entity_type_id, ef.data_type, ef.name
             FROM entity_fields ef WHERE ef.id = ?",
        )
        .bind(field_id)
        .fetch_optional(&self.pool)
        .await?;

        let (et_id, old_type, _field_name) = match row {
            Some(r) => (r.get::<String,_>("entity_type_id"), r.get::<String,_>("data_type"), r.get::<String,_>("name")),
            None => return Err(anyhow::anyhow!("field not found")),
        };

        if old_type == new_type {
            return Ok(BreakingChangeInfo {
                breaking: false, affected_count: 0,
                change_type: "type_change".into(),
                old_value: Some(old_type), new_value: Some(new_type.to_string()),
                strategies: vec![],
            });
        }

        let affected: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM ontology_objects WHERE entity_type_id = ?",
        )
        .bind(&et_id)
        .fetch_one(&self.pool)
        .await?;

        // Determine available strategies
        let strategies = if Self::types_castable(&old_type, new_type) {
            vec!["drop".to_string(), "cast".to_string()]
        } else {
            vec!["drop".to_string()]
        };

        Ok(BreakingChangeInfo {
            breaking: affected > 0,
            affected_count: affected,
            change_type: "type_change".into(),
            old_value: Some(old_type),
            new_value: Some(new_type.to_string()),
            strategies,
        })
    }

    fn types_castable(from: &str, to: &str) -> bool {
        matches!((from, to),
            ("string", "integer") | ("string", "float") | ("string", "boolean") |
            ("integer", "float") | ("integer", "string") | ("float", "string") |
            ("boolean", "string") | ("boolean", "integer")
        )
    }

    /// Apply field type change: update schema + migrate existing object data.
    pub async fn apply_field_type_change(
        &self, field_id: &str, new_type: &str, strategy: &str,
    ) -> Result<SchemaMigrationRow> {
        use palantir_meta_store::SchemaMigrationRow;
        // Get field info
        let row = sqlx::query(
            "SELECT ef.id, ef.entity_type_id, ef.data_type, ef.name
             FROM entity_fields ef WHERE ef.id = ?",
        )
        .bind(field_id)
        .fetch_one(&self.pool)
        .await?;
        let et_id: String = row.get("entity_type_id");
        let old_type: String = row.get("data_type");
        let field_name: String = row.get("name");

        // 1. Update schema
        sqlx::query("UPDATE entity_fields SET data_type = ? WHERE id = ?")
            .bind(new_type)
            .bind(field_id)
            .execute(&self.pool)
            .await?;

        // 2. Migrate object data
        let affected = self.migrate_field_data(&et_id, &field_name, &old_type, new_type, strategy).await?;

        // 3. Record migration
        let mid = Uuid::new_v4().to_string();
        let now = Self::now_iso();
        sqlx::query(
            "INSERT INTO schema_migrations
             (id, et_id, field_name, change_type, old_value, new_value, strategy, affected_count, applied_at)
             VALUES (?, ?, ?, 'type_change', ?, ?, ?, ?, ?)",
        )
        .bind(&mid).bind(&et_id).bind(&field_name)
        .bind(&old_type).bind(new_type).bind(strategy)
        .bind(affected).bind(&now)
        .execute(&self.pool)
        .await?;

        Ok(SchemaMigrationRow {
            id: mid, et_id, field_name,
            change_type: "type_change".into(),
            old_value: Some(old_type), new_value: Some(new_type.to_string()),
            strategy: strategy.to_string(), affected_count: affected,
            applied_by: None, applied_at: now,
        })
    }

    /// Migrate ontology_objects.fields JSON for a changed/deleted field.
    async fn migrate_field_data(
        &self, et_id: &str, field_name: &str, old_type: &str, new_type: &str, strategy: &str,
    ) -> Result<i64> {
        let objects = sqlx::query(
            "SELECT id, fields FROM ontology_objects WHERE entity_type_id = ?",
        )
        .bind(et_id)
        .fetch_all(&self.pool)
        .await?;

        let mut count = 0i64;
        for obj in objects {
            let obj_id: String = obj.get("id");
            let fields_str: String = obj.get("fields");
            let mut fields: serde_json::Map<String, serde_json::Value> =
                serde_json::from_str(&fields_str).unwrap_or_default();

            if fields.contains_key(field_name) {
                match strategy {
                    "drop" => { fields.remove(field_name); }
                    "cast" => {
                        let val = fields.get(field_name).cloned().unwrap_or(serde_json::Value::Null);
                        let casted = Self::cast_value(val, new_type);
                        fields.insert(field_name.to_string(), casted);
                    }
                    _ => { fields.remove(field_name); }
                }
                let new_fields = serde_json::to_string(&fields).unwrap_or_else(|_| "{}".into());
                sqlx::query("UPDATE ontology_objects SET fields = ? WHERE id = ?")
                    .bind(&new_fields).bind(&obj_id)
                    .execute(&self.pool).await?;
                count += 1;
            }
        }
        Ok(count)
    }

    fn cast_value(val: serde_json::Value, target_type: &str) -> serde_json::Value {
        use serde_json::Value;
        match target_type {
            "integer" => match &val {
                Value::Number(n) => Value::Number(serde_json::Number::from(n.as_i64().unwrap_or(0))),
                Value::String(s) => s.parse::<i64>()
                    .map(|n| Value::Number(n.into()))
                    .unwrap_or(Value::Null),
                Value::Bool(b) => Value::Number((*b as i64).into()),
                _ => Value::Null,
            },
            "float" => match &val {
                Value::Number(n) => val.clone(),
                Value::String(s) => s.parse::<f64>()
                    .ok()
                    .and_then(|f| serde_json::Number::from_f64(f))
                    .map(Value::Number)
                    .unwrap_or(Value::Null),
                _ => Value::Null,
            },
            "string" => match &val {
                Value::Null => Value::Null,
                other => Value::String(other.to_string()),
            },
            "boolean" => match &val {
                Value::Bool(_) => val.clone(),
                Value::String(s) => Value::Bool(s == "true" || s == "1"),
                Value::Number(n) => Value::Bool(n.as_i64().unwrap_or(0) != 0),
                _ => Value::Null,
            },
            _ => val,
        }
    }

    /// Check if deleting a field is breaking.
    pub async fn check_field_delete(&self, field_id: &str) -> Result<BreakingChangeInfo> {
        use palantir_meta_store::BreakingChangeInfo;
        let row = sqlx::query(
            "SELECT entity_type_id, name FROM entity_fields WHERE id = ?",
        )
        .bind(field_id)
        .fetch_optional(&self.pool)
        .await?;

        let (et_id, field_name) = match row {
            Some(r) => (r.get::<String,_>("entity_type_id"), r.get::<String,_>("name")),
            None => return Err(anyhow::anyhow!("field not found")),
        };

        let affected: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM ontology_objects WHERE entity_type_id = ?",
        )
        .bind(&et_id)
        .fetch_one(&self.pool)
        .await?;

        Ok(BreakingChangeInfo {
            breaking: affected > 0,
            affected_count: affected,
            change_type: "delete".into(),
            old_value: Some(field_name),
            new_value: None,
            strategies: vec!["drop".to_string()],
        })
    }

    /// Apply field deletion: remove from schema + drop field data from objects.
    pub async fn apply_field_delete(&self, field_id: &str) -> Result<SchemaMigrationRow> {
        use palantir_meta_store::SchemaMigrationRow;
        let row = sqlx::query(
            "SELECT entity_type_id, name, data_type FROM entity_fields WHERE id = ?",
        )
        .bind(field_id)
        .fetch_one(&self.pool)
        .await?;
        let et_id: String = row.get("entity_type_id");
        let field_name: String = row.get("name");
        let old_type: String = row.get("data_type");

        // Delete field from schema
        sqlx::query("DELETE FROM entity_fields WHERE id = ?")
            .bind(field_id)
            .execute(&self.pool)
            .await?;

        // Drop field from all object JSON
        let affected = self.migrate_field_data(&et_id, &field_name, &old_type, "", "drop").await?;

        let mid = Uuid::new_v4().to_string();
        let now = Self::now_iso();
        sqlx::query(
            "INSERT INTO schema_migrations
             (id, et_id, field_name, change_type, old_value, new_value, strategy, affected_count, applied_at)
             VALUES (?, ?, ?, 'delete', ?, NULL, 'drop', ?, ?)",
        )
        .bind(&mid).bind(&et_id).bind(&field_name)
        .bind(&old_type).bind(affected).bind(&now)
        .execute(&self.pool)
        .await?;

        Ok(SchemaMigrationRow {
            id: mid, et_id, field_name,
            change_type: "delete".into(),
            old_value: Some(old_type), new_value: None,
            strategy: "drop".to_string(), affected_count: affected,
            applied_by: None, applied_at: now,
        })
    }

    // ── P1b: Data lineage ──────────────────────────────────────────────────

    pub async fn get_et_lineage(&self, et_id: &str) -> Result<serde_json::Value> {
        let rows = sqlx::query(
            "SELECT
               d.id          AS dataset_id,
               d.name        AS dataset_name,
               ds.id         AS source_id,
               ds.name       AS source_name,
               ds.source_type,
               ds.fold_id,
               f.name        AS fold_name,
               otm.primary_key_col,
               otm.sync_mode,
               ds.last_sync_at,
               COALESCE(dv.total_rows, 0) AS record_count
             FROM object_type_mappings otm
             JOIN datasets d      ON d.id = otm.dataset_id
             JOIN data_sources ds ON ds.id = d.source_id
             LEFT JOIN folds f    ON f.id = ds.fold_id
             LEFT JOIN dataset_versions dv ON dv.dataset_id = d.id AND dv.is_current = 1
             WHERE otm.entity_type_id = ?
             ORDER BY ds.last_sync_at DESC",
        )
        .bind(et_id)
        .fetch_all(&self.pool)
        .await?;

        let total_records: i64 = rows.iter()
            .map(|r| r.try_get::<i64,_>("record_count").unwrap_or(0))
            .sum();

        let sources: Vec<serde_json::Value> = rows.into_iter().map(|r| {
            serde_json::json!({
                "dataset_id":     r.get::<String,_>("dataset_id"),
                "dataset_name":   r.get::<String,_>("dataset_name"),
                "source_id":      r.get::<String,_>("source_id"),
                "source_name":    r.get::<String,_>("source_name"),
                "source_type":    r.get::<String,_>("source_type"),
                "fold_id":        r.get::<String,_>("fold_id"),
                "fold_name":      r.get::<String,_>("fold_name"),
                "primary_key_col":r.get::<String,_>("primary_key_col"),
                "sync_mode":      r.get::<String,_>("sync_mode"),
                "last_synced_at": r.get::<Option<String>,_>("last_sync_at"),
                "record_count":   r.get::<i64,_>("record_count"),
            })
        }).collect();

        Ok(serde_json::json!({
            "entity_type_id": et_id,
            "sources": sources,
            "total_records": total_records,
        }))
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
            current_state_id: None,
            current_state_name: None,
            current_state_display: None,
            current_state_color: None,
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
            current_state_id: None,
            current_state_name: None,
            current_state_display: None,
            current_state_color: None,
        })
    }

    pub async fn list_ontology_objects(
        &self,
        entity_type_id: Option<&str>,
    ) -> Result<Vec<OntologyObjectRow>> {
        // Show object if: manually created (dataset_id IS NULL)
        // OR its dataset's source is not deleted
        let sql = "SELECT o.id, o.entity_type_id, o.entity_type_name, o.label, o.fields,
                          o.created_at, o.updated_at, o.current_state_id,
                          s.name AS state_name, s.display_name AS state_display, s.color AS state_color
                   FROM ontology_objects o
                   LEFT JOIN state_definitions s ON s.id = o.current_state_id
                   LEFT JOIN datasets d ON d.id = o.dataset_id
                   LEFT JOIN data_sources ds ON ds.id = d.source_id
                   WHERE (o.dataset_id IS NULL OR ds.deleted_at IS NULL)";
        let rows = if let Some(et) = entity_type_id {
            sqlx::query(&format!("{sql} AND o.entity_type_id = ? ORDER BY o.created_at DESC"))
                .bind(et)
                .fetch_all(&self.pool)
                .await?
        } else {
            sqlx::query(&format!("{sql} ORDER BY o.created_at DESC"))
                .fetch_all(&self.pool)
                .await?
        };
        Ok(rows
            .into_iter()
            .map(|r| OntologyObjectRow {
                id:                  r.get("id"),
                entity_type_id:      r.get("entity_type_id"),
                entity_type_name:    r.get("entity_type_name"),
                label:               r.get("label"),
                fields:              r.get("fields"),
                created_at:          r.get("created_at"),
                updated_at:          r.get("updated_at"),
                current_state_id:      r.get("current_state_id"),
                current_state_name:    r.get("state_name"),
                current_state_display: r.get("state_display"),
                current_state_color:   r.get("state_color"),
            })
            .collect())
    }

    pub async fn get_ontology_object(&self, id: &str) -> Result<Option<OntologyObjectRow>> {
        let row = sqlx::query(
            "SELECT o.id, o.entity_type_id, o.entity_type_name, o.label, o.fields,
                    o.created_at, o.updated_at, o.current_state_id,
                    s.name AS state_name, s.display_name AS state_display, s.color AS state_color
             FROM ontology_objects o
             LEFT JOIN state_definitions s ON s.id = o.current_state_id
             WHERE o.id = ?",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|r| OntologyObjectRow {
            id:                  r.get("id"),
            entity_type_id:      r.get("entity_type_id"),
            entity_type_name:    r.get("entity_type_name"),
            label:               r.get("label"),
            fields:              r.get("fields"),
            created_at:          r.get("created_at"),
            updated_at:          r.get("updated_at"),
            current_state_id:      r.get("current_state_id"),
            current_state_name:    r.get("state_name"),
            current_state_display: r.get("state_display"),
            current_state_color:   r.get("state_color"),
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
        // Build set of visible object IDs to filter edges
        let visible_ids: std::collections::HashSet<&str> =
            objects.iter().map(|o| o.id.as_str()).collect();
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
        // Only keep edges where both endpoints are visible
        .filter(|l| visible_ids.contains(l.from_id.as_str()) && visible_ids.contains(l.to_id.as_str()))
        .collect();
        Ok((objects, links))
    }

    // ── Folds ─────────────────────────────────────────────────────────────────

    pub async fn create_fold(
        &self,
        project_id: &str,
        name: &str,
        description: Option<&str>,
        fold_type: Option<&str>,
    ) -> Result<FoldRow> {
        let id = Uuid::new_v4().to_string();
        let now = Self::now_str();
        let ft = fold_type.unwrap_or("normal");
        sqlx::query(
            "INSERT INTO folds (id, project_id, name, description, fold_type, created_at)
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(project_id)
        .bind(name)
        .bind(description)
        .bind(ft)
        .bind(&now)
        .execute(&self.pool)
        .await?;
        Ok(FoldRow {
            id, project_id: project_id.to_string(), name: name.to_string(),
            description: description.map(|s| s.to_string()),
            fold_type: ft.to_string(), created_at: now,
        })
    }

    pub async fn list_folds(&self, project_id: &str) -> Result<Vec<FoldRow>> {
        let rows = sqlx::query(
            "SELECT id, project_id, name, description,
                    COALESCE(fold_type, 'normal') as fold_type, created_at
             FROM folds WHERE project_id = ? ORDER BY created_at ASC",
        )
        .bind(project_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(|r| FoldRow {
            id: r.get("id"), project_id: r.get("project_id"),
            name: r.get("name"), description: r.get("description"),
            fold_type: r.get("fold_type"), created_at: r.get("created_at"),
        }).collect())
    }

    pub async fn list_shared_kernel_folds(&self) -> Result<Vec<serde_json::Value>> {
        let rows = sqlx::query(
            "SELECT f.id, f.project_id, f.name, f.description, f.created_at,
                    COUNT(et.id) as et_count
             FROM folds f
             LEFT JOIN entity_types et ON et.fold_id = f.id
             WHERE f.fold_type = 'shared_kernel'
             GROUP BY f.id ORDER BY f.created_at ASC",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(|r| {
            serde_json::json!({
                "id": r.get::<String,_>("id"),
                "project_id": r.get::<String,_>("project_id"),
                "name": r.get::<String,_>("name"),
                "description": r.get::<Option<String>,_>("description"),
                "fold_type": "shared_kernel",
                "et_count": r.get::<i64,_>("et_count"),
                "created_at": r.get::<String,_>("created_at"),
            })
        }).collect())
    }

    pub async fn get_fold(&self, id: &str) -> Result<Option<FoldRow>> {
        let row = sqlx::query(
            "SELECT id, project_id, name, description,
                    COALESCE(fold_type, 'normal') as fold_type, created_at
             FROM folds WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|r| FoldRow {
            id: r.get("id"), project_id: r.get("project_id"),
            name: r.get("name"), description: r.get("description"),
            fold_type: r.get("fold_type"), created_at: r.get("created_at"),
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
            current_state_id: None,
            current_state_name: None,
            current_state_display: None,
            current_state_color: None,
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

    /// Insert ontology object only if external_id doesn't exist yet (append semantics).
    pub async fn insert_ontology_object_if_new(
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
        sqlx::query(
            "INSERT OR IGNORE INTO ontology_objects
                (id, entity_type_id, entity_type_name, external_id, label, fields,
                 dataset_id, sync_run_id, source_ids, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, json_array(?), ?, ?)",
        )
        .bind(&id)
        .bind(entity_type_id)
        .bind(entity_type_name)
        .bind(external_id)
        .bind(label)
        .bind(fields)
        .bind(dataset_id)
        .bind(run_id)
        .bind(dataset_id)
        .bind(&now)
        .bind(&now)
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

        // Load ET ddd_roles to apply AR direction reversal (same logic as auto_detect_links)
        let et_roles: std::collections::HashMap<String, String> = sqlx::query(
            "SELECT id, COALESCE(ddd_role, 'entity') as ddd_role FROM entity_types",
        )
        .fetch_all(&self.pool)
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|r| (r.get::<String, _>("id"), r.get::<String, _>("ddd_role")))
        .collect();

        // Look up the source entity type for this dataset via object_type_mappings
        let src_et: String = sqlx::query_scalar(
            "SELECT entity_type_id FROM object_type_mappings WHERE dataset_id = ? LIMIT 1",
        )
        .bind(dataset_id)
        .fetch_optional(&self.pool)
        .await
        .ok()
        .flatten()
        .unwrap_or_default();
        let src_et_role = et_roles.get(&src_et).map(|s| s.as_str()).unwrap_or("entity");

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

            // Rank-based: ar_candidate (rank=2) > aggregate_root (rank=1) > entity (rank=0).
            // Reverse when target outranks source so higher-rank entity is always the source (owner).
            // Exception: REFS_TO is a cross-BC reference — always stored as source→target (no reversal),
            // because the entity that holds the FK is the one that references the other BC.
            let to_et_role = et_roles.get(to_et).map(|s| s.as_str()).unwrap_or("entity");
            let role_rank = |r: &str| match r { "ar_candidate" => 2_i32, "aggregate_root" => 1, _ => 0 };
            let is_refs_to = matches!(rel.to_lowercase().as_str(), "refs_to" | "ref_to");
            let reverse = !is_refs_to && role_rank(to_et_role) > role_rank(src_et_role);

            for src in &src_rows {
                let src_id: String = src.get("id");
                let fields_str: String = src.get("fields");
                let fields: serde_json::Value = serde_json::from_str(&fields_str).unwrap_or_default();
                let fk_val = match fields.get(fk_col).and_then(|v| v.as_str()) {
                    Some(v) => v.to_string(),
                    None => continue,
                };

                // Look up target object: try external_id first, then fallback to json id field
                let tgt: Option<String> = sqlx::query_scalar(
                    "SELECT id FROM ontology_objects
                     WHERE entity_type_id = ?
                       AND (external_id = ?
                            OR json_extract(fields, '$.id') = ?
                            OR json_extract(fields, '$.\"id\"') = ?)
                       AND sync_run_id IN ('promote', 'auto-promote')
                     LIMIT 1",
                )
                .bind(to_et)
                .bind(&fk_val)
                .bind(&fk_val)
                .bind(&fk_val)
                .fetch_optional(&self.pool)
                .await?;

                if let Some(tgt_id) = tgt {
                    // AR → child when target is AR; otherwise normal FK direction
                    let (from_id, to_id) = if reverse {
                        (tgt_id.clone(), src_id.clone())
                    } else {
                        (src_id.clone(), tgt_id.clone())
                    };
                    let link_id = Uuid::new_v4().to_string();
                    let _ = sqlx::query(
                        "INSERT OR IGNORE INTO ontology_links
                         (id, from_id, to_id, rel_type, created_at)
                         VALUES (?, ?, ?, ?, ?)",
                    )
                    .bind(&link_id)
                    .bind(&from_id)
                    .bind(&to_id)
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

    /// Re-resolve links for every dataset that has link_type_mappings saved.
    pub async fn resolve_all_links(&self) -> Result<usize> {
        let dataset_ids: Vec<String> = sqlx::query_scalar(
            "SELECT DISTINCT dataset_id FROM link_type_mappings",
        )
        .fetch_all(&self.pool)
        .await
        .unwrap_or_default();

        let mut total = 0usize;
        for ds_id in dataset_ids {
            total += self.resolve_links_for_dataset(&ds_id).await.unwrap_or(0);
        }
        Ok(total)
    }

    /// Auto-detect FK relationships by scanning `*_id` fields on all promoted objects.
    /// Infer DDD roles from FK out-degree and persist to entity_types table.
    /// AR = ET whose objects carry the most *distinct* FK references to OTHER ETs
    /// (out-degree). This is always run before building links so edge direction is correct.
    /// Only updates ETs whose ddd_role is currently 'entity' (the default); explicit
    /// user overrides (aggregate_root / value_object set via UI) are never touched.
    pub async fn infer_and_persist_ddd_roles(&self) -> Result<()> {
        // Load all promoted objects
        let objs = sqlx::query(
            "SELECT id, entity_type_id, fields FROM ontology_objects
             WHERE sync_run_id IN ('promote', 'auto-promote') AND fields IS NOT NULL",
        )
        .fetch_all(&self.pool)
        .await?;

        // Build raw field id → entity_type_id index
        let mut raw_id_to_et: std::collections::HashMap<String, String> = std::collections::HashMap::new();
        for obj in &objs {
            let fields_str: String = obj.get("fields");
            let et_id: String = obj.get("entity_type_id");
            if let Ok(fields) = serde_json::from_str::<serde_json::Value>(&fields_str) {
                if let Some(raw_id) = fields.get("id").and_then(|v| v.as_str()) {
                    raw_id_to_et.insert(raw_id.to_string(), et_id);
                }
            }
        }

        // Count distinct ET types each ET's objects FK-reference (out-degree)
        // e.g. Order.customer_id → Customer, Order.address_id → Address → Order out-degree = 2
        let mut et_refs_out: std::collections::HashMap<String, std::collections::HashSet<String>> =
            std::collections::HashMap::new();
        for obj in &objs {
            let src_et: String = obj.get("entity_type_id");
            let fields_str: String = obj.get("fields");
            if let Ok(fields) = serde_json::from_str::<serde_json::Value>(&fields_str) {
                if let Some(fmap) = fields.as_object() {
                    for (col, val) in fmap {
                        if !col.ends_with("_id") || col == "id" { continue }
                        if let Some(fk_val) = val.as_str().filter(|s| !s.is_empty()) {
                            if let Some(tgt_et) = raw_id_to_et.get(fk_val) {
                                if tgt_et != &src_et {
                                    et_refs_out.entry(src_et.clone()).or_default().insert(tgt_et.clone());
                                }
                            }
                        }
                    }
                }
            }
        }

        // Build in-degree: count distinct ETs that FK-reference each ET
        let mut et_refs_in: std::collections::HashMap<String, std::collections::HashSet<String>> =
            std::collections::HashMap::new();
        for obj in &objs {
            let src_et: String = obj.get("entity_type_id");
            let fields_str: String = obj.get("fields");
            if let Ok(fields) = serde_json::from_str::<serde_json::Value>(&fields_str) {
                if let Some(fmap) = fields.as_object() {
                    for (col, val) in fmap {
                        if !col.ends_with("_id") || col == "id" { continue }
                        if let Some(fk_val) = val.as_str().filter(|s| !s.is_empty()) {
                            if let Some(tgt_et) = raw_id_to_et.get(fk_val) {
                                if tgt_et != &src_et {
                                    et_refs_in.entry(tgt_et.clone()).or_default().insert(src_et.clone());
                                }
                            }
                        }
                    }
                }
            }
        }

        // Load current ddd_role values
        let et_rows = sqlx::query(
            "SELECT id, display_name, ddd_role, COALESCE(ddd_role_locked,0) as locked FROM entity_types"
        )
        .fetch_all(&self.pool).await?;

        // Score = in_deg + out_deg.
        // AR = nodes with the highest combined connectivity (referenced by many AND/OR reference many).
        // This correctly handles both patterns:
        //   "parent has children" (children carry FK → parent has high in_deg)
        //   "AR coordinates references" (AR carries FKs → AR has high out_deg)
        let scores: Vec<(String, usize)> = et_rows.iter()
            .filter(|r| { let id: String = r.get("id"); id != "default" })
            .map(|r| {
                let id: String = r.get("id");
                let out = et_refs_out.get(&id).map(|s| s.len()).unwrap_or(0);
                let inn = et_refs_in.get(&id).map(|s| s.len()).unwrap_or(0);
                (id, out + inn)
            })
            .collect();
        let max_score = scores.iter().map(|(_, s)| *s).max().unwrap_or(0);
        let avg_score = if scores.is_empty() { 0.0 } else {
            scores.iter().map(|(_, s)| *s as f64).sum::<f64>() / scores.len() as f64
        };
        // Adaptive threshold: top 50% above average, min 2 (avoid marking everything AR)
        let ar_threshold = if max_score > 0 {
            ((avg_score + (max_score as f64 - avg_score) * 0.5).ceil() as usize).max(2)
        } else {
            usize::MAX
        };
        let score_map: std::collections::HashMap<String, usize> = scores.into_iter().collect();

        // Pass 1: mark ARs and collect the AR set for pass 2
        let mut ar_set: std::collections::HashSet<String> = std::collections::HashSet::new();
        for r in &et_rows {
            let id: String = r.get("id");
            let current_role: String = r.get("ddd_role");

            // Never overwrite roles the user explicitly locked via UI
            let locked: i64 = r.try_get("locked").unwrap_or(0);
            if locked != 0 {
                // Still track user-confirmed ARs for Pass 2
                if current_role == "aggregate_root" { ar_set.insert(id); }
                continue
            }

            let score = score_map.get(&id).copied().unwrap_or(0);
            let inferred = if score >= ar_threshold { "aggregate_root" } else { "entity" };

            if inferred == "aggregate_root" { ar_set.insert(id.clone()); }

            // Skip if no change needed
            if inferred == current_role.as_str() { continue }

            sqlx::query("UPDATE entity_types SET ddd_role = ? WHERE id = ?")
                .bind(inferred)
                .bind(&id)
                .execute(&self.pool)
                .await?;
        }

        // Pass 2: ar_candidate = referenced by an AR AND has no outgoing FKs
        // These are "possible external BC roots" that topology alone can't confirm.
        // The graph renders them with a "? AR" hint; user right-clicks to confirm.
        for r in &et_rows {
            let id: String = r.get("id");
            let locked: i64 = r.try_get("locked").unwrap_or(0);
            if locked != 0 { continue }
            if ar_set.contains(&id) { continue } // already AR

            let out_deg = et_refs_out.get(&id).map(|s| s.len()).unwrap_or(0);
            let referenced_by_ar = et_refs_in.get(&id)
                .map(|refs| refs.iter().any(|r| ar_set.contains(r)))
                .unwrap_or(false);

            let inferred = if out_deg == 0 && referenced_by_ar { "ar_candidate" } else { "entity" };

            let current_role: String = r.get("ddd_role");
            if inferred == current_role.as_str() { continue }

            sqlx::query("UPDATE entity_types SET ddd_role = ? WHERE id = ?")
                .bind(inferred)
                .bind(&id)
                .execute(&self.pool)
                .await?;
        }

        Ok(())
    }

    /// For each `foo_id` field, tries to find a promoted object whose `external_id`
    /// or `fields.id` matches the value, then creates an ontology_link.
    /// When the FK target is an Aggregate Root, the direction is reversed (AR → child)
    /// so the AR is always the source. Re-runs clear all previously auto-detected links first.
    /// Returns (created, skipped) counts.
    pub async fn auto_detect_links(&self) -> Result<(usize, usize)> {
        let now = Self::now_str();

        // Clear all previously auto-detected links before re-running
        // Covers both HAS_* (auto_detect) and 'HAS' (resolve_links default rel_type)
        // Clear all auto-detected AND resolve_links 'has' entries so direction is rebuilt cleanly.
        // User-configured refs_to / belongs_to / similar_to are kept.
        sqlx::query("DELETE FROM ontology_links WHERE rel_type LIKE 'HAS%' OR rel_type = 'has'")
            .execute(&self.pool).await?;

        // Pass 0: infer and persist DDD roles from FK out-degree (AR = most outgoing refs)
        let _ = self.infer_and_persist_ddd_roles().await;

        // Load effective ddd_roles (now includes inferred values persisted above)
        let et_rows = sqlx::query(
            "SELECT id, ddd_role, COALESCE(display_name, name, id) as et_name FROM entity_types",
        )
        .fetch_all(&self.pool)
        .await?;
        let effective_roles: std::collections::HashMap<String, String> = et_rows
            .iter()
            .map(|r| (r.get("id"), r.get("ddd_role")))
            .collect();
        // id → display name (used for rel_type label when direction is reversed)
        let et_names: std::collections::HashMap<String, String> = et_rows
            .iter()
            .map(|r| (r.get::<String, _>("id"), r.get::<String, _>("et_name")))
            .collect();

        // User-defined non-HAS mappings: (src_et_id, fk_col, to_et_id) that auto-detect must skip.
        // These are already resolved by resolve_links_for_dataset; auto-detect must not overwrite.
        let user_override_keys: std::collections::HashSet<(String, String, String)> = sqlx::query(
            "SELECT otm.entity_type_id as src_et, ltm.from_fk_col, ltm.to_entity_type_id
             FROM link_type_mappings ltm
             JOIN object_type_mappings otm ON ltm.dataset_id = otm.dataset_id
             WHERE ltm.rel_type NOT IN ('HAS', 'has')",
        )
        .fetch_all(&self.pool)
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|r| (r.get("src_et"), r.get("from_fk_col"), r.get("to_entity_type_id")))
        .collect();

        // All promoted objects with their fields
        let objs = sqlx::query(
            "SELECT id, entity_type_id, fields FROM ontology_objects
             WHERE sync_run_id IN ('promote', 'auto-promote') AND fields IS NOT NULL",
        )
        .fetch_all(&self.pool)
        .await?;

        // Build lookup: raw id field value → (object_id, entity_type_id)
        let mut id_index: std::collections::HashMap<String, (String, String)> = std::collections::HashMap::new();
        for obj in &objs {
            let obj_id: String = obj.get("id");
            let et_id: String  = obj.get("entity_type_id");
            let fields_str: String = obj.get("fields");
            let fields: serde_json::Value = serde_json::from_str(&fields_str).unwrap_or_default();
            if let Some(raw_id) = fields.get("id").and_then(|v| v.as_str()) {
                id_index.insert(raw_id.to_string(), (obj_id.clone(), et_id.clone()));
            }
        }

        // Effective role lookup using persisted values
        let effective_role = |et_id: &str| -> &str {
            effective_roles.get(et_id).map(|s| s.as_str()).unwrap_or("entity")
        };

        let mut created = 0usize;
        let mut skipped = 0usize;

        // ── Pass 2: Create links using effective roles for direction ──────────
        for obj in &objs {
            let src_id: String = obj.get("id");
            let src_et: String = obj.get("entity_type_id");
            let fields_str: String = obj.get("fields");
            let fields: serde_json::Value = serde_json::from_str(&fields_str).unwrap_or_default();

            let Some(fields_map) = fields.as_object() else { continue };

            for (col, val) in fields_map {
                if !col.ends_with("_id") || col == "id" { continue }
                let Some(fk_val) = val.as_str().filter(|s| !s.is_empty()) else { continue };

                let Some((tgt_id, tgt_et)) = id_index.get(fk_val) else {
                    skipped += 1;
                    continue
                };
                if tgt_id == &src_id { continue }

                let base = col.trim_end_matches("_id").to_uppercase();

                // Skip FK columns that have a user-defined non-HAS mapping (REFS_TO / BELONGS_TO).
                // Those links are already resolved by resolve_links_for_dataset.
                if user_override_keys.contains(&(src_et.clone(), col.clone(), tgt_et.clone())) {
                    skipped += 1;
                    continue;
                }

                // Rank-based direction: higher-rank entity is always the source (owner).
                // ar_candidate = out_degree=0, referenced by AR → terminal root (highest rank)
                // aggregate_root = most connected node → intermediate
                // entity / VO → leaf
                // Reverse when rank(target) > rank(source).
                let tgt_role = effective_role(tgt_et);
                let src_role = effective_role(&src_et);
                let role_rank = |r: &str| match r {
                    "ar_candidate"   => 2_i32,
                    "aggregate_root" => 1,
                    _                => 0,
                };
                // When direction is reversed (AR owns the source entity), the rel_type label
                // should reflect what the AR *has* — the source entity type, not the FK column base.
                // e.g. Address.order_id → reversed to Order→Address: label = HAS_ADDRESS, not HAS_ORDER
                let (from_id, to_id, rel_type) = if role_rank(tgt_role) > role_rank(src_role) {
                    let src_name = et_names.get(&src_et)
                        .map(|s| s.to_uppercase().replace(' ', "_"))
                        .unwrap_or_else(|| base.clone());
                    (tgt_id.clone(), src_id.clone(), format!("HAS_{}", src_name))
                } else {
                    (src_id.clone(), tgt_id.clone(), format!("HAS_{}", base))
                };

                let link_id = Uuid::new_v4().to_string();
                let result = sqlx::query(
                    "INSERT OR IGNORE INTO ontology_links (id, from_id, to_id, rel_type, created_at)
                     VALUES (?, ?, ?, ?, ?)",
                )
                .bind(&link_id)
                .bind(&from_id)
                .bind(&to_id)
                .bind(&rel_type)
                .bind(&now)
                .execute(&self.pool)
                .await;

                if result.map(|r| r.rows_affected()).unwrap_or(0) > 0 {
                    created += 1;
                }
            }
        }

        Ok((created, skipped))
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

    // ── Context Map ────────────────────────────────────────────────────────────

    /// Returns all BC nodes + relationships for a project (for Context Map visualization).
    pub async fn get_context_map(&self, project_id: &str) -> Result<serde_json::Value> {
        // All BCs in the project's folds
        let bc_rows = sqlx::query(
            "SELECT bc.id, bc.fold_id, bc.name, bc.color, bc.auto_detected, bc.created_at,
                    f.name AS fold_name, f.fold_type,
                    COUNT(et.id) AS et_count
             FROM bounded_contexts bc
             JOIN folds f ON f.id = bc.fold_id
             LEFT JOIN entity_types et ON et.bc_id = bc.id
             WHERE f.project_id = ?
             GROUP BY bc.id ORDER BY bc.created_at",
        )
        .bind(project_id)
        .fetch_all(&self.pool)
        .await?;

        let nodes: Vec<serde_json::Value> = bc_rows.into_iter().map(|r| serde_json::json!({
            "id":            r.get::<String,_>("id"),
            "fold_id":       r.get::<String,_>("fold_id"),
            "fold_name":     r.get::<String,_>("fold_name"),
            "fold_type":     r.get::<String,_>("fold_type"),
            "name":          r.get::<String,_>("name"),
            "color":         r.get::<String,_>("color"),
            "auto_detected": r.get::<i64,_>("auto_detected") != 0,
            "et_count":      r.get::<i64,_>("et_count"),
        })).collect();

        // All BC relationships within the project
        let rel_rows = sqlx::query(
            "SELECT r.id, r.from_bc_id, r.to_bc_id, r.relationship_type, r.notes,
                    fb.name AS from_name, tb.name AS to_name
             FROM bc_relationships r
             JOIN bounded_contexts fb ON fb.id = r.from_bc_id
             JOIN bounded_contexts tb ON tb.id = r.to_bc_id
             JOIN folds ff ON ff.id = fb.fold_id
             WHERE ff.project_id = ?
             ORDER BY r.created_at",
        )
        .bind(project_id)
        .fetch_all(&self.pool)
        .await?;

        let edges: Vec<serde_json::Value> = rel_rows.into_iter().map(|r| serde_json::json!({
            "id":                r.get::<String,_>("id"),
            "from_bc_id":        r.get::<String,_>("from_bc_id"),
            "from_name":         r.get::<String,_>("from_name"),
            "to_bc_id":          r.get::<String,_>("to_bc_id"),
            "to_name":           r.get::<String,_>("to_name"),
            "relationship_type": r.get::<String,_>("relationship_type"),
            "notes":             r.get::<Option<String>,_>("notes"),
        })).collect();

        // Fallback: if no BCs exist, expose folds as BC nodes so Context Map is never empty
        if nodes.is_empty() {
            // Count all ETs for this project — including those without fold_id assignment
            let fold_rows = sqlx::query(
                "SELECT f.id, f.name, f.fold_type FROM folds f
                 WHERE f.project_id = ? ORDER BY f.created_at",
            )
            .bind(project_id)
            .fetch_all(&self.pool)
            .await?;

            // Total ET count for the project (all ETs regardless of fold assignment)
            let total_ets: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM entity_types WHERE id != 'default'",
            )
            .fetch_one(&self.pool)
            .await
            .unwrap_or(0);

            let palette = [
                "#6366f1","#8b5cf6","#ec4899","#14b8a6",
                "#f59e0b","#10b981","#3b82f6","#f97316",
            ];

            let mut fallback_nodes: Vec<serde_json::Value> = fold_rows.iter().enumerate().map(|(i, r)| {
                let color = palette[i % palette.len()];
                serde_json::json!({
                    "id":            r.get::<String,_>("id"),
                    "fold_id":       r.get::<String,_>("id"),
                    "fold_name":     r.get::<String,_>("name"),
                    "fold_type":     r.get::<String,_>("fold_type"),
                    "name":          r.get::<String,_>("name"),
                    "color":         color,
                    "auto_detected": false,
                    "et_count":      total_ets,   // show total ETs in the single fold
                    "is_fold_fallback": true,
                })
            }).collect();

            // If no folds either, synthesise one virtual node from the project's ETs
            if fallback_nodes.is_empty() && total_ets > 0 {
                fallback_nodes.push(serde_json::json!({
                    "id":            project_id,
                    "fold_id":       project_id,
                    "fold_name":     "Default",
                    "fold_type":     "normal",
                    "name":          "Default",
                    "color":         "#6366f1",
                    "auto_detected": false,
                    "et_count":      total_ets,
                    "is_fold_fallback": true,
                }));
            }

            return Ok(serde_json::json!({
                "bounded_contexts": fallback_nodes,
                "relationships": [],
                "is_fold_fallback": true,
            }));
        }

        Ok(serde_json::json!({ "bounded_contexts": nodes, "relationships": edges }))
    }

    // ── System Interfaces ──────────────────────────────────────────────────────

    pub async fn list_interfaces(&self) -> Result<Vec<serde_json::Value>> {
        let ifaces = sqlx::query(
            "SELECT id, name, description, is_builtin, created_at FROM interfaces ORDER BY is_builtin DESC, name ASC",
        )
        .fetch_all(&self.pool)
        .await?;

        let mut result = Vec::new();
        for r in ifaces {
            let iface_id: String = r.get("id");
            let fields = sqlx::query(
                "SELECT field_name, field_type, required, description
                 FROM interface_fields WHERE interface_id = ? ORDER BY field_name",
            )
            .bind(&iface_id)
            .fetch_all(&self.pool)
            .await?;

            let fields_json: Vec<serde_json::Value> = fields.into_iter().map(|f| serde_json::json!({
                "field_name":  f.get::<String,_>("field_name"),
                "field_type":  f.get::<String,_>("field_type"),
                "required":    f.get::<i64,_>("required") != 0,
                "description": f.get::<Option<String>,_>("description"),
            })).collect();

            result.push(serde_json::json!({
                "id":          iface_id,
                "name":        r.get::<String,_>("name"),
                "description": r.get::<Option<String>,_>("description"),
                "is_builtin":  r.get::<i64,_>("is_builtin") != 0,
                "created_at":  r.get::<String,_>("created_at"),
                "fields":      fields_json,
            }));
        }
        Ok(result)
    }

    pub async fn create_interface(&self, name: &str, description: Option<&str>) -> Result<InterfaceRow> {
        let id = Uuid::new_v4().to_string();
        let now = Self::now_iso();
        sqlx::query(
            "INSERT INTO interfaces (id, name, description, is_builtin, created_at) VALUES (?, ?, ?, 0, ?)",
        )
        .bind(&id).bind(name).bind(description).bind(&now)
        .execute(&self.pool).await?;
        Ok(InterfaceRow { id, name: name.to_string(), description: description.map(|s| s.to_string()), is_builtin: false, created_at: now })
    }

    pub async fn delete_interface(&self, id: &str) -> Result<()> {
        // Guard: cannot delete built-in interfaces
        let is_builtin: i64 = sqlx::query_scalar(
            "SELECT is_builtin FROM interfaces WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?
        .unwrap_or(0);
        if is_builtin != 0 {
            return Err(anyhow::anyhow!("Cannot delete built-in interface"));
        }
        sqlx::query("DELETE FROM interfaces WHERE id = ?")
            .bind(id).execute(&self.pool).await?;
        Ok(())
    }

    pub async fn list_et_interfaces(&self, et_id: &str) -> Result<Vec<serde_json::Value>> {
        let rows = sqlx::query(
            "SELECT i.id, i.name, i.description, i.is_builtin, i.created_at
             FROM entity_type_interfaces eti
             JOIN interfaces i ON i.id = eti.interface_id
             WHERE eti.et_id = ?
             ORDER BY i.is_builtin DESC, i.name",
        )
        .bind(et_id)
        .fetch_all(&self.pool)
        .await?;

        let mut result = Vec::new();
        for r in rows {
            let iface_id: String = r.get("id");
            let fields = sqlx::query(
                "SELECT field_name, field_type, required, description
                 FROM interface_fields WHERE interface_id = ? ORDER BY field_name",
            )
            .bind(&iface_id)
            .fetch_all(&self.pool)
            .await?;
            let fields_json: Vec<serde_json::Value> = fields.into_iter().map(|f| serde_json::json!({
                "field_name":  f.get::<String,_>("field_name"),
                "field_type":  f.get::<String,_>("field_type"),
                "required":    f.get::<i64,_>("required") != 0,
                "description": f.get::<Option<String>,_>("description"),
            })).collect();
            result.push(serde_json::json!({
                "id":          iface_id,
                "name":        r.get::<String,_>("name"),
                "description": r.get::<Option<String>,_>("description"),
                "is_builtin":  r.get::<i64,_>("is_builtin") != 0,
                "created_at":  r.get::<String,_>("created_at"),
                "fields":      fields_json,
            }));
        }
        Ok(result)
    }

    pub async fn add_et_interface(&self, et_id: &str, interface_id: &str) -> Result<()> {
        sqlx::query(
            "INSERT OR IGNORE INTO entity_type_interfaces (et_id, interface_id) VALUES (?, ?)",
        )
        .bind(et_id).bind(interface_id)
        .execute(&self.pool).await?;
        Ok(())
    }

    pub async fn remove_et_interface(&self, et_id: &str, interface_id: &str) -> Result<()> {
        sqlx::query(
            "DELETE FROM entity_type_interfaces WHERE et_id = ? AND interface_id = ?",
        )
        .bind(et_id).bind(interface_id)
        .execute(&self.pool).await?;
        Ok(())
    }

    // ── P1: BC 自动推断（Union-Find 边密度算法）────────────────────────────────

    /// 推断 fold 内的 child-BC 建议，不写入数据库。
    /// 返回 Vec<SuggestedBC>，含 ET 列表、置信度、建议名称。
    pub async fn infer_child_bcs(&self, fold_id: &str) -> Result<serde_json::Value> {
        // 1. 获取 fold 内所有 ET
        let et_rows = sqlx::query(
            "SELECT id, name, display_name, COALESCE(ddd_role,'entity') as ddd_role
             FROM entity_types WHERE fold_id = ? AND name != 'default'",
        )
        .bind(fold_id)
        .fetch_all(&self.pool)
        .await?;

        if et_rows.is_empty() {
            return Ok(serde_json::json!({ "suggestions": [] }));
        }

        let et_ids: Vec<String> = et_rows.iter().map(|r| r.get::<String,_>("id")).collect();
        let et_id_set: std::collections::HashSet<&str> = et_ids.iter().map(|s| s.as_str()).collect();

        // 2. 获取 fold 内 ET 之间的 FK 边（来自 link_type_mappings）
        //    from_et = promote 时映射到的 ET，to_et = FK 指向的 ET
        let link_rows = sqlx::query(
            "SELECT otm.entity_type_id AS from_et_id, ltm.to_entity_type_id AS to_et_id
             FROM link_type_mappings ltm
             JOIN object_type_mappings otm ON otm.dataset_id = ltm.dataset_id
             WHERE otm.entity_type_id IN (SELECT id FROM entity_types WHERE fold_id = ?)
               AND ltm.to_entity_type_id IN (SELECT id FROM entity_types WHERE fold_id = ?)",
        )
        .bind(fold_id)
        .bind(fold_id)
        .fetch_all(&self.pool)
        .await?;

        // 3. 也获取 ontology_links 中的实例边（按 ET 对聚合计数）
        let instance_link_rows = sqlx::query(
            "SELECT oa.entity_type_id AS from_et_id, ob.entity_type_id AS to_et_id,
                    COUNT(*) AS edge_count
             FROM ontology_links ol
             JOIN ontology_objects oa ON oa.id = ol.from_id
             JOIN ontology_objects ob ON ob.id = ol.to_id
             WHERE oa.entity_type_id IN (SELECT id FROM entity_types WHERE fold_id = ?)
               AND ob.entity_type_id IN (SELECT id FROM entity_types WHERE fold_id = ?)
               AND oa.entity_type_id != ob.entity_type_id
             GROUP BY oa.entity_type_id, ob.entity_type_id",
        )
        .bind(fold_id)
        .bind(fold_id)
        .fetch_all(&self.pool)
        .await?;

        // 4. 构建边集合：(from_et_id, to_et_id, weight)
        //    schema links (link_type_mappings) 权重 = 3（结构信号强）
        //    instance links 权重 = edge_count（数据密度信号）
        let mut edge_weights: std::collections::HashMap<(String, String), f64> = std::collections::HashMap::new();

        for r in &link_rows {
            let from: String = r.get("from_et_id");
            let to: String = r.get("to_et_id");
            if from != to && et_id_set.contains(from.as_str()) && et_id_set.contains(to.as_str()) {
                let key = if from < to { (from.clone(), to.clone()) } else { (to.clone(), from.clone()) };
                *edge_weights.entry(key).or_insert(0.0) += 3.0;
            }
        }
        for r in &instance_link_rows {
            let from: String = r.get("from_et_id");
            let to: String = r.get("to_et_id");
            let cnt: i64 = r.get("edge_count");
            if from != to && et_id_set.contains(from.as_str()) && et_id_set.contains(to.as_str()) {
                let key = if from < to { (from.clone(), to.clone()) } else { (to.clone(), from.clone()) };
                *edge_weights.entry(key).or_insert(0.0) += cnt as f64;
            }
        }

        // 5. Union-Find — 边密度阈值决定是否合并
        //    阈值逻辑：schema link（权重≥3）直接合并；instance link 按密度判断
        let n = et_ids.len();
        let mut parent: Vec<usize> = (0..n).collect();

        fn find(parent: &mut Vec<usize>, x: usize) -> usize {
            if parent[x] != x { parent[x] = find(parent, parent[x]); }
            parent[x]
        }
        fn union(parent: &mut Vec<usize>, x: usize, y: usize) {
            let (px, py) = (find(parent, x), find(parent, y));
            if px != py { parent[px] = py; }
        }

        let et_index: std::collections::HashMap<&str, usize> = et_ids.iter().enumerate().map(|(i, id)| (id.as_str(), i)).collect();

        for ((from, to), weight) in &edge_weights {
            let fi = et_index[from.as_str()];
            let ti = et_index[to.as_str()];
            // schema link always merges; instance link merges if weight >= 2
            if *weight >= 2.0 {
                union(&mut parent, fi, ti);
            }
        }

        // 6. 收集连通分量
        let mut components: std::collections::HashMap<usize, Vec<usize>> = std::collections::HashMap::new();
        for i in 0..n {
            let root = find(&mut parent, i);
            components.entry(root).or_default().push(i);
        }

        // 7. 对每个分量：找 Aggregate Root（入度最高），计算置信度，生成名称
        // 入度 = 其他 ET 指向该 ET 的 schema FK 数
        let mut in_degree: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
        for r in &link_rows {
            let to: String = r.get("to_et_id");
            if et_id_set.contains(to.as_str()) {
                *in_degree.entry(
                    et_ids.iter().find(|id| id.as_str() == to).map(|s| s.as_str()).unwrap_or("")
                ).or_insert(0) += 1;
            }
        }

        // 总边数（用于置信度）
        let total_edges = edge_weights.values().sum::<f64>();

        let mut suggestions: Vec<serde_json::Value> = components.values().map(|indices| {
            let comp_et_ids: Vec<&str> = indices.iter().map(|&i| et_ids[i].as_str()).collect();

            // 内部边权重之和
            let internal_weight: f64 = edge_weights.iter()
                .filter(|((f, t), _)| comp_et_ids.contains(&f.as_str()) && comp_et_ids.contains(&t.as_str()))
                .map(|(_, w)| w)
                .sum();

            // 外部边权重之和（跨分量）
            let external_weight: f64 = edge_weights.iter()
                .filter(|((f, t), _)| {
                    let f_in = comp_et_ids.contains(&f.as_str());
                    let t_in = comp_et_ids.contains(&t.as_str());
                    f_in ^ t_in  // 只有一端在分量内
                })
                .map(|(_, w)| w)
                .sum();

            // 置信度 = 内部密度 / (内部 + 外部)
            let confidence = if internal_weight + external_weight > 0.0 {
                internal_weight / (internal_weight + external_weight)
            } else if comp_et_ids.len() == 1 {
                0.6  // 孤立节点：中等置信度
            } else {
                0.5
            };

            // 找 Aggregate Root（最高入度，优先 aggregate_root 角色）
            let agg_root_idx = indices.iter().max_by_key(|&&i| {
                let id = et_ids[i].as_str();
                let ddd_role: String = et_rows[i].get("ddd_role");
                let role_bonus = if ddd_role == "aggregate_root" { 100 } else { 0 };
                in_degree.get(id).copied().unwrap_or(0) + role_bonus
            }).copied().unwrap_or(indices[0]);

            let agg_root_name: String = et_rows[agg_root_idx].get("display_name");
            let agg_root_id: String = et_rows[agg_root_idx].get("id");

            // BC 名称建议：Aggregate Root 名 + " BC"
            let bc_name = Self::suggest_bc_name(&agg_root_name);

            // ET 详情
            let ets: Vec<serde_json::Value> = indices.iter().map(|&i| {
                let et_id: String = et_rows[i].get("id");
                let et_name: String = et_rows[i].get("display_name");
                let ddd_role: String = et_rows[i].get("ddd_role");
                let is_root = et_id == agg_root_id;
                serde_json::json!({
                    "id": et_id, "display_name": et_name,
                    "ddd_role": ddd_role, "is_aggregate_root": is_root,
                })
            }).collect();

            // 默认颜色（从调色板按 index 取）
            let palette = ["#6366f1","#10b981","#f59e0b","#ef4444","#8b5cf6","#06b6d4","#ec4899"];
            let color = palette[agg_root_idx % palette.len()];

            serde_json::json!({
                "suggested_name": bc_name,
                "aggregate_root_id": agg_root_id,
                "confidence": (confidence * 100.0).round() / 100.0,
                "confidence_pct": (confidence * 100.0).round() as i64,
                "color": color,
                "et_ids": comp_et_ids,
                "entity_types": ets,
                "internal_links": internal_weight as i64,
                "external_links": external_weight as i64,
            })
        }).collect();

        // 按置信度降序排列
        suggestions.sort_by(|a, b| {
            let ca = a["confidence"].as_f64().unwrap_or(0.0);
            let cb = b["confidence"].as_f64().unwrap_or(0.0);
            cb.partial_cmp(&ca).unwrap_or(std::cmp::Ordering::Equal)
        });

        // 8. 检测跨分量（跨 BC）的 FK 边 → 建议 bc_relationships
        let comp_for_et: std::collections::HashMap<&str, usize> = {
            let mut m = std::collections::HashMap::new();
            for (root, indices) in &components {
                for &i in indices { m.insert(et_ids[i].as_str(), *root); }
            }
            m
        };

        let mut cross_bc_links: Vec<serde_json::Value> = Vec::new();
        for r in &link_rows {
            let from: String = r.get("from_et_id");
            let to: String = r.get("to_et_id");
            let fc = comp_for_et.get(from.as_str()).copied();
            let tc = comp_for_et.get(to.as_str()).copied();
            if let (Some(fc), Some(tc)) = (fc, tc) {
                if fc != tc {
                    // 跨 BC FK → 建议 customer_supplier
                    cross_bc_links.push(serde_json::json!({
                        "from_bc_suggested_name": suggestions.iter()
                            .find(|s| s["et_ids"].as_array().map(|a| a.iter().any(|e| e.as_str() == Some(from.as_str()))).unwrap_or(false))
                            .and_then(|s| s["suggested_name"].as_str())
                            .unwrap_or(""),
                        "to_bc_suggested_name": suggestions.iter()
                            .find(|s| s["et_ids"].as_array().map(|a| a.iter().any(|e| e.as_str() == Some(to.as_str()))).unwrap_or(false))
                            .and_then(|s| s["suggested_name"].as_str())
                            .unwrap_or(""),
                        "from_et_id": from,
                        "to_et_id": to,
                        "suggested_relationship_type": "customer_supplier",
                    }));
                }
            }
        }
        // Deduplicate cross_bc_links by (from_bc, to_bc) pair
        let mut seen_pairs = std::collections::HashSet::new();
        cross_bc_links.retain(|l| {
            let pair = (
                l["from_bc_suggested_name"].as_str().unwrap_or("").to_string(),
                l["to_bc_suggested_name"].as_str().unwrap_or("").to_string(),
            );
            seen_pairs.insert(pair)
        });

        Ok(serde_json::json!({
            "fold_id": fold_id,
            "suggestions": suggestions,
            "cross_bc_links": cross_bc_links,
            "total_ets": n,
            "total_edges": total_edges as i64,
        }))
    }

    fn suggest_bc_name(agg_root_name: &str) -> String {
        // "Order" → "Order BC", "CustomerAddress" → "Customer BC"
        // Simple rule: take the first word and append " BC"
        let first_word = agg_root_name
            .split(|c: char| c == '_' || c == ' ' || c.is_uppercase())
            .find(|s| !s.is_empty())
            .unwrap_or(agg_root_name);
        // CamelCase: find first uppercase segment
        let name = if agg_root_name.chars().any(|c| c.is_uppercase()) {
            // Extract first CamelCase word
            let mut word = String::new();
            for (i, c) in agg_root_name.chars().enumerate() {
                if i > 0 && c.is_uppercase() && !word.is_empty() { break; }
                word.push(c);
            }
            word
        } else {
            first_word.to_string()
        };
        format!("{} BC", name)
    }

    /// 将推断建议应用（写入）到数据库。
    /// suggestions: Vec of { suggested_name, color, et_ids: [String] }
    pub async fn apply_bc_suggestions(
        &self,
        fold_id: &str,
        suggestions: &[serde_json::Value],
    ) -> Result<Vec<serde_json::Value>> {
        // 清除该 fold 下所有 auto_detected BC（保留手动创建的）
        sqlx::query(
            "UPDATE entity_types SET bc_id = NULL
             WHERE fold_id = ? AND bc_id IN (
               SELECT id FROM bounded_contexts WHERE fold_id = ? AND auto_detected = 1
             )",
        )
        .bind(fold_id).bind(fold_id)
        .execute(&self.pool).await?;

        sqlx::query(
            "DELETE FROM bounded_contexts WHERE fold_id = ? AND auto_detected = 1",
        )
        .bind(fold_id).execute(&self.pool).await?;

        let now = Self::now_iso();
        let mut created = Vec::new();

        for sug in suggestions {
            let name = sug["suggested_name"].as_str().unwrap_or("Unknown BC");
            let color = sug["color"].as_str().unwrap_or("#6366f1");
            let bc_id = uuid::Uuid::new_v4().to_string();

            sqlx::query(
                "INSERT INTO bounded_contexts (id, fold_id, name, color, auto_detected, created_at)
                 VALUES (?, ?, ?, ?, 1, ?)",
            )
            .bind(&bc_id).bind(fold_id).bind(name).bind(color).bind(&now)
            .execute(&self.pool).await?;

            if let Some(et_ids) = sug["et_ids"].as_array() {
                for et_id_val in et_ids {
                    if let Some(et_id) = et_id_val.as_str() {
                        sqlx::query("UPDATE entity_types SET bc_id = ? WHERE id = ?")
                            .bind(&bc_id).bind(et_id)
                            .execute(&self.pool).await?;
                    }
                }
            }

            created.push(serde_json::json!({
                "id": bc_id, "fold_id": fold_id,
                "name": name, "color": color, "auto_detected": true,
            }));
        }

        Ok(created)
    }

    // ── State Machine CRUD ───────────────────────────────────────────────────

    pub async fn list_state_definitions(&self, et_id: &str) -> Result<Vec<serde_json::Value>> {
        let rows = sqlx::query(
            "SELECT id, name, display_name, color, description, is_initial, is_terminal, created_at
             FROM state_definitions WHERE target_et_id = ? ORDER BY is_initial DESC, created_at ASC",
        )
        .bind(et_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.iter().map(|r| serde_json::json!({
            "id":           r.get::<String, _>("id"),
            "name":         r.get::<String, _>("name"),
            "display_name": r.get::<String, _>("display_name"),
            "color":        r.get::<String, _>("color"),
            "description":  r.get::<Option<String>, _>("description"),
            "is_initial":   r.get::<i64, _>("is_initial") != 0,
            "is_terminal":  r.get::<i64, _>("is_terminal") != 0,
            "created_at":   r.get::<String, _>("created_at"),
        })).collect())
    }

    pub async fn create_state_definition(&self, et_id: &str, req: &serde_json::Value) -> Result<serde_json::Value> {
        let id = Uuid::new_v4().to_string();
        let now = Self::now_str();
        sqlx::query(
            "INSERT INTO state_definitions
                (id, target_et_id, name, display_name, color, description, is_initial, is_terminal, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(et_id)
        .bind(req["name"].as_str().unwrap_or(""))
        .bind(req["display_name"].as_str().unwrap_or(""))
        .bind(req["color"].as_str().unwrap_or("#6366f1"))
        .bind(req["description"].as_str())
        .bind(if req["is_initial"].as_bool().unwrap_or(false) { 1i64 } else { 0 })
        .bind(if req["is_terminal"].as_bool().unwrap_or(false) { 1i64 } else { 0 })
        .bind(&now)
        .execute(&self.pool)
        .await?;
        Ok(serde_json::json!({ "id": id }))
    }

    pub async fn update_state_definition(&self, id: &str, req: &serde_json::Value) -> Result<()> {
        let display_name = req["display_name"].as_str().unwrap_or("");
        let color        = req["color"].as_str().unwrap_or("#6366f1");
        let is_initial   = req["is_initial"].as_bool().unwrap_or(false);
        let is_terminal  = req["is_terminal"].as_bool().unwrap_or(false);
        let description  = req["description"].as_str();
        sqlx::query(
            "UPDATE state_definitions SET display_name=?, color=?, is_initial=?, is_terminal=?, description=? WHERE id=?"
        )
        .bind(display_name)
        .bind(color)
        .bind(is_initial)
        .bind(is_terminal)
        .bind(description)
        .bind(id)
        .execute(&self.pool).await?;
        Ok(())
    }

    pub async fn delete_state_definition(&self, id: &str) -> Result<()> {
        sqlx::query("DELETE FROM state_definitions WHERE id = ?")
            .bind(id).execute(&self.pool).await?;
        Ok(())
    }

    pub async fn list_state_transitions(&self, et_id: &str) -> Result<Vec<serde_json::Value>> {
        let rows = sqlx::query(
            "SELECT t.id, t.from_state_id, t.to_state_id,
                    f.name as from_name, f.display_name as from_display, f.color as from_color,
                    g.name as to_name,   g.display_name as to_display,   g.color as to_color
             FROM state_transitions t
             JOIN state_definitions f ON t.from_state_id = f.id
             JOIN state_definitions g ON t.to_state_id   = g.id
             WHERE t.target_et_id = ? ORDER BY t.created_at ASC",
        )
        .bind(et_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.iter().map(|r| serde_json::json!({
            "id":           r.get::<String, _>("id"),
            "from_state_id": r.get::<String, _>("from_state_id"),
            "to_state_id":   r.get::<String, _>("to_state_id"),
            "from_name":     r.get::<String, _>("from_name"),
            "from_display":  r.get::<String, _>("from_display"),
            "from_color":    r.get::<String, _>("from_color"),
            "to_name":       r.get::<String, _>("to_name"),
            "to_display":    r.get::<String, _>("to_display"),
            "to_color":      r.get::<String, _>("to_color"),
        })).collect())
    }

    pub async fn create_state_transition(&self, et_id: &str, from_id: &str, to_id: &str) -> Result<serde_json::Value> {
        let id = Uuid::new_v4().to_string();
        let now = Self::now_str();
        sqlx::query(
            "INSERT OR IGNORE INTO state_transitions (id, target_et_id, from_state_id, to_state_id, created_at)
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(&id).bind(et_id).bind(from_id).bind(to_id).bind(&now)
        .execute(&self.pool).await?;
        Ok(serde_json::json!({ "id": id }))
    }

    pub async fn delete_state_transition(&self, id: &str) -> Result<()> {
        sqlx::query("DELETE FROM state_transitions WHERE id = ?")
            .bind(id).execute(&self.pool).await?;
        Ok(())
    }

    // ── Phase 3: object state + execution record ──────────────────────────────

    pub async fn get_object_current_state(&self, object_id: &str) -> Result<Option<serde_json::Value>> {
        let row = sqlx::query(
            "SELECT o.current_state_id, o.entity_type_id, o.fields,
                    s.name AS state_name, s.display_name AS state_display, s.color AS state_color
             FROM ontology_objects o
             LEFT JOIN state_definitions s ON s.id = o.current_state_id
             WHERE o.id = ?",
        )
        .bind(object_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|r| serde_json::json!({
            "current_state_id":      r.get::<Option<String>, _>("current_state_id"),
            "current_state_name":    r.get::<Option<String>, _>("state_name"),
            "current_state_display": r.get::<Option<String>, _>("state_display"),
            "current_state_color":   r.get::<Option<String>, _>("state_color"),
            "entity_type_id":        r.get::<Option<String>, _>("entity_type_id"),
            "fields":                r.get::<Option<String>, _>("fields"),
        })))
    }

    pub async fn update_object_state(&self, object_id: &str, state_id: Option<&str>) -> Result<()> {
        sqlx::query("UPDATE ontology_objects SET current_state_id = ? WHERE id = ?")
            .bind(state_id)
            .bind(object_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn record_action_execution(
        &self,
        action_type_id: &str,
        object_id:       &str,
        from_state_id:   Option<&str>,
        to_state_id:     Option<&str>,
        params:          &serde_json::Value,
        result:          &str,
        status:          &str,
    ) -> Result<String> {
        let id  = Uuid::new_v4().to_string();
        let now = Self::now_str();
        sqlx::query(
            "INSERT INTO action_executions
             (id, action_type_id, object_id, from_state_id, to_state_id, params, result, status, executed_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(action_type_id)
        .bind(object_id)
        .bind(from_state_id)
        .bind(to_state_id)
        .bind(params.to_string())
        .bind(result)
        .bind(status)
        .bind(&now)
        .execute(&self.pool)
        .await?;
        Ok(id)
    }

    pub async fn list_action_executions(&self, object_id: &str) -> Result<Vec<serde_json::Value>> {
        let rows = sqlx::query(
            "SELECT e.id, e.action_type_id, e.object_id, e.from_state_id, e.to_state_id,
                    e.params, e.result, e.status, e.executed_at,
                    a.name AS action_name, a.display_name AS action_display,
                    fs.display_name AS from_display, ts.display_name AS to_display
             FROM action_executions e
             JOIN action_types a ON a.id = e.action_type_id
             LEFT JOIN state_definitions fs ON fs.id = e.from_state_id
             LEFT JOIN state_definitions ts ON ts.id = e.to_state_id
             WHERE e.object_id = ?
             ORDER BY e.executed_at DESC",
        )
        .bind(object_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.iter().map(|r| serde_json::json!({
            "id":             r.get::<String, _>("id"),
            "action_name":    r.get::<String, _>("action_name"),
            "action_display": r.get::<String, _>("action_display"),
            "from_display":   r.get::<Option<String>, _>("from_display"),
            "to_display":     r.get::<Option<String>, _>("to_display"),
            "params":         serde_json::from_str::<serde_json::Value>(&r.get::<String, _>("params")).unwrap_or_default(),
            "result":         r.get::<Option<String>, _>("result"),
            "status":         r.get::<String, _>("status"),
            "executed_at":    r.get::<String, _>("executed_at"),
        })).collect())
    }

    // ── ActionType CRUD ───────────────────────────────────────────────────────

    pub async fn list_action_types(&self, target_et_id: Option<&str>) -> Result<Vec<serde_json::Value>> {
        let rows = if let Some(et_id) = target_et_id {
            sqlx::query(
                "SELECT id, name, display_name, target_et_id, level, from_states, to_state,
                        params, trigger, allowed_personas, bc_id, saga_def_id, status, created_at
                 FROM action_types WHERE target_et_id = ? ORDER BY created_at ASC",
            )
            .bind(et_id)
            .fetch_all(&self.pool)
            .await?
        } else {
            sqlx::query(
                "SELECT id, name, display_name, target_et_id, level, from_states, to_state,
                        params, trigger, allowed_personas, bc_id, saga_def_id, status, created_at
                 FROM action_types ORDER BY created_at ASC",
            )
            .fetch_all(&self.pool)
            .await?
        };
        Ok(rows.iter().map(|r| serde_json::json!({
            "id":               r.get::<String, _>("id"),
            "name":             r.get::<String, _>("name"),
            "display_name":     r.get::<String, _>("display_name"),
            "target_et_id":     r.get::<Option<String>, _>("target_et_id"),
            "level":            r.get::<String, _>("level"),
            "from_states":      serde_json::from_str::<serde_json::Value>(&r.get::<String, _>("from_states")).unwrap_or_default(),
            "to_state":         r.get::<Option<String>, _>("to_state"),
            "params":           serde_json::from_str::<serde_json::Value>(&r.get::<String, _>("params")).unwrap_or_default(),
            "trigger":          r.get::<String, _>("trigger"),
            "allowed_personas": serde_json::from_str::<serde_json::Value>(&r.get::<String, _>("allowed_personas")).unwrap_or_default(),
            "bc_id":            r.get::<Option<String>, _>("bc_id"),
            "saga_def_id":      r.get::<Option<String>, _>("saga_def_id"),
            "status":           r.get::<String, _>("status"),
            "created_at":       r.get::<String, _>("created_at"),
        })).collect())
    }

    pub async fn create_action_type(&self, req: &serde_json::Value) -> Result<serde_json::Value> {
        let id = Uuid::new_v4().to_string();
        let now = Self::now_str();
        sqlx::query(
            "INSERT INTO action_types
                (id, name, display_name, target_et_id, level, from_states, to_state,
                 params, trigger, allowed_personas, bc_id, saga_def_id, status, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 'draft', ?)",
        )
        .bind(&id)
        .bind(req["name"].as_str().unwrap_or(""))
        .bind(req["display_name"].as_str().unwrap_or(""))
        .bind(req["target_et_id"].as_str())
        .bind(req["level"].as_str().unwrap_or("object"))
        .bind(req["from_states"].as_array().map(|_| req["from_states"].to_string()).unwrap_or_else(|| "[]".to_string()))
        .bind(req["to_state"].as_str())
        .bind(req["params"].as_array().map(|_| req["params"].to_string()).unwrap_or_else(|| "[]".to_string()))
        .bind(req["trigger"].as_str().unwrap_or("manual"))
        .bind(req["allowed_personas"].as_array().map(|_| req["allowed_personas"].to_string()).unwrap_or_else(|| "[]".to_string()))
        .bind(req["bc_id"].as_str())
        .bind(req["saga_def_id"].as_str())
        .bind(&now)
        .execute(&self.pool)
        .await?;
        Ok(serde_json::json!({ "id": id }))
    }

    pub async fn update_action_type(&self, id: &str, req: &serde_json::Value) -> Result<()> {
        sqlx::query(
            "UPDATE action_types SET
                name             = COALESCE(?, name),
                display_name     = COALESCE(?, display_name),
                level            = COALESCE(?, level),
                from_states      = COALESCE(?, from_states),
                to_state         = ?,
                params           = COALESCE(?, params),
                trigger          = COALESCE(?, trigger),
                allowed_personas = COALESCE(?, allowed_personas),
                bc_id            = ?,
                saga_def_id      = ?
             WHERE id = ?",
        )
        .bind(req["name"].as_str())
        .bind(req["display_name"].as_str())
        .bind(req["level"].as_str())
        .bind(req["from_states"].as_array().map(|_| req["from_states"].to_string()))
        .bind(req["to_state"].as_str())
        .bind(req["params"].as_array().map(|_| req["params"].to_string()))
        .bind(req["trigger"].as_str())
        .bind(req["allowed_personas"].as_array().map(|_| req["allowed_personas"].to_string()))
        .bind(req["bc_id"].as_str())
        .bind(req["saga_def_id"].as_str())
        .bind(id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn set_action_type_status(&self, id: &str, status: &str) -> Result<()> {
        sqlx::query("UPDATE action_types SET status = ? WHERE id = ?")
            .bind(status).bind(id).execute(&self.pool).await?;
        Ok(())
    }

    pub async fn delete_action_type(&self, id: &str) -> Result<()> {
        sqlx::query("DELETE FROM action_types WHERE id = ?")
            .bind(id).execute(&self.pool).await?;
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
