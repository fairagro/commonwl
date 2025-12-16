use serde::{Deserialize, Serialize};

use crate::Integer;

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

#[derive(Serialize, Deserialize, Debug, PartialEq, Hash, Clone)]
#[serde(rename_all = "snake_case")]
pub struct File {
    pub location: Option<String>,
    pub path: Option<String>,
    pub basename: Option<String>,
    pub dirname: Option<String>,
    pub nameroot: Option<String>,
    pub nameext: Option<String>,
    pub checksum: Option<String>,
    pub size: Option<Integer>,
    pub secondary_files: Option<Vec<FileOrDirectory>>,
    pub format: Option<String>,
    pub contents: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Hash, Clone)]
#[serde(rename_all = "snake_case")]
pub struct Directory {
    pub location: Option<String>,
    pub path: Option<String>,
    pub basename: Option<String>,
    pub listing: Option<Vec<FileOrDirectory>>,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Hash, Clone)]
#[serde(rename_all = "snake_case")]
pub struct Dirent {
    pub entry: String,
    pub entry_name: Option<String>,
    pub writable: Option<bool>
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
        let contents = include_str!("../testdata/listing.yaml");
        let res = serde_yaml::from_str::<ListingBag>(contents);
        assert!(res.is_ok());
    }
}
