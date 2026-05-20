use base16ct::HexDisplay;
use cwl_core::{files::FileOrDirectory, inputs::DefaultValue};
use sha1::{Digest, Sha1};
use std::{
    env::current_dir,
    fs,
    io::{BufReader, Read},
    path::{Path, PathBuf},
};
use url::Url;

use crate::string_url_to_file_path;

pub(crate) mod directory;
pub(crate) mod file;
pub(crate) mod json;

const KNOWN_SCHEMES: [&str; 5] = ["file", "http", "https", "ftp", "s3"];

fn get_location(path: &str, work_dir: &Path) -> String {
    let work_dir = if work_dir.is_absolute() {
        work_dir.to_path_buf()
    } else {
        let current_dir = current_dir().unwrap();
        current_dir.join(work_dir)
    };

    let path = urlencoding::decode(path)
        .unwrap_or(std::borrow::Cow::Borrowed(path))
        .to_string();
    if KNOWN_SCHEMES
        .iter()
        .any(|scheme| path.starts_with(&format!("{scheme}://")))
    {
        return path;
    }

    let path = Path::new(&path);
    if path.is_absolute() {
        format!("file://{}", path.to_string_lossy())
    } else {
        format!("file://{}", work_dir.join(path).to_string_lossy())
    }
}

fn get_relative_path(location: &Url, work_dir: &Path) -> anyhow::Result<PathBuf> {
    let mut relative_part = location.path_segments().unwrap().next_back().unwrap();

    if location.scheme() != "file" {
        relative_part = relative_part.strip_prefix("/").unwrap_or(relative_part);
    }

    //if # is used in filename... yo! you people, don't do that!
    let relative_part = if let Some(fragment) = location.fragment() {
        &format!("{relative_part}#{fragment}")
    } else {
        relative_part
    };

    let path = Path::new(relative_part);
    let path = if path.is_absolute() {
        path.strip_prefix(work_dir)?
    } else {
        path
    };

    Ok(path.to_path_buf())
}

pub fn load_file_contents(dv: &mut DefaultValue) -> anyhow::Result<()> {
    match dv {
        DefaultValue::FileOrDirectory(FileOrDirectory::File(file)) => {
            let location = file.location.as_ref().unwrap();

            if let Some(size) = &file.size
                && size.as_i64() < 64 * 1024
            {
                file.contents = Some(fs::read_to_string(&string_url_to_file_path(location)?)?);
            } else {
                anyhow::bail!(
                    "Can not load file contents if file is larger than {} bytes.",
                    64 * 1024
                )
            }
        }
        DefaultValue::Any(serde_json::Value::Array(vec)) => {
            for item in vec {
                let mut dv: DefaultValue = serde_json::from_value(item.clone())?;
                load_file_contents(&mut dv)?;
                *item = serde_json::to_value(dv)?;
            }
        }
        DefaultValue::Any(serde_json::Value::Object(map)) => {
            for item in map.values_mut() {
                let mut dv: DefaultValue = serde_json::from_value(item.clone())?;
                load_file_contents(&mut dv)?;
                *item = serde_json::to_value(dv)?;
            }
        }
        _ => {}
    }
    Ok(())
}

pub fn unique_path(dest: &Path, file_source: Option<&String>) -> PathBuf {
    if !dest.exists() {
        return dest.to_path_buf();
    }

    if let Some(source) = file_source
        && file_checksum(dest).unwrap_or_default()
            == file_checksum(Path::new(source)).unwrap_or_default()
    {
        return dest.to_path_buf();
    }

    let stem = dest.file_stem().unwrap_or_default();
    let ext = dest.extension();
    let parent = dest.parent().unwrap();

    let mut counter = 2;
    loop {
        let new_name = match ext {
            Some(ext) => format!(
                "{}_{}.{}",
                stem.to_string_lossy(),
                counter,
                ext.to_string_lossy()
            ),
            None => format!("{}_{}", stem.to_string_lossy(), counter),
        };
        let candidate = parent.join(&new_name);
        if !candidate.exists() {
            return candidate;
        }
        counter += 1;
    }
}

pub fn checksum(str: &str) -> String {
    let mut hasher = Sha1::new();

    hasher.update(str);
    let hash = hasher.finalize();
    let dp = HexDisplay(hash.as_slice());
    format!("sha1${dp:x}")
}

fn file_checksum(path: &Path) -> std::io::Result<[u8; 20]> {
    let file = std::fs::File::open(path)?;
    let mut reader = BufReader::with_capacity(64 * 1024, file); // 64KB buffer
    let mut hasher = Sha1::new();
    let mut buf = vec![0u8; 64 * 1024];

    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }

    Ok(hasher.finalize().into())
}

pub(crate) fn normalize_url(url: &Url) -> Url {
    let mut normalized = url.clone();

    let mut stack: Vec<&str> = Vec::new();

    if let Some(segments) = url.path_segments() {
        for seg in segments {
            match seg {
                "" | "." => {}
                ".." => {
                    stack.pop();
                }
                _ => stack.push(seg),
            }
        }
    }

    let new_path = format!("/{}", stack.join("/"));
    normalized.set_path(&new_path);

    normalized
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
