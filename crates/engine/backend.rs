use crate::{
    environment::build_environment,
    input::{InputObject, load_input_file_from_file},
    requirements::{ProcessHints, ProcessRequirements, collect_hints, collect_requirements},
    schema::replace_schema_definitions,
};
use anyhow::Ok;
use cwl_core::{documents::CWLDocument, inputs::DefaultValue, load_cwl_file};
use indexmap::IndexMap;
use nonempty::NonEmpty;
use std::{
    collections::HashMap,
    env,
    path::{Path, PathBuf},
    process::ExitStatus,
};
use tokio_util::sync::CancellationToken;

pub mod docker;

pub trait TaskBackend {
    fn run(
        &self,
        request: &ExecutionRequest,
        token: CancellationToken,
    ) -> impl Future<Output = anyhow::Result<ExecutionResult>> + Send;
}
#[derive(Debug, Clone)]

pub struct ExecutionResult {
    pub exit_status: NonEmpty<ExitStatus>,
    pub stdout: String,
    pub stderr: String,
    pub outputs: HashMap<String, DefaultValue>,
}

#[derive(Debug, Clone)]
pub struct ExecutionRequest {
    pub specification: CWLDocument,
    pub inputs: HashMap<String, serde_yaml::Value>,
    pub working_dir: PathBuf,
    pub out_dir: PathBuf,
    pub requirements: Vec<ProcessRequirements>,
    pub hints: Vec<ProcessHints>,
    pub environment: IndexMap<String, String>,
}

/// Load an execution context from a CWL specification file and an inputs file.
pub fn load_execution_context(
    specification_path: impl AsRef<Path> + std::fmt::Debug,
    inputs_path: impl AsRef<Path> + std::fmt::Debug,
    outputs_path: Option<&Path>,
) -> anyhow::Result<ExecutionRequest> {
    let working_dir = env::current_dir()?;
    let base_path = specification_path.as_ref().parent().unwrap_or(&working_dir);

    let inputs = load_input_file_from_file(inputs_path, base_path)?;
    load_execution_context_with_inputs(specification_path, inputs, outputs_path)
}

/// Load an execution context from a CWL specification file and an already built inputs object (if inputs come as arguments, for example).
pub fn load_execution_context_with_inputs(
    specification_path: impl AsRef<Path> + std::fmt::Debug,
    inputs: InputObject,
    outputs_path: Option<&Path>,
) -> anyhow::Result<ExecutionRequest> {
    let doc = load_cwl_file(&specification_path, true)?;

    let working_dir = env::current_dir()?;
    let base_path = specification_path.as_ref().parent().unwrap_or(&working_dir);

    load_execution_context_from_document(doc, inputs, base_path, outputs_path)
}

/// Load an execution context from a CWL Document and an inputs object (if coming from workflow step for example).
pub fn load_execution_context_from_document(
    mut specification: CWLDocument,
    inputs: InputObject,
    base_path: impl AsRef<Path>,
    outputs_path: Option<&Path>,
) -> anyhow::Result<ExecutionRequest> {
    let environment = build_environment(&inputs);
    let requirements = collect_requirements(&specification, &inputs);
    let hints = collect_hints(&specification, &inputs);

    replace_schema_definitions(&mut specification, &requirements)?;

    let ctx = ExecutionRequest {
        requirements,
        hints,
        specification,
        inputs: inputs.inputs,
        working_dir: base_path.as_ref().to_path_buf(),
        out_dir: outputs_path.unwrap_or(base_path.as_ref()).to_path_buf(),
        environment,
    };

    Ok(ctx)
}

pub enum EngineStatus {
    Success(i32),
    Failure(i32),
    Undefined(i32),
}

pub fn evaluate_exitcodes(exit_codes: NonEmpty<ExitStatus>, doc: &CWLDocument) -> EngineStatus {
    //currently we only look at first code
    let actual_code = exit_codes.first();
    let code = actual_code.code().unwrap();
    if let CWLDocument::CommandLineTool(tool) = doc {
        let success_codes = tool.success_codes.clone().unwrap_or(vec![0]);
        let failure_codes = tool.permanent_fail_codes.clone().unwrap_or(vec![1]);
        if success_codes.contains(&code) {
            EngineStatus::Success(code)
        } else if failure_codes.contains(&code) {
            EngineStatus::Failure(code)
        } else {
            EngineStatus::Undefined(code)
        }
    } else if code != 0 {
        EngineStatus::Failure(code)
    } else {
        EngineStatus::Success(code)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_execution_context() {
        let spec_path =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/cwl/tests/cat-tool.cwl");
        let inputs_path =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/cwl/tests/cat-job.json");

        let ctx = load_execution_context(&spec_path, inputs_path, None);
        assert!(ctx.is_ok());

        let ctx = ctx.unwrap();
        assert_eq!(ctx.inputs.len(), 1);
        assert_eq!(ctx.working_dir, spec_path.parent().unwrap());
    }
}
