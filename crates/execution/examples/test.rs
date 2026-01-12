use cwl_core::{
    OneOrMany,
    documents::CommandLineTool,
    inputs::{CommandInputParameter, CommandLineBinding},
    outputs::{CommandOutputBinding, CommandOutputParameter},
    types::CWLType,
};
use cwl_execution::run_command;
use std::collections::HashMap;
use tracing_subscriber::filter::LevelFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let tool = CommandLineTool::builder()
        .base_command("touch")
        .inputs(vec![
            CommandInputParameter::builder()
                .id("my-input")
                .r#type(CWLType::String)
                .default("hello.txt")
                .input_binding(CommandLineBinding::builder().position(1).build())
                .build(),
        ])
        .outputs(vec![
            CommandOutputParameter::builder()
                .id("my-output")
                .r#type(CWLType::File)
                .output_binding(
                    CommandOutputBinding::builder()
                        .glob(OneOrMany::One("*.txt".to_string()))
                        .build(),
                )
                .build(),
        ])
        .build();

    let subscriber = tracing_subscriber::fmt()
        .with_max_level(LevelFilter::INFO)
        .finish();
    tracing::subscriber::set_global_default(subscriber)?;

    let file = "hello.txt".to_string();
    let inputs = HashMap::from([("my-input".to_string(), serde_yaml::to_value(file)?)]);

    run_command(&tool, inputs).await;

    Ok(())
}
