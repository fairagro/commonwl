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
        CommandOutputArraySchema, CommandOutputBinding, CommandOutputParameter,
        CommandOutputParameterType, CommandOutputRecordSchema, CommandOutputSchema,
        CommandOutputType,
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

pub struct OutputCollectionContext<'a> {
    pub source_dir: &'a Path,
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
    let mut output_map: HashMap<String, DefaultValue> = HashMap::new();

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

    //we start with simple cases for now: Files and Directories with glob patterns
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

///collects a single output item af
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
        CommandOutputParameterType::CommandOutputType(one_or_many) => match one_or_many {
            OneOrMany::One(item) => collect_item(
                output,
                &output.output_binding,
                item,
                format,
                output.secondary_files.as_ref(),
                context,
            ),
            OneOrMany::Many(items) => Ok(items
                .iter()
                .find_map(|item| {
                    collect_item(
                        output,
                        &output.output_binding,
                        item,
                        format.clone(),
                        output.secondary_files.as_ref(),
                        context,
                    )
                    .ok()
                })
                .unwrap_or(DefaultValue::Any(serde_yaml::Value::Null))),
        },
    }
}

fn collect_item(
    output: &CommandOutputParameter,
    output_binding: &Option<CommandOutputBinding>,
    item: &CommandOutputType,
    format: Option<String>,
    secondary_files: Option<&OneOrMany<SecondaryFileSchema>>,
    context: &OutputCollectionContext,
) -> anyhow::Result<DefaultValue> {
    let output_id = output.id.clone().unwrap_or_default();
    match item {
        CommandOutputType::CWLType(ty) => match ty {
            CWLType::File => {
                let matches =
                    add_file_impl(&output_id, output_binding, format, secondary_files, context)?;
                Ok(matches
                    .first()
                    .unwrap_or(&DefaultValue::Any(serde_yaml::Value::Null))
                    .clone())
            }
            CWLType::Directory => {
                let matches = add_dir_impl(&output_id, output_binding, context)?;
                Ok(matches
                    .first()
                    .unwrap_or(&DefaultValue::Any(serde_yaml::Value::Null))
                    .clone())
            }
            _ => add_fallback_impl(&output_id, output, context),
        },
        CommandOutputType::CommandOutputSchema(schema) => match &**schema {
            CommandOutputSchema::Record(rec) => collect_record_schema_item(output, rec, context),
            CommandOutputSchema::Array(arr) => collect_array_schema_item(
                output,
                format,
                arr,
                output_binding,
                secondary_files,
                context,
            ),
            CommandOutputSchema::Enum(_) => todo!(),
        },
        CommandOutputType::String(_) => todo!(),
    }
}

fn collect_record_schema_item(
    output: &CommandOutputParameter,
    record: &CommandOutputRecordSchema,
    context: &OutputCollectionContext,
) -> anyhow::Result<DefaultValue> {
    let mut fields = HashMap::new();
    if let Some(record_fields) = &record.fields {
        for field in record_fields {
            let field_value = match &field.r#type {
                OneOrMany::One(item) => collect_item(
                    output,
                    &field.output_binding,
                    item,
                    field.format.as_ref().map(|f| f.as_one().to_string()),
                    field.secondary_files.as_ref(),
                    context,
                )?,
                OneOrMany::Many(items) => items
                    .iter()
                    .find_map(|item| {
                        collect_item(
                            output,
                            &field.output_binding,
                            item,
                            field.format.as_ref().map(|f| f.as_one().to_string()),
                            field.secondary_files.as_ref(),
                            context,
                        )
                        .ok()
                    })
                    .unwrap_or(DefaultValue::Any(serde_yaml::Value::Null)),
            };
            fields.insert(field.name.clone(), field_value);
        }
    }
    Ok(DefaultValue::Any(serde_yaml::to_value(fields)?))
}

fn collect_array_schema_item(
    output: &CommandOutputParameter,
    format: Option<String>,
    array: &CommandOutputArraySchema,
    output_binding: &Option<CommandOutputBinding>,
    secondary_files: Option<&OneOrMany<SecondaryFileSchema>>,
    context: &OutputCollectionContext,
) -> anyhow::Result<DefaultValue> {
    let mut values: Vec<DefaultValue> = vec![];
    let output_id = output.id.clone().unwrap_or_default();
    match &array.items {
        OneOrMany::One(item) => match item {
            CommandOutputType::CWLType(ty) => match ty {
                CWLType::File => values.extend(add_file_impl(
                    &output_id,
                    output_binding,
                    format,
                    secondary_files,
                    context,
                )?),
                CWLType::Directory => {
                    values.extend(add_dir_impl(&output_id, output_binding, context)?)
                }
                _ => {}
            },
            CommandOutputType::CommandOutputSchema(_) => todo!(),
            CommandOutputType::String(_) => todo!(),
        },
        OneOrMany::Many(_) => todo!(),
    }
    Ok(DefaultValue::Any(serde_yaml::to_value(values)?))
}

