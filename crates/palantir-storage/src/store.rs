use crate::backend::StorageBackend;
use crate::manifest::DatasetManifest;
use crate::writer::DatasetWriter;
use anyhow::Result;
use std::sync::Arc;

/// Version-aware dataset store built on top of a StorageBackend.
///
/// Path convention (Iter-1, no tenant prefix):
///   `{root_prefix}/{dataset_id}/v{version}/`
///     manifest.json
///     data/part-00000.csv
///     data/part-00001.csv
///     ...
pub struct DatasetStore {
    backend:     Arc<dyn StorageBackend>,
    root_prefix: String,
}

impl DatasetStore {
    pub fn new(backend: Arc<dyn StorageBackend>, root_prefix: impl Into<String>) -> Self {
        Self { backend, root_prefix: root_prefix.into() }
    }

    fn version_prefix(&self, dataset_id: &str, version: i64) -> String {
        let rp = self.root_prefix.trim_end_matches('/');
        if rp.is_empty() {
            format!("{}/v{}", dataset_id, version)
        } else {
            format!("{}/{}/v{}", rp, dataset_id, version)
        }
    }

    /// Begin writing a new dataset version. Returns a DatasetWriter.
    pub fn begin_write(
        &self,
        dataset_id: &str,
        version: i64,
        sync_run_id: &str,
    ) -> DatasetWriter {
        DatasetWriter::new(
            Arc::clone(&self.backend),
            self.version_prefix(dataset_id, version),
            dataset_id.to_string(),
            version,
            sync_run_id.to_string(),
        )
    }

    /// Read the manifest for a committed version.
    pub async fn read_manifest(&self, dataset_id: &str, version: i64) -> Result<DatasetManifest> {
        let path = format!("{}/manifest.json", self.version_prefix(dataset_id, version));
        let bytes = self.backend.get(&path).await?;
        Ok(serde_json::from_slice(&bytes)?)
    }

    /// Delete all files under a version prefix. Returns bytes-deleted count.
    pub async fn delete_version(&self, dataset_id: &str, version: i64) -> Result<u64> {
        let prefix = self.version_prefix(dataset_id, version);
        self.backend.delete_prefix(&prefix).await
    }
}
