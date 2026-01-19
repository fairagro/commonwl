use cwl_core::{
    OneOrMany,
    documents::CWLDocument,
    inputs::{
        CommandInputParameterType, DefaultValue, InputArraySchema, InputDataProvider,
        InputEnumSchema, InputRecordSchema, InputSchema, InputType,
    },
    types::CWLType,
};
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
};

pub fn load_input_file_from_file(
    path: impl AsRef<Path>,
    base_path: impl AsRef<Path>,
) -> anyhow::Result<HashMap<String, serde_yaml::Value>> {
    let content = std::fs::read_to_string(path.as_ref())?;
    let mut values: HashMap<String, serde_yaml::Value> = serde_yaml::from_str(&content)?;

    //calculate path relativity
    let diff_path = pathdiff::diff_paths(
        path.as_ref().parent().unwrap_or(Path::new(".")),
        base_path.as_ref(),
    )
    .unwrap_or(PathBuf::from(path.as_ref()));

    for item in values.values_mut() {
        adjust_path_to_base(item, &diff_path, &mut HashSet::new());
    }

    Ok(values)
}

fn adjust_path_to_base(
    value: &mut serde_yaml::Value,
    diff_path: &Path,
    visited: &mut HashSet<*const serde_yaml::Value>,
) {
    let ptr = value as *const _;
    if !visited.insert(ptr) {
        return; // already visited → break cycle
    }

    match value {
        serde_yaml::Value::Sequence(values) => {
            for v in values {
                adjust_path_to_base(v, diff_path, visited);
            }
        }
        serde_yaml::Value::Mapping(mapping) => {
            for (_, v) in mapping.iter_mut() {
                adjust_path_to_base(v, diff_path, visited);
            }

            if let Some(path_val) = mapping.get("path").and_then(|v| v.as_str()) {
                let mut p = PathBuf::from(path_val);
                if !p.is_absolute() {
                    p = diff_path.join(p);
                }
                mapping.insert("path".into(), p.to_string_lossy().to_string().into());
            }

            if let Some(path_val) = mapping.get("location").and_then(|v| v.as_str()) {
                let mut p = PathBuf::from(path_val);
                if !p.is_absolute() {
                    p = diff_path.join(p);
                }
                mapping.insert("location".into(), p.to_string_lossy().to_string().into());
            }
        }
        _ => {}
    }
}

pub fn collect_inputs(
    doc: &CWLDocument,
    inputs: &HashMap<String, serde_yaml::Value>,
) -> anyhow::Result<HashMap<String, DefaultValue>> {
    let mut values = HashMap::new();
    for input in doc.get_input_data_providers() {
        // collect the actual value
        let value = get_input_value(input, inputs)?;

        //do some validation
        let valid = match doc {
            CWLDocument::CommandLineTool(clt) => {
                //can have stdin...
                let Some(command_input) = clt.inputs.iter().find(|i| &i.id == input.id()) else {
                    anyhow::bail!(
                        "Could not find input `{}`",
                        input.id().clone().unwrap_or_default()
                    )
                };
                validate_command_input(&command_input.r#type, &value)
            }
            //we are allowed to unwrap here, read the trait comment
            _ => match input.r#type().unwrap() {
                OneOrMany::One(item) => validate_input_type(&item.clone(), &value),
                OneOrMany::Many(items) => items
                    .iter()
                    .any(|i| validate_input_type(&i.clone(), &value)),
            },
        };

        //error if validity can not be confirmed
        if !valid {
            anyhow::bail!(
                "Value {value:?} is not valid for `{}`",
                input.id().clone().unwrap_or_default()
            )
        }

        values.insert(input.id().clone().unwrap_or_default(), value);
    }
    Ok(values)
}

pub(crate) fn get_input_value(
    input: &dyn InputDataProvider,
    inputs: &HashMap<String, serde_yaml::Value>,
) -> anyhow::Result<DefaultValue> {
    let value = inputs.get(&input.id().clone().unwrap_or_default());
    Ok(
        if let Some(value) = value
            && !value.is_null()
        {
            serde_yaml::from_value::<DefaultValue>(value.clone())?
        } else if let Some(default) = input.default() {
            default.clone()
        } else {
            DefaultValue::Any(serde_yaml::Value::Null)
        },
    )
}

pub fn validate_command_input(schema: &CommandInputParameterType, value: &DefaultValue) -> bool {
    match schema {
        CommandInputParameterType::Stdin => !value.is_null(), // for stdin we accept any existing value
        CommandInputParameterType::CommandInputType(one_or_many) => match one_or_many {
            OneOrMany::One(item) => validate_input_type(&item.clone().into(), value),
            OneOrMany::Many(items) => items
                .iter()
                .any(|i| validate_input_type(&i.clone().into(), value)),
        },
    }
}

fn validate_input_type(r#type: &InputType, value: &DefaultValue) -> bool {
    match r#type {
        InputType::CWLType(ty) => validate_cwl_type(*ty, value),
        InputType::InputSchema(schema) => validate_schema(schema, value),
        InputType::String(_) => {
            if let Some(val) = value.try_get_value_ref() {
                val.is_string()
            } else {
                false
            }
        }
    }
}

fn validate_cwl_type(r#type: CWLType, value: &DefaultValue) -> bool {
    match value {
        DefaultValue::FileOrDirectory(fod) => match r#type {
            CWLType::File => fod.is_file(),
            CWLType::Directory => fod.is_dir(),
            _ => false,
        },
        DefaultValue::Any(value) => match r#type {
            CWLType::Null => value.is_null(),
            CWLType::Boolean => value.is_bool(),
            CWLType::Int => value.is_i64(),
            CWLType::Long => value.is_i64(),
            CWLType::Float => value.is_f64(),
            CWLType::Double => value.is_f64(),
            CWLType::String => value.is_string(),
            _ => false,
        },
    }
}

