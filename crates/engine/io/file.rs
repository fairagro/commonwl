use crate::{
    checksum,
    expression::{EvaluationContext, do_eval},
    io::{directory::locate_dir, get_location, get_relative_path},
};
use cwl_core::{
    BoolOrExpression, FileMetaData, FilePathMetaData, Integer, OneOrMany,
    documents::CWLDocument,
    files::{Directory, File, FileOrDirectory},
    get_file_metadata, get_path_metadata,
    inputs::{DefaultValue, InputSchema, InputType},
    types::SecondaryFileSchema,
};
use std::{
    collections::HashMap,
    fs,
    path::{MAIN_SEPARATOR_STR, Path, PathBuf},
};
use tracing::debug;
use url::Url;

/// locates a file by writing the location as Url and filling the staged metadata
pub fn locate_file(
    file: &mut File,
    work_dir: &Path,
    stage_dir: &Path,
    load_contents: bool,
) -> anyhow::Result<()> {
    if let Some(path) = &file.path
        && file.location.is_none()
    {
        file.location = Some(get_location(path, work_dir));
    }

    if let Some(location) = &file.location {
        //make absolute URI
        let location = get_location(location, work_dir);

        let url = Url::parse(&location)?;
        let relative_path = get_relative_path(&url, work_dir)?;
        let designated_path = stage_dir.join(&relative_path);

        file.location = Some(location.clone());

        //calculate file metadata for designated path
        let FilePathMetaData {
            basename,
            nameroot,
            nameext,
            dirname,
        } = get_path_metadata(&designated_path);

        if file.basename.is_none() {
            file.basename = basename;
        }

        if file.nameroot.is_none() {
            file.nameroot = nameroot;
        }

        if file.nameext.is_none() {
            file.nameext = nameext;
        }

        file.dirname = dirname;

        //We set them before!
        let path =
            file.dirname.clone().unwrap() + MAIN_SEPARATOR_STR + file.basename.as_ref().unwrap();
        file.path = Some(path);

        //try getting checksum and size (currently for local files only). Ignores failure (which usually means the file does not exist!)
        if url.scheme() == "file"
            && let Ok(FileMetaData { size, checksum }) =
                get_file_metadata(Path::new(location.strip_prefix("file://").unwrap()))
        {
            file.checksum = checksum;
            file.size = Some(Integer::Long(size as i64));
            if load_contents && size < 64 * 1024 {
                file.contents = Some(fs::read_to_string(
                    location.strip_prefix("file://").unwrap(),
                )?);
            } else if load_contents {
                anyhow::bail!(
                    "Can not load file contents if file is larger than {} bytes.",
                    64 * 1024
                )
            }
        }
    }

    if let Some(contents) = &file.contents
        && file.location.is_none()
    {
        let mut checksum = checksum(contents);
        let path = stage_dir.join(checksum.split_off(5));
        file.path = Some(path.to_string_lossy().into());
    }

    if let Some(secondary_files) = &mut file.secondary_files {
        for item in secondary_files {
            match item {
                FileOrDirectory::File(file) => {
                    locate_file(file, work_dir, stage_dir, load_contents)?
                }
                FileOrDirectory::Directory(dir) => locate_dir(dir, work_dir, stage_dir, None)?,
            }
        }
    }
    Ok(())
}

pub fn collect_secondary_files_for_inputs(
    doc: &CWLDocument,
    values: &mut HashMap<String, DefaultValue>,
    context: &EvaluationContext,
    work_dir: &Path,
) -> anyhow::Result<()> {
    for input in doc.get_inputs() {
        let value = values.get_mut(&input.id.unwrap());
        if let Some(value) = value {
            if let Some(secondary_files) = input.secondary_files {
                handle_secondary_files_for_input(value, &secondary_files, context, work_dir)?;
            }

            //handle record field secondary files
            if let OneOrMany::One(InputType::InputSchema(schema)) = &input.r#type
                && let InputSchema::Record(rec) = &**schema
                && let Some(fields) = &rec.fields
            {
                for field in fields {
                    if let Some(sec_files) = &field.secondary_files
                        && let DefaultValue::Any(yaml_value) = value
                        && let Some(field_value) = yaml_value.get_mut(&field.name)
                    {
                        let mut dv = serde_yaml::from_value(field_value.clone())?;
                        handle_secondary_files_for_input(&mut dv, sec_files, context, work_dir)?;
                        *field_value = serde_yaml::to_value(dv)?;
                    }
                }
            }
        }
    }
    Ok(())
}

