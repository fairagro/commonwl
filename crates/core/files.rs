use crate::{FileMetaData, FilePathMetaData, Integer, get_file_metadata, get_path_metadata};
use bon::Builder;
use serde::{Deserialize, Serialize};
use std::{fs, path::Path};

#[derive(Serialize, Deserialize, Debug, Copy, PartialEq, Hash, Clone)]
#[serde(rename_all = "snake_case")]
pub enum LoadListingEnum {
    NoListing,
    ShallowListing,
    DeepListing,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Hash, Clone)]
#[serde(tag = "class")]
pub enum FileOrDirectory {
    File(File),
    Directory(Directory),
}

impl FileOrDirectory {
    pub fn path(&self) -> Option<&String> {
        match self {
            Self::File(f) => f.path.as_ref(),
            Self::Directory(d) => d.path.as_ref(),
        }
    }

    pub fn set_path(&mut self, path: Option<String>) {
        match self {
            Self::File(f) => f.path = path,
            Self::Directory(d) => d.path = path,
        }
    }

    pub fn basename(&self) -> Option<&String> {
        match self {
            Self::File(f) => f.basename.as_ref(),
            Self::Directory(d) => d.basename.as_ref(),
        }
    }

    pub fn dry_validation(&mut self) {
        match self {
            Self::File(f) => f.dry_validation(),
            Self::Directory(d) => d.dry_validation(),
        }
    }

    pub fn is_file(&self) -> bool {
        matches!(self, FileOrDirectory::File(_))
    }

    pub fn is_dir(&self) -> bool {
        matches!(self, FileOrDirectory::Directory(_))
    }

    pub fn from_mapping(value: serde_yaml::Value) -> Self {
        serde_yaml::from_value(value).expect("class not found")
    }
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Hash, Clone, Default, Builder)]
#[serde(rename_all = "camelCase")]
pub struct File {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub location: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub basename: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub dirname: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub nameroot: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub nameext: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub checksum: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub size: Option<Integer>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub secondary_files: Option<Vec<FileOrDirectory>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub format: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub contents: Option<String>,
}

impl File {
    // according to the spec this fn sets the value of location (if some) to path (if none)
    // however file's existence is not questioned, yet
    pub fn dry_validation(&mut self) {
        if let Some(location) = &self.location
            && self.path.is_none()
        {
            let location = if let Some((_, location)) = location.split_once("://") {
                location
            } else {
                location
            };
            self.path = Some(location.to_string());
        }
    }

    pub fn new_from_path(path: &Path) -> anyhow::Result<Self> {
        let path_as_str = path.to_string_lossy();
        let FilePathMetaData {
            basename,
            nameroot,
            nameext,
            dirname,
        } = get_path_metadata(path);

        let FileMetaData { size, checksum } = get_file_metadata(path)?;

        let file = File::builder()
            .location(format!("file://{}", &path_as_str))
            .path(path_as_str)
            .maybe_basename(basename)
            .maybe_nameroot(nameroot)
            .maybe_nameext(nameext)
            .maybe_dirname(dirname)
            .size(size)
            .maybe_checksum(checksum)
            .build();
        Ok(file)
    }
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Hash, Clone, Default, Builder)]
#[serde(rename_all = "camelCase")]
pub struct Directory {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub location: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub basename: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub listing: Option<Vec<FileOrDirectory>>,
}

impl Directory {
    // according to the spec this fn sets the value of location (if some) to path (if none)
    // however directory's existence is not questioned, yet
    pub fn dry_validation(&mut self) {
        if let Some(location) = &self.location
            && self.path.is_none()
        {
            self.path = Some(location.to_string());
        }
    }

    pub fn load_listing(&mut self, load_listing: LoadListingEnum) -> anyhow::Result<()> {
        self.dry_validation();
        let Some(path) = &self.path else {
            anyhow::bail!("No path given!");
        };

        match load_listing {
            LoadListingEnum::NoListing => self.listing = Some(vec![]),
            LoadListingEnum::ShallowListing => self.listing = Some(Self::read_dir(path, false)?),
            LoadListingEnum::DeepListing => self.listing = Some(Self::read_dir(path, true)?),
        }
        Ok(())
    }

    fn read_dir(path: &str, recursive: bool) -> anyhow::Result<Vec<FileOrDirectory>> {
        let mut entries = Vec::new();

        let read_dir = fs::read_dir(path)?;

        for entry in read_dir.flatten() {
            let path_buf = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();

            if path_buf.is_dir() {
                let mut dir = Directory {
                    path: Some(path_buf.to_string_lossy().to_string()),
                    basename: Some(name),
                    ..Default::default()
                };
                dir.location = dir.path.as_ref().map(|s| format!("file://{s}"));

                if recursive {
                    dir.load_listing(LoadListingEnum::DeepListing)?;
                }

                entries.push(FileOrDirectory::Directory(dir));
            } else {
                entries.push(FileOrDirectory::File(File::new_from_path(&path_buf)?));
            }
        }

        Ok(entries)
    }
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Hash, Clone, Default, Builder)]
#[serde(rename_all = "camelCase")]
pub struct Dirent {
    #[builder(into)]
    pub entry: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub entryname: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub writable: Option<bool>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[allow(unused)]
    pub fn test_file_or_directory() {
        #[derive(Deserialize)]
        struct ListingBag {
            listing: Vec<FileOrDirectory>,
        }
        let contents = include_str!("../../testdata/listing.yaml");
        let res = serde_yaml::from_str::<ListingBag>(contents);
        assert!(res.is_ok());
    }
}
