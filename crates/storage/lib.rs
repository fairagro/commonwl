use crate::{local_storage::LocalStorage, s3_storage::S3Storage};
use async_trait::async_trait;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};
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

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum StoragePath {
    Local(PathBuf),
    Remote(Url),
}

impl StoragePath {
    pub fn from_url(url: Url) -> Self {
        if url.scheme() == "file"
            && let Ok(path) = url.to_file_path()
        {
            return Self::Local(path);
        }

        Self::Remote(url)
    }

    pub fn from_local(path: &Path) -> Self {
        Self::Local(path.to_path_buf())
    }

    pub fn is_local(&self) -> bool {
        matches!(self, Self::Local(_)) || matches!(self, Self::Remote(r) if r.scheme() == "file")
    }

    pub fn as_local_path(&self) -> anyhow::Result<PathBuf> {
        if let Self::Local(path) = self {
            Ok(path.clone())
        } else if let Self::Remote(url) = self
            && url.scheme() == "file"
        {
            url.to_file_path()
                .map_err(|_| anyhow::anyhow!("Not a local path: {}", url))
        } else {
            anyhow::bail!("URL {self:?} is not local!")
        }
    }

    pub fn as_url(&self) -> anyhow::Result<Url> {
        match self {
            Self::Remote(url) => Ok(url.clone()),
            Self::Local(path) => Url::from_file_path(path)
                .map_err(|_| anyhow::anyhow!("Could not convert path to URL: {}", path.display())),
        }
    }

    pub fn join(&self, segment: &str) -> anyhow::Result<Self> {
        match self {
            Self::Local(path) => Ok(Self::Local(path.join(segment))),
            Self::Remote(url) => {
                let base = if url.path().ends_with("/") {
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
