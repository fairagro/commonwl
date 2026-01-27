use crate::expression::{EvaluationContext, do_eval};
use cwl_core::{
    OneOrMany,
    files::Dirent,
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
        ListingItems::FileOrDirectory(file_or_directory) => todo!(),
        ListingItems::Vec(items) => todo!(),
    }
}

fn stage_dirent(
    dirent: &Dirent,
    _workdir: &Path,
    stagedir: &Path,
    context: &EvaluationContext,
) -> anyhow::Result<()> {
    let evaluated_content = do_eval(&dirent.entry, context)?;
    let string_content = evaluated_content.as_str().unwrap();
    fs::write(stagedir.join(dirent.entryname.clone().unwrap()), string_content)?;
    Ok(())
}
