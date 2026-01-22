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

pub fn collect_command_outputs(
    outputs: &[CommandOutputParameter],
    source_dir: &Path,
    dest_dir: &Path,
) -> anyhow::Result<HashMap<String, DefaultValue>> {
    let mut output_map: HashMap<String, DefaultValue> = HashMap::new();

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
            _ => todo!(),
        }
    }

    Ok(output_map)
}

fn handle_file(path: &Path, source_dir: &Path, dest_dir: &Path) -> anyhow::Result<DefaultValue> {
    let relative_path = path.strip_prefix(source_dir)?.to_path_buf();
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
    let cmd = Command::new("ls").arg("-la").arg(source_dir.to_string_lossy().into_owned()).output()?;
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
