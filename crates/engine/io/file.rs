use crate::{
    expression::{EvaluationContext, do_eval},
    io::{checksum, directory::locate_dir, get_location, get_relative_path},
    string_url_to_file_path,
};
use cwl_core::{
    BoolOrExpression, FileMetaData, FilePathMetaData, Integer, OneOrMany,
    documents::CWLDocument,
    files::{Directory, File, FileOrDirectory},
    get_file_metadata, get_path_metadata,
    inputs::{DefaultValue, InputSchema, InputType},
    types::SecondaryFileSchema,
};
use cwl_engine_storage::{Storage, StorageBackend};
use futures_util::{FutureExt, future::BoxFuture};
use std::{
    collections::HashMap,
    fs,
    path::{MAIN_SEPARATOR_STR, Path, PathBuf},
};
use tracing::debug;
use url::Url;

/// Reads size/checksum (and, if requested, contents) for a file
fn read_metadata_and_contents(
    path: &Path,
    load_contents: bool,
) -> anyhow::Result<(Option<FileMetaData>, Option<String>)> {
    let Ok(meta) = get_file_metadata(path) else {
        return Ok((None, None));
    };
    let contents = if load_contents {
        if meta.size < 64 * 1024 {
            Some(fs::read_to_string(path)?)
        } else {
            anyhow::bail!(
                "Can not load file contents if file is larger than {} bytes.",
                64 * 1024
            )
        }
    } else {
        None
    };
    Ok((Some(meta), contents))
}

/// locates a file by writing the location as Url and filling the staged metadata
pub(crate) fn locate_file<'a>(
    file: &'a mut File,
    work_dir: &'a Path,
    stage_dir: &'a Path,
    load_contents: bool,
    storage: &'a StorageBackend,
) -> BoxFuture<'a, anyhow::Result<()>> {
    async move {
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
            let path = file.dirname.clone().unwrap()
                + MAIN_SEPARATOR_STR
                + file.basename.as_ref().unwrap();
            file.path = Some(path);

            //try getting checksum, size and (if requested) contents. 
            let (metadata, contents) = if url.scheme() == "file" {
                read_metadata_and_contents(&string_url_to_file_path(&location)?, load_contents)?
            } else if storage.exists(&url).await? {
                let tmp = tempfile::NamedTempFile::new()?;
                storage.download(&url, tmp.path()).await?;
                read_metadata_and_contents(tmp.path(), load_contents)?
            } else {
                (None, None)
            };

            if let Some(FileMetaData { size, checksum }) = metadata {
                file.checksum = checksum;
                file.size = Some(Integer::Long(size.cast_signed()));
            }
            if let Some(contents) = contents {
                file.contents = Some(contents);
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
                        locate_file(file, work_dir, stage_dir, load_contents, storage).await?;
                    }
                    FileOrDirectory::Directory(dir) => {
                        locate_dir(dir, work_dir, stage_dir, None, storage).await?;
                    }
                }
            }
        }
        Ok(())
    }
    .boxed()
}

pub(crate) async fn collect_secondary_files_for_inputs(
    doc: &CWLDocument,
    values: &mut HashMap<String, DefaultValue>,
    context: &EvaluationContext<'_>,
    work_dir: &Path,
    storage: &StorageBackend,
) -> anyhow::Result<()> {
    for input in doc.get_inputs() {
        let value = values.get_mut(&input.id.unwrap());
        if let Some(value) = value {
            if let Some(secondary_files) = &input.secondary_files {
                handle_secondary_files_for_input(
                    value,
                    secondary_files,
                    context,
                    work_dir,
                    storage,
                )
                .await?;
            }

            //handle record field secondary files
            if let OneOrMany::One(InputType::InputSchema(schema)) = &input.r#type
                && let InputSchema::Record(rec) = &**schema
                && let Some(fields) = &rec.fields
            {
                for field in fields {
                    if let DefaultValue::Any(yaml_value) = value
                        && let Some(field_value) = yaml_value.get_mut(&field.name)
                    {
                        let mut dv = serde_json::from_value(field_value.clone())?;
                        if let Some(sec_files) = &field.secondary_files {
                            handle_secondary_files_for_input(
                                &mut dv, sec_files, context, work_dir, storage,
                            )
                            .await?;
                        } else {
                            set_secondary_files_empty(&mut dv)?;
                        }
                        *field_value = serde_json::to_value(dv)?;
                    }
                }
            }

            if input.secondary_files.is_none() {
                set_secondary_files_empty(value)?;
            }
        }
    }
    Ok(())
}

