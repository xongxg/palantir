use crate::adapters::{DiscoveredSchema, SourceAdapter, SourceDescriptor, discover_from_records};
use crate::errors::AdapterError;
use crate::model::{CanonicalRecord, Cursor};
use async_trait::async_trait;
use futures_core::Stream;
use futures_util::stream;
use time::OffsetDateTime;

/// REST API 适配器
///
/// 支持分页模式：
///   - offset/limit:  `?offset=0&limit=100`
///   - page/size:     `?page=1&page_size=100`
///   - cursor:        `?cursor={next_cursor}`（从 response 中提取 next_cursor）
///   - 无分页:        只拉一次
pub struct RestAdapter {
    pub id:           String,
    pub url:          String,
    pub ns:           String,
    pub schema:       String,

    /// 可选 Bearer Token
    pub bearer_token: Option<String>,
    /// 可选 API Key Header，如 ("X-API-Key", "secret")
    pub api_key:      Option<(String, String)>,

    /// 从 response JSON 中提取 records 的路径，如 "data" 或 "results"
    /// None 表示 response 本身就是 array
    pub records_path: Option<String>,

    /// 分页：页面大小（0 = 不分页，只拉一次）
    pub page_size:    usize,
    /// 分页参数名（default: "page"），offset 模式用 "offset"
    pub page_param:   String,
    /// 页面大小参数名（default: "limit"）
    pub size_param:   String,
}

impl RestAdapter {
    pub fn new(
        id: impl Into<String>,
        url: impl Into<String>,
        ns: impl Into<String>,
        schema: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(), url: url.into(), ns: ns.into(), schema: schema.into(),
            bearer_token: None, api_key: None, records_path: None,
            page_size: 0,
            page_param: "page".to_string(),
            size_param: "limit".to_string(),
        }
    }

    pub fn with_bearer(mut self, token: impl Into<String>) -> Self {
        self.bearer_token = Some(token.into()); self
    }
    pub fn with_api_key(mut self, header: impl Into<String>, value: impl Into<String>) -> Self {
        self.api_key = Some((header.into(), value.into())); self
    }
    pub fn with_records_path(mut self, path: impl Into<String>) -> Self {
        self.records_path = Some(path.into()); self
    }
    pub fn with_pagination(mut self, page_size: usize, page_param: &str, size_param: &str) -> Self {
        self.page_size  = page_size;
        self.page_param = page_param.to_string();
        self.size_param = size_param.to_string();
        self
    }

    fn build_client(&self) -> reqwest::Client {
        reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .unwrap_or_default()
    }

    fn build_request_with_client(&self, client: &reqwest::Client, url: &str, page: usize) -> reqwest::RequestBuilder {
        let mut req = client.get(url);
        if self.page_size > 0 {
            req = req.query(&[
                (&self.page_param, page.to_string()),
                (&self.size_param, self.page_size.to_string()),
            ]);
        }
        if let Some(token) = &self.bearer_token {
            req = req.bearer_auth(token);
        }
        if let Some((h, v)) = &self.api_key {
            req = req.header(h.as_str(), v.as_str());
        }
        req
    }

    fn build_request(&self, url: &str, page: usize) -> reqwest::RequestBuilder {
        let client = self.build_client();
        let mut req = client.get(url);

        if self.page_size > 0 {
            req = req.query(&[
                (&self.page_param, page.to_string()),
                (&self.size_param, self.page_size.to_string()),
            ]);
        }
        if let Some(token) = &self.bearer_token {
            req = req.bearer_auth(token);
        }
        if let Some((h, v)) = &self.api_key {
            req = req.header(h.as_str(), v.as_str());
        }
        req
    }

    fn extract_records(&self, body: serde_json::Value) -> Vec<serde_json::Value> {
        let node = if let Some(path) = &self.records_path {
            let mut cur = body;
            for key in path.split('.') {
                cur = cur.get(key).cloned().unwrap_or(serde_json::Value::Null);
            }
            cur
        } else {
            body
        };
        match node {
            serde_json::Value::Array(arr) => arr,
            // records_path 未设置且 response 是 Object 时，自动找第一个 Array 字段
            serde_json::Value::Object(map) => {
                for (_, v) in map {
                    if let serde_json::Value::Array(arr) = v {
                        return arr;
                    }
                }
                vec![]
            }
            _ => vec![],
        }
    }

    async fn fetch_all(&self) -> Result<Vec<serde_json::Value>, AdapterError> {
        let mut all = vec![];
        if self.page_size == 0 {
            // 不分页，拉一次
            let body: serde_json::Value = self.build_request(&self.url, 0)
                .send().await.map_err(|e| AdapterError::Message(e.to_string()))?
                .json().await.map_err(|e| AdapterError::Message(e.to_string()))?;
            all.extend(self.extract_records(body));
        } else {
            // 分页拉取，直到空
            let mut page = 1usize;
            loop {
                let body: serde_json::Value = self.build_request(&self.url, page)
                    .send().await.map_err(|e| AdapterError::Message(e.to_string()))?
                    .json().await.map_err(|e| AdapterError::Message(e.to_string()))?;
                let records = self.extract_records(body);
                if records.is_empty() { break; }
                let n = records.len();
                all.extend(records);
                if n < self.page_size { break; }  // 最后一页
                page += 1;
            }
        }
        Ok(all)
    }
}

