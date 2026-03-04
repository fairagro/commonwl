use anyhow::Context;
use url::Url;

use crate::{documents::CWLDocument, packed::PackedCWL};
use std::{env, fs, path::Path};

/// Loads and may preprocesses a `CWLDocument` from Disk
/// # Errors
/// If file does not exist
pub fn load_cwl_file<P: AsRef<Path> + std::fmt::Debug>(
    path: P,
    preprocess: bool,
) -> anyhow::Result<CWLDocument> {
    if path.as_ref().to_string_lossy().contains('#') {
        return load_cwl_from_url(path.as_ref(), preprocess);
    }

    let contents = if preprocess {
        preprocess_cwl_file(&path)?
    } else {
        fs::read_to_string(&path).with_context(|| format!("CWL File {path:?}"))?
    };

    if contents.contains("$graph") {
        let packed = serde_yaml::from_str::<PackedCWL>(&contents)
            .context("Could not parse to packed CWL")?;
        packed.unpack(None)
    } else {
        serde_yaml::from_str::<CWLDocument>(&contents).map_err(Into::into)
    }
}

fn load_cwl_from_url(path: &Path, preprocess: bool) -> anyhow::Result<CWLDocument> {
    let working_dir = env::current_dir()?;
    let absolute_path = if path.is_absolute() {
        path
    } else {
        &working_dir.join(path)
    };
    let path_url = format!("file://{}", absolute_path.to_string_lossy());

    let url = Url::parse(&path_url).map_err(|_| anyhow::anyhow!("Could not parse url"))?;

    if let Some(fragment) = url.fragment() {
        let path = Path::new(url.path());
        let contents = if preprocess {
            preprocess_cwl_file(path)?
        } else {
            fs::read_to_string(path).with_context(|| format!("CWL File {}", path.display()))?
        };
        let pack = serde_yaml::from_str::<PackedCWL>(&contents).map_err(|e| anyhow::anyhow!(e))?;
        return pack.unpack(Some(fragment));
    }
    anyhow::bail!("Packed CWL could not be loaded. Can not guess fragment")
}

/// Preprocesses the $import sections of CWL Files
/// # Errors
/// Throws if CWL File or some of the imports do not exist
pub fn preprocess_cwl_file<P: AsRef<Path> + std::fmt::Debug>(path: P) -> anyhow::Result<String> {
    let contents =
        fs::read_to_string(&path).with_context(|| format!("Could not read CWL File {path:?}"))?;
    let mut yaml: serde_yaml::Value = serde_yaml::from_str(&contents)?;
    let path = path.as_ref().parent().unwrap_or_else(|| Path::new("."));

    resolve_imports(&mut yaml, path)?;

    Ok(serde_yaml::to_string(&yaml)?)
}

fn resolve_imports(value: &mut serde_yaml::Value, base_path: &Path) -> anyhow::Result<()> {
    match value {
        serde_yaml::Value::Mapping(map) => {
            if map.len() == 1
                && let Some(serde_yaml::Value::String(file)) =
                    map.get(serde_yaml::Value::String("$import".to_string()))
            {
                let path = base_path.join(file);
                let contents = fs::read_to_string(&path).with_context(|| {
                    format!("Could not read imported fragment {}", path.display())
                })?;
                let mut imported_value: serde_yaml::Value = serde_yaml::from_str(&contents)?;
                resolve_imports(&mut imported_value, path.parent().unwrap_or(base_path))?;
                *value = imported_value;
                return Ok(());
            }
            for val in map.values_mut() {
                resolve_imports(val, base_path)?;
            }
        }
        serde_yaml::Value::Sequence(seq) => {
            for val in seq.iter_mut() {
                resolve_imports(val, base_path)?;
            }
        }
        _ => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn load_test() {
        //move to cwl submodule
        let base_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/cwl");

        //check whether all cwl files load without error, we can get files from conformance test file
        let instruction_file = base_dir.join("conformance_tests.yaml");
        let parsed = load_instruction_file(&instruction_file);
        evaluate_cwl_files(parsed, &base_dir);

        fn evaluate_cwl_files(values: Vec<serde_yaml::Value>, base_dir: &Path) {
            for item in values {
                if let serde_yaml::Value::Mapping(map) = &item {
                    if let Some(file) = map.get(serde_yaml::Value::String("$import".to_string())) {
                        //recurse the import
                        let instruction_file = base_dir.join(file.as_str().unwrap());
                        let base_dir = instruction_file.parent().unwrap();
                        let parsed = load_instruction_file(&instruction_file);
                        evaluate_cwl_files(parsed, base_dir);
                    } else {
                        let file = item.get("tool").unwrap().as_str().unwrap(); //always given as string!

                        //we skip packed cwl, testing somewhere else
                        if file.contains("#") || file.contains("packed") {
                            continue;
                        }

                        let cwl_file = base_dir.join(file);
                        eprintln!("Loading {cwl_file:?}");
                        let result = load_cwl_file(cwl_file, true);
                        if result.is_err() {
                            eprintln!("Error: {result:?}");
                        }
                        assert!(result.is_ok());
                    }
                }
            }
        }

        fn load_instruction_file(instruction_file: &Path) -> Vec<serde_yaml::Value> {
            let contents = fs::read_to_string(instruction_file).unwrap();
            serde_yaml::from_str(&contents).unwrap()
        }
    }
}
