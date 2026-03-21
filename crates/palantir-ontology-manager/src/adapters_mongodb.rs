/// MongoDB 适配器
///
/// 支持集合全量拉取和增量查询（基于 ObjectId 或时间戳字段）。
///
/// 依赖：`mongodb` crate（官方 Rust driver，async）
///
/// 配置示例（deployment.toml）：
/// ```toml
/// [sources.orders_mongo]
/// type       = "mongodb"
/// uri        = "mongodb://user:pass@10.0.0.1:27017"
/// database   = "ecommerce"
/// collection = "orders"
/// filter     = '{"status": "active"}'   # 可选，MongoDB Query Filter JSON
/// projection = '{"_id":1,"amount":1}'   # 可选，字段投影
/// cursor_field = "updatedAt"            # 增量水位线，可选
/// limit      = 0                        # 0 = 全量
/// ```
use crate::adapters::{DiscoveredSchema, SourceAdapter, SourceDescriptor};
use crate::errors::AdapterError;
use crate::model::{CanonicalRecord, Cursor};
use async_trait::async_trait;
use futures_core::Stream;
use futures_util::stream;

pub struct MongoDbAdapter {
    pub id:           String,
    pub ns:           String,
    pub schema:       String,

    /// MongoDB 连接 URI
    pub uri:          String,
    pub database:     String,
    pub collection:   String,
    /// 可选 Query Filter（JSON 字符串，如 `{"status":"active"}`）
    pub filter:       Option<String>,
    /// 可选字段投影（JSON 字符串）
    pub projection:   Option<String>,
    /// 增量水位线字段（如 updatedAt / _id）
    pub cursor_field: Option<String>,
}

impl MongoDbAdapter {
    pub fn new(
        id: impl Into<String>,
        uri: impl Into<String>,
        database: impl Into<String>,
        collection: impl Into<String>,
        ns: impl Into<String>,
        schema: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(), ns: ns.into(), schema: schema.into(),
            uri: uri.into(), database: database.into(),
            collection: collection.into(),
            filter: None, projection: None, cursor_field: None,
        }
    }

    pub fn with_filter(mut self, f: impl Into<String>) -> Self { self.filter = Some(f.into()); self }
    pub fn with_projection(mut self, p: impl Into<String>) -> Self { self.projection = Some(p.into()); self }
    pub fn with_cursor(mut self, field: impl Into<String>) -> Self { self.cursor_field = Some(field.into()); self }
}

#[async_trait]
impl SourceAdapter for MongoDbAdapter {
    fn id(&self) -> &str { &self.id }
    fn adapter_type(&self) -> &'static str { "mongodb" }

    async fn describe(&self) -> SourceDescriptor {
        SourceDescriptor {
            id: self.id.clone(),
            adapter_type: "mongodb".to_string(),
            has_cursor: self.cursor_field.is_some(),
            partitions: None,
        }
    }

    async fn test_connection(&self) -> Result<String, AdapterError> {
        // TODO:
        // let client = mongodb::Client::with_uri_str(&self.uri).await?;
        // client.database(&self.database).run_command(doc!{"ping":1}, None).await?;
        Err(AdapterError::Message("MongoDB adapter: not yet implemented".to_string()))
    }

    async fn fetch_preview(&self, _limit: usize) -> Result<Vec<serde_json::Value>, AdapterError> {
        // TODO:
        // 1. 连接 client
        // 2. collection.find(filter, options.limit(_limit))
        // 3. 将 BSON Document 转换为 serde_json::Value
        Err(AdapterError::Message("MongoDB adapter: not yet implemented".to_string()))
    }

    async fn discover_schema(&self) -> Result<DiscoveredSchema, AdapterError> {
        Err(AdapterError::Message("MongoDB adapter: not yet implemented".to_string()))
    }

    fn stream(
        &self,
        _since: Option<Cursor>,
    ) -> Box<dyn Stream<Item = Result<CanonicalRecord, AdapterError>> + Unpin + Send> {
        Box::new(stream::iter(vec![Err(AdapterError::Message(
            "MongoDB adapter: not yet implemented".to_string(),
        ))]))
    }
}
