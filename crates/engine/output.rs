use crate::{
    expression::{EvaluationContext, do_eval, do_eval_to_string},
    io::{
        file::{PathOrFile, handle_secondary_file_schema},
        unique_path,
    },
    schema::{format_validation::FormatValidator, validation::validate_type},
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
use dircpy::copy_dir;
use futures_util::{FutureExt, future::BoxFuture};
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    sync::Arc,
};
use tracing::{debug, warn};

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
                    let path = f.path.clone().or(f.location.clone()).unwrap();
                    let path = path.strip_prefix("file://").unwrap_or(&path);
                    let path = correct_output_path(Path::new(&path), context);
                    //can file have secondary files here?
                    *value = handle_file(&path, None, context, storage.clone()).await?;
                }
                DefaultValue::FileOrDirectory(FileOrDirectory::Directory(d)) => {
                    let path = d.path.clone().or(d.location.clone()).unwrap();
                    let path = path.strip_prefix("file://").unwrap_or(&path);
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
                let fod = if item.is_dir() {
                    let basename = item.file_name();
                    let mut dir = Directory::builder()
                        .path(item.path())
                        .location(item.as_url()?)
                        .maybe_basename(basename)
                        .build();
                    //handle load_listing
                    if let Some(load_listing) = binding.load_listing {
                        dir.load_listing(load_listing)?;
                    } else {
                        dir.load_listing(LoadListingEnum::DeepListing)?;
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
                serde_yaml::Value::Sequence(vals) => vals
                    .into_iter()
                    .filter_map(|item| serde_yaml::from_value(item).ok())
                    .collect(),
                single_value => vec![serde_yaml::from_value(single_value)?],
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
                    handle_secondary_files(file, secondary_files, &eval_context).ok();
            }
        }
    }

    Ok(results)
}

fn handle_secondary_files(
    file: &File,
    secondary_files: &OneOrMany<SecondaryFileSchema>,
    context: &EvaluationContext,
) -> anyhow::Result<Vec<FileOrDirectory>> {
    let mut secondaries = vec![];
    for item in &secondary_files.as_many() {
        let Some(secondary_file) = handle_secondary_file_schema(file, item, context)? else {
            continue;
        };
        for item in secondary_file {
            match item {
                PathOrFile::Path(secondary_path) => {
                    if secondary_path.is_file() {
                        let file = File::new_from_path(&secondary_path)?;
                        secondaries.push(FileOrDirectory::File(file));
                    } else if secondary_path.is_dir() {
                        let mut dir = Directory::new_from_path(&secondary_path)?;
                        dir.load_listing(LoadListingEnum::DeepListing)?;
                        secondaries.push(FileOrDirectory::Directory(dir));
                    }
                }
                PathOrFile::File(fod) => secondaries.push(*fod),
            }
        }
    }
    Ok(secondaries)
}

/// validates new paths for files and directories
fn validate_output_item(
    item: &mut FileOrDirectory,
    format: Option<&String>,
    context: &OutputCollectionContext,
    base_path: &Path,
    copy: bool, // indicates whether we need to copy file and dir
) -> anyhow::Result<()> {
    match item {
        FileOrDirectory::File(file) => validate_file(file, format, context, base_path, copy)?,
        FileOrDirectory::Directory(dir) => validate_dir(dir, context, base_path, copy)?,
    }

    Ok(())
}

/// sets the designated path to the file and copies it and its `secondary_files` recursively to the output folder
fn validate_file(
    file: &mut File,
    format: Option<&String>,
    context: &OutputCollectionContext,
    base_path: &Path,
    copy: bool,
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

    if let Some(source_path) = &file.path
        && let Some(dest_path) = &path
    {
        let mut source_path = source_path.to_owned();
        if !Path::new(&source_path).exists() {
            debug!("Path field contains container path. Trying to use location: {file:?}");
            source_path = file
                .location
                .as_ref()
                .unwrap()
                .strip_prefix("file://")
                .unwrap()
                .to_string();
        }

        let parent = dest_path.parent().unwrap();
        if !parent.exists() {
            fs::create_dir_all(parent)?;
        }

        if copy {
            fs::copy(&source_path, dest_path).with_context(|| {
                format!("Could not copy {source_path:?} to {}", dest_path.display())
            })?;
        }
        if file.size.is_none() || file.checksum.is_none() {
            let FileMetaData { size, checksum } = get_file_metadata(dest_path)?;
            file.checksum = checksum;
            file.size = Some(Integer::Long(size.cast_signed()));
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
    file.location = file
        .path
        .as_ref()
        .and_then(|p| format!("file://{p}").into());

    if file.basename.is_none() {
        file.basename = path
            .as_ref()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned());
    }

    if let Some(secondary_files) = &mut file.secondary_files {
        for item in secondary_files {
            validate_output_item(item, None, context, base_path, true)?;
        }
    }

    Ok(())
}

/// sets the designated path to the directory and copies it and its contents recursively to the output folder
fn validate_dir(
    dir: &mut Directory,
    context: &OutputCollectionContext,
    base_path: &Path,
    copy: bool,
) -> anyhow::Result<()> {
    let path = get_designated_path(dir.path.as_ref(), base_path, dir.basename.as_ref())
        .map(|p| if copy { unique_path(&p, None) } else { p });

    if let Some(source_path) = &dir.path
        && let Some(dest_path) = &path
    {
        let mut source_path = source_path.to_owned();
        if !Path::new(&source_path).exists() {
            debug!("Path field contains container path. Trying to use location: {dir:?}");
            source_path = dir
                .location
                .as_ref()
                .unwrap()
                .strip_prefix("file://")
                .unwrap()
                .to_string();
        }

        let parent = dest_path.parent().unwrap();
        if !parent.exists() {
            fs::create_dir_all(parent)?;
        }
        if copy {
            copy_dir(&source_path, dest_path).with_context(|| {
                format!("Could not copy {source_path} to {}", dest_path.display())
            })?;
        }
    } else if let Some(dest_path) = &path {
        //no source path, but we still want to create the directory
        fs::create_dir_all(dest_path)?;
    }

    let copy = dir.path.is_none();
    dir.path = path.as_ref().map(|p| p.to_string_lossy().into_owned());
    dir.location = dir.path.as_ref().and_then(|p| format!("file://{p}").into());

    let base_path = path.unwrap();
    if let Some(listing) = &mut dir.listing {
        for item in listing {
            validate_output_item(item, None, context, &base_path, copy)?;
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
            DefaultValue::Any(serde_yaml::to_value(fields)?)
        }
        _ => {
            if let Some(binding) = output_binding {
                let mut values = evaluate_command_binding(
                    binding,
                    context,
                    secondary_files,
                    &output_id,
                    storage,
                )
                .await?;
                for item in &mut values {
                    validate_output_item_recurse(item, format, context)?;
                }

                if single && !values.is_empty() || is_any && values.len() == 1 {
                    values[0].clone()
                } else if is_optional && values.is_empty() {
                    DefaultValue::Any(serde_yaml::Value::Null)
                } else {
                    let value = serde_yaml::to_value(values)?;
                    DefaultValue::Any(value)
                }
            } else {
                DefaultValue::Any(serde_yaml::Value::Null)
            }
        }
    };
    Ok(value)
}.boxed()
}

