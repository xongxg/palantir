use anyhow::Result;
use async_trait::async_trait;
use bytes::Bytes;

/// Pluggable storage backend — aware of bytes, not versions.
///
/// Iter-1: LocalFsBackend (local disk)
/// Iter-2: S3Backend (object_store crate, AWS/OSS/MinIO/RustFS)
#[async_trait]
pub trait StorageBackend: Send + Sync {
    async fn put(&self, path: &str, data: Bytes) -> Result<()>;
    async fn get(&self, path: &str) -> Result<Bytes>;
    async fn exists(&self, path: &str) -> Result<bool>;
    async fn list(&self, prefix: &str) -> Result<Vec<String>>;
    async fn delete(&self, path: &str) -> Result<()>;
    /// Delete all objects under prefix; returns count deleted.
    async fn delete_prefix(&self, prefix: &str) -> Result<u64>;
}
