use crate::{
    expression::{EvaluationContext, do_eval, do_eval_to_string, extract_input_name},
    io::json::to_string_dump,
    io::{directory::move_dir, file::move_file},
};
use anyhow::Context;
use cwl_core::{
    OneOrMany,
    files::{Dirent, FileOrDirectory},
    inputs::DefaultValue,
    requirements::{InitialWorkDirRequirement, ListingItems, WorkDirItems},
};
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};
use tracing::debug;

#[derive(Debug, Clone)]
pub enum MountType {
    File,
    Directory,
}

#[derive(Debug, Clone)]
pub enum Source {
    File(PathBuf),
    Contents(Vec<u8>),
}

#[derive(Debug, Clone)]
pub struct WorkDirMount {
    pub source: Source,
    pub target: PathBuf,
    pub ty: MountType,
    pub readonly: bool,
}

pub fn stage_work_dir(
    iwdr: &InitialWorkDirRequirement,
    workdir: &Path,
    stagedir: &Path,
    context: &EvaluationContext,
    container_workdir: &str,
    inputs: &mut HashMap<String, DefaultValue>,
) -> anyhow::Result<Vec<WorkDirMount>> {
    match &iwdr.listing {
        WorkDirItems::Expression(expression) => {
            let evaluated = do_eval(expression, context)?;
            let items = &serde_yaml::from_value(evaluated)?;
            update_inputs(expression, inputs, container_workdir, None);
            stage_item(items, workdir, stagedir, context, container_workdir, inputs)
        }
        WorkDirItems::ListingItems(items) => match &**items {
            OneOrMany::One(item) => {
                stage_item(item, workdir, stagedir, context, container_workdir, inputs)
            }
            OneOrMany::Many(items) => {
                let mut mounts = vec![];
                for item in items {
                    mounts.extend(stage_item(
                        item,
                        workdir,
                        stagedir,
                        context,
                        container_workdir,
                        inputs,
                    )?);
                }
                Ok(mounts)
            }
        },
    }
}

fn stage_item(
    item: &ListingItems,
    workdir: &Path,
    stagedir: &Path,
    context: &EvaluationContext,
    container_workdir: &str,
    inputs: &mut HashMap<String, DefaultValue>,
) -> anyhow::Result<Vec<WorkDirMount>> {
    match item {
        ListingItems::Expression(expression) => {
            let evaluated = do_eval(expression, context)?;
            if evaluated.is_null() {
                //could be an optional type which is checked in input collection
                debug!("expression returned null: {expression}");
                return Ok(vec![]);
            }
            let items = &serde_yaml::from_value(evaluated)?;
            update_inputs(expression, inputs, container_workdir, None);
            stage_item(items, workdir, stagedir, context, container_workdir, inputs)
        }
        ListingItems::Dirent(dirent) => stage_dirent(
            dirent,
            workdir,
            stagedir,
            context,
            container_workdir,
            inputs,
        ),
        ListingItems::FileOrDirectory(fod) => stage_files(fod, workdir, stagedir, None, true),
        ListingItems::Vec(items) => {
            let mut mounts = vec![];
            for item in items {
                mounts.extend(stage_files(item, workdir, stagedir, None, true)?);
            }
            Ok(mounts)
        }
    }
}

