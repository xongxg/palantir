/// PostgreSQL / MySQL 适配器
///
/// 与 SqlAdapter (SQLite) 逻辑相同，连接串格式不同：
///   PostgreSQL: `postgresql://user:pass@host:5432/dbname`
///   MySQL:      `mysql://user:pass@host:3306/dbname`
///
/// 依赖：sqlx features = ["postgres", "mysql", "runtime-tokio-native-tls"]
///
/// 配置示例（deployment.toml）：
/// ```toml
/// [sources.hr_pg]
/// type  = "postgres"
/// url   = "postgresql://admin:secret@10.0.0.1:5432/hr_db"
/// query = "SELECT id, name, department, salary FROM employees WHERE active = true"
/// id_column     = "id"
/// cursor_column = "updated_at"   # 增量水位线，可选
/// ```
use crate::adapters::{DiscoveredSchema, SourceAdapter, SourceDescriptor};
use crate::errors::AdapterError;
use crate::model::{CanonicalRecord, Cursor};
use async_trait::async_trait;
use futures_core::Stream;
use futures_util::stream;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RdbmsType {
    Postgres,
    Mysql,
}

pub struct PostgresAdapter {
    pub id:            String,
    pub db_type:       RdbmsType,
    /// 完整连接 URL
    pub url:           String,
    pub query:         String,
    pub ns:            String,
    pub schema:        String,
    pub id_column:     String,
    pub cursor_column: Option<String>,
}

impl PostgresAdapter {
    pub fn postgres(
        id: impl Into<String>,
        url: impl Into<String>,
        query: impl Into<String>,
        ns: impl Into<String>,
        schema: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(), db_type: RdbmsType::Postgres,
            url: url.into(), query: query.into(),
            ns: ns.into(), schema: schema.into(),
            id_column: "id".into(), cursor_column: None,
        }
    }

    pub fn mysql(
        id: impl Into<String>,
        url: impl Into<String>,
        query: impl Into<String>,
        ns: impl Into<String>,
        schema: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(), db_type: RdbmsType::Mysql,
            url: url.into(), query: query.into(),
            ns: ns.into(), schema: schema.into(),
            id_column: "id".into(), cursor_column: None,
        }
    }

    pub fn with_cursor(mut self, col: impl Into<String>) -> Self {
        self.cursor_column = Some(col.into()); self
    }
    pub fn with_id_column(mut self, col: impl Into<String>) -> Self {
        self.id_column = col.into(); self
    }
}

#[async_trait]
impl SourceAdapter for PostgresAdapter {
    fn id(&self) -> &str { &self.id }
    fn adapter_type(&self) -> &'static str {
        match self.db_type { RdbmsType::Postgres => "postgres", RdbmsType::Mysql => "mysql" }
    }

    async fn describe(&self) -> SourceDescriptor {
        SourceDescriptor {
            id: self.id.clone(),
            adapter_type: self.adapter_type().to_string(),
            has_cursor: self.cursor_column.is_some(),
            partitions: None,
        }
    }

    async fn test_connection(&self) -> Result<String, AdapterError> {
        // TODO: sqlx::PgPool::connect(&self.url) 或 MySqlPool::connect
        // 然后执行 SELECT 1 验证
        Err(AdapterError::Message(format!("{} adapter: not yet implemented", self.adapter_type())))
    }

    async fn fetch_preview(&self, _limit: usize) -> Result<Vec<serde_json::Value>, AdapterError> {
        // TODO: 与 SqlAdapter.fetch_records 逻辑相同，连接池换成 PgPool / MySqlPool
        Err(AdapterError::Message(format!("{} adapter: not yet implemented", self.adapter_type())))
    }

    async fn discover_schema(&self) -> Result<DiscoveredSchema, AdapterError> {
        Err(AdapterError::Message(format!("{} adapter: not yet implemented", self.adapter_type())))
    }

    fn stream(
        &self,
        _since: Option<Cursor>,
    ) -> Box<dyn Stream<Item = Result<CanonicalRecord, AdapterError>> + Unpin + Send> {
        let msg = format!("{} adapter: not yet implemented", self.adapter_type());
        Box::new(stream::iter(vec![Err(AdapterError::Message(msg))]))
    }
}
