use crate::backend::convert_to_task;
use crankshaft::{
    Engine,
    config::backend::{Kind, docker::Config},
};
use cwl_core::documents::CommandLineTool;
use std::collections::HashMap;
use tokio_util::sync::CancellationToken;
use tracing::info;

mod backend;
pub mod command;
pub mod inputs;

pub async fn run_command(tool: &CommandLineTool, inputs: HashMap<String, serde_yaml::Value>) {
    let config = crankshaft::config::backend::Config::builder()
        .name("docker")
        .kind(Kind::Docker(Config::default()))
        .max_tasks(10)
        .build();

    let engine = Engine::default().with(config).await.unwrap();

    let task = convert_to_task(tool.into(), inputs).unwrap();

    let cancellation = CancellationToken::new();
    engine
        .spawn("docker", task, cancellation)
        .await
        .unwrap()
        .wait()
        .await
        .unwrap();
    info!("Task completed successfully");
}
