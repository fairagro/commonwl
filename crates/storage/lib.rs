use crate::{local_storage::LocalStorage, s3_storage::S3Storage};
use async_trait::async_trait;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Arc,
};
use tempfile::NamedTempFile;
use url::Url;
use uuid::Uuid;

pub mod local_storage;
pub mod s3_storage;

#[async_trait]
pub trait Storage: Send + Sync + std::fmt::Debug {
    async fn upload(&self, local: &Path, dest: &Url) -> anyhow::Result<()>;
    async fn download(&self, src: &Url, local: &Path) -> anyhow::Result<()>;
    async fn exists(&self, uri: &Url) -> anyhow::Result<bool>;
    async fn delete(&self, uri: &Url) -> anyhow::Result<()>;
    async fn read_file(&self, uri: &Url) -> anyhow::Result<String>;
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

    async fn delete(&self, uri: &Url) -> anyhow::Result<()> {
        self.inner
            .get(uri.scheme())
            .ok_or(anyhow::anyhow!("Could not find matching storage backend"))?
            .delete(uri)
            .await
    }

    async fn read_file(&self, uri: &Url) -> anyhow::Result<String> {
        self.inner
            .get(uri.scheme())
            .ok_or(anyhow::anyhow!("Could not find matching storage backend"))?
            .read_file(uri)
            .await
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum StoragePath {
    Local(PathBuf),
    Remote(Url),
}

impl StoragePath {
    #[must_use]
    pub fn from_url(url: Url) -> Self {
        if url.scheme() == "file"
            && let Ok(path) = url.to_file_path()
        {
            return Self::Local(path);
        }

        Self::Remote(url)
    }

    #[must_use]
    pub fn from_local(path: &Path) -> Self {
        Self::Local(path.to_path_buf())
    }

    #[must_use]
    pub fn is_local(&self) -> bool {
        matches!(self, Self::Local(_)) || matches!(self, Self::Remote(r) if r.scheme() == "file")
    }

    /// Returns an owned `PathBuf`
    /// # Errors
    /// Returns an error if either `Url::to_file_path` fails or self is not local
    pub fn as_local_path(&self) -> anyhow::Result<PathBuf> {
        if let Self::Local(path) = self {
            Ok(path.clone())
        } else if let Self::Remote(url) = self
            && url.scheme() == "file"
        {
            url.to_file_path()
                .map_err(|()| anyhow::anyhow!("Not a local path: {url}"))
        } else {
            anyhow::bail!("URL {self:?} is not local!")
        }
    }

    /// Returns an `Url` representation
    /// # Errors
    /// if `Url::from_file_path` fails
    pub fn as_url(&self) -> anyhow::Result<Url> {
        match self {
            Self::Remote(url) => Ok(url.clone()),
            Self::Local(path) => Url::from_file_path(path)
                .map_err(|()| anyhow::anyhow!("Could not convert path to URL: {}", path.display())),
        }
    }

    /// Creates an owned copy with segmnet adjoined to self.
    /// # Errors
    /// if `Url::join` fails
    pub fn join(&self, segment: &str) -> anyhow::Result<Self> {
        match self {
            Self::Local(path) => Ok(Self::Local(path.join(segment))),
            Self::Remote(url) => {
                let base = if url.path().ends_with('/') {
                    url.clone()
                } else {
                    let mut u = url.clone();
                    u.set_path(&format!("{}/", url.path()));
                    u
                };

                Ok(Self::Remote(base.join(segment)?))
            }
        }
    }
}

#[derive(Debug)]
pub struct RemoteTempDir {
    path: StoragePath,
    storage: Arc<StorageBackend>,
}
impl RemoteTempDir {
    /// creates a new tempdir at a base path
    /// # Errors
    /// if dir is local and can not be created
    pub async fn new_under(
        base: &StoragePath,
        storage: Arc<StorageBackend>,
    ) -> anyhow::Result<Self> {
        let unique = Uuid::new_v4().to_string();
        let path = base.join(&unique[..8])?;

        if path.is_local() {
            tokio::fs::create_dir_all(path.as_local_path()?).await?;
        }
        //remote typically does not know what a folder is

        Ok(Self { path, storage })
    }

    pub fn storage_path(&self) -> &StoragePath {
        &self.path
    }
}

impl Drop for RemoteTempDir {
    fn drop(&mut self) {
        let storage = self.storage.clone();
        let path = self.path.clone().as_url().ok();

        tokio::spawn(async move {
            if let Some(path) = path
                && let Err(e) = storage.delete(&path).await
            {
                tracing::warn!("Failed to clean up temp dir {path}: {e}");
            }
        });
    }
}
