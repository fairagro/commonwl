use crate::{local_storage::LocalStorage, s3_storage::S3Storage};
use async_trait::async_trait;
use std::{collections::HashMap, path::Path};
use tempfile::NamedTempFile;
use url::Url;

pub mod local_storage;
pub mod s3_storage;

#[async_trait]
pub trait Storage: Send + Sync + std::fmt::Debug {
    async fn upload(&self, local: &Path, dest: &Url) -> anyhow::Result<()>;
    async fn download(&self, src: &Url, local: &Path) -> anyhow::Result<()>;
    async fn exists(&self, uri: &Url) -> anyhow::Result<bool>;
}

#[derive(Debug)]
pub struct StorageBackend {
    inner: HashMap<String, Box<dyn Storage>>,
}

impl StorageBackend {
    #[must_use]
    pub fn new() -> Self {
        let mut backends: HashMap<String, Box<dyn Storage>> = HashMap::new();
        backends.insert("file".to_string(), Box::new(LocalStorage {}));
        backends.insert("s3".to_string(), Box::new(S3Storage::new()));
        Self { inner: backends }
    }

    /// Uploads a file by its contents as byte slice
    /// # Errors
    /// Fails if tempfile can not be written or uploaded
    pub async fn upload_bytes(&self, data: &[u8], dest: &Url) -> anyhow::Result<()> {
        let mut tmp = NamedTempFile::new()?;
        std::io::Write::write_all(&mut tmp, data)?;
        self.upload(tmp.path(), dest).await
    }
}

impl Default for StorageBackend {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Storage for StorageBackend {
    async fn upload(&self, local: &Path, dest: &Url) -> anyhow::Result<()> {
        self.inner
            .get(dest.scheme())
            .ok_or(anyhow::anyhow!("Could not find matching storage backend"))?
            .upload(local, dest)
            .await
    }

    async fn download(&self, src: &Url, local: &Path) -> anyhow::Result<()> {
        self.inner
            .get(src.scheme())
            .ok_or(anyhow::anyhow!("Could not find matching storage backend"))?
            .download(src, local)
            .await
    }

    async fn exists(&self, uri: &Url) -> anyhow::Result<bool> {
        self.inner
            .get(uri.scheme())
            .ok_or(anyhow::anyhow!("Could not find matching storage backend"))?
            .exists(uri)
            .await
    }
}
