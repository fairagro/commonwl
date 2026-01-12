use crate::{command, inputs::collect_inputs};
use crankshaft::engine::{
    Task,
    service::runner::backend::TaskRunError,
    task::{
        Execution, Input, Output,
        input::{self, Contents},
        output,
    },
};
use cwl_core::{
    documents::{CommandLineTool, ExpressionTool, WorkflowStep},
    files::FileOrDirectory,
    inputs::{DefaultValue, InputDataProvider},
};
use nonempty::{NonEmpty, nonempty};
use std::{collections::HashMap, fs, path::Path, process::ExitStatus};
use tokio_util::sync::CancellationToken;
use tracing::info;
use url::Url;

pub mod docker;

pub(crate) enum TaskKind<'a> {
    WorkflowStep(&'a WorkflowStep),
    CommandLineTool(&'a CommandLineTool),
    ExpressionTool(&'a ExpressionTool),
}

impl<'a> From<&'a WorkflowStep> for TaskKind<'a> {
    fn from(step: &'a WorkflowStep) -> Self {
        TaskKind::WorkflowStep(step)
    }
}

impl<'a> From<&'a CommandLineTool> for TaskKind<'a> {
    fn from(tool: &'a CommandLineTool) -> Self {
        TaskKind::CommandLineTool(tool)
    }
}

impl<'a> From<&'a ExpressionTool> for TaskKind<'a> {
    fn from(tool: &'a ExpressionTool) -> Self {
        TaskKind::ExpressionTool(tool)
    }
}

pub(crate) fn convert_to_task(
    kind: TaskKind,
    inputs: HashMap<String, serde_yaml::Value>,
) -> anyhow::Result<Task> {
    match kind {
        TaskKind::CommandLineTool(tool) => convert_command_line_tool(tool, inputs),
        TaskKind::ExpressionTool(tool) => convert_expression_tool(tool),
        TaskKind::WorkflowStep(step) => convert_workflow_step(step),
    }
}

fn convert_command_line_tool(
    tool: &CommandLineTool,
    inputs: HashMap<String, serde_yaml::Value>,
) -> anyhow::Result<Task> {
    let args = command::build_command(tool, &inputs)?;
    info!("scheduling execution: {}", args.join(" "));

    let mut task = Task::builder()
        .maybe_name(tool.label.clone())
        .executions(nonempty![
            Execution::builder()
                .program(&args[0])
                .work_dir("/workdir")
                .args(&args[1..])
                .image("alpine") //todo get real imnage
                .stdout("/stdout")
                .stderr("/stderr")
                .build()
        ])
        .build();

    //collect inputs and use staging mechanisms for file in dir
    let input_values = collect_inputs(&tool.clone().into(), &inputs)?;

    for input in &tool.inputs {
        let value = input_values.get(&input.id().clone().unwrap()).unwrap();
        //we stage here, so we want to only get file or dir
        if let DefaultValue::FileOrDirectory(fod) = value {
            let str = value.to_string();
            let ty = if matches!(fod, FileOrDirectory::File(_)) {
                input::Type::File
            } else {
                input::Type::Directory
            };
            task.add_input(
                Input::builder()
                    .name(input.id().clone().unwrap())
                    .contents(Contents::Path(
                        Path::new(&str).canonicalize()?.to_path_buf(),
                    ))
                    .path(format!("/workdir/{str}"))
                    .ty(ty)
                    .build(),
            );
        }
    }

    fs::File::create("./stdout")?;
    let stdout = Path::new("./stdout").canonicalize()?;
    task.add_output(
        Output::builder()
            .path("/stdout")
            .url(
                Url::from_file_path(&stdout)
                    .map_err(|_| anyhow::anyhow!("failed to get stdout URL"))?,
            )
            .ty(output::Type::File)
            .build(),
    );

    fs::File::create("./stderr")?;
    let stderr = Path::new("./stderr").canonicalize()?;
    task.add_output(
        Output::builder()
            .path("/stderr")
            .url(
                Url::from_file_path(&stderr)
                    .map_err(|_| anyhow::anyhow!("failed to get stderr URL"))?,
            )
            .ty(output::Type::File)
            .build(),
    );

    Ok(task)
}

fn convert_expression_tool(_tool: &ExpressionTool) -> anyhow::Result<Task> {
    unimplemented!()
}

fn convert_workflow_step(_step: &WorkflowStep) -> anyhow::Result<Task> {
    unimplemented!()
}

pub trait TaskBackend {
    async fn run(
        self,
        task: Task,
        token: CancellationToken,
    ) -> Result<NonEmpty<ExitStatus>, TaskRunError>;
}
