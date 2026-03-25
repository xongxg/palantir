use crate::backend::StorageBackend;
use crate::manifest::{DatasetManifest, DatasetSchema, FileEntry};
use anyhow::Result;
use bytes::Bytes;
use sha2::{Digest, Sha256};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

const ROWS_PER_PART: usize = 50_000;

/// Collects records and writes them as CSV part files under
/// `{prefix}/{dataset_id}/v{version}/data/part-NNNNN.csv`.
///
/// Call `append_records()` any number of times, then `commit()` to finalise.
pub struct DatasetWriter {
    backend:    Arc<dyn StorageBackend>,
    base_prefix: String,  // e.g. "{dataset_id}/v{version}"
    dataset_id: String,
    version:    i64,
    run_id:     String,
    headers:    Option<Vec<String>>,
    part_buf:   Vec<serde_json::Value>,
    parts:      Vec<FileEntry>,
    part_idx:   usize,
    total_rows: u64,
}

impl DatasetWriter {
    pub(crate) fn new(
        backend: Arc<dyn StorageBackend>,
        base_prefix: String,
        dataset_id: String,
        version: i64,
        run_id: String,
    ) -> Self {
        Self {
            backend,
            base_prefix,
            dataset_id,
            version,
            run_id,
            headers: None,
            part_buf: Vec::new(),
            parts: Vec::new(),
            part_idx: 0,
            total_rows: 0,
        }
    }

    /// Append a batch of JSON object records.
    pub async fn append_records(&mut self, records: &[serde_json::Value]) -> Result<()> {
        // Infer headers from first non-empty batch
        if self.headers.is_none() {
            let first = records.iter().find(|r| r.is_object());
            if let Some(r) = first {
                self.headers = Some(r.as_object().unwrap().keys().cloned().collect());
            }
        }
        for rec in records {
            self.part_buf.push(rec.clone());
            if self.part_buf.len() >= ROWS_PER_PART {
                self.flush_part().await?;
            }
        }
        Ok(())
    }

    async fn flush_part(&mut self) -> Result<()> {
        if self.part_buf.is_empty() { return Ok(()); }
        let headers = self.headers.clone().unwrap_or_default();
        let rows = self.part_buf.len();

        // Build CSV bytes
        let mut wtr = csv::Writer::from_writer(vec![]);
        wtr.write_record(&headers)?;
        for rec in &self.part_buf {
            let row: Vec<String> = headers.iter().map(|h| {
                match &rec[h] {
                    serde_json::Value::String(s) => s.clone(),
                    serde_json::Value::Null       => String::new(),
                    v => v.to_string(),
                }
            }).collect();
            wtr.write_record(&row)?;
        }
        let csv_bytes = Bytes::from(wtr.into_inner()?);
        let byte_len = csv_bytes.len() as u64;

        // SHA-256
        let digest = Sha256::digest(&csv_bytes);
        let sha = hex::encode(digest);

        // Path: data/part-00000.csv  (relative to base_prefix)
        let rel_path = format!("data/part-{:05}.csv", self.part_idx);
        let full_path = format!("{}/{}", self.base_prefix, rel_path);
        self.backend.put(&full_path, csv_bytes).await?;

        self.parts.push(FileEntry {
            path: rel_path,
            sha256: sha,
            rows: rows as u64,
            bytes: byte_len,
        });
        self.total_rows += rows as u64;
        self.part_idx += 1;
        self.part_buf.clear();
        Ok(())
    }

    /// Finalise: flush remaining records, write manifest.json, return manifest.
    pub async fn commit(mut self, schema: DatasetSchema) -> Result<DatasetManifest> {
        self.flush_part().await?;

        let total_bytes: u64 = self.parts.iter().map(|p| p.bytes).sum();

        // content_hash = SHA-256 of concatenated per-file sha256 strings
        let mut hasher = Sha256::new();
        for p in &self.parts {
            hasher.update(p.sha256.as_bytes());
        }
        let content_hash = hex::encode(hasher.finalize());

        let created_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        let manifest = DatasetManifest {
            dataset_id:   self.dataset_id.clone(),
            version:      self.version,
            sync_run_id:  self.run_id.clone(),
            created_at,
            schema,
            files:        self.parts,
            total_rows:   self.total_rows,
            total_bytes,
            content_hash,
        };

        let manifest_json = serde_json::to_vec_pretty(&manifest)?;
        let manifest_path = format!("{}/manifest.json", self.base_prefix);
        self.backend.put(&manifest_path, Bytes::from(manifest_json)).await?;

        Ok(manifest)
    }

    /// Abort: delete all part files written so far (best-effort GC).
    pub async fn abort(self) -> Result<()> {
        let _ = self.backend.delete_prefix(&self.base_prefix).await;
        Ok(())
    }
}
