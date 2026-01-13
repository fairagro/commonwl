use crate::backend::{TaskBackend, TaskRequest};
use cwl_core::documents::CWLDocument;
use std::collections::HashMap;
use tokio_util::sync::CancellationToken;
use tracing::info;

pub mod backend;
pub mod command;
pub(crate) mod docker;
pub mod inputs;

pub async fn run_command(
    definition: &CWLDocument,
    inputs: HashMap<String, serde_yaml::Value>,
    backend: impl TaskBackend,
    token: CancellationToken,
) -> anyhow::Result<()> {
    if !matches!(definition, CWLDocument::CommandLineTool(_)) {
        anyhow::bail!("Definition is not of type CommandLineTool!");
    }

    let task_request = TaskRequest {
        definition,
        inputs: &inputs,
    };

    let result = backend.run(&task_request, token).await?;
    info!("Task completed successfully");
    Ok(())
}
