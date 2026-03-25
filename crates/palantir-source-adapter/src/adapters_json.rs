use crate::adapters::{DiscoveredSchema, SourceAdapter, SourceDescriptor, discover_from_records};
use crate::errors::AdapterError;
use crate::model::{CanonicalRecord, Cursor};
use async_trait::async_trait;
use futures_core::Stream;
use futures_util::stream;
use std::path::PathBuf;
use time::OffsetDateTime;

/// JSON / JSONL 文件适配器
///
/// 支持两种格式：
///   1. JSON 数组文件:  `[{...}, {...}]`
///   2. JSONL 文件:     每行一个 JSON 对象
///   3. 嵌套数组:       `{"data": [{...}]}` — 指定 records_path = "data"
pub struct JsonAdapter {
    pub id:           String,
    pub path:         PathBuf,
    pub ns:           String,
    pub schema:       String,
    /// 可选：JSON 路径提取 records 数组，如 "data" 或 "items.list"
    pub records_path: Option<String>,
}

impl JsonAdapter {
    pub fn new(
        id: impl Into<String>,
        path: impl Into<PathBuf>,
        ns: impl Into<String>,
        schema: impl Into<String>,
    ) -> Self {
        Self { id: id.into(), path: path.into(), ns: ns.into(), schema: schema.into(), records_path: None }
    }

    pub fn with_records_path(mut self, path: impl Into<String>) -> Self {
        self.records_path = Some(path.into());
        self
    }

    fn load_records(&self) -> Result<Vec<serde_json::Value>, AdapterError> {
        let content = std::fs::read_to_string(&self.path)
            .map_err(|e| AdapterError::Message(format!("read error: {e}")))?;

        // 尝试解析为 JSON Array 或 Object
        let raw: serde_json::Value = serde_json::from_str(&content)
            .or_else(|_| {
                // JSONL: 每行一个对象
                let arr: serde_json::Value = serde_json::Value::Array(
                    content.lines()
                        .filter(|l| !l.trim().is_empty())
                        .filter_map(|l| serde_json::from_str(l).ok())
                        .collect()
                );
                Ok::<_, serde_json::Error>(arr)
            })
            .map_err(|e: serde_json::Error| AdapterError::Message(e.to_string()))?;

        // 如果指定了 records_path，按路径提取
        let node = if let Some(path) = &self.records_path {
            let mut cur = &raw;
            for key in path.split('.') {
                cur = cur.get(key)
                    .ok_or_else(|| AdapterError::Message(format!("path key '{key}' not found")))?;
            }
            cur.clone()
        } else {
            raw
        };

        match node {
            serde_json::Value::Array(arr) => Ok(arr),
            // records_path 未设置且根节点是 Object 时，自动找第一个 Array 字段
            serde_json::Value::Object(map) if self.records_path.is_none() => {
                for (_, v) in map {
                    if let serde_json::Value::Array(arr) = v {
                        return Ok(arr);
                    }
                }
                Err(AdapterError::Message("no array field found in JSON object".to_string()))
            }
            other => Err(AdapterError::Message(format!(
                "expected JSON array, got: {}",
                other.to_string().chars().take(80).collect::<String>()
            ))),
        }
    }
}

#[async_trait]
impl SourceAdapter for JsonAdapter {
    fn id(&self) -> &str { &self.id }
    fn adapter_type(&self) -> &'static str { "json" }

    async fn describe(&self) -> SourceDescriptor {
        SourceDescriptor {
            id: self.id.clone(),
            adapter_type: "json".to_string(),
            has_cursor: false,
            partitions: None,
        }
    }

    async fn test_connection(&self) -> Result<String, AdapterError> {
        if !self.path.exists() {
            return Err(AdapterError::Message(format!("File not found: {}", self.path.display())));
        }
        let records = self.load_records()?;
        Ok(format!("OK — {} records found", records.len()))
    }

    async fn fetch_preview(&self, limit: usize) -> Result<Vec<serde_json::Value>, AdapterError> {
        let records = self.load_records()?;
        Ok(records.into_iter().take(limit).collect())
    }

    async fn discover_schema(&self) -> Result<DiscoveredSchema, AdapterError> {
        let records = self.load_records()?;
        let mut schema = discover_from_records(&records);
        schema.record_count_hint = Some(records.len() as u64);
        Ok(schema)
    }

    fn stream(
        &self,
        _since: Option<Cursor>,
    ) -> Box<dyn Stream<Item = Result<CanonicalRecord, AdapterError>> + Unpin + Send> {
        let records = match self.load_records() {
            Ok(r) => r,
            Err(e) => return Box::new(stream::iter(vec![Err(e)])),
        };

        let id_str   = self.id.clone();
        let ns       = self.ns.clone();
        let schema   = self.schema.clone();

        let items: Vec<_> = records.into_iter().enumerate().map(move |(i, rec)| {
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
