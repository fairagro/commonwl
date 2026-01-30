use crate::{
    checksum,
    command::to_str,
    pathmapper::PathMapper,
    requirements::{ProcessHints, ProcessRequirements},
};
use cwl_core::{
    ExtractFromEnum, FileMetaData, FilePathMetaData, Integer, OneOrMany,
    documents::{CWLDocument, CommandLineTool},
    files::{File, FileOrDirectory},
    get_file_metadata, get_path_metadata,
    inputs::{
        CommandInputParameterType, DefaultValue, InputArraySchema, InputEnumSchema,
        InputRecordSchema, InputSchema, InputType, OperationInputParameter,
    },
    types::CWLType,
};
use serde::Deserialize;
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
};

#[derive(Deserialize, Debug, Clone, Default)]
pub struct InputObject {
    pub inputs: HashMap<String, serde_yaml::Value>,
    pub requirements: Vec<ProcessRequirements>,
    pub hints: Vec<ProcessHints>,
}
impl InputObject {
    pub fn get_requirement<T>(&self) -> Option<&T>
    where
        T: ExtractFromEnum<ProcessRequirements>,
    {
        self.requirements.iter().find_map(|req| T::get(req))
    }

    pub fn get_requirement_or_hint<T>(&self) -> Option<&T>
    where
        T: ExtractFromEnum<ProcessRequirements>,
    {
        let maybe_req = self.requirements.iter().find_map(|req| T::get(req));
        let maybe_hint = self.hints.iter().find_map(|hint| {
            if let ProcessHints::Requirement(inner) = hint {
                T::get(inner)
            } else {
                None
            }
        });
        maybe_req.or(maybe_hint)
    }

    pub fn has_requirement<T>(&self) -> bool
    where
        T: ExtractFromEnum<ProcessRequirements>,
    {
        self.get_requirement::<T>().is_some()
    }

    pub fn has_requirement_or_hint<T>(&self) -> bool
    where
        T: ExtractFromEnum<ProcessRequirements>,
    {
        self.get_requirement_or_hint::<T>().is_some()
    }
}

pub fn load_input_file_from_file(
    path: impl AsRef<Path>,
    base_path: impl AsRef<Path>,
) -> anyhow::Result<InputObject> {
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

    let mut input_object = InputObject::default();

    //trying to get inputs:
    if let Some(req_raw) = values.remove("cwl:requirements") {
        let reqs: Vec<ProcessRequirements> = serde_yaml::from_value(req_raw)?;
        input_object.requirements = reqs;
    }
    if let Some(hints_raw) = values.remove("cwl:hints") {
        let hints: Vec<ProcessHints> = serde_yaml::from_value(hints_raw)?;
        input_object.hints = hints;
    }

    //move inputs off scope here
    input_object.inputs = values;

    Ok(input_object)
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
                //insert to path also if none
                if mapping.get("path").is_none() {
                    mapping.insert("path".into(), p.to_string_lossy().to_string().into());
                }
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
    for input in &doc.get_inputs() {
        // collect the actual value
        let mut value = get_input_value(input, inputs)?;

        //update file path field
        if let DefaultValue::FileOrDirectory(fod) = &mut value {
            fod.dry_validation();
        }

        //do some validation
        let valid = match doc {
            CWLDocument::CommandLineTool(clt) => {
                //can have stdin...
                let Some(command_input) = clt.inputs.iter().find(|i| i.id == input.id) else {
                    anyhow::bail!(
                        "Could not find input `{}`",
                        input.id.clone().unwrap_or_default()
                    )
                };
                validate_command_input(&command_input.r#type, &value)
            }

            _ => match &input.r#type {
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
                input.id.clone().unwrap_or_default()
            )
        }

        values.insert(input.id.clone().unwrap_or_default(), value);
    }

    Ok(values)
}

