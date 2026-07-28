use crate::{
    expression::{EvaluationContext, do_eval, do_eval_to_string},
    io::{
        file::{PathOrFile, handle_secondary_file_schema, resolve_location_from_primary},
        unique_path,
    },
    schema::{format_validation::FormatValidator, validation::validate_type},
    string_url_to_path_string,
    workflow::{handle_link_merge, handle_pick_value},
};
use anyhow::Context;
use cwl_core::{
    FileMetaData, FilePathMetaData, Integer, OneOrMany,
    files::{Directory, File, FileOrDirectory, LoadListingEnum},
    get_file_metadata, get_path_metadata,
    inputs::DefaultValue,
    outputs::{
        CommandOutputBinding, CommandOutputParameter, CommandOutputParameterType,
        CommandOutputSchema, CommandOutputType, ExpressionToolOutputParameter, LinkMergeMethod,
        OutputType, WorkflowOutputParameter,
    },
    requirements::MultipleInputFeatureRequirement,
    types::{CWLType, SecondaryFileSchema},
};
use cwl_engine_storage::{Storage, StorageBackend, StoragePath};
use futures_util::{FutureExt, future::BoxFuture};
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    sync::Arc,
};
use tracing::{debug, warn};
use url::Url;

#[derive(Debug)]
pub struct OutputCollectionContext<'a> {
    pub source_dir: &'a StoragePath,
    pub dest_dir: &'a Path,
    pub workdir: &'a Path,
    pub eval_context: &'a EvaluationContext<'a>,
    pub validator: &'a FormatValidator,
}

/// handles collection of command outputs after execution
pub(crate) async fn collect_command_outputs(
    outputs: &[CommandOutputParameter],
    stdout_file: &StoragePath,
    stderr_file: &StoragePath,
    context: &OutputCollectionContext<'_>,
    storage: Arc<StorageBackend>,
) -> anyhow::Result<HashMap<String, DefaultValue>> {
    let cwl_output_json = &context.source_dir.join("cwl.output.json")?.as_url()?;
    if storage.exists(cwl_output_json).await? {
        let contents = storage.read_file(cwl_output_json).await?;
        let mut values: HashMap<String, DefaultValue> = serde_json::from_str(&contents)?;
        for value in values.values_mut() {
            match value {
                DefaultValue::FileOrDirectory(FileOrDirectory::File(f)) => {
                    let path = f.path.clone().or(f.location.clone()).ok_or_else(|| {
                        anyhow::anyhow!(
                            "cwl.output.json file entry has neither 'path' nor 'location'"
                        )
                    })?;
                    let path = string_url_to_path_string(&path).unwrap_or(path);
                    let path = correct_output_path(Path::new(&path), context);
                    //can file have secondary files here?
                    *value = handle_file(&path, None, context, storage.clone()).await?;
                }
                DefaultValue::FileOrDirectory(FileOrDirectory::Directory(d)) => {
                    let path = d.path.clone().or(d.location.clone()).ok_or_else(|| {
                        anyhow::anyhow!(
                            "cwl.output.json directory entry has neither 'path' nor 'location'"
                        )
                    })?;
                    let path = string_url_to_path_string(&path).unwrap_or(path);
                    let path = correct_output_path(Path::new(&path), context);
                    *value = handle_dir(&path, context, storage.clone()).await?;
                }
                _ => {}
            }
        }
        return Ok(values);
    }

    //collect outputs first
    let mut output_map = HashMap::new();
    for output in outputs {
        let output_id = output.id.clone().unwrap_or_default();
        let value =
            collect_output_item(output, stdout_file, stderr_file, context, storage.clone()).await?;
        output_map.insert(output_id, value);
    }

    Ok(output_map)
}

fn correct_output_path(path: &Path, context: &OutputCollectionContext) -> StoragePath {
    match &context.source_dir {
        StoragePath::Local(base) => {
            // existing prefix-stripping logic, returns StoragePath::Local
            if path.starts_with(base) {
                StoragePath::Local(path.to_path_buf())
            } else if let Ok(stripped) = path.strip_prefix(context.dest_dir) {
                StoragePath::Local(base.join(stripped))
            } else if let Ok(stripped) = path.strip_prefix(context.workdir) {
                StoragePath::Local(base.join(stripped))
            } else {
                StoragePath::Local(base.join(path))
            }
        }
        StoragePath::Remote(base_url) => {
            // path here came from cwl.output.json which uses container paths
            // strip container workdir prefix and rebase onto S3
            let stripped = path.strip_prefix(context.workdir).unwrap_or(path);
            StoragePath::from_url(base_url.join(&stripped.to_string_lossy()).unwrap())
        }
    }
}

