use cwl_core::{
    OneOrMany,
    files::{Directory, File, FileOrDirectory, LoadListingEnum},
    inputs::DefaultValue,
    outputs::{CommandOutputParameter, CommandOutputParameterType, CommandOutputType},
    types::CWLType,
};
use dircpy::copy_dir;
use glob::glob;
use std::{collections::HashMap, fs, path::Path, process::Command};
use tracing::info;

use crate::expression::{EvaluationContext, do_eval, do_eval_to_string};

pub fn collect_command_outputs(
    outputs: &[CommandOutputParameter],
    source_dir: &Path,
    dest_dir: &Path,
    stdout_file: &Path,
    stderr_file: &Path,
    context: &EvaluationContext,
) -> anyhow::Result<HashMap<String, DefaultValue>> {
    let mut output_map: HashMap<String, DefaultValue> = HashMap::new();

    if source_dir.join("cwl.output.json").exists() {
        let contents = fs::read_to_string(source_dir.join("cwl.output.json"))?;
        let mut values: HashMap<String, DefaultValue> = serde_json::from_str(&contents)?;
        for value in values.values_mut() {
            match value {
                DefaultValue::FileOrDirectory(FileOrDirectory::File(f)) => {
                    f.dry_validation();
                    let path = f.path.clone().unwrap();
                    let path = Path::new(&path);
                    *value = handle_file(path, source_dir, dest_dir)?
                }
                DefaultValue::FileOrDirectory(FileOrDirectory::Directory(d)) => {
                    d.dry_validation();
                    let path = d.path.clone().unwrap();
                    let path = Path::new(&path);
                    *value = handle_dir(path, source_dir, dest_dir)?
                }
                _ => {}
            }
        }
        return Ok(values);
    }

    //we start with simple cases for now: Files and Directories with glob patterns
    for output in outputs {
        let output_id = output.id.clone().unwrap_or_default();
        //File, Dir, etc. with binding
        match output.r#type {
            CommandOutputParameterType::CommandOutputType(OneOrMany::One(
                CommandOutputType::CWLType(CWLType::File),
            )) => {
                if let Some(binding) = &output.output_binding
                    && let Some(globs) = &binding.glob
                {
                    let glob_ = globs.as_one();
                    let glob_ = do_eval_to_string(glob_, context);
                    let full_glob = format!("{}/{}", source_dir.display(), glob_);
                    let entry = glob(&full_glob)?.next();
                    let Some(Ok(item)) = entry else {
                        info!("Output glob {full_glob} did not match any files for {output_id}");
                        continue;
                    };
                    output_map.insert(output_id, handle_file(&item, source_dir, dest_dir)?);
                }
            }
            CommandOutputParameterType::CommandOutputType(OneOrMany::One(
                CommandOutputType::CWLType(CWLType::Directory),
            )) => {
                if let Some(binding) = &output.output_binding
                    && let Some(globs) = &binding.glob
                {
                    let glob_ = globs.as_one();
                    let glob_ = do_eval_to_string(glob_, context);
                    let full_glob = format!("{}/{}", source_dir.display(), glob_);
                    let entry = glob(&full_glob)?.next();
                    let Some(Ok(item)) = entry else {
                        info!(
                            "Output glob {full_glob} did not match any directories for {output_id}"
                        );
                        continue;
                    };
                    output_map.insert(output_id, handle_dir(&item, source_dir, dest_dir)?);
                }
            }
            CommandOutputParameterType::Stdout => {
                output_map.insert(output_id, handle_file(stdout_file, source_dir, dest_dir)?);
            }
            CommandOutputParameterType::Stderr => {
                output_map.insert(output_id, handle_file(stderr_file, source_dir, dest_dir)?);
            }
            _ => {
                if let Some(binding) = &output.output_binding {
                    if let Some(globs) = &binding.glob {
                        let glob_ = globs.as_one();
                        let glob_ = do_eval_to_string(glob_, context);
                        let full_glob = format!("{}/{}", source_dir.display(), glob_);

                        let entry = glob(&full_glob)?.next();
                        let Some(Ok(entry)) = entry else {
                            info!(
                                "Output glob {full_glob} did not match any directories for {output_id}"
                            );
                            continue;
                        };
                        let contents = fs::read_to_string(&entry)?;
                        if let Some(expression) = &binding.output_eval {
                            let mut file = File::new_from_path(&entry)?;
                            file.contents = Some(contents);
                            let file_value = serde_json::to_value(vec![file])?; //could be array also so vec is expected
                            let value =
                                do_eval(expression, &context.clone().with_context(&file_value))?;
                            output_map.insert(output_id, DefaultValue::Any(value));
                        } else {
                            output_map.insert(
                                output_id,
                                DefaultValue::Any(serde_yaml::Value::String(contents)),
                            );
                        }
                    } else if let Some(expression) = &binding.output_eval {
                        let value = do_eval(expression, context)?;
                        output_map.insert(output_id, DefaultValue::Any(value));
                    }
                }
            }
        }
    }

    Ok(output_map)
}

fn handle_file(path: &Path, source_dir: &Path, dest_dir: &Path) -> anyhow::Result<DefaultValue> {
    let relative_path = if let Ok(relative_path) = path.strip_prefix(source_dir) {
        relative_path
    } else {
        Path::new(path.file_name().unwrap_or_default())
    }
    .to_path_buf();

    let dest_path = dest_dir.join(&relative_path);

    fs::copy(path, &dest_path)?;
    let file = File::new_from_path(&dest_path)?;
    Ok(DefaultValue::FileOrDirectory(FileOrDirectory::File(file)))
}

fn handle_dir(path: &Path, source_dir: &Path, dest_dir: &Path) -> anyhow::Result<DefaultValue> {
    let relative_path = path.strip_prefix(source_dir)?.to_path_buf();
    let dest_path = dest_dir.join(&relative_path);
    let dest_path_as_str = dest_path.to_string_lossy();

    copy_dir(path, &dest_path)?;
    let cmd = Command::new("ls")
        .arg("-la")
        .arg(source_dir.to_string_lossy().into_owned())
        .output()?;
    println!("{}", String::from_utf8_lossy(&cmd.stdout));

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
