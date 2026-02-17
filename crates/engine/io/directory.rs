use crate::io::{file::locate_file, get_location, get_relative_path};
use anyhow::Context;
use cwl_core::{
    FilePathMetaData,
    files::{Directory, File, FileOrDirectory, LoadListingEnum},
    get_path_metadata,
};
use std::{
    fs,
    path::{MAIN_SEPARATOR_STR, Path},
};
use url::Url;

/// locates a directory and writes metadata
pub fn locate_dir(
    dir: &mut Directory,
    work_dir: &Path,
    stage_dir: &Path,
    load_listing: Option<LoadListingEnum>,
) -> anyhow::Result<()> {
    if let Some(path) = &dir.path
        && dir.location.is_none()
    {
        dir.location = Some(get_location(path, work_dir));
    }

    if let Some(location) = &dir.location {
        //make absolute URI
        let location = get_location(location, work_dir);
        let url = Url::parse(&location)?;

        let relative_path = get_relative_path(&url, work_dir)?;
        let designated_path = stage_dir.join(&relative_path);

        dir.location = Some(location.clone());

        //calculate file metadata for designated path
        let FilePathMetaData {
            basename,
            nameroot: _,
            nameext: _,
            dirname: _,
        } = get_path_metadata(&designated_path);

        if dir.basename.is_none() {
            dir.basename = basename;
        }

        //We set them before!
        let parent = designated_path.parent();
        let path = parent.unwrap().to_string_lossy().into_owned()
            + MAIN_SEPARATOR_STR
            + dir.basename.as_ref().unwrap();
        dir.path = Some(path.clone());

        //try getting checksum and size (currently for local files only). Ignores failure (which usually means the file does not exist!)
        if url.scheme() == "file" {
            let path = Path::new(location.strip_prefix("file://").unwrap());

            let listing = match load_listing {
                Some(LoadListingEnum::NoListing) | None => None,
                Some(LoadListingEnum::ShallowListing) => {
                    Some(read_dir(path, false, work_dir, stage_dir)?)
                }
                Some(LoadListingEnum::DeepListing) => {
                    Some(read_dir(path, true, work_dir, stage_dir)?)
                }
            };
            dir.listing = listing;
        }
    } else if let Some(basename) = &dir.basename {
        let path = stage_dir.join(basename);
        dir.path = Some(path.to_string_lossy().into_owned());

        //locate items in listing
        if let Some(listing) = &mut dir.listing {
            for item in listing {
                match item {
                    FileOrDirectory::File(file) => locate_file(file, work_dir, &path)?,
                    FileOrDirectory::Directory(dir) => {
                        locate_dir(dir, work_dir, &path, load_listing)?
                    }
                }
            }
        }
    }

    Ok(())
}

fn read_dir(
    path: &Path,
    recursive: bool,
    work_dir: &Path,
    stage_dir: &Path,
) -> anyhow::Result<Vec<FileOrDirectory>> {
    let mut entries = Vec::new();
    let read_dir =
        fs::read_dir(path).with_context(|| format!("Could not read directory {path:?}"))?;

    for entry in read_dir.flatten() {
        let path_buf = entry.path();

        if path_buf.is_dir() {
            let mut dir = Directory {
                location: Some(path_buf.to_string_lossy().to_string()),
                ..Default::default()
            };

            let load_listing = if recursive {
                Some(LoadListingEnum::DeepListing)
            } else {
                Some(LoadListingEnum::NoListing)
            };

            locate_dir(&mut dir, work_dir, stage_dir, load_listing)?;

            entries.push(FileOrDirectory::Directory(dir));
        } else {
            let mut file = File {
                location: Some(path_buf.to_string_lossy().to_string()),
                ..Default::default()
            };
            locate_file(&mut file, work_dir, stage_dir)?;
            entries.push(FileOrDirectory::File(file));
        }
    }
    entries.sort_by_key(|e| e.basename().cloned());

    Ok(entries)
}

pub fn move_dir(dir: &mut Directory, workdir: &Path, basename: Option<&String>) {
    let basename = basename.unwrap_or(dir.basename.as_ref().unwrap());
    let designated_path = workdir.join(basename);
    let FilePathMetaData {
        basename,
        nameroot: _,
        nameext: _,
        dirname: _,
    } = get_path_metadata(&designated_path);

    let path = designated_path.to_string_lossy().into_owned();
    dir.path = Some(path);

    dir.basename = basename;
}
