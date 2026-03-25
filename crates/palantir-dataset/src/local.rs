use crate::backend::StorageBackend;
use anyhow::{Context, Result};
use async_trait::async_trait;
use bytes::Bytes;
use std::path::{Path, PathBuf};
use tokio::fs;

/// Local filesystem backend.
///
/// Writes to `{root}/{path}` using temp-rename for atomic puts.
pub struct LocalFsBackend {
    root: PathBuf,
}

impl LocalFsBackend {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    fn abs(&self, path: &str) -> PathBuf {
        self.root.join(path)
    }
}

#[async_trait]
impl StorageBackend for LocalFsBackend {
    async fn put(&self, path: &str, data: Bytes) -> Result<()> {
        let dest = self.abs(path);
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)
                .await
                .with_context(|| format!("create_dir_all {}", parent.display()))?;
        }
        // Write to temp file then rename for atomicity
        let tmp = dest.with_extension("tmp");
        fs::write(&tmp, &data)
            .await
            .with_context(|| format!("write tmp {}", tmp.display()))?;
        fs::rename(&tmp, &dest)
            .await
            .with_context(|| format!("rename {} → {}", tmp.display(), dest.display()))?;
        Ok(())
    }

    async fn get(&self, path: &str) -> Result<Bytes> {
        let src = self.abs(path);
        let data = fs::read(&src)
            .await
            .with_context(|| format!("read {}", src.display()))?;
        Ok(Bytes::from(data))
    }

    async fn exists(&self, path: &str) -> Result<bool> {
        Ok(self.abs(path).exists())
    }

    async fn list(&self, prefix: &str) -> Result<Vec<String>> {
        let dir = self.abs(prefix);
        if !dir.exists() {
            return Ok(vec![]);
        }
        let mut result = Vec::new();
        collect_files(&dir, &self.root, &mut result).await?;
        Ok(result)
    }

    async fn delete(&self, path: &str) -> Result<()> {
        let p = self.abs(path);
        if p.exists() {
            fs::remove_file(&p)
                .await
                .with_context(|| format!("remove {}", p.display()))?;
        }
        Ok(())
    }

    async fn delete_prefix(&self, prefix: &str) -> Result<u64> {
        let dir = self.abs(prefix);
        if !dir.exists() {
            return Ok(0);
        }
        let mut paths = Vec::new();
        collect_files(&dir, &self.root, &mut paths).await?;
        let count = paths.len() as u64;
        fs::remove_dir_all(&dir).await.ok();
        Ok(count)
    }
}

async fn collect_files(dir: &Path, root: &Path, out: &mut Vec<String>) -> Result<()> {
    let mut rd = fs::read_dir(dir).await?;
    while let Some(entry) = rd.next_entry().await? {
        let path = entry.path();
        if path.is_dir() {
            Box::pin(collect_files(&path, root, out)).await?;
        } else {
            // Return path relative to root
            if let Ok(rel) = path.strip_prefix(root) {
                out.push(rel.to_string_lossy().to_string());
            }
        }
    }
    Ok(())
}
