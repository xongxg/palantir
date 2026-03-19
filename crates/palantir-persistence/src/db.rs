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

// ── Db ────────────────────────────────────────────────────────────────────────

pub struct Db {
    pool: SqlitePool,
}

impl Db {
    pub async fn open(path: &str) -> Result<Self> {
        // mode=rwc creates the file if it doesn't exist
        let url = format!("sqlite://{}?mode=rwc", path);
        let pool = SqlitePool::connect(&url).await?;
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

        // Enable foreign key enforcement
        sqlx::query("PRAGMA foreign_keys = ON")
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
