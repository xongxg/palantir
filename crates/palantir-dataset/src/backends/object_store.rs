//! ObjectStoreBackend — byte-level object storage.
//!
//! Concrete implementations: LocalFsBackend (Iter-1), S3Backend (Iter-2),
//! RustFsBackend (Iter-2 variant), AzureBlobBackend / GcsBackend (future).
//!
//! This is the ONLY backend with a real implementation today.
//! All other backends in this module are stubs.

use anyhow::Result;
use async_trait::async_trait;
use bytes::Bytes;

/// Low-level object/file storage — aware of bytes, not dataset semantics.
/// DatasetStore / DatasetWriter sit on top of this trait.
#[async_trait]
pub trait ObjectStoreBackend: Send + Sync {
    async fn put(&self, path: &str, data: Bytes) -> Result<()>;
    async fn get(&self, path: &str) -> Result<Bytes>;
    async fn exists(&self, path: &str) -> Result<bool>;
    async fn list(&self, prefix: &str) -> Result<Vec<String>>;
    async fn delete(&self, path: &str) -> Result<()>;
    /// Delete all objects under prefix; returns count deleted.
    async fn delete_prefix(&self, prefix: &str) -> Result<u64>;
}