fn validate_output_item_recurse(
    item: &mut DefaultValue,
    format: Option<&String>,
    context: &OutputCollectionContext,
) -> anyhow::Result<()> {
    match item {
        DefaultValue::FileOrDirectory(fod) => {
            validate_output_item(fod, format, context, context.dest_dir, true)?;
        }
        DefaultValue::Any(serde_yaml::Value::Mapping(map)) => {
            for item in map.values_mut() {
                let mut dv = serde_yaml::from_value(item.clone())?;
                validate_output_item_recurse(&mut dv, format, context)?;
                *item = serde_yaml::to_value(dv)?;
            }
        }
        DefaultValue::Any(serde_yaml::Value::Sequence(arr)) => {
            for item in arr {
                let mut dv = serde_yaml::from_value(item.clone())?;
                validate_output_item_recurse(&mut dv, format, context)?;
                *item = serde_yaml::to_value(dv)?;
            }
        }
        DefaultValue::Any(_) => {}
    }

    Ok(())
}

fn get_globs(glob: &OneOrMany<String>, context: &EvaluationContext) -> Vec<String> {
    let mut globs = vec![];
    match glob {
        OneOrMany::One(glob) => {
            if let Ok(value) = do_eval(glob, context) {
                match value {
                    //we can get a list here also
                    serde_yaml::Value::Sequence(vec) => {
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
    let url = path.as_url()?.to_owned();
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
        .location(format!("file://{}", &dest_path_as_str))
        .path(dest_path_as_str)
        .maybe_basename(basename)
        .build();

    dir.load_listing(LoadListingEnum::DeepListing)?;

    Ok(DefaultValue::FileOrDirectory(FileOrDirectory::Directory(
        dir,
    )))
}

pub(crate) fn collect_expression_outputs(
    outputs: &[ExpressionToolOutputParameter],
    value: &serde_yaml::Value,
    context: &OutputCollectionContext,
) -> anyhow::Result<HashMap<String, DefaultValue>> {
    let mut output_map = HashMap::new();
    for output in outputs {
        let output_id = output.id.clone().unwrap_or_default();
        if let Some(result) = value.get(&output_id) {
            let mut value: DefaultValue = serde_yaml::from_value(result.clone())?;

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
            validate_output_item_recurse(&mut value, format.as_ref(), context)?;
            output_map.insert(output_id, value);
        }
    }
    Ok(output_map)
}

pub(crate) fn collect_workflow_outputs(
    outputs: &[WorkflowOutputParameter],
    values: &HashMap<String, DefaultValue>,
    context: &OutputCollectionContext,
    mir: Option<&MultipleInputFeatureRequirement>,
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
                                DefaultValue::Any(serde_yaml::Value::Sequence(arr)) => arr
                                    .into_iter()
                                    .map(|v| serde_yaml::from_value(v).map_err(Into::into))
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
                        validate_output_item_recurse(&mut value, format.as_ref(), context)?;
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
                                .unwrap_or(DefaultValue::Any(serde_yaml::Value::Null))
                        })
                        .collect::<Vec<_>>();
                    let merged = handle_link_merge(
                        output.link_merge.unwrap_or(LinkMergeMethod::MergeNested),
                        resolved,
                    )?;
                    let mut value = if let Some(pick_value) = output.pick_value {
                        handle_pick_value(&output_id, pick_value, merged)?
                    } else if mir.is_some() {
                        DefaultValue::Any(serde_yaml::to_value(merged)?)
                    } else {
                        anyhow::bail!(
                            "Needs to use either pick_value or MultipleInputFeatureRequirement with multiple output_sources"
                        );
                    };

                    let format = output.format.as_ref().map(|f| f.as_one().clone());
                    validate_output_item_recurse(&mut value, format.as_ref(), context)?;
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
