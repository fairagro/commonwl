use crate::{Storage, StoragePath};
use anyhow::{Context, ensure};
use async_trait::async_trait;
use dircpy::copy_dir;
use glob::glob;
use std::path::{Path, PathBuf};
use url::Url;

#[derive(Default, Debug)]
pub struct LocalStorage;

#[async_trait]
impl Storage for LocalStorage {
    async fn upload(&self, local: &Path, dest: &Url) -> anyhow::Result<()> {
        ensure!(dest.scheme() == "file");
        let dest = url_to_path(dest)?;
        if local.is_file() {
            tokio::fs::copy(local, &dest).await.with_context(|| {
                format!(
                    "Could not copy from {} to {}",
                    local.display(),
                    dest.display()
                )
            })?;
        } else {
            copy_dir(local, &dest).with_context(|| {
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
        let src = url_to_path(src)?;
        let src = dunce::canonicalize(&src).unwrap_or(src); //resolve simlinks and stuff

        if src.is_file() {
            tokio::fs::copy(&src, local).await.with_context(|| {
                format!(
                    "Could not copy from {} to {}",
                    src.display(),
                    local.display()
                )
            })?;
        } else {
            copy_dir(&src, local).with_context(|| {
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
        let uri = url_to_path(uri)?;
        Ok(tokio::fs::try_exists(uri).await?)
    }

    async fn delete(&self, uri: &Url) -> anyhow::Result<()> {
        ensure!(uri.scheme() == "file");
        let uri = url_to_path(uri)?;

        tokio::fs::remove_dir_all(&uri)
            .await
            .with_context(|| format!("Can not remove directory: {}", uri.display()))
    }

    async fn read_file(&self, uri: &Url) -> anyhow::Result<String> {
        ensure!(uri.scheme() == "file");
        let uri = url_to_path(uri)?;
        tokio::fs::read_to_string(&uri)
            .await
            .with_context(|| format!("Can not read file: {}", uri.display()))
    }

    async fn glob(
        &self,
        base: &Url,
        pattern: &str,
    ) -> anyhow::Result<Box<dyn Iterator<Item = StoragePath> + Send>> {
        ensure!(base.scheme() == "file");
        let base = url_to_path(base)?;

        // WINDOWS FIX: Cast pattern to a native path to run platform-agnostic checks
        let pattern_path = Path::new(pattern);

        let full_glob = if pattern_path.is_absolute() {
            if !pattern.starts_with(&base.to_string_lossy().into_owned()) {
                anyhow::bail!("Can not access objects outside the working directory: {pattern}.");
            }
            pattern.to_string()
        } else {
            base.join(pattern).to_string_lossy().into_owned()
        };

        Ok(Box::new(
            glob(&full_glob)?
                .filter_map(Result::ok)
                .map(StoragePath::Local),
        ))
    }
}

/// Converts a `file://` URL to a `PathBuf`, handling the Windows edge case
/// where `url::Url::to_file_path` returns `/C:/...` instead of `C:/...`.
fn url_to_path(url: &Url) -> anyhow::Result<PathBuf> {
    let path = url
        .to_file_path()
        .map_err(|()| anyhow::anyhow!("Could not create file path from URL: {url}"))?;

    #[cfg(windows)]
    {
        let s = path.to_string_lossy();
        // Strip the spurious leading `/` before a Windows drive letter: /C:/... → C:/...
        if let Some(rest) = s.strip_prefix('/')
            && rest.len() >= 2
            && rest.as_bytes()[0].is_ascii_alphabetic()
            && rest.as_bytes()[1] == b':'
        {
            return Ok(PathBuf::from(rest));
        }
    }

    Ok(path)
}