fn validate_schema(schema: &InputSchema, value: &DefaultValue) -> bool {
    match schema {
        InputSchema::Record(rec) => validate_record_schema(rec, value),
        InputSchema::Enum(enu) => validate_enum_schema(enu, value),
        InputSchema::Array(arr) => validate_array_schema(arr, value),
    }
}

fn validate_record_schema(schema: &InputRecordSchema, value: &DefaultValue) -> bool {
    let mapping = match value {
        DefaultValue::Any(serde_yaml::Value::Mapping(map)) => map,
        _ => return false,
    };

    if let Some(fields) = &schema.fields {
        return fields.iter().all(|f| {
            let key = serde_yaml::Value::String(f.name.clone());
            if let Some(field_value) = mapping.get(&key) {
                match &f.r#type {
                    OneOrMany::One(item) => {
                        validate_input_type(item, &DefaultValue::Any(field_value.clone()))
                    }
                    OneOrMany::Many(items) => items.iter().any(|item| {
                        validate_input_type(item, &DefaultValue::Any(field_value.clone()))
                    }),
                }
            } else {
                false
            }
        });
    }
    false
}

fn validate_array_schema(schema: &InputArraySchema, value: &DefaultValue) -> bool {
    //check whether we have a sequence!
    if let DefaultValue::Any(serde_yaml::Value::Sequence(seq)) = value {
        seq.iter().all(|item| {
            let item_value: DefaultValue =
                serde_yaml::from_value(item.clone()).expect("DefaultValue violates itself");
            match &schema.items {
                OneOrMany::One(t) => validate_input_type(&t.clone().into(), &item_value),
                OneOrMany::Many(ts) => ts
                    .iter()
                    .all(|t| validate_input_type(&t.clone().into(), &item_value)),
            }
        })
    } else {
        false
    }
}

fn validate_enum_schema(schema: &InputEnumSchema, value: &DefaultValue) -> bool {
    if let DefaultValue::Any(val) = value {
        if let Some(s) = val.as_str() {
            schema.symbols.contains(&s.to_string())
        } else {
            false
        }
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cwl_core::{documents::CommandLineTool, files::File};
    use std::collections::HashMap;

    #[test]
    fn test_input_validation_complex() {
        let tool: CommandLineTool =
            serde_yaml::from_str(include_str!("../../testdata/cwl/tests/binding-test.cwl"))
                .unwrap();
        let mut inputs_values: HashMap<String, DefaultValue> =
            serde_yaml::from_str(include_str!("../../testdata/cwl/tests/bwa-mem-job.json"))
                .unwrap();

        //append the default value as we do not test that here
        inputs_values.insert(
            "#args.py".to_string(),
            File::builder().path("args.py").build().into(),
        );

        for input in tool.inputs {
            let id = input.id.unwrap();
            let fetched_value = &inputs_values[&id];
            assert!(
                validate_command_input(&input.r#type, fetched_value),
                "failed in {id}"
            )
        }
    }

    #[test]
    fn test_input_validation_enum() {
        let tool: CommandLineTool = serde_yaml::from_str(include_str!(
            "../../testdata/cwl/tests/anon_enum_inside_array.cwl"
        ))
        .unwrap();
        let inputs_values: HashMap<String, DefaultValue> = serde_yaml::from_str(include_str!(
            "../../testdata/cwl/tests/anon_enum_inside_array.yml"
        ))
        .unwrap();

        for input in tool.inputs {
            let id = input.id.unwrap();
            let fetched_value = &inputs_values[&id];
            assert!(
                validate_command_input(&input.r#type, fetched_value),
                "failed in {id}"
            )
        }
    }

    #[test]
    fn test_collect_inputs() {
        let tool: CommandLineTool = serde_yaml::from_str(include_str!(
            "../../testdata/cwl/tests/anon_enum_inside_array.cwl"
        ))
        .unwrap();
        let inputs_values: HashMap<String, serde_yaml::Value> = serde_yaml::from_str(include_str!(
            "../../testdata/cwl/tests/anon_enum_inside_array.yml"
        ))
        .unwrap();

        let inputs = collect_inputs(&CWLDocument::CommandLineTool(tool), &inputs_values);
        assert!(inputs.is_ok());

        assert_eq!(inputs.unwrap().len(), 2);
    }

    #[test]
    fn test_load_input_file_same_base() {
        let tool_path =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/cwl/tests/cat-tool.cwl");
        let inputs_path =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/cwl/tests/cat-job.json");

        let inputs = load_input_file_from_file(&inputs_path, tool_path.parent().unwrap());
        assert!(inputs.is_ok());

        let inputs = inputs.unwrap();
        let file1_loc = inputs
            .get("file1")
            .unwrap()
            .as_mapping()
            .unwrap()
            .get("location")
            .unwrap()
            .as_str()
            .unwrap();
        assert_eq!(file1_loc, "hello.txt");
    }

    #[test]
    fn test_load_input_file_differentbase() {
        let tool_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../testdata/cwl/tests/secondaryfiles/rename-inputs.cwl");
        let inputs_path =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/cwl/tests/cat-job.json");

        let inputs = load_input_file_from_file(&inputs_path, tool_path.parent().unwrap());
        assert!(inputs.is_ok());

        let inputs = inputs.unwrap();
        let file1_loc = inputs
            .get("file1")
            .unwrap()
            .as_mapping()
            .unwrap()
            .get("location")
            .unwrap()
            .as_str()
            .unwrap();
        assert_eq!(file1_loc, "../hello.txt");
    }
}