#[async_trait]
impl SourceAdapter for RestAdapter {
    fn id(&self) -> &str { &self.id }
    fn adapter_type(&self) -> &'static str { "rest" }

    async fn describe(&self) -> SourceDescriptor {
        SourceDescriptor {
            id: self.id.clone(),
            adapter_type: "rest".to_string(),
            has_cursor: false,
            partitions: None,
        }
    }

    async fn test_connection(&self) -> Result<String, AdapterError> {
        let client = self.build_client();
        let mut req = client.get(&self.url);
        if let Some(t) = &self.bearer_token { req = req.bearer_auth(t); }
        if let Some((h, v)) = &self.api_key  { req = req.header(h.as_str(), v.as_str()); }
        let res = req.send().await.map_err(|e| AdapterError::Message(e.to_string()))?;
        let status = res.status();
        if status.is_success() {
            Ok(format!("HTTP {status} — connected"))
        } else {
            Err(AdapterError::Message(format!("HTTP {status}")))
        }
    }

    async fn fetch_preview(&self, limit: usize) -> Result<Vec<serde_json::Value>, AdapterError> {
        // 用同一个 client 把所有页拉完，纯 async，不经过 block_on
        let client = self.build_client();
        let mut all: Vec<serde_json::Value> = vec![];
        if self.page_size == 0 {
            let body: serde_json::Value = self.build_request_with_client(&client, &self.url, 0)
                .send().await.map_err(|e| AdapterError::Message(e.to_string()))?
                .json().await.map_err(|e| AdapterError::Message(e.to_string()))?;
            all.extend(self.extract_records(body));
        } else {
            let mut page = 1usize;
            loop {
                let body: serde_json::Value = self.build_request_with_client(&client, &self.url, page)
                    .send().await.map_err(|e| AdapterError::Message(e.to_string()))?
                    .json().await.map_err(|e| AdapterError::Message(e.to_string()))?;
                let records = self.extract_records(body);
                if records.is_empty() { break; }
                let n = records.len();
                all.extend(records);
                if all.len() >= limit || n < self.page_size { break; }
                page += 1;
            }
        }
        Ok(all.into_iter().take(limit).collect())
    }

    async fn discover_schema(&self) -> Result<DiscoveredSchema, AdapterError> {
        // 只拉第一页（最多 5 条）用于 Schema 推断
        let body: serde_json::Value = self.build_request(&self.url, 1)
            .send().await.map_err(|e| AdapterError::Message(e.to_string()))?
            .json().await.map_err(|e| AdapterError::Message(e.to_string()))?;
        let records = self.extract_records(body);
        let sample: Vec<_> = records.into_iter().take(5).collect();
        Ok(discover_from_records(&sample))
    }

    fn stream(
        &self,
        _since: Option<Cursor>,
    ) -> Box<dyn Stream<Item = Result<CanonicalRecord, AdapterError>> + Unpin + Send> {
        let rt = tokio::runtime::Handle::current();

        // 克隆需要的字段
        let url           = self.url.clone();
        let ns            = self.ns.clone();
        let schema        = self.schema.clone();
        let id_str        = self.id.clone();
        let records_path  = self.records_path.clone();
        let bearer_token  = self.bearer_token.clone();
        let api_key       = self.api_key.clone();
        let page_size     = self.page_size;
        let page_param    = self.page_param.clone();
        let size_param    = self.size_param.clone();

        let adapter = RestAdapter {
            id: id_str.clone(), url, ns: ns.clone(), schema: schema.clone(),
            bearer_token, api_key, records_path, page_size, page_param, size_param,
        };

        let records = tokio::task::block_in_place(|| rt.block_on(adapter.fetch_all()));

        match records {
            Err(e) => Box::new(stream::iter(vec![Err(e)])),
            Ok(recs) => {
                let items: Vec<_> = recs.into_iter().enumerate().map(move |(i, rec)| {
                    Ok(CanonicalRecord {
                        source: id_str.clone(),
                        ns: ns.clone(), schema: schema.clone(),
                        cursor: Some(serde_json::Value::Number(i.into())),
                        ts: OffsetDateTime::now_utc(),
                        payload: rec,
                    })
                }).collect();
                Box::new(stream::iter(items))
            }
        }
    }
}
