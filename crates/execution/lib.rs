use crate::backend::{TaskBackend, convert_to_task, docker::DockerBackend};
use crankshaft::config::backend::docker::Config;
use cwl_core::documents::CommandLineTool;
use std::collections::HashMap;
use tokio_util::sync::CancellationToken;
use tracing::info;

mod backend;
pub mod command;
pub mod inputs;

pub async fn run_command(tool: &CommandLineTool, inputs: HashMap<String, serde_yaml::Value>) {
    let backend = DockerBackend::new(Config::default()).await.unwrap();

    let task = convert_to_task(tool.into(), inputs).unwrap();

    let cancellation = CancellationToken::new();
    let result = backend.run(task, cancellation).await.unwrap();

    println!("{result:?}");
    info!("Task completed successfully");
}