async fn evaluate_command_binding(
    binding: &CommandOutputBinding,
    context: &OutputCollectionContext<'_>,
    secondary_files: Option<&OneOrMany<SecondaryFileSchema>>,
    output_id: &String,
    storage: Arc<StorageBackend>,
) -> anyhow::Result<Vec<DefaultValue>> {
    let mut results = vec![];

    //collect items via globs
    if let Some(globs) = &binding.glob {
        for glob_ in get_globs(globs, context.eval_context) {
            for item in storage.glob(&context.source_dir.as_url()?, &glob_).await? {
                // never surface S3-empty-directory placeholder as it were real tool output - it can end up glob-matched here directly
                if item.file_name().as_deref() == Some(crate::backend::mount::S3_EMPTY_DIR_MARKER) {
                    continue;
                }
                let fod = if item.is_dir() {
                    let basename = item.file_name();
                    let mut dir = Directory::builder()
                        .path(item.path())
                        .location(item.as_url()?)
                        .maybe_basename(basename)
                        .build();
                    // listing needs a real local path to walk
                    if item.is_local() {
                        if let Some(load_listing) = binding.load_listing {
                            dir.load_listing(load_listing)?;
                        } else {
                            dir.load_listing(LoadListingEnum::DeepListing)?;
                        }
                    }
                    FileOrDirectory::Directory(dir)
                } else {
                    let mut file = File::builder()
                        .maybe_basename(item.file_name())
                        .location(item.as_url()?)
                        .path(item.path())
                        .build();
                    let FilePathMetaData {
                        basename,
                        nameroot,
                        nameext,
                        dirname,
                    } = get_path_metadata(Path::new(&item.path()));
                    file.basename = basename;
                    file.nameext = nameext;
                    file.nameroot = nameroot;
                    file.dirname = dirname;

                    //handle load_contents
                    if let Some(load_contents) = &binding.load_contents
                        && *load_contents
                    {
                        file.contents = storage.read_file(&item.as_url()?).await.ok();
                    }
                    FileOrDirectory::File(file)
                };
                results.push(DefaultValue::FileOrDirectory(fod));
            }
        }
    }

    //handle output_eval
    if let Some(output_eval) = &binding.output_eval {
        let value = serde_json::to_value(&results)?;
        let eval_context = context.eval_context.clone().with_context(&value);
        results = match do_eval(output_eval, &eval_context) {
            Ok(value) => match value {
                serde_json::Value::Array(vals) => vals
                    .into_iter()
                    .filter_map(|item| serde_json::from_value(item).ok())
                    .collect(),
                single_value => vec![serde_json::from_value(single_value)?],
            },
            Err(e) => anyhow::bail!(
                "Failed to evaluate outputEval expression for output {output_id}: {e}"
            ),
        };
    }

    //handle secondary_files
    if let Some(secondary_files) = secondary_files {
        for item in &mut results {
            if let DefaultValue::FileOrDirectory(FileOrDirectory::File(file)) = item {
                let json_value = serde_json::to_value(&file)?;
                let eval_context = EvaluationContext {
                    context: Some(&json_value),
                    ..*context.eval_context
                };
                file.secondary_files =
                    handle_secondary_files(file, secondary_files, &eval_context, &storage)
                        .await
                        .ok();
            }
        }
    }

    Ok(results)
}