pub(crate) fn get_input_value(
    input: &OperationInputParameter,
    inputs: &HashMap<String, serde_yaml::Value>,
) -> anyhow::Result<DefaultValue> {
    let value = inputs.get(&input.id.clone().unwrap_or_default());
    Ok(
        if let Some(value) = value
            && !value.is_null()
        {
            serde_yaml::from_value::<DefaultValue>(value.clone())?
        } else if let Some(default) = &input.default {
            default.clone()
        } else {
            DefaultValue::Any(serde_yaml::Value::Null)
        },
    )
}

pub fn get_stdin(tool: &CommandLineTool, inputs: &HashMap<String, DefaultValue>) -> Option<String> {
    if let Some(stdin) = &tool.stdin {
        return Some(stdin.to_string());
    }

    if let Some(input) = tool
        .inputs
        .iter()
        .find(|i| matches!(i.r#type, CommandInputParameterType::Stdin))
    {
        return inputs
            .get(&input.id.clone().unwrap_or_default())
            .map(to_str);
    }
    None
}

//flattens inputs of any type to a list of file or directory
pub fn flatten_inputs<'a, I: Iterator<Item = &'a DefaultValue>>(inputs: I) -> Vec<FileOrDirectory> {
    let mut flattened = vec![];
    for input in inputs {
        flatten_inputs_impl(input, &mut flattened);
    }
    flattened
}

fn flatten_inputs_impl(dv: &DefaultValue, flattened: &mut Vec<FileOrDirectory>) {
    match dv {
        DefaultValue::FileOrDirectory(fod) => {
            flattened.push(fod.clone());
            if let FileOrDirectory::File(f) = fod
                && let Some(secondary_files) = &f.secondary_files
            {
                flattened.extend(secondary_files.clone());
            }
        }
        DefaultValue::Any(v) => match v {
            serde_yaml::Value::Sequence(values) => {
                for v in values {
                    if let Ok(dv) = serde_yaml::from_value(v.clone()) {
                        flatten_inputs_impl(&dv, flattened);
                    }
                }
            }
            serde_yaml::Value::Mapping(mapping) => {
                for v in mapping.values() {
                    if let Ok(dv) = serde_yaml::from_value(v.clone()) {
                        flatten_inputs_impl(&dv, flattened);
                    }
                }
            }
            _ => {}
        },
    }
}

pub fn fill_input_metadata(
    inputs: &HashMap<String, DefaultValue>,
    doc: &CWLDocument,
    path_mapper: &PathMapper,
) -> anyhow::Result<HashMap<String, DefaultValue>> {
    let mut map = HashMap::new();
    let providers = doc.get_inputs();

    for (key, value) in inputs {
        let input = providers
            .iter()
            .find(|i| i.id == Some(key.to_string()))
            .unwrap();
        let value = create_metadata_for_input(value, input, path_mapper)?;
        map.insert(key.clone(), value);
    }

    Ok(map)
}

fn create_metadata_for_input(
    value: &DefaultValue,
    input: &OperationInputParameter,
    path_mapper: &PathMapper,
) -> anyhow::Result<DefaultValue> {
    match value {
        DefaultValue::FileOrDirectory(FileOrDirectory::File(f)) if f.path.is_some() => {
            let path = f.path.clone().unwrap();
            let path = Path::new(&path);
            let guest_path = path_mapper.get_guest(path).unwrap();
            let host_path = path_mapper.get_host(guest_path).unwrap();
            let FilePathMetaData {
                basename,
                nameroot,
                nameext,
                dirname,
            } = get_path_metadata(host_path);
            let FileMetaData { size, checksum } = get_file_metadata(host_path)?;

            Ok(DefaultValue::FileOrDirectory(FileOrDirectory::File(
                File::builder()
                    .path(host_path.to_string_lossy())
                    .maybe_basename(basename)
                    .maybe_nameroot(nameroot)
                    .maybe_nameext(nameext)
                    .maybe_dirname(dirname)
                    .maybe_checksum(checksum)
                    .size(Integer::Long(size as i64))
                    .maybe_format(f.format.clone())
                    .build(),
            )))
        }
        DefaultValue::FileOrDirectory(FileOrDirectory::Directory(d)) if d.path.is_some() => {
            let path = d.path.clone().unwrap();
            let path = Path::new(&path);
            let guest_path = path_mapper.get_guest(path).unwrap();
            let host_path = path_mapper.get_host(guest_path).unwrap();
            let mut d = d.clone();
            d.path = Some(host_path.to_string_lossy().to_string());
            if let Some(load_listing) = input.load_listing {
                d.load_listing(load_listing)?;
            }

            Ok(DefaultValue::FileOrDirectory(FileOrDirectory::Directory(d)))
        }
        DefaultValue::FileOrDirectory(FileOrDirectory::File(file)) if file.contents.is_some() => {
            let contents = file.contents.clone().unwrap();
            let mut f = file.clone();
            f.checksum = Some(checksum(&contents));
            Ok(DefaultValue::FileOrDirectory(FileOrDirectory::File(f)))
        }
        DefaultValue::Any(serde_yaml::Value::Sequence(vec)) => {
            let mut items = vec![];
            for item in vec {
                let dv = serde_yaml::from_value(item.clone())?;
                items.push(create_metadata_for_input(&dv, input, path_mapper)?);
            }
            let value = serde_yaml::to_value(&items)?;
            Ok(DefaultValue::Any(value))
        }
        //TODO: records
        default => Ok(default.clone()),
    }
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
            CWLType::Any => true,
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
                    OneOrMany::One(item) => validate_input_type(
                        item,
                        &serde_yaml::from_value(field_value.clone())
                            .expect("DefaultValue violates itself"),
                    ),
                    OneOrMany::Many(items) => items.iter().any(|item| {
                        validate_input_type(
                            item,
                            &serde_yaml::from_value(field_value.clone())
                                .expect("DefaultValue violates itself"),
                        )
                    }),
                }
            } else {
                // Field is missing - check if it's optional (has null in union type)
                match &f.r#type {
                    OneOrMany::One(InputType::CWLType(CWLType::Null)) => true,
                    OneOrMany::Many(items) => items
                        .iter()
                        .any(|item| matches!(item, InputType::CWLType(CWLType::Null))),
                    _ => false,
                }
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
                    .any(|t| validate_input_type(&t.clone().into(), &item_value)),
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
    use cwl_core::{documents::CommandLineTool, files::File, load_cwl_file};
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
            .inputs
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
    fn test_load_input_file_different_base() {
        let tool_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../testdata/cwl/tests/secondaryfiles/rename-inputs.cwl");
        let inputs_path =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/cwl/tests/cat-job.json");

        let inputs = load_input_file_from_file(&inputs_path, tool_path.parent().unwrap());
        assert!(inputs.is_ok());

        let inputs = inputs.unwrap();
        let file1_loc = inputs
            .inputs
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

    #[test]
    fn test_load_input_file_requirements() {
        let tool_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../testdata/cwl/tests/secondaryfiles/rename-inputs.cwl");
        let inputs_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../testdata/cwl/tests/env-job4.yaml");

        let inputs = load_input_file_from_file(&inputs_path, tool_path.parent().unwrap());
        assert!(inputs.is_ok());

        let inputs = inputs.unwrap();
        assert_eq!(inputs.requirements.len(), 1);
    }

    #[test]
    fn test_get_stdin() {
        let base_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../testdata/cwl/tests")
            .canonicalize()
            .unwrap();
        let specification_path = base_dir.join("cat-tool-shortcut.cwl");
        let inputs_path = base_dir.join("cat-job.json");

        let inputs = load_input_file_from_file(&inputs_path, base_dir).unwrap();
        let doc = load_cwl_file(specification_path, false).unwrap();
        let inputs = collect_inputs(&doc, &inputs.inputs).unwrap();

        let CWLDocument::CommandLineTool(tool) = doc else {
            panic!("Oh no!")
        };
        let stdin = get_stdin(&tool, &inputs);
        assert_eq!(stdin, Some("hello.txt".into()));
    }
}