fn handle_secondary_files_for_input(
    value: &mut DefaultValue,
    secondary_files: &OneOrMany<SecondaryFileSchema>,
    context: &EvaluationContext,
    work_dir: &Path,
) -> anyhow::Result<()> {
    match value {
        DefaultValue::FileOrDirectory(FileOrDirectory::File(file)) => {
            handle_secondary_files(file, secondary_files, work_dir, context)?;
        }
        DefaultValue::Any(serde_yaml::Value::Sequence(vec)) => {
            for item in vec {
                let mut dv = serde_yaml::from_value(item.clone())?;
                handle_secondary_files_for_input(&mut dv, secondary_files, context, work_dir)?;
                *item = serde_yaml::to_value(dv)?;
            }
        }
        DefaultValue::Any(serde_yaml::Value::Mapping(rec)) => {
            for item in rec.values_mut() {
                let mut dv = serde_yaml::from_value(item.clone())?;
                handle_secondary_files_for_input(&mut dv, secondary_files, context, work_dir)?;
                *item = serde_yaml::to_value(dv)?;
            }
        }
        _ => {}
    }
    Ok(())
}

pub fn move_file(file: &mut File, workdir: &Path, basename: Option<&String>) {
    let basename = basename.unwrap_or(file.basename.as_ref().unwrap());
    let designated_path = workdir.join(basename);
    let FilePathMetaData {
        basename,
        nameroot: _,
        nameext: _,
        dirname,
    } = get_path_metadata(&designated_path);

    let path = designated_path.to_string_lossy().into_owned();
    file.path = Some(path);

    file.basename = basename;
    file.dirname = dirname;
}

pub fn handle_secondary_files(
    file: &mut File,
    secondary_files: &OneOrMany<SecondaryFileSchema>,
    work_dir: &Path,
    context: &EvaluationContext,
) -> anyhow::Result<()> {
    let Some(location) = &file.location else {
        debug!("Can not evaluate secondary_files as location is not set");
        return Ok(());
    };

    let url = Url::parse(location)?;
    if url.scheme() != "file" {
        debug!("Only local files are supported for secondary_files right now");
        return Ok(());
    }
    let location_path = url.path();

    let stage_dir = Path::new(file.dirname.as_ref().unwrap());

    let mut sec_files = vec![];

    //set self to file
    let json_context = serde_json::to_value(file.clone())?;
    let context = EvaluationContext {
        context: Some(&json_context),
        ..*context
    };

    for schema in secondary_files.as_many() {
        let result = handle_secondary_file_schema(location_path, &schema, &context)?;

        if let Some(mut items) = result {
            for item in &mut items {
                match item {
                    PathOrFile::Path(item) => {
                        if item.is_file() {
                            let mut sec_file = File {
                                location: Some(item.to_string_lossy().into_owned()),
                                ..Default::default()
                            };
                            //fill metadata
                            locate_file(&mut sec_file, work_dir, stage_dir, false)?;
                            sec_files.push(FileOrDirectory::File(sec_file));
                        } else if item.is_dir() {
                            let mut sec_dir = Directory {
                                location: Some(item.to_string_lossy().into_owned()),
                                ..Default::default()
                            };
                            //fill metadata
                            locate_dir(&mut sec_dir, work_dir, stage_dir, None)?;
                            sec_files.push(FileOrDirectory::Directory(sec_dir));
                        }
                    }
                    PathOrFile::File(item) => {
                        match &mut **item {
                            FileOrDirectory::File(file) => {
                                locate_file(file, work_dir, stage_dir, false)?
                            }
                            FileOrDirectory::Directory(dir) => {
                                locate_dir(dir, work_dir, stage_dir, None)?
                            }
                        }
                        sec_files.push(*item.clone());
                    }
                }
            }
        }
    }

    file.secondary_files = Some(sec_files);

    Ok(())
}

