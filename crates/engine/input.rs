use crate::{
    command::to_str,
    input::validation::{validate_command_input, validate_input_type},
    requirements::{ProcessHints, ProcessRequirements},
};
use cwl_core::{
    ExtractFromEnum, OneOrMany,
    documents::{CWLDocument, CommandLineTool},
    inputs::{CommandInputParameterType, DefaultValue, OperationInputParameter},
};
use serde::Deserialize;
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
};

pub mod file_system;
pub mod validation;

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
        sanitize_paths(&mut value)?;
        values.insert(input.id.clone().unwrap_or_default(), value);
    }

    Ok(values)
}

fn sanitize_paths(value: &mut DefaultValue) -> anyhow::Result<()> {
    match value {
        DefaultValue::FileOrDirectory(fod) => {
            fod.dry_validation();
            if let Some(path) = fod.path()
                && path.starts_with("./")
            {
                fod.set_path(Some(path.strip_prefix("./").unwrap_or(path).into()));
            }
        }
        DefaultValue::Any(serde_yaml::Value::Sequence(vec)) => {
            for item in vec {
                let mut dv = serde_yaml::from_value(item.clone())?;
                sanitize_paths(&mut dv)?;
                *item = serde_yaml::to_value(&dv)?;
            }
        }
        DefaultValue::Any(serde_yaml::Value::Mapping(map)) => {
            for item in map.values_mut() {
                let mut dv = serde_yaml::from_value(item.clone())?;
                sanitize_paths(&mut dv)?;
                *item = serde_yaml::to_value(&dv)?;
            }
        }
        _ => {}
    }
    Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;
    use cwl_core::{documents::CommandLineTool, load_cwl_file};
    use std::collections::HashMap;

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
