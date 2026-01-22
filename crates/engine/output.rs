use cwl_core::{
    OneOrMany,
    files::{File, FileOrDirectory},
    inputs::DefaultValue,
    outputs::{CommandOutputParameter, CommandOutputParameterType, CommandOutputType},
    types::CWLType,
};
use glob::glob;
use sha1::{Digest, Sha1};
use std::{collections::HashMap, fs, path::Path};
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
            )) => todo!(),
            _ => todo!(),
        }
    }

    Ok(output_map)
}

fn handle_file(path: &Path, source_dir: &Path, dest_dir: &Path) -> anyhow::Result<DefaultValue> {
    let relative_path = path.strip_prefix(source_dir)?.to_path_buf();
    let dest_path = dest_dir.join(&relative_path);
    let dest_path_as_str = dest_path.to_string_lossy();

    fs::copy(path, &dest_path)?;

    let basename = dest_path
        .file_name()
        .map(|f| f.to_string_lossy().into_owned());
    let nameroot = dest_path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned());
    let nameext = dest_path
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy()));
    let dirname = dest_path.parent().map(|p| p.to_string_lossy().into_owned());
    let metadata = fs::metadata(path)?;

    let size = metadata.len();

    let mut hasher = Sha1::new();
    let hash = fs::read(&dest_path).ok().map(|f| {
        hasher.update(&f);
        let hash = hasher.finalize();
        format!("sha1${hash:x}")
    });

    let file = File::builder()
        .location(format!("file://{}", &dest_path_as_str))
        .path(dest_path_as_str)
        .maybe_basename(basename)
        .maybe_nameroot(nameroot)
        .maybe_nameext(nameext)
        .maybe_dirname(dirname)
        .size(size)
        .maybe_checksum(hash)
        .build();

    Ok(DefaultValue::FileOrDirectory(FileOrDirectory::File(file)))
}
