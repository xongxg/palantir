use crate::adapters::{SourceAdapter, SourceDescriptor};
use crate::errors::AdapterError;
use crate::model::{CanonicalRecord, Cursor};
use async_trait::async_trait;
use futures_core::Stream;
use futures_util::stream;
use std::path::PathBuf;
use time::OffsetDateTime;

#[derive(Clone)]
pub struct CsvAdapter {
    id: String,
    path: PathBuf,
    ns: String,
    schema: String,
}

impl CsvAdapter {
    pub fn new(
        id: impl Into<String>,
        path: impl Into<PathBuf>,
        ns: impl Into<String>,
        schema: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            path: path.into(),
            ns: ns.into(),
            schema: schema.into(),
        }
    }
}

#[async_trait]
impl SourceAdapter for CsvAdapter {
    fn id(&self) -> &str {
        &self.id
    }

    fn adapter_type(&self) -> &'static str { "csv" }

    async fn describe(&self) -> SourceDescriptor {
        SourceDescriptor {
            id: self.id.clone(),
            adapter_type: "csv".to_string(),
            has_cursor: false,
            partitions: None,
        }
    }

    async fn test_connection(&self) -> Result<String, AdapterError> {
        if self.path.exists() {
            Ok(format!("File found: {}", self.path.display()))
        } else {
            Err(AdapterError::Message(format!("File not found: {}", self.path.display())))
        }
    }

    async fn fetch_preview(&self, limit: usize) -> Result<Vec<serde_json::Value>, AdapterError> {
        let mut rdr = csv::Reader::from_path(&self.path)
            .map_err(|e| AdapterError::Message(e.to_string()))?;
        let headers = rdr.headers()
            .map_err(|e| AdapterError::Message(e.to_string()))?.clone();
        let mut records = vec![];
        for row in rdr.records().take(limit) {
            let row = row.map_err(|e| AdapterError::Message(e.to_string()))?;
            let mut obj = serde_json::Map::new();
            for (i, v) in row.iter().enumerate() {
                if let Some(h) = headers.get(i) {
                    obj.insert(h.to_string(), serde_json::Value::String(v.to_string()));
                }
            }
            records.push(serde_json::Value::Object(obj));
        }
        Ok(records)
    }

    async fn discover_schema(&self) -> Result<crate::adapters::DiscoveredSchema, AdapterError> {
        let mut rdr = csv::Reader::from_path(&self.path)
            .map_err(|e| AdapterError::Message(e.to_string()))?;
        let headers = rdr.headers()
            .map_err(|e| AdapterError::Message(e.to_string()))?.clone();
        let mut records = vec![];
        for row in rdr.records().take(5) {
            let row = row.map_err(|e| AdapterError::Message(e.to_string()))?;
            let mut obj = serde_json::Map::new();
            for (i, v) in row.iter().enumerate() {
                if let Some(h) = headers.get(i) {
                    obj.insert(h.to_string(), serde_json::Value::String(v.to_string()));
                }
            }
            records.push(serde_json::Value::Object(obj));
        }
        Ok(crate::adapters::discover_from_records(&records))
    }

    fn stream(
        &self,
        _since: Option<Cursor>,
    ) -> Box<dyn Stream<Item = Result<CanonicalRecord, AdapterError>> + Unpin + Send> {
        let path = self.path.clone();
        let ns = self.ns.clone();
        let schema = self.schema.clone();
        let items: Vec<Result<CanonicalRecord, AdapterError>> = (|| {
            let mut rdr = csv::Reader::from_path(&path)
                .map_err(|e| AdapterError::Message(format!("csv open error: {}", e)))?;
            let headers = rdr
                .headers()
                .map(|h| h.clone())
                .map_err(|e| AdapterError::Message(e.to_string()))?;
            let mut out = Vec::new();
            for rec in rdr.records() {
                let rec = rec.map_err(|e| AdapterError::Message(e.to_string()))?;
                let mut obj = serde_json::Map::new();
                for (i, val) in rec.iter().enumerate() {
                    let key = headers.get(i).unwrap_or("");
                    obj.insert(key.to_string(), serde_json::Value::String(val.to_string()));
                }
                out.push(Ok(CanonicalRecord {
                    source: self.id.clone(),
                    ns: ns.clone(),
                    schema: schema.clone(),
                    payload: serde_json::Value::Object(obj),
                    ts: OffsetDateTime::now_utc(),
                    cursor: None,
                }));
            }
            Ok::<_, AdapterError>(out)
        })()
        .unwrap_or_else(|e| vec![Err(e)]);
        Box::new(stream::iter(items))
    }
}
