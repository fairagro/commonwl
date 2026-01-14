use crate::Integer;
use bon::Builder;
use serde::{Deserialize, Serialize};

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
            self.path = Some(location.to_string());
        }
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
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Hash, Clone, Default, Builder)]
#[serde(rename_all = "camelCase")]
pub struct Dirent {
    #[builder(into)]
    pub entry: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub entry_name: Option<String>,
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
