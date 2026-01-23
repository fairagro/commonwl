use crate::{
    input::{InputObject, load_input_file_from_file},
    pathmapper::PathMapper,
    requirements::{ProcessHints, ProcessRequirements, collect_hints, collect_requirements},
};
use anyhow::Ok;
use cwl_core::{documents::CWLDocument, files::FileOrDirectory, load_cwl_file};
use dircpy::copy_dir;
use nonempty::NonEmpty;
use std::{
    collections::HashMap,
    env, fs,
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
    ) -> impl Future<Output = anyhow::Result<NonEmpty<ExitStatus>>> + Send;
}

#[derive(Debug, Clone)]
pub struct ExecutionRequest {
    pub specification: CWLDocument,
    pub inputs: HashMap<String, serde_yaml::Value>,
    pub working_dir: PathBuf,
    pub out_dir: PathBuf,
    pub requirements: Vec<ProcessRequirements>,
    pub hints: Vec<ProcessHints>,
}

/// Load an execution context from a CWL specification file and an inputs file.
pub fn load_execution_context(
    specification_path: impl AsRef<Path>,
    inputs_path: impl AsRef<Path>,
    outputs_path: Option<&Path>,
) -> anyhow::Result<ExecutionRequest> {
    let working_dir = env::current_dir()?;
    let base_path = specification_path.as_ref().parent().unwrap_or(&working_dir);

    let inputs = load_input_file_from_file(inputs_path, base_path)?;
    load_execution_context_with_inputs(specification_path, inputs, outputs_path)
}

/// Load an execution context from a CWL specification file and an already built inputs object (if inputs come as arguments, for example).
pub fn load_execution_context_with_inputs(
    specification_path: impl AsRef<Path>,
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
    specification: CWLDocument,
    inputs: InputObject,
    base_path: impl AsRef<Path>,
    outputs_path: Option<&Path>,
) -> anyhow::Result<ExecutionRequest> {
    let ctx = ExecutionRequest {
        requirements: collect_requirements(&specification, &inputs),
        hints: collect_hints(&specification, &inputs),
        specification,
        inputs: inputs.inputs,
        working_dir: base_path.as_ref().to_path_buf(),
        out_dir: outputs_path.unwrap_or(base_path.as_ref()).to_path_buf(),
    };

    Ok(ctx)
}

/// Creates the synthetic directory and adds it to the pathmapper
pub(crate) fn handle_synthetic_directories(
    flattened_inputs: &mut Vec<FileOrDirectory>,
    path_mapper: &mut PathMapper,
    work_dir: &Path,
    tmpdir: &Path,
) -> anyhow::Result<()> {
    for mut input in flattened_inputs {
        input.dry_validation();
        let mut path = input.path().cloned();

        if path.is_none()
            && let FileOrDirectory::Directory(dir) = &mut input
            && let Some(listing) = &dir.listing
            && let Some(basename) = &dir.basename
        {
            //create from listing
            let host_path = tmpdir.join(basename);
            fs::create_dir(&host_path)?;

            let base_path = Path::new(basename);

            //fix path
            let host_path_str = host_path.to_string_lossy().into_owned();
            path = Some(host_path_str);
            dir.path = path;

            for item in listing {
                let mut item = item.clone();
                item.dry_validation();

                let c_path = item.path().unwrap();
                let c_host_path = host_path.join(c_path);
                let staged_path = path_mapper.predict_staged_path(base_path.join(c_path));

                path_mapper.add_tripel(&c_host_path, staged_path, c_path)?;

                let source_path = work_dir.join(c_path);
                //copy into tmpdir
                match item {
                    FileOrDirectory::File(_) => {
                        fs::copy(&source_path, &c_host_path)?;
                    }
                    FileOrDirectory::Directory(_) => copy_dir(&source_path, &c_host_path)?,
                }
            }

            let staged_path = path_mapper.predict_staged_path(basename);
            path_mapper.add_tripel(&host_path, staged_path, basename)?;
        }
    }

    Ok(())
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
