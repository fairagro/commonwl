use crankshaft::engine::{Task, task::Execution};
use cwl_core::documents::{CommandLineTool, ExpressionTool, WorkflowStep};
use nonempty::nonempty;
use std::collections::HashMap;

use crate::command;

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

    let task = Task::builder()
        .maybe_name(tool.label.clone())
        .executions(nonempty![
            Execution::builder()
                .program(&args[0])
                .args(&args[1..])
                .image("python:3.12") //todo get real imnage
                .build()
        ]);

    Ok(task.build())
}

fn convert_expression_tool(_tool: &ExpressionTool) -> anyhow::Result<Task> {
    unimplemented!()
}

fn convert_workflow_step(_step: &WorkflowStep) -> anyhow::Result<Task> {
    unimplemented!()
}
