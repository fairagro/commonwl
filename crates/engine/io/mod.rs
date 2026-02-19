use std::path::{Path, PathBuf};
use url::Url;

pub mod directory;
pub mod file;
pub mod json;

pub fn get_location(path: &str, work_dir: &Path) -> String {
    let path = urlencoding::decode(path)
        .unwrap_or(std::borrow::Cow::Borrowed(path))
        .to_string();
    if path.starts_with("file://") {
        return path;
    }

    let path = Path::new(&path);
    if path.is_absolute() {
        format!("file://{}", path.to_string_lossy())
    } else {
        format!("file://{}", work_dir.join(path).to_string_lossy())
    }
}

pub fn get_relative_path(location: &Url, work_dir: &Path) -> anyhow::Result<PathBuf> {
    let mut relative_part = location.path_segments().unwrap().next_back().unwrap();

    if location.scheme() != "file" {
        relative_part = relative_part.strip_prefix("/").unwrap_or(relative_part);
    }

    let path = Path::new(relative_part);
    let path = if path.is_absolute() {
        path.strip_prefix(work_dir)?
    } else {
        path
    };

    Ok(path.to_path_buf())
}

pub fn location_to_path(location: &str) -> anyhow::Result<PathBuf> {
    let url = Url::parse(location)?;
    if url.scheme() != "file" {
        anyhow::bail!("Only file URLs are supported, got: {}", location);
    }
    url.to_file_path()
        .map_err(|_| anyhow::anyhow!("Invalid file URL: {}", location))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_location() {
        let path = "/mnt/my_file.txt";
        let workdir = Path::new("/mnt");

        assert_eq!(get_location(path, workdir), "file:///mnt/my_file.txt");

        let path = "my_file.txt";
        assert_eq!(get_location(path, workdir), "file:///mnt/my_file.txt");
    }

    #[test]
    fn test_get_relative_part() {
        let path = Url::parse("file:///mnt/my_file.txt").unwrap();
        let workdir = Path::new("/mnt");
        assert_eq!(*get_relative_path(&path, workdir).unwrap(), *"my_file.txt");

        let path = Url::parse("https:///google.com/my_file.txt").unwrap();
        assert_eq!(*get_relative_path(&path, workdir).unwrap(), *"my_file.txt");
    }
}