#[derive(Debug)]
pub(crate) enum PathOrFile {
    Path(PathBuf),
    File(Box<FileOrDirectory>),
}

pub(crate) fn handle_secondary_file_schema(
    path: impl AsRef<Path>,
    item: &SecondaryFileSchema,
    context: &EvaluationContext,
) -> anyhow::Result<Option<Vec<PathOrFile>>> {
    if let Ok(pattern_value) = do_eval(&item.pattern, context)
        && pattern_value != item.pattern
    {
        //we got a filename, list of filenames, fod or list of fod
        let dv: DefaultValue = serde_yaml::from_value(pattern_value)?;
        return handle_secondary_file_from_expression(dv, path);
    }

    let pattern = item.pattern.clone();
    let mut secondary_path_str = path.as_ref().as_os_str().to_owned();
    if let Some(new_ext) = pattern.strip_prefix("^.") {
        let mut pathbuf = PathBuf::from(&secondary_path_str);
        pathbuf.set_extension(new_ext);
        secondary_path_str = pathbuf.into_os_string();
    } else {
        secondary_path_str.push(&pattern);
    }

    //check required and existent
    let is_required = if let Some(BoolOrExpression::Expression(req_exp)) = &item.required {
        do_eval(req_exp, context)?.as_bool().unwrap_or(false)
    } else {
        matches!(&item.required, Some(BoolOrExpression::Bool(true)))
    };
    let secondary_path = Path::new(&secondary_path_str);
    if !secondary_path.exists() && !context.workdir.unwrap().join(secondary_path).exists() {
        if is_required {
            anyhow::bail!("required secondary file not found {pattern}");
        }
        debug!("secondary file not found {pattern}");
        return Ok(None);
    }

    Ok(Some(vec![PathOrFile::Path(PathBuf::from(
        secondary_path_str,
    ))]))
}

fn handle_secondary_file_from_expression(
    dv: DefaultValue,
    path: impl AsRef<Path>,
) -> anyhow::Result<Option<Vec<PathOrFile>>> {
    match dv {
        DefaultValue::FileOrDirectory(fod) => Ok(Some(vec![PathOrFile::File(Box::new(fod))])),
        DefaultValue::Any(serde_yaml::Value::String(filename)) => {
            let parent = path.as_ref().parent().unwrap();
            Ok(Some(vec![PathOrFile::Path(parent.join(filename))]))
        }
        DefaultValue::Any(serde_yaml::Value::Sequence(vec)) => {
            let mut values = vec![];
            for item in vec {
                let dv: DefaultValue = serde_yaml::from_value(item)?;
                let res = handle_secondary_file_from_expression(dv, path.as_ref())?;
                if let Some(res) = res {
                    values.extend(res);
                }
            }
            Ok(Some(values))
        }
        _ => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_locate_file() {
        let path = "my_file.txt";
        let workdir = Path::new("/mnt/mydir");
        let stagedir = Path::new("/mnt/task/inputs/");

        let mut file = File::builder().location(path).build();
        let expected = File::builder()
            .location("file:///mnt/mydir/my_file.txt")
            .path("/mnt/task/inputs/my_file.txt")
            .basename("my_file.txt")
            .nameext(".txt")
            .nameroot("my_file")
            .dirname("/mnt/task/inputs")
            .build();

        locate_file(&mut file, workdir, stagedir, false).unwrap();

        assert_eq!(file, expected);
    }
}
