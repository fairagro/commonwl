use std::collections::HashMap;

use cwl_core::{
    documents::CommandLineTool,
    files::{File, FileOrDirectory},
    inputs::{CommandInputParameter, CommandLineBinding},
    types::CWLType,
};
use cwl_execution::run_command;
use tracing_subscriber::filter::LevelFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let tool = CommandLineTool::builder()
        .base_command("ls")
        .inputs(vec![
            CommandInputParameter::builder()
                .id("my-input")
                .r#type(CWLType::File)
                .default(File::builder().path(".gitignore").build())
                .input_binding(
                    CommandLineBinding::builder()
                        .prefix("-la")
                        .position(0)
                        .build(),
                )
                .build(),
        ])
        .build();

    let subscriber = tracing_subscriber::fmt()
        .with_max_level(LevelFilter::INFO)
        .finish();
    tracing::subscriber::set_global_default(subscriber)?;

    let file = FileOrDirectory::File(File::builder().path("Cargo.lock").build());
    let inputs = HashMap::from([("my-input".to_string(), serde_yaml::to_value(file)?)]);

    run_command(&tool, inputs).await;

    Ok(())
}
