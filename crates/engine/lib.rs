#![deny(unused_crate_dependencies)]
use cwl_core::documents::CWLDocument;
use semver::Version;

pub(crate) mod backend;
pub(crate) mod command;
pub(crate) mod docker;
pub(crate) mod environment;
pub(crate) mod expression;
pub(crate) mod input;
pub(crate) mod io;
pub(crate) mod output;
pub(crate) mod request;
pub(crate) mod requirements;
pub(crate) mod scatter;
pub(crate) mod schema;
pub(crate) mod tree;
pub(crate) mod workflow;

pub(crate) fn cwl_version(doc: &CWLDocument) -> anyhow::Result<Version> {
    let default = "v1.2".to_string();
    let version = doc.cwl_version().unwrap_or(&default);
    let version = version.trim_start_matches('v');
    let version = if version.matches('.').count() == 1 {
        format!("{version}.0")
    } else {
        version.to_owned()
    };
    Ok(Version::parse(&version)?)
}

pub(crate) const V1_2_0: Version = Version::new(1, 2, 0);

#[cfg(feature = "tes")]
pub use backend::tes::TesBackend;
pub use backend::{
    EngineStatus, ExecutionResult, TaskBackend, TaskExecutionRequest, TaskExecutionResult,
    docker::DockerBackend,
    evaluate_exitcodes, execute, execute_commandline_tool, execute_workflow,
    local::{LocalBackend, command::CommandBackend},
};
pub use docker::{
    ContainerBuildOptions, ContainerBuildOptionsBuilder, ContainerEngine, build_container_command,
};
pub use request::{
    ExecutionRequest, InputObject, create_execution_request,
    create_execution_request_from_document, create_execution_request_with_inputs,
    load_input_file_from_file,
};
