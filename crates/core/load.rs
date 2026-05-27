use crate::{Result, documents::CWLDocument, packed::PackedCWL};
use anyhow::Context;
use serde_json::Value;
use serde_saphyr::Options;
use std::{fs, path::Path};
use url::Url;

pub fn saphyr_options() -> Options {
    serde_saphyr::options! { strict_booleans: true, with_snippet: false }
}
/// Loads and may preprocesses a `CWLDocument` from Disk
/// # Errors
/// If file does not exist
pub fn load_cwl_file<P: AsRef<Path> + std::fmt::Debug>(
    path: P,
    preprocess: bool,
) -> Result<CWLDocument> {
    if path.as_ref().to_string_lossy().contains('#') {
        return load_cwl_from_url(path.as_ref(), preprocess);
    }

    let contents = if preprocess {
        preprocess_cwl_file(&path)?
    } else {
        fs::read_to_string(&path).with_context(|| format!("CWL File {path:?}"))?
    };

    from_str(&contents)
}

/// Loads a `CWLDocument` from a string, may be packed or unpacked
/// # Errors
/// If the string is not a valid CWL Document, or if it is a packed CWL Document but can not be unpacked
pub fn from_str(contents: &str) -> Result<CWLDocument> {
    if contents.contains("$graph") {
        let packed =
            serde_saphyr::from_str_with_options_validate::<PackedCWL>(contents, saphyr_options())
                .context("Could not parse to packed CWL")?;
        packed.unpack(None)
    } else {
        serde_saphyr::from_str_with_options_validate::<CWLDocument>(contents, saphyr_options())
            .map_err(Into::into)
    }
}

fn load_cwl_from_url(path: &Path, preprocess: bool) -> Result<CWLDocument> {
    let absolute_path = if path.is_absolute() {
        path
    } else {
        &std::path::absolute(path)?
    };
    let path_url = format!("file://{}", absolute_path.to_string_lossy());

    let url = Url::parse(&path_url)?;

    if let Some(fragment) = url.fragment() {
        let path = url
            .to_file_path()
            .map_err(|()| anyhow::anyhow!("Could not convert URL to file path: {url}"))?;
        let contents = if preprocess {
            preprocess_cwl_file(&path)?
        } else {
            fs::read_to_string(&path).with_context(|| format!("CWL File {}", path.display()))?
        };
        let pack =
            serde_saphyr::from_str_with_options_validate::<PackedCWL>(&contents, saphyr_options())?;
        return pack.unpack(Some(fragment));
    }
    Err(anyhow::anyhow!("Packed CWL could not be loaded. Can not guess fragment").into())
}

/// Preprocesses the $import sections of CWL Files
/// # Errors
/// Throws if CWL File or some of the imports do not exist
pub fn preprocess_cwl_file<P: AsRef<Path> + std::fmt::Debug>(path: P) -> Result<String> {
    let contents =
        fs::read_to_string(&path).with_context(|| format!("Could not read CWL File {path:?}"))?;
    let mut yaml: Value = serde_saphyr::from_str_with_options(&contents, saphyr_options())?;
    let path = path.as_ref().parent().unwrap_or_else(|| Path::new("."));

    resolve_imports(&mut yaml, path)?;

    Ok(serde_saphyr::to_string(&yaml)?)
}

fn resolve_imports(value: &mut Value, base_path: &Path) -> Result<()> {
    match value {
        Value::Object(map) => {
            if map.len() == 1
                && let Some(Value::String(file)) = map.get("$import")
            {
                let path = base_path.join(file);
                let contents = fs::read_to_string(&path).with_context(|| {
                    format!("Could not read imported fragment {}", path.display())
                })?;
                let mut imported_value: Value =
                    serde_saphyr::from_str_with_options(&contents, saphyr_options())?;
                resolve_imports(&mut imported_value, path.parent().unwrap_or(base_path))?;
                *value = imported_value;
                return Ok(());
            }
            for val in map.values_mut() {
                resolve_imports(val, base_path)?;
            }
        }
        Value::Array(seq) => {
            for val in seq.iter_mut() {
                resolve_imports(val, base_path)?;
            }
        }
        _ => {}
    }
    Ok(())
}

#[cfg(test)]
#[cfg(not(target_os = "windows"))] //cwl submodule is not available on windows
mod tests {
    use super::*;
    use crate::{documents::CommandLineTool, error::Error};
    use std::path::PathBuf;
    use validator::Validate;

    #[test]
    fn load_test() {
        //move to cwl submodule
        let base_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/cwl");

        //check whether all cwl files load without error, we can get files from conformance test file
        let instruction_file = base_dir.join("conformance_tests.yaml");
        let parsed = load_instruction_file(&instruction_file);
        evaluate_cwl_files(parsed, &base_dir);

        fn evaluate_cwl_files(values: Vec<Value>, base_dir: &Path) {
            for item in values {
                if let Value::Object(map) = &item {
                    if let Some(file) = map.get("$import") {
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
                        let result = load_cwl_file(&cwl_file, true);
                        if let Err(e) = &result {
                            if let Error::ParsingFailed(ex) = e {
                                eprintln!("{ex}");
                            }
                            eprintln!("Error: {result:?}");
                        }
                        assert!(result.is_ok());
                    }
                }
            }
        }

        fn load_instruction_file(instruction_file: &Path) -> Vec<Value> {
            let contents = fs::read_to_string(instruction_file).unwrap();
            serde_saphyr::from_str(&contents).unwrap()
        }
    }

    #[test]
    fn validate_cwl_version() {
        let tool = CommandLineTool::builder().cwl_version("1vaa1.2").build();
        tool.validate().expect_err("");
    }
}
