use crate::{
    input::{InputObject, load_input_file_from_file},
    requirements::{ProcessHints, ProcessRequirements, collect_hints, collect_requirements},
};
use anyhow::Ok;
use crankshaft::engine::service::runner::backend::TaskRunError;
use cwl_core::{documents::CWLDocument, load_cwl_file};
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
    ) -> impl Future<Output = Result<NonEmpty<ExitStatus>, TaskRunError>> + Send;
}

#[derive(Debug, Clone)]
pub struct ExecutionRequest {
    pub specification: CWLDocument,
    pub inputs: HashMap<String, serde_yaml::Value>,
    pub working_dir: PathBuf,
    pub requirements: Vec<ProcessRequirements>,
    pub hints: Vec<ProcessHints>,
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
        hints: collect_hints(&specification, &inputs),
        specification,
        inputs: inputs.inputs,
        working_dir: base_path.as_ref().to_path_buf(),
    };

    Ok(ctx)
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

        let ctx = load_execution_context(&spec_path, inputs_path);
        assert!(ctx.is_ok());

        let ctx = ctx.unwrap();
        assert_eq!(ctx.inputs.len(), 1);
        assert_eq!(ctx.working_dir, spec_path.parent().unwrap());
    }
}
