use crate::Storage;
use anyhow::Context;
use async_trait::async_trait;
use std::path::Path;

pub struct LocalStorage;

#[async_trait]
impl Storage for LocalStorage {
    async fn upload(&self, local: &Path, dest: &str) -> anyhow::Result<()> {
        tokio::fs::copy(local, dest)
            .await
            .with_context(|| format!("Could not copy from {} to {dest}", local.display()))?;
        Ok(())
    }

    async fn download(&self, src: &str, local: &Path) -> anyhow::Result<()> {
        tokio::fs::copy(src, local)
            .await
            .with_context(|| format!("Could not copy from {src} to {}", local.display()))?;
        Ok(())
    }

    async fn exists(&self, uri: &str) -> anyhow::Result<bool> {
        Ok(Path::new(uri).exists())
    }
}
