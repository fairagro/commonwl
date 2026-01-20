use crate::{
    input::{InputObject, load_input_file_from_file},
    requirements::{ProcessRequirements, collect_requirements},
};
use anyhow::Ok;
use cwl_core::{documents::CWLDocument, load_cwl_file, requirements::ToolHints};
use std::{collections::HashMap, env, path::Path};

pub mod command;
pub mod context;
pub mod input;
pub mod pathmapper;
pub mod requirements;

pub struct ExecutionRequest {
    pub specification: CWLDocument,
    pub inputs: HashMap<String, serde_yaml::Value>,
    pub working_dir: std::path::PathBuf,
    pub requirements: Vec<ProcessRequirements>,
    pub hints: Vec<ToolHints>,
}

/// Load an execution context from a CWL specification file and an inputs file.
pub fn load_execution_context(
    specification_path: impl AsRef<Path>,
    inputs_path: impl AsRef<Path>,
) -> anyhow::Result<ExecutionRequest> {
    let working_dir = env::current_dir()?;
    let base_path = specification_path.as_ref().parent().unwrap_or(&working_dir);

    let inputs = load_input_file_from_file(inputs_path, base_path)?;
    load_execution_context_with_inputs(specification_path, inputs)
}

/// Load an execution context from a CWL specification file and an already built inputs object (if inputs come as arguments, for example).
pub fn load_execution_context_with_inputs(
    specification_path: impl AsRef<Path>,
    inputs: InputObject,
) -> anyhow::Result<ExecutionRequest> {
    let doc = load_cwl_file(&specification_path, true)?;

    let working_dir = env::current_dir()?;
    let base_path = specification_path.as_ref().parent().unwrap_or(&working_dir);

    load_execution_context_from_document(doc, inputs, base_path)
}

/// Load an execution context from a CWL Document and an inputs object (if coming from workflow step for example).
pub fn load_execution_context_from_document(
    specification: CWLDocument,
    inputs: InputObject,
    base_path: impl AsRef<Path>,
) -> anyhow::Result<ExecutionRequest> {
    
    let ctx = ExecutionRequest {
        requirements: collect_requirements(&specification, &inputs),
        specification,
        inputs: inputs.inputs,
        hints: inputs.hints,
        working_dir: base_path.as_ref().to_path_buf(),
    };

    Ok(ctx)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_load_execution_context() {
        let spec_path =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/cwl/tests/cat-tool.cwl");
        let inputs_path =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/cwl/tests/cat-job.json");

        let ctx = load_execution_context(&spec_path, inputs_path);
        assert!(ctx.is_ok());

        let ctx = ctx.unwrap();
        assert_eq!(ctx.inputs.len(), 1);
        assert_eq!(ctx.working_dir, spec_path.parent().unwrap());
    }
}
