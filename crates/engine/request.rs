use crate::{
    environment::env::build_environment,
    requirements::{ProcessHints, ProcessRequirements, collect_hints, collect_requirements, merge},
    schema::schema_def::replace_schema_definitions,
};
use anyhow::Context;
use cwl_core::{ExtractFromEnum, documents::CWLDocument, inputs::DefaultValue, load_cwl_file};
use indexmap::IndexMap;
use serde::Deserialize;
use std::{
    collections::{HashMap, HashSet},
    env,
    path::{Path, PathBuf},
    ptr,
};

#[derive(Debug, Clone)]
pub struct ExecutionRequest {
    pub specification: CWLDocument,
    pub inputs: HashMap<String, DefaultValue>,
    pub working_dir: PathBuf,
    pub out_dir: PathBuf,
    pub requirements: Vec<ProcessRequirements>,
    pub hints: Vec<ProcessHints>,
    pub environment: IndexMap<String, String>,
}

impl ExecutionRequest {
    #[must_use]
    pub fn get_requirement<T>(&self) -> Option<&T>
    where
        T: ExtractFromEnum<ProcessRequirements>,
    {
        self.requirements.iter().find_map(|req| T::get(req))
    }
    #[must_use]
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
    #[must_use]
    pub fn has_requirement<T>(&self) -> bool
    where
        T: ExtractFromEnum<ProcessRequirements>,
    {
        self.get_requirement::<T>().is_some()
    }
    #[must_use]
    pub fn has_requirement_or_hint<T>(&self) -> bool
    where
        T: ExtractFromEnum<ProcessRequirements>,
    {
        self.get_requirement_or_hint::<T>().is_some()
    }
}

#[derive(Deserialize, Debug, Clone, Default)]
pub struct InputObject {
    pub inputs: HashMap<String, DefaultValue>,
    pub requirements: Vec<ProcessRequirements>,
    pub hints: Vec<ProcessHints>,
}
impl InputObject {
    #[must_use]
    pub fn get_requirement<T>(&self) -> Option<&T>
    where
        T: ExtractFromEnum<ProcessRequirements>,
    {
        self.requirements.iter().find_map(|req| T::get(req))
    }
    #[must_use]
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
    #[must_use]
    pub fn has_requirement<T>(&self) -> bool
    where
        T: ExtractFromEnum<ProcessRequirements>,
    {
        self.get_requirement::<T>().is_some()
    }

    #[must_use]
    pub fn has_requirement_or_hint<T>(&self) -> bool
    where
        T: ExtractFromEnum<ProcessRequirements>,
    {
        self.get_requirement_or_hint::<T>().is_some()
    }
}

/// Load an execution context from a CWL specification file and an inputs file.
/// # Errors
/// Returns error on failed `serde_json` parsing when evaluating `SchemaDefRequirement` and on `load_input_file_from_file`
pub fn create_execution_request(
    specification_path: impl AsRef<Path> + std::fmt::Debug,
    inputs_path: impl AsRef<Path> + std::fmt::Debug,
    outputs_path: Option<&Path>,
) -> crate::Result<ExecutionRequest> {
    let working_dir = env::current_dir()?;
    let base_path = base_path(&specification_path, working_dir)?;

    let inputs = load_input_file_from_file(inputs_path, base_path)?;
    create_execution_request_with_inputs(specification_path, inputs, outputs_path, None)
}

/// Load an execution context from a CWL specification file and an already built inputs object (if inputs come as arguments, for example).
/// # Errors
/// Returns error on failed `serde_json` parsing when evaluating `SchemaDefRequirement`
pub fn create_execution_request_with_inputs(
    specification_path: impl AsRef<Path> + std::fmt::Debug,
    inputs: InputObject,
    outputs_path: Option<&Path>,
    parent: Option<&ExecutionRequest>,
) -> crate::Result<ExecutionRequest> {
    let doc = load_cwl_file(&specification_path, true)?;

    let working_dir = env::current_dir()?;
    let base_path = base_path(&specification_path, working_dir)?;

    create_execution_request_from_document(doc, inputs, base_path, outputs_path, parent)
}

/// Load an execution context from a CWL Document and an inputs object (if coming from workflow step for example).
/// # Errors
/// Returns error on failed `serde_json` parsing when evaluating `SchemaDefRequirement`
pub fn create_execution_request_from_document(
    mut specification: CWLDocument,
    inputs: InputObject,
    base_path: impl AsRef<Path>,
    outputs_path: Option<&Path>,
    parent: Option<&ExecutionRequest>,
) -> crate::Result<ExecutionRequest> {
    let environment = build_environment(&inputs);

    let mut requirements = collect_requirements(&specification, &inputs);
    let mut hints = collect_hints(&specification, &inputs);
    if let Some(parent) = parent {
        //get parent requirements and only add if not already set
        let mut wf_reqs = collect_requirements(&parent.specification, &InputObject::default());
        merge(&mut wf_reqs, &requirements);
        requirements = wf_reqs;

        let mut wf_hints = collect_hints(&parent.specification, &InputObject::default());
        merge(&mut wf_hints, &hints);
        hints = wf_hints;
    }

    replace_schema_definitions(&mut specification, &requirements)?;

    let cwd = env::current_dir()?;
    let working_dir = dunce::canonicalize(base_path.as_ref())?;

    let ctx = ExecutionRequest {
        requirements,
        hints,
        specification,
        inputs: inputs.inputs,
        working_dir,
        out_dir: outputs_path.unwrap_or(&cwd).to_path_buf(),
        environment,
    };

    Ok(ctx)
}

