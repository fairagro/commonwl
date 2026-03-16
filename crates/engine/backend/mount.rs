use crate::{
    environment::workdir::{MountType, Source, WorkDirMount},
    io::normalize_url,
};
use anyhow::Context;
use crankshaft::engine::{
    Task,
    task::{
        Input,
        input::{self, Contents},
    },
};
use cwl_core::files::FileOrDirectory;
use cwl_engine_storage::{Storage, StorageBackend};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::Arc,
};
use url::Url;

pub(crate) fn mount_input(task: &mut Task, input: &FileOrDirectory) -> anyhow::Result<()> {
    let ty = match input {
        FileOrDirectory::File(_) => input::Type::File,
        FileOrDirectory::Directory(_) => input::Type::Directory,
    };
    if let Some(path) = input.path()
        && let Some(location) = input.location()
    {
        let location = if input.is_dir() && !location.ends_with('/') {
            format!("{location}/")
        } else {
            location.clone()
        };
        let contents = if location.starts_with("file://") {
            let location = location.strip_prefix("file://").unwrap();
            Contents::Path(PathBuf::from(location))
        } else {
            Contents::Url(Url::parse(&location)?)
        };

        task.add_input(
            Input::builder()
                .contents(contents)
                .path(path)
                .ty(ty)
                .build(),
        );
    } else if let FileOrDirectory::File(file) = &input
        && let Some(contents) = &file.contents
        && let Some(path) = &file.path
    {
        //make content checksum
        task.add_input(
            Input::builder()
                .contents(Contents::Literal(contents.as_bytes().to_vec()))
                .path(path)
                .ty(ty)
                .build(),
        );
    } else if let FileOrDirectory::Directory(dir) = &input
        && let Some(listing) = &dir.listing
    {
        for item in listing {
            mount_input(task, item)?;
        }
    }

    Ok(())
}

pub enum MountStrategy {
    Local,
    Remote { base_url: Url },
}

pub(crate) async fn mount_workdir_item(
    mount: WorkDirMount,
    outdir: &Path,
    workdir: &str,
    use_container: bool,
    task: &mut Task,
    backend: Arc<StorageBackend>,
    strategy: MountStrategy,
) -> anyhow::Result<()> {
    match strategy {
        MountStrategy::Local => {
            mount_workdir_item_local(mount, outdir, use_container, task, backend).await
        }
        MountStrategy::Remote { base_url } => {
            mount_workdir_item_remote(mount, outdir, workdir, task, backend, &base_url).await
        }
    }
}

async fn mount_workdir_item_local(
    mount: WorkDirMount,
    outdir: &Path,
    use_container: bool,
    task: &mut Task,
    backend: Arc<StorageBackend>,
) -> anyhow::Result<()> {
    if mount.target.starts_with(outdir) {
        if let Some(parent) = mount.target.parent()
            && !parent.exists()
        {
            fs::create_dir_all(parent).with_context(|| {
                format!(
                    "Could not create parent directories for {}",
                    mount.target.display()
                )
            })?;
        }
        match (mount.ty, mount.source) {
            (MountType::File, Source::Url(path)) => backend.download(&path, &mount.target).await?,
            (MountType::File, Source::Contents(items)) => {
                fs::write(&mount.target, &items)
                    .with_context(|| format!("Could not write to {}", mount.target.display()))?;
            }
            (MountType::Directory, Source::Url(path)) => {
                backend.download(&path, &mount.target).await?;
            }
            (MountType::Directory, Source::Contents(_)) => {
                fs::create_dir_all(&mount.target).with_context(|| {
                    format!(
                        "Could not create parent directories for {}",
                        mount.target.display()
                    )
                })?;
            }
        }
    } else if use_container {
        task.add_input(
            Input::builder()
                .path(mount.target.to_string_lossy())
                .contents(match mount.source {
                    Source::Url(path) => Contents::Url(path),
                    Source::Contents(data) => Contents::Literal(data),
                })
                .ty(match mount.ty {
                    MountType::File => input::Type::File,
                    MountType::Directory => input::Type::Directory,
                })
                .read_only(mount.readonly)
                .build(),
        );
    } else {
        anyhow::bail!(
            "Workdir item target {} is outside of working directory and container is not used, can not stage",
            mount.target.display()
        );
    }
    Ok(())
}

async fn mount_workdir_item_remote(
    mount: WorkDirMount,
    outdir: &Path,
    workdir: &str,
    task: &mut Task,
    backend: Arc<StorageBackend>,
    base_url: &Url,
) -> anyhow::Result<()> {
    let rel = mount.target.strip_prefix(outdir).unwrap_or(&mount.target);
    let guest_path = if mount.target.starts_with(outdir) {
        format!("{}/{}", workdir, rel.display())
    } else {
        mount.target.to_string_lossy().to_string()
    };

    match (mount.ty, mount.source) {
        (MountType::File, Source::Url(url)) => {
            let dest = if url.scheme() == "file" {
                let local = Path::new(url.path());
                let dest = base_url.join(&rel.to_string_lossy())?;
                backend.upload(local, &dest).await?;
                dest
            } else {
                url // already remote, use directly
            };
            task.add_input(
                Input::builder()
                    .path(&guest_path)
                    .contents(Contents::Url(dest))
                    .ty(input::Type::File)
                    .read_only(true)
                    .build(),
            );
        }
        (MountType::File, Source::Contents(data)) => {
            let dest = base_url.join(&rel.to_string_lossy())?;
            backend.upload_bytes(&data, &dest).await?;
            task.add_input(
                Input::builder()
                    .path(&guest_path)
                    .contents(Contents::Url(dest))
                    .ty(input::Type::File)
                    .read_only(true)
                    .build(),
            );
        }
        (MountType::Directory, Source::Url(url)) => {
            let dest = if url.scheme() == "file" {
                let local = Path::new(url.path());
                let dest = base_url.join(&format!("{}/", rel.display()))?;
                backend.upload(local, &dest).await?;
                dest
            } else {
                url
            };
            task.add_input(
                Input::builder()
                    .path(&guest_path)
                    .contents(Contents::Url(dest))
                    .ty(input::Type::Directory)
                    .read_only(true)
                    .build(),
            );
        }
        (MountType::Directory, Source::Contents(_)) => {}
    }

    Ok(())
}

pub(crate) fn remove_materialized_inputs(
    flattened_inputs: Vec<FileOrDirectory>,
    mounts: &[WorkDirMount],
    workdir: &String,
) -> Vec<FileOrDirectory> {
    let mut materialized_inputs = vec![];
    let mut remaining_inputs = vec![];

    for input in flattened_inputs {
        let mut materialized = false;
        for mount in mounts {
            let Some(location) = input.location() else {
                continue;
            };
            let loc_url = Url::parse(location).unwrap();
            let Source::Url(mount_path) = &mount.source else {
                continue;
            };
            if loc_url == normalize_url(mount_path) {
                materialized = true;
                break;
            }
            if input.path().is_some_and(|p| p.starts_with(workdir)) {
                materialized = true;
                break;
            }
        }

        if materialized {
            materialized_inputs.push(input);
        } else {
            remaining_inputs.push(input);
        }
    }

    remaining_inputs
}