fn stage_dirent(
    dirent: &Dirent,
    workdir: &Path,
    stagedir: &Path,
    context: &EvaluationContext,
    container_workdir: &str,
    inputs: &mut HashMap<String, DefaultValue>,
) -> anyhow::Result<Vec<WorkDirMount>> {
    //evaluate expression if so
    let evaluated_content =
        do_eval(&dirent.entry, context).unwrap_or(serde_yaml::Value::String(dirent.entry.clone()));
    if evaluated_content.is_null() {
        debug!("Workdir Entry evaluated to null: {dirent:?}");
        return Ok(vec![]);
    }

    //probably array of files is given here, why is dirent used in the first place?
    if dirent.entryname.is_none()
        && let Ok(items) =
            serde_yaml::from_value::<OneOrMany<ListingItems>>(evaluated_content.clone())
    {
        let mut mounts = vec![];
        for item in &items.as_many() {
            mounts.extend(stage_item(
                item,
                workdir,
                stagedir,
                context,
                container_workdir,
                inputs,
            )?);
        }
        return Ok(mounts);
    }

    //parse to DefaultValue
    let dv: DefaultValue = serde_yaml::from_value(evaluated_content)?;

    //get entryname
    let entryname = dirent.clone().entryname.unwrap();
    let entryname = do_eval_to_string(&entryname, context);
    if entryname.starts_with("../") {
        //illegal
        anyhow::bail!("dirent.entryname must not start with ../")
    }
    //relocate used inputs
    update_inputs(&dirent.entry, inputs, container_workdir, Some(&entryname));

    //if dirent ends with newline and has expression we use string interpolation which means we do json serialization
    let has_trailing_newline = dirent.entry.ends_with("\n");

    let staged_path = stagedir.join(&entryname);
    let mut string_content = match dv {
        DefaultValue::FileOrDirectory(FileOrDirectory::File(file)) if !has_trailing_newline => {
            if let Some(contents) = file.contents {
                contents.to_string()
            } else {
                let mut path = file.location.clone().unwrap();
                path = path.strip_prefix("file://").unwrap_or(&path).to_owned();

                if Path::new(&path).is_absolute() {
                    fs::read_to_string(&path)
                        .with_context(|| format!("Could not read file {path}"))?
                } else {
                    fs::read_to_string(workdir.join(&path))
                        .with_context(|| format!("Could not read file {path}"))?
                }
            }
        }
        DefaultValue::FileOrDirectory(FileOrDirectory::Directory(dir)) if !has_trailing_newline => {
            return stage_files(
                &FileOrDirectory::Directory(dir),
                workdir,
                stagedir,
                dirent.entryname.as_ref(),
                !dirent.writable.unwrap_or(false),
            );
        }
        DefaultValue::Any(value) => match value {
            serde_yaml::Value::String(s) => s,
            _ => to_string_dump(&value)?,
        },
        _ => to_string_dump(&dv)?,
    };

    if has_trailing_newline && !string_content.ends_with("\n") {
        string_content += "\n"
    }

    Ok(vec![WorkDirMount {
        source: Source::Contents(string_content.into_bytes()),
        target: staged_path,
        ty: MountType::File,
        readonly: !dirent.writable.unwrap_or(false),
    }])
}

fn stage_files(
    item: &FileOrDirectory,
    workdir: &Path,
    stagedir: &Path,
    entryname: Option<&String>,
    readonly: bool,
) -> anyhow::Result<Vec<WorkDirMount>> {
    let mut mounts = vec![];
    let staged_path = if let Some(entryname) = &entryname {
        stagedir.join(entryname)
    } else if let Some(basename) = item.basename() {
        stagedir.join(basename)
    } else {
        let path = item
            .location()
            .or(item.path())
            .expect("Can not guess staged_path");
        let path = PathBuf::from(path);
        stagedir.join(path.file_name().unwrap())
    };

    if let Some(path) = item.location() {
        let path = path.strip_prefix("file://").unwrap_or(path); //TODO: check scheme
        let mut path = PathBuf::from(path);
        if !path.is_absolute() {
            path = workdir.join(path);
        }
        if item.is_file() {
            mounts.push(WorkDirMount {
                source: Source::File(path.clone()),
                target: staged_path.clone(),
                ty: MountType::File,
                readonly,
            });
        } else if item.is_dir() {
            mounts.push(WorkDirMount {
                source: Source::File(path.clone()),
                target: staged_path.clone(),
                ty: MountType::Directory,
                readonly,
            });
        }
    } else if let FileOrDirectory::Directory(dir) = item
        && let Some(listing) = &dir.listing
    {
        mounts.push(WorkDirMount {
            source: Source::Contents(vec![]),
            target: staged_path.clone(),
            ty: MountType::Directory,
            readonly,
        });
        for item in listing {
            mounts.extend(stage_files(item, workdir, &staged_path, None, readonly)?);
        }
    }

    //secondary files
    if let FileOrDirectory::File(f) = item
        && let Some(sec_files) = &f.secondary_files
    {
        for item in sec_files {
            mounts.extend(stage_files(item, workdir, stagedir, entryname, readonly)?);
        }
    }

    Ok(mounts)
}

fn update_inputs(
    expression: &str,
    inputs: &mut HashMap<String, DefaultValue>,
    container_workdir: &str,
    entryname: Option<&String>,
) {
    if let Some(input_used) = extract_input_name(expression) {
        debug!("Input {input_used} was used in an expression in InitialWorkDirRequirement");
        if let Some(input) = inputs.get_mut(&input_used) {
            match input {
                DefaultValue::FileOrDirectory(FileOrDirectory::File(file)) => {
                    debug!("Moving {file:?} into {container_workdir:?}");
                    move_file(file, Path::new(container_workdir), entryname)
                }
                DefaultValue::FileOrDirectory(FileOrDirectory::Directory(dir)) => {
                    debug!("Moving {dir:?} into {container_workdir:?}");
                    move_dir(dir, Path::new(container_workdir), entryname)
                }
                _ => {}
            }
        }
    }
}
