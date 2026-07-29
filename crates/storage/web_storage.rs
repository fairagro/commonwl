use crate::{Storage, StoragePath};
use async_trait::async_trait;
use std::{path::Path, sync::Arc};
use tokio::sync::OnceCell;
use url::Url;

#[derive(Debug, Clone)]
pub struct WebStorage {
    client: OnceCell<Arc<reqwest::Client>>,
}

impl WebStorage {
    #[must_use]
    pub fn new() -> Self {
        Self {
            client: OnceCell::new(),
        }
    }

    async fn client(&self) -> anyhow::Result<Arc<reqwest::Client>> {
        self.client
            .get_or_try_init(|| async { Ok(Arc::new(reqwest::Client::new())) })
            .await
            .cloned()
    }
}
impl Default for WebStorage {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Storage for WebStorage {
    async fn upload(&self, _local: &Path, dest: &Url) -> anyhow::Result<()> {
        anyhow::bail!("WebStorage does not support upload ({dest})")
    }

    async fn download(&self, src: &Url, local: &Path) -> anyhow::Result<()> {
        let bytes = self
            .client()
            .await?
            .get(src.as_str())
            .send()
            .await?
            .error_for_status()
            .map_err(|e| anyhow::anyhow!("HTTP error downloading {src}: {e}"))?
            .bytes()
            .await?;

        if let Some(parent) = local.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        tokio::fs::write(local, &bytes)
            .await
            .map_err(|e| anyhow::anyhow!("Could not write to {}: {e}", local.display()))
    }

    async fn exists(&self, uri: &Url) -> anyhow::Result<bool> {
        let status = self
            .client()
            .await?
            .head(uri.as_str())
            .send()
            .await?
            .status();

        Ok(status.is_success())
    }

    async fn is_dir(&self, uri: &Url) -> anyhow::Result<bool> {
        // plain HTTP(S) has no directory metadata to query; treat a trailing '/' as the
        // conventional directory marker, same as directory URLs built elsewhere (e.g. S3Storage)
        Ok(uri.path().ends_with('/'))
    }

    async fn delete(&self, uri: &Url) -> anyhow::Result<()> {
        anyhow::bail!("WebStorage does not support delete ({uri})")
    }

    async fn read_file(&self, uri: &Url) -> anyhow::Result<String> {
        self.client()
            .await?
            .get(uri.as_str())
            .send()
            .await?
            .error_for_status()
            .map_err(|e| anyhow::anyhow!("HTTP error reading {uri}: {e}"))?
            .text()
            .await
            .map_err(|e| anyhow::anyhow!("Could not decode response from {uri}: {e}"))
    }

    async fn glob(
        &self,
        base: &Url,
        pattern: &str,
    ) -> anyhow::Result<Box<dyn Iterator<Item = StoragePath> + Send>> {
        anyhow::bail!("WebStorage does not support glob ({base}, {pattern})")
    }
}
