use serde::{Deserialize, Serialize};

/// Manifest written at `{dataset_id}/v{version}/manifest.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatasetManifest {
    pub dataset_id:   String,
    pub version:      i64,
    pub sync_run_id:  String,
    pub created_at:   u64,           // Unix timestamp (seconds)
    pub schema:       DatasetSchema,
    pub files:        Vec<FileEntry>,
    pub total_rows:   u64,
    pub total_bytes:  u64,
    /// SHA-256 of the concatenated per-file sha256 strings
    pub content_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileEntry {
    pub path:   String,   // relative: data/part-00000.csv
    pub sha256: String,
    pub rows:   u64,
    pub bytes:  u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DatasetSchema {
    pub fields: Vec<SchemaField>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchemaField {
    pub name:      String,
    pub data_type: String,   // string | integer | float | boolean | timestamp
    pub nullable:  bool,
}

impl DatasetSchema {
    /// Infer schema from a sample of JSON records.
    pub fn infer_from_records(records: &[serde_json::Value]) -> Self {
        let first = records.iter().find(|r| r.is_object());
        let fields = match first {
            None => vec![],
            Some(r) => r.as_object().unwrap().iter().map(|(k, v)| {
                let data_type = match v {
                    serde_json::Value::Number(n) if n.is_i64() => "integer",
                    serde_json::Value::Number(_)                => "float",
                    serde_json::Value::Bool(_)                  => "boolean",
                    _                                           => "string",
                };
                SchemaField { name: k.clone(), data_type: data_type.to_string(), nullable: true }
            }).collect(),
        };
        DatasetSchema { fields }
    }
}
