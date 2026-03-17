//! Infrastructure: Data Source Adapters
//!
//! DDD Layer: Infrastructure — "driven" ports that load data from external sources.
//! The domain and application layers never know WHERE data comes from.
//! Swap CsvLoader for DatabaseLoader, KafkaLoader, RestApiLoader — zero changes above.

use std::fs;

use super::pipeline::dataset::{Dataset, Record, Value};

/// Reads a CSV file and produces a typed Dataset.
/// DDD role: Infrastructure adapter (the "driven" port in hexagonal architecture).
pub struct CsvLoader;

impl CsvLoader {
    pub fn load(path: &str, object_type: &str) -> Result<Dataset, String> {
        let content = fs::read_to_string(path)
            .map_err(|e| format!("Cannot read {}: {}", path, e))?;

        let mut lines = content.lines();

        let headers: Vec<&str> = lines
            .next()
            .ok_or("CSV has no header row")?
            .split(',')
            .collect();

        let mut dataset = Dataset::new(path, object_type);

        for (row_idx, line) in lines.enumerate() {
            if line.trim().is_empty() { continue; }

            let values: Vec<&str> = line.split(',').collect();
            if values.len() != headers.len() {
                return Err(format!(
                    "Row {} has {} columns, expected {}",
                    row_idx + 2, values.len(), headers.len()
                ));
            }

            let id = headers.iter().position(|h| *h == "id")
                .and_then(|i| values.get(i).copied())
                .unwrap_or("unknown")
                .trim()
                .to_string();

            let mut record = Record::new(id);
            for (header, raw) in headers.iter().zip(values.iter()) {
                let raw = raw.trim();
                let value = infer_value(raw);
                record.fields.insert(header.trim().to_string(), value);
            }
            dataset.push(record);
        }

        Ok(dataset)
    }
}

fn infer_value(raw: &str) -> Value {
    if let Ok(i) = raw.parse::<i64>() { return Value::Int(i); }
    if let Ok(f) = raw.parse::<f64>() { return Value::Float(f); }
    if raw == "true"  { return Value::Bool(true); }
    if raw == "false" { return Value::Bool(false); }
    Value::String(raw.to_string())
}
