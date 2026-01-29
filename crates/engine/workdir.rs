use crate::expression::{EvaluationContext, do_eval, do_eval_to_string};
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
) -> anyhow::Result<()> {
    match &iwdr.listing {
        WorkDirItems::Expression(expression) => {
            let evaluated = do_eval(expression, context)?;
            let items = &serde_yaml::from_value(evaluated)?;
            stage_item(items, workdir, stagedir, context)?;
            Ok(())
        }
        WorkDirItems::ListingItems(items) => match &**items {
            OneOrMany::One(item) => stage_item(item, workdir, stagedir, context),
            OneOrMany::Many(items) => {
                for item in items {
                    stage_item(item, workdir, stagedir, context)?;
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
) -> anyhow::Result<()> {
    match item {
        ListingItems::Expression(expression) => {
            let evaluated = do_eval(expression, context)?;
            let items = &serde_yaml::from_value(evaluated)?;
            stage_item(items, workdir, stagedir, context)?;
            Ok(())
        }
        ListingItems::Dirent(dirent) => stage_dirent(dirent, workdir, stagedir, context),
        ListingItems::FileOrDirectory(fod) => stage_files(fod, stagedir),
        ListingItems::Vec(items) => {
            for item in items {
                stage_files(item, stagedir)?;
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
) -> anyhow::Result<()> {
    //evaluate expression if so
    let evaluated_content = do_eval(&dirent.entry, context)?;

    //parse to DefaultValue
    let dv: DefaultValue = serde_yaml::from_value(evaluated_content)?;

    let string_content = match dv {
        DefaultValue::FileOrDirectory(FileOrDirectory::File(file)) => {
            if let Some(contents) = file.contents {
                contents.to_string()
            } else {
                let path = file.path.unwrap();
                if Path::new(&path).is_absolute() {
                    fs::read_to_string(path)?
                } else {
                    fs::read_to_string(workdir.join(path))?
                }
            }
        }
        DefaultValue::Any(value) => value.as_str().unwrap().to_string(),
        _ => unimplemented!(),
    };

    let entryname = dirent.clone().entryname.unwrap();
    let entryname = do_eval_to_string(&entryname, context);

    let staged_path = stagedir.join(entryname);

    let parent = staged_path.parent().unwrap();
    fs::create_dir_all(parent)?;
    //create the file
    fs::write(staged_path, string_content)?;
    Ok(())
}

fn stage_files(item: &FileOrDirectory, stagedir: &Path) -> anyhow::Result<()> {
    let path = item.path().unwrap();
    let path = PathBuf::from(path);
    let staged_path = stagedir.join(item.basename().unwrap());
    let parent = staged_path.parent().unwrap();
    fs::create_dir_all(parent)?;

    if item.is_file() {
        fs::copy(&path, &staged_path)?;
    } else if item.is_dir() {
        copy_dir(&path, &staged_path)?;
    }

    Ok(())
}
