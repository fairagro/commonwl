use crate::environment::workdir::{MountType, Source, WorkDirMount};
use anyhow::Context;
use crankshaft::engine::{
    Task,
    task::{
        Input,
        input::{self, Contents},
    },
};
use cwl_core::files::FileOrDirectory;
use dircpy::copy_dir;
use std::{
    fs,
    path::{Path, PathBuf},
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
        let contents = if location.starts_with("file://") {
            let location = location.strip_prefix("file://").unwrap();
            Contents::Path(PathBuf::from(location))
        } else {
            Contents::Url(Url::parse(location)?)
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

pub(crate) fn mount_workdir_item(
    mount: WorkDirMount,
    outdir: &Path,
    use_container: bool,
    task: &mut Task,
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
            (MountType::File, Source::File(path)) => {
                fs::copy(&path, &mount.target).with_context(|| {
                    format!(
                        "Could not copy from {} to {}",
                        path.display(),
                        mount.target.display()
                    )
                })?;
            }
            (MountType::File, Source::Contents(items)) => {
                fs::write(&mount.target, &items)
                    .with_context(|| format!("Could not write to {}", mount.target.display()))?;
            }
            (MountType::Directory, Source::File(path)) => copy_dir(&path, &mount.target)
                .with_context(|| {
                    format!(
                        "Could not copy from {} to {}",
                        path.display(),
                        mount.target.display()
                    )
                })?,
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
                    Source::File(path) => Contents::Path(path),
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
            let loc_path = location.strip_prefix("file://").unwrap();
            let Source::File(mount_path) = &mount.source else {
                continue;
            };
            if loc_path == mount_path {
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
