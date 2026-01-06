use crankshaft::{
    Engine,
    config::backend::{Kind, docker::Config},
    engine::{
        Task,
        task::{Execution, Output, output::Type},
    },
};
use cwl_core::documents::CommandLineTool;
use nonempty::nonempty;
use std::{env::current_dir, fs, path::Path};
use tokio_util::sync::CancellationToken;
use tracing::info;
use url::Url;

pub async fn run_command(_tool: &CommandLineTool) {
    fs::File::create("stdout").unwrap();
    let path = Path::new("stdout").canonicalize().unwrap();
    let config = crankshaft::config::backend::Config::builder()
        .name("docker")
        .kind(Kind::Docker(Config::builder().build()))
        .max_tasks(10)
        .build();

    let engine = Engine::default().with(config).await.unwrap();

    let task = Task::builder()
        .executions(nonempty![
            Execution::builder()
                .work_dir(
                    current_dir()
                        .expect("a current working directory")
                        .display()
                        .to_string()
                )
                .image("python:3.8-slim")
                .program("echo")
                .stdout("/stdout")
                .stderr("/stderr")
                .args(["pen island".to_string()])
                .build(),
        ])
        .outputs(vec![
            Output::builder()
                .name("stdout")
                .path("/stdout")
                .url(Url::from_file_path(path).expect("wat"))
                .ty(Type::File)
                .build(),
        ])
        .build();

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
