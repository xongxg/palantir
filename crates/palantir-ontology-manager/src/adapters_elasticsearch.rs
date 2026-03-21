/// Elasticsearch / OpenSearch 适配器
///
/// 支持 Search API（query DSL）和 Scroll API（全量导出）。
/// 增量模式：基于 @timestamp 或自定义时间字段的范围查询。
///
/// 依赖：直接使用 reqwest 调用 ES REST API（避免引入重量级 ES client crate）
///
/// 配置示例（deployment.toml）：
/// ```toml
/// [sources.logs_es]
/// type     = "elasticsearch"
/// url      = "https://es-cluster:9200"
/// index    = "app-logs-*"
/// query    = '{"query":{"range":{"@timestamp":{"gte":"now-7d"}}}}'
/// username = "elastic"
/// password = "${ES_PASSWORD}"
/// cursor_field = "@timestamp"
/// scroll_size  = 1000           # 每页拉取条数（Scroll API）
/// ```
use crate::adapters::{DiscoveredSchema, SourceAdapter, SourceDescriptor};
use crate::errors::AdapterError;
use crate::model::{CanonicalRecord, Cursor};
use async_trait::async_trait;
use futures_core::Stream;
use futures_util::stream;

pub struct ElasticsearchAdapter {
    pub id:           String,
    pub ns:           String,
    pub schema:       String,

    /// ES 节点 URL（如 https://localhost:9200）
    pub url:          String,
    /// 索引名，支持通配符（如 logs-*）
    pub index:        String,
    /// Query DSL JSON 字符串；None = match_all
    pub query:        Option<String>,
    /// HTTP Basic Auth
    pub username:     Option<String>,
    pub password:     Option<String>,
    /// API Key（x-api-key 头）
    pub api_key:      Option<String>,
    /// 增量水位线字段（如 @timestamp）
    pub cursor_field: Option<String>,
    /// Scroll 每页大小
    pub scroll_size:  usize,
}

impl ElasticsearchAdapter {
    pub fn new(
        id: impl Into<String>,
        url: impl Into<String>,
        index: impl Into<String>,
        ns: impl Into<String>,
        schema: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(), ns: ns.into(), schema: schema.into(),
            url: url.into(), index: index.into(),
            query: None, username: None, password: None,
            api_key: None, cursor_field: None, scroll_size: 1000,
        }
    }

    pub fn with_query(mut self, q: impl Into<String>) -> Self { self.query = Some(q.into()); self }
    pub fn with_basic_auth(mut self, u: impl Into<String>, p: impl Into<String>) -> Self {
        self.username = Some(u.into()); self.password = Some(p.into()); self
    }
    pub fn with_api_key(mut self, k: impl Into<String>) -> Self { self.api_key = Some(k.into()); self }
    pub fn with_cursor(mut self, f: impl Into<String>) -> Self { self.cursor_field = Some(f.into()); self }
    pub fn with_scroll_size(mut self, n: usize) -> Self { self.scroll_size = n; self }
}

#[async_trait]
impl SourceAdapter for ElasticsearchAdapter {
    fn id(&self) -> &str { &self.id }
    fn adapter_type(&self) -> &'static str { "elasticsearch" }

    async fn describe(&self) -> SourceDescriptor {
        SourceDescriptor {
            id: self.id.clone(),
            adapter_type: "elasticsearch".to_string(),
            has_cursor: self.cursor_field.is_some(),
            partitions: None,
        }
    }

    async fn test_connection(&self) -> Result<String, AdapterError> {
        // TODO: GET /_cluster/health 验证连通性
        Err(AdapterError::Message("Elasticsearch adapter: not yet implemented".to_string()))
    }

    async fn fetch_preview(&self, _limit: usize) -> Result<Vec<serde_json::Value>, AdapterError> {
        // TODO:
        // POST /{index}/_search  with size=_limit, query=self.query
        // 从 hits.hits[*]._source 提取记录
        Err(AdapterError::Message("Elasticsearch adapter: not yet implemented".to_string()))
    }

    async fn discover_schema(&self) -> Result<DiscoveredSchema, AdapterError> {
        // TODO: 可通过 GET /{index}/_mapping 获取精确 schema
        // 或复用 fetch_preview(5) + discover_from_records
        Err(AdapterError::Message("Elasticsearch adapter: not yet implemented".to_string()))
    }

    fn stream(
        &self,
        _since: Option<Cursor>,
    ) -> Box<dyn Stream<Item = Result<CanonicalRecord, AdapterError>> + Unpin + Send> {
        // TODO: 使用 Scroll API 分批拉取全量数据
        // POST /{index}/_search?scroll=1m  → 获取 scroll_id
        // POST /_search/scroll { scroll_id }  → 循环直到空
        Box::new(stream::iter(vec![Err(AdapterError::Message(
            "Elasticsearch adapter: not yet implemented".to_string(),
        ))]))
    }
}
