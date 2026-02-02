use crate::{
    expression::{EvaluationContext, do_eval, do_eval_to_string},
    pathmapper::PathMapper,
};
use cwl_core::{
    OneOrMany,
    files::{Dirent, FileOrDirectory},
    inputs::DefaultValue,
    requirements::{InitialWorkDirRequirement, ListingItems, WorkDirItems},
};
use dircpy::copy_dir;
use std::{
    fs,
    path::{Path, PathBuf},
};

pub fn stage_work_dir(
    iwdr: &InitialWorkDirRequirement,
    workdir: &Path,
    stagedir: &Path,
    context: &EvaluationContext,
    guest_workdir: &str,
    path_mapper: &mut PathMapper,
) -> anyhow::Result<()> {
    match &iwdr.listing {
        WorkDirItems::Expression(expression) => {
            let evaluated = do_eval(expression, context)?;
            let items = &serde_yaml::from_value(evaluated)?;
            stage_item(
                items,
                workdir,
                stagedir,
                context,
                guest_workdir,
                path_mapper,
            )?;
            Ok(())
        }
        WorkDirItems::ListingItems(items) => match &**items {
            OneOrMany::One(item) => {
                stage_item(item, workdir, stagedir, context, guest_workdir, path_mapper)
            }
            OneOrMany::Many(items) => {
                for item in items {
                    stage_item(item, workdir, stagedir, context, guest_workdir, path_mapper)?;
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
    guest_workdir: &str,
    path_mapper: &mut PathMapper,
) -> anyhow::Result<()> {
    match item {
        ListingItems::Expression(expression) => {
            let evaluated = do_eval(expression, context)?;
            let items = &serde_yaml::from_value(evaluated)?;
            stage_item(
                items,
                workdir,
                stagedir,
                context,
                guest_workdir,
                path_mapper,
            )?;
            Ok(())
        }
        ListingItems::Dirent(dirent) => stage_dirent(
            dirent,
            workdir,
            stagedir,
            context,
            guest_workdir,
            path_mapper,
        ),
        ListingItems::FileOrDirectory(fod) => {
            stage_files(fod, workdir, stagedir, None, guest_workdir, path_mapper)
        }
        ListingItems::Vec(items) => {
            for item in items {
                stage_files(item, workdir, stagedir, None, guest_workdir, path_mapper)?;
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
    guest_workdir: &str,
    path_mapper: &mut PathMapper,
) -> anyhow::Result<()> {
    //evaluate expression if so
    let evaluated_content = do_eval(&dirent.entry, context)?;

    //parse to DefaultValue
    let dv: DefaultValue = serde_yaml::from_value(evaluated_content)?;

    //get entryname
    let entryname = dirent.clone().entryname.unwrap();
    let entryname = do_eval_to_string(&entryname, context);

    let staged_path = stagedir.join(&entryname);

    let string_content = match dv {
        DefaultValue::FileOrDirectory(FileOrDirectory::File(file)) => {
            if let Some(contents) = file.contents {
                contents.to_string()
            } else {
                let path = file.path.clone().unwrap();
                update_pathmap(
                    guest_workdir,
                    path_mapper,
                    &FileOrDirectory::File(file),
                    workdir,
                    Some(&entryname),
                )?;
                if Path::new(&path).is_absolute() {
                    fs::read_to_string(path)?
                } else {
                    fs::read_to_string(workdir.join(path))?
                }
            }
        }
        DefaultValue::FileOrDirectory(FileOrDirectory::Directory(dir)) => {
            stage_files(
                &FileOrDirectory::Directory(dir),
                workdir,
                stagedir,
                dirent.entryname.as_ref(),
                guest_workdir,
                path_mapper,
            )?;
            return Ok(());
        }
        DefaultValue::Any(value) => value.as_str().unwrap().to_string(),
    };

    let parent = staged_path.parent().unwrap();
    fs::create_dir_all(parent)?;
    //create the file
    fs::write(staged_path, string_content)?;
    Ok(())
}

fn stage_files(
    item: &FileOrDirectory,
    workdir: &Path,
    stagedir: &Path,
    entryname: Option<&String>,
    guest_workdir: &str,
    path_mapper: &mut PathMapper,
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
    fs::create_dir_all(parent)?;

    if let Some(path) = item.path() {
        if item.is_file() {
            fs::copy(path, staged_path)?;
        } else if item.is_dir() {
            copy_dir(path, staged_path)?;
        }
    } else if item.is_dir() {
        fs::create_dir_all(staged_path)?;

        //handle listing??
    }

    update_pathmap(guest_workdir, path_mapper, item, workdir, None)?;

    Ok(())
}

fn update_pathmap(
    guest_workdir: impl AsRef<Path>,
    path_mapper: &mut PathMapper,
    item: &FileOrDirectory,
    workdir: &Path,
    new_basename: Option<&String>,
) -> anyhow::Result<()> {
    let Some(path) = item.path() else {
        return Ok(());
    };
    let path = Path::new(path);

    let relative_path = if path.is_absolute()
        && let Ok(stripped) = path.strip_prefix(workdir)
    {
        stripped
    } else if path.is_absolute() {
        Path::new(path.file_name().unwrap())
    } else {
        path
    };

    let from = workdir.join(path);
    let staged = if let Some(new_basename) = new_basename {
        guest_workdir.as_ref().join(new_basename)
    } else {
        guest_workdir.as_ref().join(relative_path)
    };

    path_mapper.add_tripel(from, staged, relative_path)
}