fn handle_secondary_files_for_input<'a>(
    value: &'a mut DefaultValue,
    secondary_files: &'a OneOrMany<SecondaryFileSchema>,
    context: &'a EvaluationContext<'a>,
    work_dir: &'a Path,
    storage: &'a StorageBackend,
) -> BoxFuture<'a, anyhow::Result<()>> {
    async move {
        match value {
            DefaultValue::FileOrDirectory(FileOrDirectory::File(file)) => {
                handle_secondary_files(file, secondary_files, work_dir, context, storage).await?;
            }
            DefaultValue::Any(serde_json::Value::Array(vec)) => {
                for item in vec {
                    let mut dv = serde_json::from_value(item.clone())?;
                    handle_secondary_files_for_input(
                        &mut dv,
                        secondary_files,
                        context,
                        work_dir,
                        storage,
                    )
                    .await?;
                    *item = serde_json::to_value(dv)?;
                }
            }
            DefaultValue::Any(serde_json::Value::Object(rec)) => {
                for item in rec.values_mut() {
                    let mut dv = serde_json::from_value(item.clone())?;
                    handle_secondary_files_for_input(
                        &mut dv,
                        secondary_files,
                        context,
                        work_dir,
                        storage,
                    )
                    .await?;
                    *item = serde_json::to_value(dv)?;
                }
            }
            _ => {}
        }
        Ok(())
    }
    .boxed()
}

fn set_secondary_files_empty(value: &mut DefaultValue) -> anyhow::Result<()> {
    match value {
        DefaultValue::FileOrDirectory(FileOrDirectory::File(file))
            if file.secondary_files.is_none() =>
        {
            file.secondary_files = Some(vec![]);
        }
        DefaultValue::Any(serde_json::Value::Array(vec)) => {
            for item in vec {
                let mut dv = serde_json::from_value(item.clone())?;
                set_secondary_files_empty(&mut dv)?;
                *item = serde_json::to_value(dv)?;
            }
        }
        DefaultValue::Any(serde_json::Value::Object(rec)) => {
            for item in rec.values_mut() {
                let mut dv = serde_json::from_value(item.clone())?;
                set_secondary_files_empty(&mut dv)?;
                *item = serde_json::to_value(dv)?;
            }
        }
        _ => {}
    }
    Ok(())
}

