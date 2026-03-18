use crate::Storage;
use anyhow::{Context, ensure};
use async_trait::async_trait;
use dircpy::copy_dir;
use std::path::Path;
use url::Url;

#[derive(Default, Debug)]
pub struct LocalStorage;

#[async_trait]
impl Storage for LocalStorage {
    async fn upload(&self, local: &Path, dest: &Url) -> anyhow::Result<()> {
        ensure!(dest.scheme() == "file");
        let dest = Path::new(dest.path());
        if local.is_file() {
            tokio::fs::copy(local, dest).await.with_context(|| {
                format!(
                    "Could not copy from {} to {}",
                    local.display(),
                    dest.display()
                )
            })?;
        } else {
            copy_dir(local, dest).with_context(|| {
                format!(
                    "Could not copy from {} to {}",
                    local.display(),
                    dest.display()
                )
            })?;
        }
        Ok(())
    }

    async fn download(&self, src: &Url, local: &Path) -> anyhow::Result<()> {
        ensure!(src.scheme() == "file");
        let src = Path::new(src.path());
        if src.is_file() {
            tokio::fs::copy(src, local).await.with_context(|| {
                format!(
                    "Could not copy from {} to {}",
                    src.display(),
                    local.display()
                )
            })?;
        } else {
            copy_dir(src, local).with_context(|| {
                format!(
                    "Could not copy from {} to {}",
                    src.display(),
                    local.display()
                )
            })?;
        }
        Ok(())
    }

    async fn exists(&self, uri: &Url) -> anyhow::Result<bool> {
        let uri = Path::new(uri.path());
        Ok(tokio::fs::try_exists(uri).await?)
    }

    async fn delete(&self, uri: &Url) -> anyhow::Result<()> {
        ensure!(uri.scheme() == "file");
        let uri = Path::new(uri.path());

        tokio::fs::remove_dir_all(uri)
            .await
            .with_context(|| format!("Can not remove directory: {uri:?}"))
    }
}