async fn handle_secondary_files(
    file: &File,
    secondary_files: &OneOrMany<SecondaryFileSchema>,
    context: &EvaluationContext<'_>,
    storage: &StorageBackend,
) -> anyhow::Result<Vec<FileOrDirectory>> {
    if file.location.is_none() {
        debug!("Can not evaluate secondary_files as location is not set");
        return Ok(vec![]);
    }
    let mut secondaries = vec![];
    for item in &secondary_files.as_many() {
        let Some(secondary_file) =
            handle_secondary_file_schema(file, item, context, storage).await?
        else {
            continue;
        };
        for item in secondary_file {
            match item {
                PathOrFile::Location(url) => {
                    let basename = Path::new(url.path())
                        .file_name()
                        .map(|s| s.to_string_lossy().into_owned());
                    if storage.is_dir(&url).await? {
                        let mut dir = Directory {
                            location: Some(url.to_string()),
                            basename,
                            ..Default::default()
                        };
                        if url.scheme() == "file" {
                            dir.path = url
                                .to_file_path()
                                .ok()
                                .map(|p| p.to_string_lossy().into_owned());
                            dir.load_listing(LoadListingEnum::DeepListing)?;
                        }
                        secondaries.push(FileOrDirectory::Directory(dir));
                    } else {
                        // metadata (size/checksum) and staging into the output dir happen later
                        let file = File {
                            location: Some(url.to_string()),
                            basename,
                            ..Default::default()
                        };
                        secondaries.push(FileOrDirectory::File(file));
                    }
                }
                PathOrFile::File(mut fod) => {
                    resolve_location_from_primary(file, &mut fod)?;
                    secondaries.push(*fod);
                }
            }
        }
    }
    Ok(secondaries)
}

/// validates new paths for files and directories
fn validate_output_item<'a>(
    item: &'a mut FileOrDirectory,
    format: Option<&'a String>,
    context: &'a OutputCollectionContext<'_>,
    base_path: &'a Path,
    copy: bool, // indicates whether we need to copy file and dir
    storage: Arc<StorageBackend>,
) -> BoxFuture<'a, anyhow::Result<()>> {
    async move {
        match item {
            FileOrDirectory::File(file) => {
                validate_file(file, format, context, base_path, copy, storage).await?;
            }
            FileOrDirectory::Directory(dir) => {
                validate_dir(dir, context, base_path, copy, storage).await?;
            }
        }

        Ok(())
    }
    .boxed()
}

/// sets the designated path to the file and copies it and its `secondary_files` recursively to the output folder
async fn validate_file(
    file: &mut File,
    format: Option<&String>,
    context: &OutputCollectionContext<'_>,
    base_path: &Path,
    copy: bool,
    storage: Arc<StorageBackend>,
) -> anyhow::Result<()> {
    let path =
        get_designated_path(file.path.as_ref(), base_path, file.basename.as_ref()).map(|p| {
            if copy {
                unique_path(&p, file.path.as_ref())
            } else {
                p
            }
        });
    let dirname = path.as_ref().and_then(|p| p.parent());
    let location = file.location.as_ref().or(file.path.as_ref());

    if let Some(source_location) = &location
        && let Some(dest_path) = &path
    {
        let u_source_location = Url::parse(source_location)
            .or_else(|_| Url::from_file_path(source_location))
            .map_err(|()| anyhow::anyhow!("invalid source location: {source_location}"))?;
        if storage.exists(&u_source_location).await? {
            if copy {
                let parent = dest_path.parent().unwrap();
                if !parent.exists() {
                    fs::create_dir_all(parent)?;
                }

                storage.download(&u_source_location, dest_path).await?;
            }
            if file.size.is_none() || file.checksum.is_none() {
                let FileMetaData { size, checksum } = get_file_metadata(dest_path)?;
                file.checksum = checksum;
                file.size = Some(Integer::Long(size.cast_signed()));
            }
        } else {
            anyhow::bail!("Source location {source_location} does not exist for output file");
        }
    } else if let Some(dest_path) = &path
        && let Some(contents) = &file.contents
    {
        //create the literal file
        let parent = dest_path.parent().unwrap();
        if !parent.exists() {
            fs::create_dir_all(parent)?;
        }

        fs::write(dest_path, contents)
            .with_context(|| format!("Could not write contents to {}", dest_path.display()))?;

        if file.size.is_none() || file.checksum.is_none() {
            let FileMetaData { size, checksum } = get_file_metadata(dest_path)?;
            file.checksum = checksum;
            file.size = Some(Integer::Long(size.cast_signed()));
        }
    }

    let format = context.validator.handle(format, Some(context.eval_context));
    file.format = format;

    file.path = path.as_ref().map(|p| p.to_string_lossy().into_owned());
    file.dirname = dirname.as_ref().map(|p| p.to_string_lossy().into_owned());
    file.location = file.path.as_ref().and_then(|p| {
        Url::from_file_path(p)
            .map_err(|()| anyhow::anyhow!("Could not create URL from {p}"))
            .unwrap()
            .to_string()
            .into()
    });

    if file.basename.is_none() {
        file.basename = path
            .as_ref()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned());
    }

    if let Some(secondary_files) = &mut file.secondary_files {
        for item in secondary_files {
            validate_output_item(item, None, context, base_path, true, storage.clone()).await?;
        }
    }

    Ok(())
}

