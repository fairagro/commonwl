use crate::{
    OneOrMany,
    documents::{
        CWLDocument, CommandLineTool, ExpressionTool, Operation, StringOrDocument, Workflow,
        WorkflowStep,
    },
    outputs::StringOrWorkflowStepOutput,
};
use salad::Identifiable;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Default, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PackedCWL {
    #[serde(rename = "$graph")]
    pub graph: Vec<CWLDocument>,
    pub cwl_version: Option<String>,
}

impl PackedCWL {
    pub fn unpack(self, root_entity: Option<&str>) -> anyhow::Result<CWLDocument> {
        let needle = root_entity.unwrap_or("main");

        //get main entity
        let main = self
            .graph
            .iter()
            .find(|i| {
                i.get_id() == Some(&needle.to_owned())
                    || i.get_id() == Some(&("#".to_owned() + needle))
            })
            .cloned();

        let Some(mut main) = main else {
            anyhow::bail!("Could not find root entity")
        };

        //unpack main entity
        match &mut main {
            CWLDocument::CommandLineTool(clt) => unpack_command_line_tool(clt),
            CWLDocument::ExpressionTool(et) => unpack_expression_tool(et),
            CWLDocument::Operation(op) => unpack_operation(op),
            CWLDocument::Workflow(wf) => unpack_workflow(wf, &self.graph),
        }

        Ok(main)
    }
}

fn unpack_workflow(wf: &mut Workflow, graph: &[CWLDocument]) {
    let base_id = wf.id.clone().unwrap();

    for input in &mut wf.inputs {
        unpack_identifiable(input, &base_id);
    }

    for step in &mut wf.steps {
        unpack_workflow_step(step, &base_id, graph);
    }

    for output in &mut wf.outputs {
        unpack_identifiable(output, &base_id);

        if let Some(output_source) = &mut output.output_source {
            match output_source {
                OneOrMany::One(src) => {
                    *src = src
                        .strip_prefix(&format!("{base_id}/"))
                        .unwrap_or(src)
                        .to_string()
                }
                OneOrMany::Many(items) => {
                    for src in items {
                        *src = src
                            .strip_prefix(&format!("{base_id}/"))
                            .unwrap_or(src)
                            .to_string();
                    }
                }
            }
        }
    }
}

fn unpack_command_line_tool(clt: &mut CommandLineTool) {
    let base_id = clt.id.clone().unwrap();

    for input in &mut clt.inputs {
        unpack_identifiable(input, &base_id);
    }

    for output in &mut clt.outputs {
        unpack_identifiable(output, &base_id);
    }
}

fn unpack_expression_tool(et: &mut ExpressionTool) {
    let base_id = et.id.clone().unwrap();

    for input in &mut et.inputs {
        unpack_identifiable(input, &base_id);
    }

    for output in &mut et.outputs {
        unpack_identifiable(output, &base_id);
    }
}

fn unpack_operation(op: &mut Operation) {
    let base_id = op.id.clone().unwrap();

    for input in &mut op.inputs {
        unpack_identifiable(input, &base_id);
    }

    for output in &mut op.outputs {
        unpack_identifiable(output, &base_id);
    }
}

fn unpack_identifiable(ident: &mut dyn Identifiable, base_id: &str) {
    let new_id = ident.get_id().map(|id| {
        id.strip_prefix(&format!("{base_id}/"))
            .unwrap_or(id)
            .to_string()
    });

    if let Some(new_id) = new_id {
        ident.set_id(&new_id);
    }
}

fn unpack_workflow_step(step: &mut WorkflowStep, base_id: &str, graph: &[CWLDocument]) {
    let step_id = step.id.clone().unwrap();

    if let StringOrDocument::String(run) = &step.run {
        let run = run.strip_prefix("#").unwrap_or(run);
        let op = graph.iter().find(|i| {
            i.get_id() == Some(&("#".to_string() + run))
                || i.get_id().map(String::as_str) == Some(run)
        });

        if let Some(op) = op {
            let mut op = op.clone(); //we need it owned
            match &mut op {
                CWLDocument::CommandLineTool(clt) => unpack_command_line_tool(clt),
                CWLDocument::ExpressionTool(et) => unpack_expression_tool(et),
                CWLDocument::Operation(op) => unpack_operation(op),
                CWLDocument::Workflow(wf) => unpack_workflow(wf, graph),
            }

            step.run = StringOrDocument::Document(Box::new(op));
        }
    }

    for input in &mut step.r#in {
        unpack_identifiable(input, &step_id);
    }

    for output in &mut step.out {
        match output {
            StringOrWorkflowStepOutput::String(string) => {
                *string = string
                    .strip_prefix(&format!("{step_id}/"))
                    .unwrap_or(string)
                    .to_owned();
            }
            StringOrWorkflowStepOutput::WorkflowStepOutput(step_output) => {
                unpack_identifiable(step_output, &step_id)
            }
        }
    }

    unpack_identifiable(step, base_id);
}
