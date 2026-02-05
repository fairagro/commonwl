use crate::documents::CWLDocument;
use std::{fs, path::Path};

pub fn load_cwl_file<P: AsRef<Path>>(path: P, preprocess: bool) -> anyhow::Result<CWLDocument> {
    let contents = if preprocess {
        preprocess_cwl_file(&path)?
    } else {
        fs::read_to_string(&path)?
    };
    serde_yaml::from_str::<CWLDocument>(&contents).map_err(|e| e.into())
}

pub fn preprocess_cwl_file<P: AsRef<Path>>(path: P) -> anyhow::Result<String> {
    let contents = fs::read_to_string(&path)?;
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
                let contents = fs::read_to_string(&path)?;
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
