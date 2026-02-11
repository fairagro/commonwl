use crate::{
    expression::{EvaluationContext, do_eval, do_eval_to_string},
    format::FormatValidator,
    secondary_files::handle_secondary_file_schema,
};
use cwl_core::{
    OneOrMany,
    files::{Directory, File, FileOrDirectory, LoadListingEnum},
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
use tracing::info;

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
                    *value = handle_file(&path, None, None, context)?
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
            Err(_) => results,
        }
    }

    //handle secondary_files
    if let Some(secondary_files) = secondary_files {
        for item in &mut results {
            if let DefaultValue::FileOrDirectory(FileOrDirectory::File(file)) = item {
                let path = file.path.clone().unwrap();
                file.secondary_files =
                    handle_secondary_files(Path::new(&path), secondary_files, context.eval_context)
                        .ok();
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
        let Some(secondary_path) = handle_secondary_file_schema(path, item, context)? else {
            continue;
        };
        let file = File::new_from_path(&secondary_path)?;
        secondaries.push(FileOrDirectory::File(file));
    }
    Ok(secondaries)
}

/// validates new paths for files and directories
fn validate_output_item(
    item: &mut FileOrDirectory,
    format: Option<&String>,
    context: &OutputCollectionContext,
) -> anyhow::Result<()> {
    match item {
        FileOrDirectory::File(file) => validate_file(file, format, context)?,
        FileOrDirectory::Directory(dir) => validate_dir(dir, context)?,
    };

    Ok(())
}

/// sets the designated path to the file and copies it and its secondary_files recursively to the output folder
fn validate_file(
    file: &mut File,
    format: Option<&String>,
    context: &OutputCollectionContext,
) -> anyhow::Result<()> {
    let path = get_designated_path(file.path.as_ref(), context);
    let dirname = path.as_ref().and_then(|p| p.parent());

    if let Some(source_path) = &file.path
        && let Some(dest_path) = &path
    {
        let parent = dest_path.parent().unwrap();
        if !parent.exists() {
            fs::create_dir_all(parent)?;
        }

        fs::copy(source_path, dest_path)?;
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
            validate_output_item(item, None, context)?;
        }
    }

    Ok(())
}

/// sets the designated path to the directory and copies it and its contents recursively to the output folder
fn validate_dir(dir: &mut Directory, context: &OutputCollectionContext) -> anyhow::Result<()> {
    let path = get_designated_path(dir.path.as_ref(), context);

    if let Some(source_path) = &dir.path
        && let Some(dest_path) = &path
    {
        let parent = dest_path.parent().unwrap();
        if !parent.exists() {
            fs::create_dir_all(parent)?;
        }

        copy_dir(source_path, dest_path)?;
    }

    dir.path = path.as_ref().map(|p| p.to_string_lossy().into_owned());
    dir.location = dir.path.as_ref().and_then(|p| format!("file://{p}").into());

    if let Some(listing) = &mut dir.listing {
        for item in listing {
            validate_output_item(item, None, context)?;
        }
    }

    Ok(())
}

/// creates the new output path by stripping prefixes of known folders 
fn get_designated_path(
    path: Option<&String>,
    context: &OutputCollectionContext,
) -> Option<PathBuf> {
    path.as_ref().and_then(|p| {
        let path = Path::new(p);
        path.strip_prefix(context.source_dir)
            .ok()
            .map(|relative| context.dest_dir.join(relative))
            .or_else(|| {
                path.strip_prefix(context.tmp_dir)
                    .ok()
                    .map(|relative| context.dest_dir.join(relative))
            })
            .or_else(|| {
                path.strip_prefix(context.eval_context.workdir.unwrap())
                    .ok()
                    .map(|relative| context.dest_dir.join(relative))
            })
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
        CommandOutputParameterType::Stdout => handle_file(stdout_file, format, None, context),
        CommandOutputParameterType::Stderr => handle_file(stderr_file, format, None, context),
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

    let optional = matches!(item, OneOrMany::Many(i) if i.contains(&CommandOutputType::CWLType(CWLType::Null)));
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
            if matches!(&**schema, CommandOutputSchema::Record(_)) =>
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
                        validate_output_item(fod, format, context)?;
                    }
                }

                if single && !values.is_empty() || is_any && values.len() == 1 {
                    values[0].clone()
                } else if optional && values.is_empty() {
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
    secondary_files: Option<&OneOrMany<SecondaryFileSchema>>,
    context: &OutputCollectionContext,
) -> anyhow::Result<DefaultValue> {
    let filename = Path::new(path.file_name().unwrap_or_default());

    let dest_path = context.dest_dir.join(filename);

    fs::copy(path, &dest_path)?;
    let mut file = File::new_from_path(&dest_path)?;
    file.format = format;

    //handle secondaries
    if let Some(secondary_files) = secondary_files {
        let secondary_files =
            copy_secondary_files(path, &dest_path, secondary_files, context.eval_context)?;
        file.secondary_files = Some(secondary_files);
    }

    Ok(DefaultValue::FileOrDirectory(FileOrDirectory::File(file)))
}

fn copy_secondary_files(
    from_path: &Path,
    to_path: &Path,
    secondary_files: &OneOrMany<SecondaryFileSchema>,
    context: &EvaluationContext,
) -> anyhow::Result<Vec<FileOrDirectory>> {
    let mut secondaries = vec![];

    for item in &secondary_files.as_many() {
        let Some(secondary_path) = handle_secondary_file_schema(from_path, item, context)? else {
            continue;
        };

        let copy_to_path = secondary_path
            .strip_prefix(from_path.parent().unwrap())
            .map(|relative| Path::new(&to_path.parent().unwrap()).join(relative))?;

        fs::copy(secondary_path, &copy_to_path)?;
        let file = File::new_from_path(&copy_to_path)?;
        secondaries.push(FileOrDirectory::File(file));
    }

    //remove none values
    Ok(secondaries)
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
