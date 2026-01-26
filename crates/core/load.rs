use crate::documents::CWLDocument;
use serde_yaml::Value;
use std::{collections::HashMap, fs, path::Path};

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
    resolve_schema_definitions(&mut yaml)?;

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

fn resolve_schema_definitions(value: &mut serde_yaml::Value) -> anyhow::Result<()> {
    let mut defs = extract_schema_definitions(value)?;

    if !defs.is_empty() {
        let snapshot = &defs.clone();
        for item in defs.values_mut() {
            replace_schema_references(item, snapshot)?;
        }

        replace_schema_references(value, &defs)?;
    }
    Ok(())
}

fn extract_schema_definitions(
    doc: &serde_yaml::Value,
) -> anyhow::Result<HashMap<String, serde_yaml::Value>> {
    let mut schemas = HashMap::new();
    if let serde_yaml::Value::Mapping(root) = doc
        && let Some(serde_yaml::Value::Sequence(requirements)) =
            root.get(serde_yaml::Value::String("requirements".to_string()))
    {
        for req in requirements {
            if let serde_yaml::Value::Mapping(req_map) = req
                && let Some(serde_yaml::Value::String(class)) =
                    req_map.get(serde_yaml::Value::String("class".to_string()))
                && class == "SchemaDefRequirement"
                && let Some(serde_yaml::Value::Sequence(types)) =
                    req_map.get(serde_yaml::Value::String("types".to_string()))
            {
                for type_def in types {
                    if let serde_yaml::Value::Mapping(type_map) = type_def
                        && let Some(serde_yaml::Value::String(name)) =
                            type_map.get(serde_yaml::Value::String("name".to_string()))
                    {
                        schemas.insert(format!("#{}", name), type_def.clone());
                    }
                }
            }
        }
    }

    Ok(schemas)
}

fn replace_schema_references(
    value: &mut serde_yaml::Value,
    schemas: &HashMap<String, Value>,
) -> anyhow::Result<()> {
    match value {
        serde_yaml::Value::String(s) => {
            if s.starts_with('#') && schemas.contains_key(s) {
                *value = schemas[s].clone();
            }
        }
        serde_yaml::Value::Sequence(arr) => {
            for item in arr {
                replace_schema_references(item, schemas)?;
            }
        }
        serde_yaml::Value::Mapping(map) => {
            for v in map.values_mut() {
                replace_schema_references(v, schemas)?;
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

    #[test]
    fn test_extract_schema_definitions() {
        let base_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/cwl");
        let tool_path = base_dir.join("tests/tmap-tool.cwl");
        let contents = fs::read_to_string(&tool_path).unwrap();
        let yaml: serde_yaml::Value = serde_yaml::from_str(&contents).unwrap();

        let items = extract_schema_definitions(&yaml).unwrap();
        assert_eq!(items.len(), 5);
    }

    #[test]
    fn test_resolve_schema_definitions() {
        let base_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/cwl");
        let tool_path = base_dir.join("tests/tmap-tool.cwl");
        let contents = fs::read_to_string(&tool_path).unwrap();
        let mut yaml: serde_yaml::Value = serde_yaml::from_str(&contents).unwrap();

        resolve_schema_definitions(&mut yaml).unwrap();
        dbg!(yaml);
    }
}
