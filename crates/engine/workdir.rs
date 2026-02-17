use crate::{
    expression::{EvaluationContext, do_eval, do_eval_to_string, extract_input_name},
    io::{directory::move_dir, file::move_file},
    serialize::to_string_dump,
};
use anyhow::Context;
use cwl_core::{
    OneOrMany,
    files::{Dirent, FileOrDirectory},
    inputs::DefaultValue,
    requirements::{InitialWorkDirRequirement, ListingItems, WorkDirItems},
};
use dircpy::copy_dir;
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};
use tracing::debug;

pub fn stage_work_dir(
    iwdr: &InitialWorkDirRequirement,
    workdir: &Path,
    stagedir: &Path,
    context: &EvaluationContext,
    container_workdir: &str,
    inputs: &mut HashMap<String, DefaultValue>,
) -> anyhow::Result<()> {
    match &iwdr.listing {
        WorkDirItems::Expression(expression) => {
            let evaluated = do_eval(expression, context)?;
            let items = &serde_yaml::from_value(evaluated)?;
            update_inputs(expression, inputs, container_workdir, None);
            stage_item(items, workdir, stagedir, context, container_workdir, inputs)?;
            Ok(())
        }
        WorkDirItems::ListingItems(items) => match &**items {
            OneOrMany::One(item) => {
                stage_item(item, workdir, stagedir, context, container_workdir, inputs)
            }
            OneOrMany::Many(items) => {
                for item in items {
                    stage_item(item, workdir, stagedir, context, container_workdir, inputs)?;
                }
                Ok(())
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
) -> anyhow::Result<()> {
    match item {
        ListingItems::Expression(expression) => {
            let evaluated = do_eval(expression, context)?;
            if evaluated.is_null() {
                //could be an optional type which is checked in input collection
                debug!("expression returned null: {expression}");
                return Ok(());
            }
            let items = &serde_yaml::from_value(evaluated)?;
            update_inputs(expression, inputs, container_workdir, None);
            stage_item(items, workdir, stagedir, context, container_workdir, inputs)?;
            Ok(())
        }
        ListingItems::Dirent(dirent) => stage_dirent(
            dirent,
            workdir,
            stagedir,
            context,
            container_workdir,
            inputs,
        ),
        ListingItems::FileOrDirectory(fod) => stage_files(fod, stagedir, None),
        ListingItems::Vec(items) => {
            for item in items {
                stage_files(item, stagedir, None)?;
            }
            Ok(())
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
) -> anyhow::Result<()> {
    //evaluate expression if so
    let evaluated_content =
        do_eval(&dirent.entry, context).unwrap_or(serde_yaml::Value::String(dirent.entry.clone()));
    if evaluated_content.is_null() {
        debug!("Workdir Entry evaluated to null: {dirent:?}");
        return Ok(());
    }
    //probably array of files is given here, why is dirent used in the first place?
    if dirent.entryname.is_none()
        && let Ok(items) =
            serde_yaml::from_value::<OneOrMany<ListingItems>>(evaluated_content.clone())
    {
        for item in &items.as_many() {
            stage_item(item, workdir, stagedir, context, container_workdir, inputs)?;
        }
        return Ok(());
    }

    //parse to DefaultValue
    let dv: DefaultValue = serde_yaml::from_value(evaluated_content)?;

    //get entryname
    let entryname = dirent.clone().entryname.unwrap();
    let entryname = do_eval_to_string(&entryname, context);

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
        DefaultValue::FileOrDirectory(FileOrDirectory::Directory(dir))
            if !has_trailing_newline =>
        {
            stage_files(
                &FileOrDirectory::Directory(dir),
                stagedir,
                dirent.entryname.as_ref(),
            )?;
            return Ok(());
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

    let parent = staged_path.parent().unwrap();
    fs::create_dir_all(parent)
        .with_context(|| format!("Could not create directory at {parent:?}"))?;
    //create the file
    fs::write(&staged_path, string_content)
        .with_context(|| format!("Could not write to {staged_path:?}"))?;
    Ok(())
}

fn stage_files(
    item: &FileOrDirectory,
    stagedir: &Path,
    entryname: Option<&String>,
) -> anyhow::Result<()> {
    let staged_path = if let Some(entryname) = &entryname {
        stagedir.join(entryname)
    } else if let Some(basename) = item.basename() {
        stagedir.join(basename)
    } else {
        let path = item.path().unwrap();
        let path = PathBuf::from(path);
        stagedir.join(path.file_name().unwrap())
    };

    let parent = staged_path.parent().unwrap();
    fs::create_dir_all(parent)
        .with_context(|| format!("Could not create directory at {parent:?}"))?;

    if let Some(path) = item.location() {
        let path = path.strip_prefix("file://").unwrap_or(path); //TODO: check scheme

        if item.is_file() {
            fs::copy(path, &staged_path)
                .with_context(|| format!("Could not copy from {path:?} to {staged_path:?}"))?;
        } else if item.is_dir() {
            copy_dir(path, &staged_path)
                .with_context(|| format!("Could not copy from {path:?} to {staged_path:?}"))?;
        }
    } else if let FileOrDirectory::Directory(dir) = item {
        fs::create_dir_all(&staged_path)
            .with_context(|| format!("Could not create directory at {staged_path:?}"))?;
        if let Some(listing) = &dir.listing {
            for item in listing {
                stage_files(item, &staged_path, None)?;
            }
        }
    }

    //secondary files
    if let FileOrDirectory::File(f) = item
        && let Some(sec_files) = &f.secondary_files
    {
        for item in sec_files {
            stage_files(item, stagedir, entryname)?;
        }
    }

    Ok(())
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
