use cwl_core::{
    documents::CommandLineTool,
    requirements::DockerRequirement,
};
use cwl_execution::run_command;
use tracing_subscriber::filter::LevelFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let tool = CommandLineTool::builder()
        .base_command("python")
        .requirements(vec![DockerRequirement::builder().build().into()])
        .build();

    let subscriber = tracing_subscriber::fmt()
        .with_max_level(LevelFilter::INFO)
        .finish();
    tracing::subscriber::set_global_default(subscriber)?;

    run_command(&tool).await;

    Ok(())
}
