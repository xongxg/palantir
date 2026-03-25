use crate::adapters::{DiscoveredSchema, SourceAdapter, SourceDescriptor, discover_from_records};
use crate::errors::AdapterError;
use crate::model::{CanonicalRecord, Cursor};
use async_trait::async_trait;
use futures_core::Stream;
use futures_util::stream;
use sqlx::{Column, Row, sqlite::SqliteConnectOptions};
use time::OffsetDateTime;

/// SQLite 适配器（demo 阶段；生产换 MySQL 只需换连接串和 feature flag）
///
/// 增量策略：
///   - cursor_column 为空 → 全量拉取
///   - cursor_column 非空 → `WHERE {cursor_column} > {last_cursor} ORDER BY {cursor_column}`
pub struct SqlAdapter {
    pub id:            String,
    pub db_path:       String,    // SQLite: 文件路径；MySQL: "mysql://user:pass@host/db"
    pub query:         String,    // 完整 SELECT（不含 WHERE cursor 条件，适配器自动追加）
    pub ns:            String,
    pub schema:        String,
    pub id_column:     String,    // 哪列作为 external_id
    pub cursor_column: Option<String>, // 增量水位线列（如 updated_at）
}

impl SqlAdapter {
    pub fn new(
        id: impl Into<String>,
        db_path: impl Into<String>,
        query: impl Into<String>,
        ns: impl Into<String>,
        schema: impl Into<String>,
        id_column: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            db_path: db_path.into(),
            query: query.into(),
            ns: ns.into(),
            schema: schema.into(),
            id_column: id_column.into(),
            cursor_column: None,
        }
    }

    pub fn with_cursor(mut self, col: impl Into<String>) -> Self {
        self.cursor_column = Some(col.into());
        self
    }

    async fn open_pool(&self) -> Result<sqlx::SqlitePool, AdapterError> {
        use std::str::FromStr;
        use sqlx::sqlite::SqliteJournalMode;
        let opts = SqliteConnectOptions::from_str(&format!("sqlite://{}", self.db_path))
            .map_err(|e| AdapterError::Message(format!("DB path error: {e}")))?
            .journal_mode(SqliteJournalMode::Wal)
            .read_only(true);
        sqlx::SqlitePool::connect_with(opts)
            .await
            .map_err(|e| AdapterError::Message(format!("DB connect error: {e}")))
    }

    async fn fetch_records(
        &self,
        since: Option<&str>,
        limit: Option<usize>,
    ) -> Result<Vec<serde_json::Value>, AdapterError> {
        let pool = self.open_pool().await?;

        // 构造实际查询
        let sql = {
            let base = self.query.trim_end_matches(';');
            let mut parts = vec![base.to_string()];
            if let (Some(col), Some(cursor)) = (&self.cursor_column, since) {
                parts.push(format!("WHERE {} > '{}'", col, cursor));
            }
            if let Some(col) = &self.cursor_column {
                parts.push(format!("ORDER BY {}", col));
            }
            let base_upper = base.to_uppercase();
            if let Some(n) = limit {
                if !base_upper.contains("LIMIT") {
                    parts.push(format!("LIMIT {}", n));
                }
            }
            // 如果原始 query 已含 WHERE，追加 AND
            let joined = parts.join(" ");
            if joined.to_uppercase().contains("WHERE") && parts.len() > 1 {
                // simple heuristic: replace second WHERE with AND
                let base2 = parts[0].clone();
                let rest = parts[1..].join(" ")
                    .replacen("WHERE ", "AND ", 1);
                format!("{} {}", base2, rest)
            } else {
                joined
            }
        };

        let rows = sqlx::query(&sql)
            .fetch_all(&pool)
            .await
            .map_err(|e| AdapterError::Message(format!("query error: {e}\nSQL: {sql}")))?;

        let records: Vec<serde_json::Value> = rows.iter().map(|row| {
            let cols = row.columns();
            let mut obj = serde_json::Map::new();
            for col in cols {
                let name = col.name();
                // 尝试各种类型
                let val: serde_json::Value =
                    row.try_get::<i64, _>(name).map(|v| serde_json::json!(v))
                    .or_else(|_| row.try_get::<f64, _>(name).map(|v| serde_json::json!(v)))
                    .or_else(|_| row.try_get::<bool, _>(name).map(|v| serde_json::json!(v)))
                    .or_else(|_| row.try_get::<String, _>(name).map(|v| serde_json::json!(v)))
                    .unwrap_or(serde_json::Value::Null);
                obj.insert(name.to_string(), val);
            }
            serde_json::Value::Object(obj)
        }).collect();

        Ok(records)
    }
}

#[async_trait]
impl SourceAdapter for SqlAdapter {
    fn id(&self) -> &str { &self.id }
    fn adapter_type(&self) -> &'static str { "sql" }

    async fn describe(&self) -> SourceDescriptor {
        SourceDescriptor {
            id: self.id.clone(),
            adapter_type: "sql".to_string(),
            has_cursor: self.cursor_column.is_some(),
            partitions: None,
        }
    }

    async fn test_connection(&self) -> Result<String, AdapterError> {
        let pool = self.open_pool().await?;
        sqlx::query("SELECT 1").fetch_one(&pool).await
            .map_err(|e| AdapterError::Message(e.to_string()))?;
        Ok(format!("Connected to {}", self.db_path))
    }

    async fn fetch_preview(&self, limit: usize) -> Result<Vec<serde_json::Value>, AdapterError> {
        self.fetch_records(None, Some(limit)).await
    }

    async fn discover_schema(&self) -> Result<DiscoveredSchema, AdapterError> {
        let records = self.fetch_records(None, Some(5)).await?;
        Ok(discover_from_records(&records))
    }

    fn stream(
        &self,
        since: Option<Cursor>,
    ) -> Box<dyn Stream<Item = Result<CanonicalRecord, AdapterError>> + Unpin + Send> {
        let cursor_str = since
            .as_ref()
            .and_then(|c| c.as_str().map(|s| s.to_string()));

        // 同步拉取（tokio::task::block_in_place 替代方案：用 Runtime::block_on）
        let db_path  = self.db_path.clone();
        let query    = self.query.clone();
        let id_str   = self.id.clone();
        let ns       = self.ns.clone();
        let schema   = self.schema.clone();
        let cursor_col = self.cursor_column.clone();

        // 构建 RT 同步拉取
        let rt = tokio::runtime::Handle::current();
        let adapter = SqlAdapter {
            id: id_str.clone(), db_path, query, ns: ns.clone(),
            schema: schema.clone(), id_column: self.id_column.clone(),
            cursor_column: cursor_col,
        };

        let records = tokio::task::block_in_place(|| {
            rt.block_on(adapter.fetch_records(cursor_str.as_deref(), None))
        });

        match records {
            Err(e) => Box::new(stream::iter(vec![Err(e)])),
            Ok(recs) => {
                let items: Vec<_> = recs.into_iter().enumerate().map(move |(i, rec)| {
                    Ok(CanonicalRecord {
                        source: id_str.clone(),
                        ns:     ns.clone(),
                        schema: schema.clone(),
                        cursor: Some(serde_json::Value::Number(i.into())),
                        ts:     OffsetDateTime::now_utc(),
                        payload: rec,
                    })
                }).collect();
                Box::new(stream::iter(items))
            }
        }
    }
}
