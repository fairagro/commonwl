use crate::{
    expression::{EvaluationContext, do_eval, do_eval_to_string},
    format::FormatValidator,
    io::file::{PathOrFile, handle_secondary_file_schema},
};
use anyhow::Context;
use cwl_core::{
    FileMetaData, Integer, OneOrMany,
    files::{Directory, File, FileOrDirectory, LoadListingEnum},
    get_file_metadata,
    inputs::DefaultValue,
    outputs::{
        CommandOutputBinding, CommandOutputParameter, CommandOutputParameterType,
        CommandOutputSchema, CommandOutputType,
    },
    types::{CWLType, SecondaryFileSchema},
};
use dircpy::copy_dir;
use glob::glob;
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};
use tracing::{debug, info};

#[derive(Debug)]
pub struct OutputCollectionContext<'a> {
    pub source_dir: &'a Path,
    pub tmp_dir: &'a Path,
    pub dest_dir: &'a Path,
    pub workdir: &'a Path,
    pub eval_context: &'a EvaluationContext<'a>,
    pub validator: &'a FormatValidator,
}

/// handles collection of command outputs after execution
pub fn collect_command_outputs(
    outputs: &[CommandOutputParameter],
    stdout_file: &Path,
    stderr_file: &Path,
    context: &OutputCollectionContext,
) -> anyhow::Result<HashMap<String, DefaultValue>> {
    if context.source_dir.join("cwl.output.json").exists() {
        let contents = fs::read_to_string(context.source_dir.join("cwl.output.json"))?;
        let mut values: HashMap<String, DefaultValue> = serde_json::from_str(&contents)?;
        for value in values.values_mut() {
            match value {
                DefaultValue::FileOrDirectory(FileOrDirectory::File(f)) => {
                    f.dry_validation();
                    let path = f.path.clone().unwrap();
                    let path = correct_output_path(Path::new(&path), context);
                    //can file have secondary files here?
                    *value = handle_file(&path, None, context)?
                }
                DefaultValue::FileOrDirectory(FileOrDirectory::Directory(d)) => {
                    d.dry_validation();
                    let path = d.path.clone().unwrap();
                    let path = correct_output_path(Path::new(&path), context);
                    *value = handle_dir(&path, context)?
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
        let value = collect_output_item(output, stdout_file, stderr_file, context)?;
        output_map.insert(output_id, value);
    }

    Ok(output_map)
}

fn correct_output_path(path: &Path, context: &OutputCollectionContext) -> PathBuf {
    if path.starts_with(context.source_dir) {
        path.to_path_buf()
    } else if let Ok(stripped) = path.strip_prefix(context.dest_dir) {
        context.source_dir.join(stripped)
    } else if let Ok(stripped) = path.strip_prefix(context.workdir) {
        context.source_dir.join(stripped)
    } else {
        context.source_dir.join(path)
    }
}

fn evaluate_command_binding(
    binding: &CommandOutputBinding,
    context: &OutputCollectionContext,
    secondary_files: Option<&OneOrMany<SecondaryFileSchema>>,
    output_id: &String,
) -> anyhow::Result<Vec<DefaultValue>> {
    let mut results = vec![];

    //collect items via globs
    if let Some(globs) = &binding.glob {
        for glob_ in get_globs(globs, context.eval_context)? {
            let full_glob = make_full_glob(&glob_, context)?;
            for entry in glob(&full_glob)? {
                let Ok(item) = entry else {
                    info!("Output glob {full_glob} did not match any files for {output_id}");
                    continue;
                };
                let fod = if item.is_dir() {
                    let basename = item.file_name().map(|i| i.to_string_lossy().into_owned());
                    let mut dir = Directory::builder()
                        .path(item.to_string_lossy())
                        .location(format!("file://{}", item.display()))
                        .maybe_basename(basename)
                        .build();
                    //handle load_listing
                    if let Some(load_listing) = binding.load_listing {
                        dir.load_listing(load_listing)?
                    } else {
                        dir.load_listing(LoadListingEnum::DeepListing)?;
                    }
                    FileOrDirectory::Directory(dir)
                } else {
                    let mut file = File::new_from_path(&item)?;
                    //handle load_contents
                    if let Some(load_contents) = &binding.load_contents
                        && *load_contents
                    {
                        file.contents = fs::read_to_string(item).ok();
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
                let path = file.path.clone().unwrap();

                let json_value = serde_json::to_value(&file)?;
                let eval_context = EvaluationContext {
                    context: Some(&json_value),
                    ..*context.eval_context
                };
                file.secondary_files =
                    handle_secondary_files(Path::new(&path), secondary_files, &eval_context).ok();
            }
        }
    }

    Ok(results)
}

fn handle_secondary_files(
    path: &Path,
    secondary_files: &OneOrMany<SecondaryFileSchema>,
    context: &EvaluationContext,
) -> anyhow::Result<Vec<FileOrDirectory>> {
    let mut secondaries = vec![];

    for item in &secondary_files.as_many() {
        let Some(secondary_file) = handle_secondary_file_schema(path, item, context)? else {
            continue;
        };
        match secondary_file {
            PathOrFile::Path(secondary_path) => {
                let file = File::new_from_path(&secondary_path)?;
                secondaries.push(FileOrDirectory::File(file));
            }
            PathOrFile::File(vec) => secondaries.extend(vec),
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
) -> anyhow::Result<()> {
    match item {
        FileOrDirectory::File(file) => validate_file(file, format, context, base_path)?,
        FileOrDirectory::Directory(dir) => validate_dir(dir, context, base_path)?,
    };

    Ok(())
}

/// sets the designated path to the file and copies it and its secondary_files recursively to the output folder
fn validate_file(
    file: &mut File,
    format: Option<&String>,
    context: &OutputCollectionContext,
    base_path: &Path,
) -> anyhow::Result<()> {
    let path = get_designated_path(file.path.as_ref(), base_path, file.basename.as_ref());
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
                .to_string()
        }

        let parent = dest_path.parent().unwrap();
        if !parent.exists() {
            fs::create_dir_all(parent)?;
        }

        fs::copy(&source_path, dest_path)
            .with_context(|| format!("Could not copy {source_path:?} to {dest_path:?}"))?;

        if file.size.is_none() || file.checksum.is_none() {
            let FileMetaData { size, checksum } = get_file_metadata(dest_path)?;
            file.checksum = checksum;
            file.size = Some(Integer::Long(size as i64))
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

    if let Some(secondary_files) = &mut file.secondary_files {
        for item in secondary_files {
            validate_output_item(item, None, context, base_path)?;
        }
    }

    Ok(())
}

/// sets the designated path to the directory and copies it and its contents recursively to the output folder
fn validate_dir(
    dir: &mut Directory,
    context: &OutputCollectionContext,
    base_path: &Path,
) -> anyhow::Result<()> {
    let path = get_designated_path(dir.path.as_ref(), base_path, dir.basename.as_ref());

    let mut base_path = base_path.to_path_buf();

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
                .to_string()
        }

        let parent = dest_path.parent().unwrap();
        if !parent.exists() {
            fs::create_dir_all(parent)?;
        }

        copy_dir(&source_path, dest_path)
            .with_context(|| format!("Could not copy {source_path:?} to {dest_path:?}"))?;
        base_path = dest_path.to_path_buf();
    }

    dir.path = path.as_ref().map(|p| p.to_string_lossy().into_owned());
    dir.location = dir.path.as_ref().and_then(|p| format!("file://{p}").into());

    if let Some(listing) = &mut dir.listing {
        for item in listing {
            validate_output_item(item, None, context, &base_path)?;
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
    path.as_ref().map(|p| {
        let path = Path::new(p);
        let filename = path.file_name().unwrap().to_string_lossy();

        base_path.join(basename.unwrap_or(&filename.to_string()))
    })
}

///collects a single output item
fn collect_output_item(
    output: &CommandOutputParameter,
    stdout_file: &Path,
    stderr_file: &Path,
    context: &OutputCollectionContext,
) -> anyhow::Result<DefaultValue> {
    let format = output.format.as_ref().map(|f| f.as_one().to_string());
    match &output.r#type {
        CommandOutputParameterType::Stdout => handle_file(stdout_file, format, context),
        CommandOutputParameterType::Stderr => handle_file(stderr_file, format, context),
        CommandOutputParameterType::CommandOutputType(r#type) => collect_item(
            output,
            &output.output_binding,
            r#type,
            format.as_ref(),
            output.secondary_files.as_ref(),
            context,
        ),
    }
}

// recursive implementention of item collection
fn collect_item(
    output: &CommandOutputParameter,
    output_binding: &Option<CommandOutputBinding>,
    item: &OneOrMany<CommandOutputType>,
    format: Option<&String>,
    secondary_files: Option<&OneOrMany<SecondaryFileSchema>>,
    context: &OutputCollectionContext,
) -> anyhow::Result<DefaultValue> {
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
        _ => true,
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
                            &field.output_binding,
                            &field.r#type,
                            field.format.as_ref().map(|f| f.as_one()),
                            field.secondary_files.as_ref(),
                            context,
                        )?,
                    );
                }
            }
            DefaultValue::Any(serde_yaml::to_value(fields)?)
        }
        _ => {
            if let Some(binding) = output_binding {
                let mut values =
                    evaluate_command_binding(binding, context, secondary_files, &output_id)?;
                for item in &mut values {
                    if let DefaultValue::FileOrDirectory(fod) = item {
                        validate_output_item(fod, format, context, context.dest_dir)?;
                    }
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
}

fn get_globs(glob: &OneOrMany<String>, context: &EvaluationContext) -> anyhow::Result<Vec<String>> {
    let mut globs = vec![];
    match glob {
        OneOrMany::One(glob) => {
            if let Ok(value) = do_eval(glob, context) {
                match value {
                    //we can get a list here also
                    serde_yaml::Value::Sequence(vec) => {
                        for item in vec {
                            globs.push(item.as_str().unwrap().into())
                        }
                    }
                    _ => globs.push(value.as_str().unwrap().into()),
                }
            } else {
                //no expression
                globs.push(glob.to_string());
            }
        }
        OneOrMany::Many(items) => {
            for item in items {
                globs.push(do_eval_to_string(item, context));
            }
        }
    }

    Ok(globs)
}

//returns a file created in the output directory
fn handle_file(
    path: &Path,
    format: Option<String>,
    context: &OutputCollectionContext,
) -> anyhow::Result<DefaultValue> {
    let filename = Path::new(path.file_name().unwrap_or_default());

    let dest_path = context.dest_dir.join(filename);

    fs::copy(path, &dest_path)
        .with_context(|| format!("Could not copy {path:?} to {dest_path:?}"))?;
    let mut file = File::new_from_path(&dest_path)?;
    file.format = format;

    Ok(DefaultValue::FileOrDirectory(FileOrDirectory::File(file)))
}

//returns a directory created in the output directory
fn handle_dir(path: &Path, context: &OutputCollectionContext) -> anyhow::Result<DefaultValue> {
    let relative_path = path.strip_prefix(context.source_dir)?.to_path_buf();
    let dest_path = context.dest_dir.join(&relative_path);
    let dest_path_as_str = dest_path.to_string_lossy();

    copy_dir(path, &dest_path)?;

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

fn make_full_glob(glob_: &str, context: &OutputCollectionContext) -> anyhow::Result<String> {
    let full_glob = if !glob_.starts_with("/") {
        format!("{}/{}", context.source_dir.display(), glob_)
    } else {
        if !glob_.starts_with(&context.source_dir.to_string_lossy().to_string()) {
            anyhow::bail!("Can not access objects outside the working directory: {glob_}.");
        }
        glob_.to_owned()
    };

    Ok(full_glob)
}
