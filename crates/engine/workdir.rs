use crate::expression::{EvaluationContext, do_eval, do_eval_to_string};
use cwl_core::{
    OneOrMany,
    files::{Dirent, FileOrDirectory},
    inputs::DefaultValue,
    requirements::{InitialWorkDirRequirement, ListingItems, WorkDirItems},
};
use std::{fs, path::Path};

pub fn stage_work_dir(
    iwdr: &InitialWorkDirRequirement,
    workdir: &Path,
    stagedir: &Path,
    context: &EvaluationContext,
) -> anyhow::Result<()> {
    match &iwdr.listing {
        WorkDirItems::Expression(_) => todo!(), //TODO: expression eval
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
        ListingItems::Expression(_) => todo!(),
        ListingItems::Dirent(dirent) => stage_dirent(dirent, workdir, stagedir, context),
        ListingItems::FileOrDirectory(_file_or_directory) => todo!(),
        ListingItems::Vec(_items) => todo!(),
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
                fs::read_to_string(workdir.join(file.path.unwrap()))?
            }
        }
        DefaultValue::Any(value) => value.as_str().unwrap().to_string(),
        _ => unimplemented!(),
    };

    let entryname = dirent.clone().entryname.unwrap();
    let entryname = do_eval_to_string(&entryname, context);

    //create the file
    fs::write(stagedir.join(entryname), string_content)?;
    Ok(())
}
