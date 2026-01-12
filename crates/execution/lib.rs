use crate::backend::{TaskBackend, TaskRequest, docker::DockerBackend};
use crankshaft::config::backend::docker::Config;
use cwl_core::documents::{CWLDocument, CommandLineTool};
use std::collections::HashMap;
use tokio_util::sync::CancellationToken;
use tracing::info;

pub mod backend;
pub mod command;
pub mod inputs;

pub async fn run_command(tool: &CommandLineTool, inputs: HashMap<String, serde_yaml::Value>) {
    let backend = DockerBackend::new(Config::default()).await.unwrap();

    let task_request = TaskRequest {
        definition: &CWLDocument::CommandLineTool(tool.clone()),
        inputs: &inputs,
    };

    let cancellation = CancellationToken::new();
    let result = backend.run(&task_request, cancellation).await.unwrap();

    println!("{result:?}");
    info!("Task completed successfully");
}