/// sets the designated path to the directory and copies it and its contents recursively to the output folder
async fn validate_dir(
    dir: &mut Directory,
    context: &OutputCollectionContext<'_>,
    base_path: &Path,
    copy: bool,
    storage: Arc<StorageBackend>,
) -> anyhow::Result<()> {
    let path = get_designated_path(dir.path.as_ref(), base_path, dir.basename.as_ref())
        .map(|p| if copy { unique_path(&p, None) } else { p });

    let location = dir.location.as_ref().or(dir.path.as_ref());

    if let Some(source_location) = &location
        && let Some(dest_path) = &path
    {
        let u_source_location = Url::parse(source_location)
            .or_else(|_| Url::from_file_path(source_location))
            .map_err(|()| anyhow::anyhow!("invalid source location: {source_location}"))?;

        let mut downloaded = false;

        if storage.exists(&u_source_location).await? || storage.is_dir(&u_source_location).await? {
            if copy {
                let parent = dest_path.parent().unwrap();
                if !parent.exists() {
                    fs::create_dir_all(parent)?;
                }
                storage.download(&u_source_location, dest_path).await?;
                downloaded = true;
            }
        } else {
            anyhow::bail!("Source location {source_location} does not exist for output directory");
        }

        // remove the marker file
        if downloaded && dir.listing.is_none() && u_source_location.scheme() != "file" {
            remove_empty_dir_markers(dest_path)?;
            // load listing for remote objects after download
            dir.path = Some(dest_path.to_string_lossy().into_owned());
            dir.load_listing(LoadListingEnum::DeepListing)?;
        }
    } else if let Some(dest_path) = &path {
        //no source path, but we still want to create the directory
        fs::create_dir_all(dest_path)?;
    }

    let copy = dir.path.is_none();
    dir.path = path.as_ref().map(|p| p.to_string_lossy().into_owned());
    dir.location = dir.path.as_ref().and_then(|p| {
        Url::from_file_path(p)
            .map_err(|()| anyhow::anyhow!("Could not create URL from {p}"))
            .unwrap()
            .to_string()
            .into()
    });

    let base_path = path.unwrap();
    if let Some(listing) = &mut dir.listing {
        for item in listing {
            validate_output_item(item, None, context, &base_path, copy, storage.clone()).await?;
        }
    }

    Ok(())
}

/// Recursively removes any `S3_EMPTY_DIR_MARKER` placeholder file
fn remove_empty_dir_markers(dir: &Path) -> anyhow::Result<()> {
    for entry in fs::read_dir(dir)?.flatten() {
        let path = entry.path();
        if path.is_dir() {
            remove_empty_dir_markers(&path)?;
        } else if path.file_name().and_then(|n| n.to_str())
            == Some(crate::backend::mount::S3_EMPTY_DIR_MARKER)
        {
            fs::remove_file(&path)?;
        }
    }
    Ok(())
}

/// creates the new output path by stripping prefixes of known folders
fn get_designated_path(
    path: Option<&String>,
    base_path: &Path,
    basename: Option<&String>,
) -> Option<PathBuf> {
    if path.is_none() {
        return Some(base_path.join(basename.unwrap()));
    }
    path.as_ref().map(|p| {
        let path = Path::new(p);
        let filename = path.file_name().unwrap().to_string_lossy();

        base_path.join(basename.unwrap_or(&filename.to_string()))
    })
}

///collects a single output item
async fn collect_output_item(
    output: &CommandOutputParameter,
    stdout_file: &StoragePath,
    stderr_file: &StoragePath,
    context: &OutputCollectionContext<'_>,
    storage: Arc<StorageBackend>,
) -> anyhow::Result<DefaultValue> {
    let format = output.format.as_ref().map(|f| f.as_one().clone());
    let value = match &output.r#type {
        CommandOutputParameterType::Stdout => {
            handle_file(stdout_file, format, context, storage).await
        }
        CommandOutputParameterType::Stderr => {
            handle_file(stderr_file, format, context, storage).await
        }
        CommandOutputParameterType::CommandOutputType(r#type) => {
            collect_item(
                output,
                output.output_binding.as_ref(),
                r#type,
                format.as_ref(),
                output.secondary_files.as_ref(),
                context,
                storage,
            )
            .await
        }
    }?;

    //validate output to schema
    if let CommandOutputParameterType::CommandOutputType(r#type) = &output.r#type {
        let valid = validate_output_type(&r#type.clone().into(), &value);
        if !valid {
            anyhow::bail!("Output value {value:?} does not match output type {type:?}")
        }
    }

    Ok(value)
}

