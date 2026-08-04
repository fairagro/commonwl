use crate::{
    io::{file::locate_file, get_location, get_relative_path},
    string_url_to_file_path,
};
use anyhow::Context;
use cwl_core::{
    FilePathMetaData,
    files::{Directory, File, FileOrDirectory, LoadListingEnum},
    get_path_metadata,
};
use cwl_engine_storage::StorageBackend;
use futures_util::{FutureExt, future::BoxFuture};
use std::{
    collections::HashSet,
    fs,
    path::{MAIN_SEPARATOR_STR, Path, PathBuf},
};
use url::Url;

/// locates a directory and writes metadata
pub(crate) fn locate_dir<'a>(
    dir: &'a mut Directory,
    work_dir: &'a Path,
    stage_dir: &'a Path,
    load_listing: Option<LoadListingEnum>,
    storage: &'a StorageBackend,
) -> BoxFuture<'a, crate::Result<()>> {
    async move {
        let mut visited = HashSet::new();
        locate_dir_impl(dir, work_dir, stage_dir, load_listing, &mut visited, storage).await
    }
    .boxed()
}

fn locate_dir_impl<'a>(
    dir: &'a mut Directory,
    work_dir: &'a Path,
    stage_dir: &'a Path,
    load_listing: Option<LoadListingEnum>,
    visited: &'a mut HashSet<PathBuf>,
    storage: &'a StorageBackend,
) -> BoxFuture<'a, crate::Result<()>> {
    async move {
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

            //listing generation currently only supports local directories - a remote directory
            //keeps whatever listing (if any) it already had.
            if url.scheme() == "file" {
                let path = string_url_to_file_path(&location)?;
                let listing = match load_listing {
                    Some(LoadListingEnum::NoListing) | None => None,
                    Some(LoadListingEnum::ShallowListing) => {
                        Some(read_dir(&path, false, work_dir, stage_dir, visited, storage).await?)
                    }
                    Some(LoadListingEnum::DeepListing) => {
                        Some(read_dir(&path, true, work_dir, stage_dir, visited, storage).await?)
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
                        FileOrDirectory::File(file) => {
                            locate_file(file, work_dir, &path, false, storage).await?;
                        }
                        FileOrDirectory::Directory(dir) => {
                            locate_dir_impl(dir, work_dir, &path, load_listing, visited, storage)
                                .await?;
                        }
                    }
                }
            }
        }

        Ok(())
    }
    .boxed()
}

fn read_dir<'a>(
    path: &'a Path,
    recursive: bool,
    work_dir: &'a Path,
    stage_dir: &'a Path,
    visited: &'a mut HashSet<PathBuf>,
    storage: &'a StorageBackend,
) -> BoxFuture<'a, crate::Result<Vec<FileOrDirectory>>> {
    async move {
        let mut entries = Vec::new();
        let read_dir = fs::read_dir(path)
            .with_context(|| format!("Could not read directory {}", path.display()))?;

        for entry in read_dir.flatten() {
            let path_buf = entry.path();

            if path_buf.file_name().and_then(|n| n.to_str())
                == Some(crate::backend::mount::EMPTY_DIR_MARKER)
            {
                continue;
            }

            if path_buf.is_dir() {
                let mut dir = Directory {
                    location: Some(path_buf.to_string_lossy().to_string()),
                    ..Default::default()
                };

                let canonical = fs::canonicalize(&path_buf).unwrap_or_else(|_| path_buf.clone());
                let load_listing = if recursive && visited.insert(canonical) {
                    Some(LoadListingEnum::DeepListing)
                } else {
                    Some(LoadListingEnum::NoListing)
                };

                locate_dir_impl(&mut dir, work_dir, stage_dir, load_listing, visited, storage)
                    .await?;

                entries.push(FileOrDirectory::Directory(dir));
            } else {
                let mut file = File {
                    location: Some(path_buf.to_string_lossy().to_string()),
                    ..Default::default()
                };
                locate_file(&mut file, work_dir, stage_dir, false, storage).await?;
                entries.push(FileOrDirectory::File(file));
            }
        }
        entries.sort_by_key(|e| e.basename().cloned());

        Ok(entries)
    }
    .boxed()
}

pub(crate) fn move_dir(dir: &mut Directory, workdir: &Path, basename: Option<&String>) {
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
