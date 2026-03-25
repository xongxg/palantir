use crate::backend::StorageBackend;
use anyhow::{Context, Result, anyhow};
use async_trait::async_trait;
use bytes::Bytes;
use futures_util::TryStreamExt;
use object_store::{ObjectStore, aws::AmazonS3Builder, path::Path as OsPath};

/// S3-compatible storage backend (AWS S3 / Alibaba OSS / MinIO / RustFS).
///
/// Configured at construction time; each DataSource can have its own instance
/// pointing to the customer's own bucket.
pub struct S3Backend {
    store:  Box<dyn ObjectStore>,
    bucket: String,
}

impl S3Backend {
    /// Build from explicit parameters.  `endpoint` may be empty for real AWS.
    pub fn new(
        endpoint:   &str,
        bucket:     &str,
        access_key: &str,
        secret_key: &str,
        region:     &str,
    ) -> Result<Self> {
        let region = if region.trim().is_empty() { "us-east-1" } else { region.trim() };

        let mut builder = AmazonS3Builder::new()
            .with_bucket_name(bucket)
            .with_access_key_id(access_key)
            .with_secret_access_key(secret_key)
            .with_region(region);

        if !endpoint.trim().is_empty() {
            builder = builder
                .with_endpoint(endpoint)
                .with_virtual_hosted_style_request(false)
                .with_allow_http(true);
        }

        let store = builder.build().context("build S3 store")?;
        Ok(Self { store: Box::new(store), bucket: bucket.to_string() })
    }

    /// Build from a JSON config (same shape as DataSource.config for S3 sources).
    pub fn from_config(cfg: &serde_json::Value) -> Result<Self> {
        let bucket   = cfg["bucket"].as_str().unwrap_or("").trim();
        let ak       = cfg["access_key"].as_str().unwrap_or("").trim();
        let sk       = cfg["secret_key"].as_str().unwrap_or("").trim();
        let endpoint = cfg["endpoint"].as_str().unwrap_or("").trim();
        let region   = cfg["region"].as_str().unwrap_or("").trim();

        if bucket.is_empty() { return Err(anyhow!("S3Backend: bucket required")); }
        if ak.is_empty()     { return Err(anyhow!("S3Backend: access_key required")); }
        if sk.is_empty()     { return Err(anyhow!("S3Backend: secret_key required")); }

        Self::new(endpoint, bucket, ak, sk, region)
    }
}

#[async_trait]
impl StorageBackend for S3Backend {
    async fn put(&self, path: &str, data: Bytes) -> Result<()> {
        let p = OsPath::from(path);
        self.store.put(&p, data).await.with_context(|| format!("S3 put {path}"))?;
        Ok(())
    }

    async fn get(&self, path: &str) -> Result<Bytes> {
        let p = OsPath::from(path);
        let result = self.store.get(&p).await.with_context(|| format!("S3 get {path}"))?;
        result.bytes().await.context("S3 read bytes")
    }

    async fn exists(&self, path: &str) -> Result<bool> {
        let p = OsPath::from(path);
        match self.store.head(&p).await {
            Ok(_)  => Ok(true),
            Err(object_store::Error::NotFound { .. }) => Ok(false),
            Err(e) => Err(e.into()),
        }
    }

    async fn list(&self, prefix: &str) -> Result<Vec<String>> {
        let stream = if prefix.is_empty() {
            self.store.list(None)
        } else {
            self.store.list(Some(&OsPath::from(prefix)))
        };
        let metas: Vec<_> = stream.try_collect().await?;
        Ok(metas.into_iter().map(|m| m.location.to_string()).collect())
    }

    async fn delete(&self, path: &str) -> Result<()> {
        let p = OsPath::from(path);
        self.store.delete(&p).await.with_context(|| format!("S3 delete {path}"))?;
        Ok(())
    }

    async fn delete_prefix(&self, prefix: &str) -> Result<u64> {
        let paths: Vec<_> = self.store
            .list(Some(&OsPath::from(prefix)))
            .try_collect::<Vec<_>>()
            .await?
            .into_iter()
            .map(|m| m.location)
            .collect();
        let count = paths.len() as u64;
        for p in paths {
            let _ = self.store.delete(&p).await;
        }
        Ok(count)
    }
}

impl std::fmt::Debug for S3Backend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "S3Backend(bucket={})", self.bucket)
    }
}