// recursive implementention of item collection
fn collect_item<'a>(
    output: &'a CommandOutputParameter,
    output_binding: Option<&'a CommandOutputBinding>,
    item: &'a OneOrMany<CommandOutputType>,
    format: Option<&'a String>,
    secondary_files: Option<&'a OneOrMany<SecondaryFileSchema>>,
    context: &'a OutputCollectionContext<'_>,
    storage: Arc<StorageBackend>,
) -> BoxFuture<'a, anyhow::Result<DefaultValue>> {
    async move {
    let output_id = output.id.clone().unwrap_or_default();

    let is_optional = matches!(item, OneOrMany::Many(i) if i.contains(&CommandOutputType::CWLType(CWLType::Null)));
    let single = match item {
        OneOrMany::One(CommandOutputType::CommandOutputSchema(schema))
            if matches!(&**schema, CommandOutputSchema::Array(_)) =>
        {
            false
        }
        OneOrMany::Many(items) => {
            if items.contains(&CommandOutputType::CWLType(CWLType::Any)) {
                false
            } else {
                // Check if any item is a CommandOutputSchema::Array
                !items.iter().any(|item| {
                    matches!(
                        item,
                        CommandOutputType::CommandOutputSchema(schema)
                            if matches!(&**schema, CommandOutputSchema::Array(_))
                    )
                })
            }
        }
        OneOrMany::One(CommandOutputType::CWLType(CWLType::Any)) => false,
        OneOrMany::One(_) => true,
    };
    let is_any = match item {
        OneOrMany::One(CommandOutputType::CWLType(CWLType::Any)) => true,
        OneOrMany::Many(items) if items.contains(&CommandOutputType::CWLType(CWLType::Any)) => true,
        _ => false,
    };

    let value = match item {
        OneOrMany::One(CommandOutputType::CommandOutputSchema(schema))
            if matches!(&**schema, CommandOutputSchema::Record(_)) && output_binding.is_none() =>
        //collect fields only if no binding on output is present
        {
            let mut fields = HashMap::new();
            if let CommandOutputSchema::Record(record) = &**schema
                && let Some(record_fields) = &record.fields
            {
                for field in record_fields {
                    fields.insert(
                        field.name.clone(),
                        collect_item(
                            output,
                            field.output_binding.as_ref(),
                            &field.r#type,
                            field.format.as_ref().map(OneOrMany::as_one),
                            field.secondary_files.as_ref(),
                            context,
                            storage.clone(),
                        )
                        .await?,
                    );
                }
            }
            DefaultValue::Any(serde_json::to_value(fields)?)
        }
        _ => {
            if let Some(binding) = output_binding {
                let mut values = evaluate_command_binding(
                    binding,
                    context,
                    secondary_files,
                    &output_id,
                    storage.clone(),
                )
                .await?;
                for item in &mut values {
                    validate_output_item_recurse(item, format, context, storage.clone()).await?;
                }

                if single && !values.is_empty() || is_any && values.len() == 1 {
                    values[0].clone()
                } else if is_optional && values.is_empty() {
                    DefaultValue::Any(serde_json::Value::Null)
                } else {
                    let value = serde_json::to_value(values)?;
                    DefaultValue::Any(value)
                }
            } else {
                DefaultValue::Any(serde_json::Value::Null)
            }
        }
    };
    Ok(value)
}.boxed()
}

