use crate::errors::AdapterError;
use crate::model::{CanonicalRecord, Cursor};
use async_trait::async_trait;
use futures_core::Stream;

#[derive(Debug, Clone)]
pub struct SourceDescriptor {
    pub id: String,
    pub adapter_type: String,
    pub has_cursor: bool,
    pub partitions: Option<u32>,
}

/// 发现的字段（Schema 推断）
#[derive(Debug, Clone, serde::Serialize)]
pub struct DiscoveredField {
    pub name: String,
    pub inferred_type: String,       // "string" | "number" | "boolean" | "date"
    pub sample_values: Vec<String>,  // 前 3 个样本值
    pub nullable: bool,
}

/// discover_schema 返回
#[derive(Debug, Clone, serde::Serialize)]
pub struct DiscoveredSchema {
    pub fields: Vec<DiscoveredField>,
    pub record_count_hint: Option<u64>,
    pub sample_records: Vec<serde_json::Value>,  // 前 3 行
}

#[async_trait]
pub trait SourceAdapter: Send + Sync {
    fn id(&self) -> &str;

    /// adapter 类型标识："csv" | "json" | "sql" | "rest"
    fn adapter_type(&self) -> &'static str { "unknown" }

    async fn describe(&self) -> SourceDescriptor;

    /// 连通性测试（保存前验证）→ Ok(info_msg) or Err
    async fn test_connection(&self) -> Result<String, AdapterError> {
        Ok("OK".to_string())
    }

    /// 自动推断字段 Schema（前 5 行采样）
    async fn discover_schema(&self) -> Result<DiscoveredSchema, AdapterError> {
        Ok(DiscoveredSchema { fields: vec![], record_count_hint: None, sample_records: vec![] })
    }

    /// 异步拉取前 N 条原始 JSON 记录，用于预览（不经过 block_on）
    async fn fetch_preview(&self, limit: usize) -> Result<Vec<serde_json::Value>, AdapterError> {
        Ok(vec![])
    }

    fn stream(
        &self,
        since: Option<Cursor>,
    ) -> Box<dyn Stream<Item = Result<CanonicalRecord, AdapterError>> + Unpin + Send>;
}

// ── 工具函数：从 JSON Value 推断字段类型 ──────────────────────────────────────
pub fn infer_type(v: &serde_json::Value) -> &'static str {
    match v {
        serde_json::Value::Number(_) => "number",
        serde_json::Value::Bool(_)   => "boolean",
        serde_json::Value::String(s) => {
            // 简单日期检测
            if s.len() >= 10 && s.chars().nth(4) == Some('-') { "date" } else { "string" }
        }
        _ => "string",
    }
}

pub fn discover_from_records(records: &[serde_json::Value]) -> DiscoveredSchema {
    use std::collections::HashMap;
    let mut field_samples: HashMap<String, Vec<String>> = HashMap::new();
    let mut field_types:   HashMap<String, String>      = HashMap::new();
    let mut nullable:      HashMap<String, bool>        = HashMap::new();

    for rec in records {
        if let serde_json::Value::Object(map) = rec {
            for (k, v) in map {
                let ty = infer_type(v).to_string();
                field_types.entry(k.clone()).or_insert_with(|| ty.clone());
                let sample = field_samples.entry(k.clone()).or_default();
                if sample.len() < 3 {
                    let s = match v {
                        serde_json::Value::Null => String::new(),
                        _                       => v.to_string().trim_matches('"').to_string(),
                    };
                    if !s.is_empty() { sample.push(s); }
                }
                if matches!(v, serde_json::Value::Null) {
                    *nullable.entry(k.clone()).or_insert(false) = true;
                }
            }
        }
    }

    let fields = field_types.into_iter().map(|(name, inferred_type)| DiscoveredField {
        nullable: *nullable.get(&name).unwrap_or(&false),
        sample_values: field_samples.remove(&name).unwrap_or_default(),
        name, inferred_type,
    }).collect();

    DiscoveredSchema {
        fields,
        record_count_hint: Some(records.len() as u64),
        sample_records: records.iter().take(3).cloned().collect(),
    }
}
