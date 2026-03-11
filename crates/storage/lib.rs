use async_trait::async_trait;
use std::path::Path;
use url::Url;

use crate::{local_storage::LocalStorage, s3_storage::S3Storage};

pub mod local_storage;
pub mod s3_storage;

#[async_trait]
pub trait Storage {
    async fn upload(&self, local: &Path, dest: &str) -> anyhow::Result<()>;
    async fn download(&self, src: &str, local: &Path) -> anyhow::Result<()>;
    async fn exists(&self, uri: &str) -> anyhow::Result<bool>;
}

pub enum StorageBackend {
    Local(LocalStorage),
    S3(S3Storage),
}

impl StorageBackend {
    pub async fn from_uri(uri: &Url) -> Self {
        if uri.scheme() == "file" {
            StorageBackend::Local(LocalStorage)
        } else if uri.scheme() == "s3" {
            StorageBackend::S3(S3Storage::new().await)
        } else {
            panic!("Unsupported Storage Backend")
        }
    }
}

#[async_trait]
impl Storage for StorageBackend {
    async fn upload(&self, local: &Path, dest: &str) -> anyhow::Result<()> {
        match self {
            Self::Local(local_storage) => local_storage.upload(local, dest).await,
            Self::S3(s3_storage) => s3_storage.upload(local, dest).await,
        }
    }

    async fn download(&self, src: &str, local: &Path) -> anyhow::Result<()> {
        match self {
            Self::Local(local_storage) => local_storage.download(src, local).await,
            Self::S3(s3_storage) => s3_storage.download(src, local).await,
        }
    }

    async fn exists(&self, uri: &str) -> anyhow::Result<bool> {
        match self {
            Self::Local(local_storage) => local_storage.exists(uri).await,
            Self::S3(s3_storage) => s3_storage.exists(uri).await,
        }
    }
}