fn validate_output_item_recurse<'a>(
    item: &'a mut DefaultValue,
    format: Option<&'a String>,
    context: &'a OutputCollectionContext<'_>,
    storage: Arc<StorageBackend>,
) -> BoxFuture<'a, anyhow::Result<()>> {
    async move {
        match item {
            DefaultValue::FileOrDirectory(fod) => {
                validate_output_item(fod, format, context, context.dest_dir, true, storage).await?;
            }
            DefaultValue::Any(serde_json::Value::Object(map)) => {
                for item in map.values_mut() {
                    let mut dv = serde_json::from_value(item.clone())?;
                    validate_output_item_recurse(&mut dv, format, context, storage.clone()).await?;
                    *item = serde_json::to_value(dv)?;
                }
            }
            DefaultValue::Any(serde_json::Value::Array(arr)) => {
                for item in arr {
                    let mut dv = serde_json::from_value(item.clone())?;
                    validate_output_item_recurse(&mut dv, format, context, storage.clone()).await?;
                    *item = serde_json::to_value(dv)?;
                }
            }
            DefaultValue::Any(_) => {}
        }

        Ok(())
    }
    .boxed()
}

fn get_globs(glob: &OneOrMany<String>, context: &EvaluationContext) -> Vec<String> {
    let mut globs = vec![];
    match glob {
        OneOrMany::One(glob) => {
            if let Ok(value) = do_eval(glob, context) {
                match value {
                    //we can get a list here also
                    serde_json::Value::Array(vec) => {
                        for item in vec {
                            globs.push(item.as_str().unwrap().into());
                        }
                    }
                    _ => globs.push(value.as_str().unwrap().into()),
                }
            } else {
                //no expression
                globs.push(glob.clone());
            }
        }
        OneOrMany::Many(items) => {
            for item in items {
                globs.push(do_eval_to_string(item, context));
            }
        }
    }

    globs
}

//returns a file created in the output directory
async fn handle_file(
    path: &StoragePath,
    format: Option<String>,
    context: &OutputCollectionContext<'_>,
    storage: Arc<StorageBackend>,
) -> anyhow::Result<DefaultValue> {
    let filename = path.file_name().unwrap();
    let filename = Path::new(&filename);

    let dest_path = context.dest_dir.join(filename);
    storage.download(&path.as_url()?, &dest_path).await?;

    let mut file = File::new_from_path(&dest_path)?;
    file.format = format;

    Ok(DefaultValue::FileOrDirectory(FileOrDirectory::File(file)))
}

//returns a directory created in the output directory
async fn handle_dir(
    path: &StoragePath,
    context: &OutputCollectionContext<'_>,
    storage: Arc<StorageBackend>,
) -> anyhow::Result<DefaultValue> {
    let url = path.as_url()?.clone();
    let relative_path = url
        .path()
        .strip_prefix(context.source_dir.as_url()?.as_str())
        .unwrap();
    let dest_path = context.dest_dir.join(relative_path);
    let dest_path_as_str = dest_path.to_string_lossy();

    storage.download(&url, &dest_path).await?;

    let basename = dest_path
        .file_name()
        .map(|f| f.to_string_lossy().into_owned());

    let mut dir = Directory::builder()
        .location(
            Url::from_file_path(&dest_path)
                .map_err(|()| anyhow::anyhow!("Could not create URL from {dest_path_as_str}"))?
                .to_string(),
        )
        .path(dest_path_as_str)
        .maybe_basename(basename)
        .build();

    dir.load_listing(LoadListingEnum::DeepListing)?;

    Ok(DefaultValue::FileOrDirectory(FileOrDirectory::Directory(
        dir,
    )))
}