pub(crate) fn move_file(file: &mut File, workdir: &Path, basename: Option<&String>) {
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

pub(crate) async fn handle_secondary_files(
    file: &mut File,
    secondary_files: &OneOrMany<SecondaryFileSchema>,
    work_dir: &Path,
    context: &EvaluationContext<'_>,
    storage: &StorageBackend,
) -> anyhow::Result<()> {
    let Some(location) = &file.location else {
        debug!("Can not evaluate secondary_files as location is not set");
        return Ok(());
    };
    // just used to validate the location is a well-formed URL.
    Url::parse(location)?;

    let stage_dir = Path::new(file.dirname.as_ref().unwrap());

    let mut sec_files = vec![];

    //set self to file
    let json_context = serde_json::to_value(file.clone())?;
    let context = EvaluationContext {
        context: Some(&json_context),
        ..*context
    };

    for schema in secondary_files.as_many() {
        let result = handle_secondary_file_schema(file, &schema, &context, storage).await?;

        if let Some(mut items) = result {
            for item in &mut items {
                match item {
                    PathOrFile::Location(url) => {
                        if storage.is_dir(url).await? {
                            let mut sec_dir = Directory {
                                location: Some(url.to_string()),
                                ..Default::default()
                            };
                            //fill metadata
                            locate_dir(&mut sec_dir, work_dir, stage_dir, None, storage).await?;
                            sec_files.push(FileOrDirectory::Directory(sec_dir));
                        } else {
                            let mut sec_file = File {
                                location: Some(url.to_string()),
                                ..Default::default()
                            };
                            //fill metadata
                            locate_file(&mut sec_file, work_dir, stage_dir, false, storage).await?;
                            sec_files.push(FileOrDirectory::File(sec_file));
                        }
                    }
                    PathOrFile::File(item) => {
                        resolve_location_from_primary(file, item)?;
                        match &mut **item {
                            FileOrDirectory::File(file) => {
                                locate_file(file, work_dir, stage_dir, false, storage).await?;
                            }
                            FileOrDirectory::Directory(dir) => {
                                locate_dir(dir, work_dir, stage_dir, None, storage).await?;
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

pub(crate) fn resolve_location_from_primary(
    primary: &File,
    item: &mut FileOrDirectory,
) -> anyhow::Result<()> {
    if item.location().is_none()
        && let Some(path) = item.path().cloned()
    {
        let mut location = Url::parse(primary.location.as_ref().unwrap())?;
        location.set_path(&path);
        item.set_location(Some(location.to_string()));
    }
    Ok(())
}

#[derive(Debug)]
pub(crate) enum PathOrFile {
    Location(Url),
    File(Box<FileOrDirectory>),
}

pub(crate) async fn handle_secondary_file_schema(
    file: &File,
    item: &SecondaryFileSchema,
    context: &EvaluationContext<'_>,
    storage: &StorageBackend,
) -> anyhow::Result<Option<Vec<PathOrFile>>> {
    let location = file.location.as_ref().unwrap();
    let url = Url::parse(location)?;

    if let Ok(pattern_value) = do_eval(&item.pattern, context)
        && pattern_value != item.pattern
    {
        //we got a filename, list of filenames, fod or list of fod
        let dv: DefaultValue = serde_json::from_value(pattern_value)?;
        return handle_secondary_file_from_expression(dv, &url);
    }

    let pattern = item.pattern.clone();
    let secondary_url = apply_secondary_pattern(&url, &pattern);

    //check required and existent
    let is_required = if let Some(BoolOrExpression::Expression(req_exp)) = &item.required {
        do_eval(req_exp, context)?.as_bool().unwrap_or(false)
    } else if item.required.is_none() {
        true
    } else {
        matches!(&item.required, Some(BoolOrExpression::Bool(true)))
    };

    //if there are secondary files already we just validate what is there
    if let Some(sec_files) = &file.secondary_files
        && is_required
        && !sec_files
            .iter()
            .any(|f| f.location().unwrap() == &secondary_url.to_string())
    {
        anyhow::bail!("required secondary file not found {pattern} for {file:?}");
    }

    if !storage.exists(&secondary_url).await? {
        if is_required {
            anyhow::bail!("required secondary file not found {pattern}");
        }
        debug!("secondary file not found {pattern}");
        return Ok(None);
    }

    Ok(Some(vec![PathOrFile::Location(secondary_url)]))
}

/// Appends (or, for a `^.`-prefixed pattern, replaces the last extension of) `pattern` onto
/// `url`'s path, keeping the same scheme/host
fn apply_secondary_pattern(url: &Url, pattern: &str) -> Url {
    let mut new_url = url.clone();
    let mut path = url.path().to_owned();
    if let Some(new_ext) = pattern.strip_prefix("^.") {
        let mut pathbuf = PathBuf::from(&path);
        pathbuf.set_extension(new_ext);
        path = pathbuf.to_string_lossy().into_owned();
    } else {
        path.push_str(pattern);
    }
    new_url.set_path(&path);
    new_url
}

fn handle_secondary_file_from_expression(
    dv: DefaultValue,
    base_url: &Url,
) -> anyhow::Result<Option<Vec<PathOrFile>>> {
    match dv {
        DefaultValue::FileOrDirectory(fod) => Ok(Some(vec![PathOrFile::File(Box::new(fod))])),
        DefaultValue::Any(serde_json::Value::String(filename)) => {
            let url = base_url.join(&filename).map_err(|e| {
                anyhow::anyhow!("Could not resolve secondary file name {filename}: {e}")
            })?;
            Ok(Some(vec![PathOrFile::Location(url)]))
        }
        DefaultValue::Any(serde_json::Value::Array(vec)) => {
            let mut values = vec![];
            for item in vec {
                let dv: DefaultValue = serde_json::from_value(item)?;
                let res = handle_secondary_file_from_expression(dv, base_url)?;
                if let Some(res) = res {
                    values.extend(res);
                }
            }
            Ok(Some(values))
        }
        DefaultValue::Any(_) => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[cfg(unix)]
    async fn test_locate_file() {
        let path = "my_file.txt";
        let workdir = Path::new("/mnt/mydir");
        let stagedir = Path::new("/mnt/task/inputs/");
        let storage = StorageBackend::new();

        let mut file = File::builder().location(path).build();
        let expected = File::builder()
            .location("file:///mnt/mydir/my_file.txt")
            .path("/mnt/task/inputs/my_file.txt")
            .basename("my_file.txt")
            .nameext(".txt")
            .nameroot("my_file")
            .dirname("/mnt/task/inputs")
            .build();

        locate_file(&mut file, workdir, stagedir, false, &storage)
            .await
            .unwrap();
        assert_eq!(file, expected);
    }

    #[tokio::test]
    #[cfg(windows)]
    async fn test_locate_file() {
        let path = "my_file.txt";
        let workdir = Path::new(r"C:\mnt\mydir");
        let stagedir = Path::new(r"C:\mnt\task\inputs");
        let storage = StorageBackend::new();

        let mut file = File::builder().location(path).build();
        let expected = File::builder()
            .location("file:///C:/mnt/mydir/my_file.txt")
            .path(r"C:\mnt\task\inputs\my_file.txt")
            .basename("my_file.txt")
            .nameext(".txt")
            .nameroot("my_file")
            .dirname(r"C:\mnt\task\inputs")
            .build();

        locate_file(&mut file, workdir, stagedir, false, &storage)
            .await
            .unwrap();
        assert_eq!(file, expected);
    }

    #[test]
    fn test_apply_secondary_pattern_append_local() {
        let url = Url::parse("file:///out/sample.bam").unwrap();
        let result = apply_secondary_pattern(&url, ".bai");
        assert_eq!(result.as_str(), "file:///out/sample.bam.bai");
    }

    #[test]
    fn test_apply_secondary_pattern_replace_extension_remote() {
        let url = Url::parse("s3://my-bucket/tmp/xyz/sample.bam").unwrap();
        let result = apply_secondary_pattern(&url, "^.bai");
        assert_eq!(result.as_str(), "s3://my-bucket/tmp/xyz/sample.bai");
    }

    #[test]
    fn test_resolve_location_from_primary_reconstructs_remote_location() {
        // s3:// URLs put the bucket in the host, not the path (see S3Storage::parse_uri) -
        // `.path()` on a StoragePath::Remote is just the key, e.g. "/tmp/xyz/out.txt".
        let primary = File::builder()
            .location("s3://my-bucket/tmp/xyz/out.txt")
            .build();
        let mut secondary =
            FileOrDirectory::File(File::builder().path("/tmp/xyz/out.accessory").build());

        resolve_location_from_primary(&primary, &mut secondary).unwrap();

        assert_eq!(
            secondary.location().unwrap(),
            "s3://my-bucket/tmp/xyz/out.accessory"
        );
    }

    #[test]
    fn test_resolve_location_from_primary_leaves_existing_location_untouched() {
        let primary = File::builder().location("s3://my-bucket/out.txt").build();
        let mut secondary = FileOrDirectory::File(
            File::builder()
                .location("s3://my-bucket/already-resolved.txt")
                .path("/some/other/path")
                .build(),
        );

        resolve_location_from_primary(&primary, &mut secondary).unwrap();

        assert_eq!(
            secondary.location().unwrap(),
            "s3://my-bucket/already-resolved.txt"
        );
    }
}