fn add_file_impl(
    output_id: &String,
    output_binding: &Option<CommandOutputBinding>,
    format: Option<String>,
    secondary_files: Option<&OneOrMany<SecondaryFileSchema>>,
    context: &OutputCollectionContext,
) -> anyhow::Result<Vec<DefaultValue>> {
    let mut files = vec![];

    if let Some(binding) = output_binding {
        if let Some(globs) = &binding.glob {
            for glob_ in get_globs(globs, context.eval_context)? {
                let full_glob = make_full_glob(&glob_, context)?;
                for entry in glob(&full_glob)? {
                    let Ok(item) = entry else {
                        info!("Output glob {full_glob} did not match any files for {output_id}");
                        continue;
                    };
                    let format = context
                        .validator
                        .handle(format.as_ref(), Some(context.eval_context));
                    files.push(handle_file(&item, format, secondary_files, context)?);
                }
            }
        } else if let Some(output_eval) = &binding.output_eval {
            let value = do_eval(output_eval, context.eval_context)?;
            let mut dv = serde_yaml::from_value(value)?;
            if let DefaultValue::FileOrDirectory(FileOrDirectory::File(file)) = &mut dv {
                file.dry_validation();
                let Some(path) = &file.path else {
                    panic!("File has no path")
                };
                *file = File::new_from_path(Path::new(&path))?;
                let format = context
                    .validator
                    .handle(format.as_ref(), Some(context.eval_context));
                file.format = format;
            }
            files.push(dv);
        }
    }
    Ok(files)
}

fn add_dir_impl(
    output_id: &String,
    output_binding: &Option<CommandOutputBinding>,
    context: &OutputCollectionContext,
) -> anyhow::Result<Vec<DefaultValue>> {
    let mut dirs = vec![];
    if let Some(binding) = output_binding {
        if let Some(globs) = &binding.glob {
            for glob_ in get_globs(globs, context.eval_context)? {
                let full_glob = make_full_glob(&glob_, context)?;
                for entry in glob(&full_glob)? {
                    let Ok(item) = entry else {
                        info!(
                            "Output glob {full_glob} did not match any directories for {output_id}"
                        );
                        continue;
                    };
                    dirs.push(handle_dir(&item, context)?);
                }
            }
        } else if let Some(output_eval) = &binding.output_eval {
            let value = do_eval(output_eval, context.eval_context)?;
            let dv = serde_yaml::from_value(value)?;
            dirs.push(dv);
        }
    }
    Ok(dirs)
}

fn add_fallback_impl(
    output_id: &String,
    output: &CommandOutputParameter,
    context: &OutputCollectionContext,
) -> anyhow::Result<DefaultValue> {
    if let Some(binding) = &output.output_binding {
        if let Some(globs) = &binding.glob {
            let glob_ = globs.as_one();
            let glob_ = do_eval_to_string(glob_, context.eval_context);
            let full_glob = format!("{}/{}", context.source_dir.display(), glob_);

            let entry = glob(&full_glob)?.next();
            let Some(Ok(entry)) = entry else {
                info!("Output glob {full_glob} did not match any directories for {output_id}");
                return Ok(DefaultValue::Any(serde_yaml::Value::Null));
            };
            let contents = fs::read_to_string(&entry)?;
            if let Some(expression) = &binding.output_eval {
                let mut file = File::new_from_path(&entry)?;
                file.contents = Some(contents);
                let file_value = serde_json::to_value(vec![file])?; //could be array also so vec is expected
                let value = do_eval(
                    expression,
                    &context.eval_context.clone().with_context(&file_value),
                )?;
                return Ok(DefaultValue::Any(value));
            } else {
                return Ok(DefaultValue::Any(serde_yaml::Value::String(contents)));
            }
        } else if let Some(expression) = &binding.output_eval {
            return Ok(DefaultValue::Any(do_eval(
                expression,
                context.eval_context,
            )?));
        }
    }
    Ok(DefaultValue::Any(serde_yaml::Value::Null))
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