pub(crate) async fn collect_expression_outputs(
    outputs: &[ExpressionToolOutputParameter],
    value: &serde_json::Value,
    context: &OutputCollectionContext<'_>,
    storage: Arc<StorageBackend>,
) -> anyhow::Result<HashMap<String, DefaultValue>> {
    let mut output_map = HashMap::new();
    for output in outputs {
        let output_id = output.id.clone().unwrap_or_default();
        if let Some(result) = value.get(&output_id) {
            let mut value: DefaultValue = serde_json::from_value(result.clone())?;

            //validate output to schema
            let valid = validate_output_type(&output.r#type, &value);
            //outputs are considered valid, hinting when something invalid was given
            if !valid {
                warn!(
                    "Output value {value:?} does not match output type {:?}",
                    output.r#type
                );
            }
            let format = output.format.as_ref().map(|f| f.as_one().clone());
            validate_output_item_recurse(&mut value, format.as_ref(), context, storage.clone())
                .await?;
            output_map.insert(output_id, value);
        }
    }
    Ok(output_map)
}

pub(crate) async fn collect_workflow_outputs(
    outputs: &[WorkflowOutputParameter],
    values: &HashMap<String, DefaultValue>,
    context: &OutputCollectionContext<'_>,
    mir: Option<&MultipleInputFeatureRequirement>,
    storage: Arc<StorageBackend>,
) -> anyhow::Result<HashMap<String, DefaultValue>> {
    let mut output_map = HashMap::new();

    for output in outputs {
        let output_id = output.id.clone().unwrap();
        if let Some(output_source) = &output.output_source {
            match output_source {
                OneOrMany::One(item) => {
                    if let Some(value) = values.get(item) {
                        let mut value = if let Some(pick_value) = output.pick_value {
                            // scatter+when produces an array under a single source — filter its elements
                            let items = match value.clone() {
                                DefaultValue::Any(serde_json::Value::Array(arr)) => arr
                                    .into_iter()
                                    .map(|v| serde_json::from_value(v).map_err(Into::into))
                                    .collect::<anyhow::Result<Vec<DefaultValue>>>()?,
                                other => vec![other],
                            };
                            let merged = handle_link_merge(
                                output.link_merge.unwrap_or(LinkMergeMethod::MergeNested),
                                items,
                            )?;
                            handle_pick_value(&output_id, pick_value, merged)?
                        } else {
                            value.clone()
                        };

                        let format = output.format.as_ref().map(|f| f.as_one().clone());
                        validate_output_item_recurse(
                            &mut value,
                            format.as_ref(),
                            context,
                            storage.clone(),
                        )
                        .await?;
                        if let CommandOutputParameterType::CommandOutputType(r#type) =
                            &output.r#type
                        {
                            let valid = validate_output_type(&r#type.clone().into(), &value);
                            if !valid {
                                anyhow::bail!(
                                    "Invalid value for {output_id}. {type:?} does not match {value:?}",
                                );
                            }
                        }
                        output_map.insert(output_id.clone(), value);
                    }
                }
                OneOrMany::Many(items) => {
                    let resolved = items
                        .iter()
                        .map(|item| {
                            values
                                .get(item)
                                .cloned()
                                .unwrap_or(DefaultValue::Any(serde_json::Value::Null))
                        })
                        .collect::<Vec<_>>();
                    let merged = handle_link_merge(
                        output.link_merge.unwrap_or(LinkMergeMethod::MergeNested),
                        resolved,
                    )?;
                    let mut value = if let Some(pick_value) = output.pick_value {
                        handle_pick_value(&output_id, pick_value, merged)?
                    } else if mir.is_some() {
                        DefaultValue::Any(serde_json::to_value(merged)?)
                    } else {
                        anyhow::bail!(
                            "Needs to use either pick_value or MultipleInputFeatureRequirement with multiple output_sources"
                        );
                    };

                    let format = output.format.as_ref().map(|f| f.as_one().clone());
                    validate_output_item_recurse(
                        &mut value,
                        format.as_ref(),
                        context,
                        storage.clone(),
                    )
                    .await?;
                    if let CommandOutputParameterType::CommandOutputType(r#type) = &output.r#type {
                        let valid = validate_output_type(&r#type.clone().into(), &value);
                        if !valid {
                            anyhow::bail!(
                                "Invalid value for {output_id}. {type:?} does not match {value:?}"
                            );
                        }
                    }
                    output_map.insert(output_id.clone(), value);
                }
            }
        }
    }
    Ok(output_map)
}

fn validate_output_type(r#type: &OneOrMany<OutputType>, value: &DefaultValue) -> bool {
    //validate output to schema
    match r#type {
        OneOrMany::One(r#type) => validate_type(
            &Into::<OutputType>::into(r#type.clone()).into(),
            value,
            None,
            None,
        ),
        OneOrMany::Many(items) => items.iter().any(|t| {
            validate_type(
                &Into::<OutputType>::into(t.clone()).into(),
                value,
                None,
                None,
            )
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_handle_secondary_files_missing_location_does_not_panic() {
        let file = File::default();
        let schema = OneOrMany::One(SecondaryFileSchema {
            pattern: ".idx".to_string(),
            required: None,
        });
        let context = EvaluationContext::default();
        let storage = StorageBackend::new();

        let result = handle_secondary_files(&file, &schema, &context, &storage)
            .await
            .unwrap();
        assert!(result.is_empty());
    }
}
