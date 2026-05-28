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

        if uri.is_file() {
            tokio::fs::remove_file(&uri)
                .await
                .with_context(|| format!("Can not remove file: {}", uri.display()))
        } else {
            tokio::fs::remove_dir_all(&uri)
                .await
                .with_context(|| format!("Can not remove directory: {}", uri.display()))
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    use url::Url;

    #[tokio::test]
    async fn test_windows_path_handling_upload() {
        let source_dir = tempdir().unwrap();
        let dest_dir = tempdir().unwrap();

        let source_file = source_dir.path().join("test_file.txt");
        let dest_file = dest_dir.path().join("copied_file.txt");

        std::fs::write(&source_file, b"test content").unwrap();

        let url = Url::from_file_path(&dest_file)
            .expect("Failed to create file URL from path");

        let storage = LocalStorage {};
        storage
            .upload(&source_file, &url)
            .await
            .expect("Upload failed");

        assert!(dest_file.exists());
        let content = std::fs::read_to_string(&dest_file).unwrap();
        assert_eq!(content, "test content");
    }

    #[tokio::test]
    async fn test_windows_path_handling_download() {
        let source_dir = tempdir().unwrap();
        let dest_dir = tempdir().unwrap();

        let source_file = source_dir.path().join("test_file.txt");
        let dest_file = dest_dir.path().join("copied_file.txt");

        std::fs::write(&source_file, b"test content from download").unwrap();

        let url = Url::from_file_path(&source_file)
            .expect("Failed to create file URL from path");

        let storage = LocalStorage {};
        storage
            .download(&url, &dest_file)
            .await
            .expect("Download failed");

        assert!(dest_file.exists());
        let content = std::fs::read_to_string(&dest_file).unwrap();
        assert_eq!(content, "test content from download");
    }

    #[tokio::test]
    async fn test_windows_path_handling_directory_upload() {
        let source_dir = tempdir().unwrap();
        let dest_base = tempdir().unwrap();

        let source_subdir = source_dir.path().join("subdir");
        std::fs::create_dir(&source_subdir).unwrap();
        std::fs::write(source_subdir.join("file1.txt"), b"content1").unwrap();
        std::fs::write(source_subdir.join("file2.txt"), b"content2").unwrap();

        let dest_subdir = dest_base.path().join("subdir");
        let url = Url::from_file_path(&dest_subdir)
            .expect("Failed to create file URL from path");

        let storage = LocalStorage {};
        storage
            .upload(&source_subdir, &url)
            .await
            .expect("Directory upload failed");

        assert!(dest_subdir.exists());
        assert!(dest_subdir.join("file1.txt").exists());
        assert!(dest_subdir.join("file2.txt").exists());
        let content1 = std::fs::read_to_string(dest_subdir.join("file1.txt")).unwrap();
        let content2 = std::fs::read_to_string(dest_subdir.join("file2.txt")).unwrap();
        assert_eq!(content1, "content1");
        assert_eq!(content2, "content2");
    }

    #[tokio::test]
    async fn test_windows_path_handling_read_file() {
        let source_dir = tempdir().unwrap();
        let source_file = source_dir.path().join("test_file.txt");
        std::fs::write(&source_file, b"content to read").unwrap();

        let url = Url::from_file_path(&source_file)
            .expect("Failed to create file URL from path");

        let storage = LocalStorage {};
        let content = storage
            .read_file(&url)
            .await
            .expect("read_file failed");

        assert_eq!(content, "content to read");
    }

    #[tokio::test]
    async fn test_windows_path_handling_exists() {
        let source_dir = tempdir().unwrap();
        let source_file = source_dir.path().join("test_file.txt");
        std::fs::write(&source_file, b"content").unwrap();

        let url = Url::from_file_path(&source_file)
            .expect("Failed to create file URL from path");

        let storage = LocalStorage {};
        let exists = storage
            .exists(&url)
            .await
            .expect("exists check failed");

        assert!(exists);

        let non_existent = source_dir.path().join("does_not_exist.txt");
        let url_non_existent = Url::from_file_path(&non_existent)
            .expect("Failed to create file URL from path");

        let exists_non_existent = storage
            .exists(&url_non_existent)
            .await
            .expect("exists check failed");

        assert!(!exists_non_existent);
    }

    #[tokio::test]
    async fn test_windows_path_handling_delete() {
        let _source_dir = tempdir().unwrap();
        let dest_dir = tempdir().unwrap();
        let file_to_delete = dest_dir.path().join("to_delete.txt");
        std::fs::write(&file_to_delete, b"content").unwrap();
        assert!(file_to_delete.exists());

        let url = Url::from_file_path(&file_to_delete)
            .expect("Failed to create file URL from path");

        let storage = LocalStorage {};
        storage.delete(&url).await.expect("delete failed");

        assert!(!file_to_delete.exists());
    }
}