/// Loads an `InputObject` from file via `path`
/// # Errors
/// Returns error on failed `serde_json` parsing
pub fn load_input_file_from_file(
    path: impl AsRef<Path> + std::fmt::Debug,
    base_path: impl AsRef<Path>,
) -> crate::Result<InputObject> {
    //if not absolute it should be relative to the current working directory
    let path = if path.as_ref().is_absolute() {
        dunce::canonicalize(path)?
    } else {
        std::path::absolute(path)?
    };

    let base_path = if base_path.as_ref().is_absolute() {
        dunce::canonicalize(base_path)?
    } else {
        std::path::absolute(base_path)?
    };

    let content = std::fs::read_to_string(&path)
        .with_context(|| format!("Could not read input file {}", path.display()))?;
    let mut values: HashMap<String, serde_json::Value> = serde_saphyr::from_str(&content)?;

    //calculate path relativity
    let diff_path =
        pathdiff::diff_paths(path.parent().unwrap_or(Path::new(".")), base_path).unwrap_or(path);

    for item in values.values_mut() {
        adjust_path_to_base(item, &diff_path, &mut HashSet::new());
    }

    let mut input_object = InputObject::default();

    //trying to get inputs:
    if let Some(req_raw) = values.remove("cwl:requirements") {
        let reqs: Vec<ProcessRequirements> = serde_json::from_value(req_raw)?;
        input_object.requirements = reqs;
    }
    if let Some(hints_raw) = values.remove("cwl:hints") {
        let hints: Vec<ProcessHints> = serde_json::from_value(hints_raw)?;
        input_object.hints = hints;
    }

    //move inputs off scope here
    let values = values
        .into_iter()
        .map(|(k, v)| Ok((k, serde_json::from_value::<DefaultValue>(v)?)))
        .collect::<Result<HashMap<_, _>, serde_json::Error>>()?;

    input_object.inputs = values;

    Ok(input_object)
}

fn adjust_path_to_base(
    value: &mut serde_json::Value,
    diff_path: &Path,
    visited: &mut HashSet<*const serde_json::Value>,
) {
    let ptr = ptr::from_ref(value);
    if !visited.insert(ptr) {
        return; // already visited → break cycle
    }

    match value {
        serde_json::Value::Array(values) => {
            for v in values {
                adjust_path_to_base(v, diff_path, visited);
            }
        }
        serde_json::Value::Object(mapping) => {
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

fn base_path(
    specification_path: impl AsRef<Path> + std::fmt::Debug,
    working_dir: impl AsRef<Path>,
) -> crate::Result<PathBuf> {
    let p = specification_path
        .as_ref()
        .parent()
        .unwrap_or(working_dir.as_ref());
    Ok(if p.is_absolute() {
        dunce::canonicalize(p)?
    } else {
        working_dir.as_ref().join(p)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_execution_context() {
        let manifest_dir = dunce::canonicalize(env!("CARGO_MANIFEST_DIR")).unwrap();
        let spec_path = manifest_dir.join("../../testdata/cat-tool.cwl");
        let inputs_path = manifest_dir.join("../../testdata/cat-job.json");

        let ctx = create_execution_request(&spec_path, inputs_path, None);
        assert!(ctx.is_ok());

        let ctx = ctx.unwrap();
        assert_eq!(ctx.inputs.len(), 1);
        assert_eq!(
            ctx.working_dir,
            dunce::canonicalize(spec_path.parent().unwrap()).unwrap()
        );
    }

    #[test]
    fn test_load_input_file_same_base() {
        let tool_path =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/cat-tool.cwl");
        let inputs_path =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/cat-job.json");

        let inputs = load_input_file_from_file(&inputs_path, tool_path.parent().unwrap());
        assert!(inputs.is_ok());

        let inputs = inputs.unwrap();
        let file1 = inputs.inputs.get("file1").unwrap().as_file().unwrap();
        assert_eq!(file1.location.clone().unwrap(), "hello.txt");
    }

    #[test]
    #[cfg(not(target_os = "windows"))] //cwl submodule is not available on windows
    fn test_load_input_file_different_base() {
        let tool_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../testdata/cwl/tests/secondaryfiles/rename-inputs.cwl");
        let inputs_path =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/cwl/tests/cat-job.json");

        let inputs = load_input_file_from_file(&inputs_path, tool_path.parent().unwrap());
        assert!(inputs.is_ok());

        let inputs = inputs.unwrap();
        let file1 = inputs.inputs.get("file1").unwrap().as_file().unwrap();
        assert_eq!(file1.location.clone().unwrap(), "../hello.txt");
    }

    #[test]
    #[cfg(not(target_os = "windows"))] //cwl submodule is not available on windows
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
}
