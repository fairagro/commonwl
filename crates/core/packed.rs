use crate::{
    documents::{
        CWLDocument, CommandLineTool, ExpressionTool, Operation, StringOrDocument, Workflow,
        WorkflowStep,
    },
    files::FileOrDirectory,
    inputs::{CommandInputParameter, DefaultValue, WorkflowInputParameter},
    normalize_path,
    outputs::StringOrWorkflowStepOutput,
};
use anyhow::ensure;
use commonwl_salad::Identifiable;
use serde::{Deserialize, Serialize};
use std::path::Path;
use url::Url;

#[derive(Serialize, Deserialize, Debug, Default, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PackedCWL {
    #[serde(rename = "$graph")]
    pub graph: Vec<CWLDocument>,
    pub cwl_version: Option<String>,
}

impl PackedCWL {
    /// Tries to unpack `PackedCWL` to `CWLDocument`
    /// # Errors
    /// - If root entity is invalid (e.g. #main)
    pub fn unpack(self, root_entity: Option<&str>) -> anyhow::Result<CWLDocument> {
        let needle = root_entity.unwrap_or_else(|| {
            if self.graph.len() == 1 {
                self.graph[0].get_id().map_or("main", |v| v)
            } else {
                "main"
            }
        });

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
            *output_source = output_source.clone().map(|src| {
                src.strip_prefix(&format!("{base_id}/"))
                    .unwrap_or(&src)
                    .to_string()
            });
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
        if let Some(source) = &mut input.source {
            *source = source.clone().map(|src| {
                src.strip_prefix(&format!("{base_id}/"))
                    .unwrap_or(&src)
                    .to_string()
            });
        }
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
                unpack_identifiable(step_output, &step_id);
            }
        }
    }

    unpack_identifiable(step, base_id);
}

pub fn pack_cwl(
    spec: &CWLDocument,
    filename: impl AsRef<Path>,
    id: Option<&str>,
) -> anyhow::Result<PackedCWL> {
    Ok(match spec {
        CWLDocument::CommandLineTool(_) => PackedCWL {
            graph: vec![pack_tool(spec.clone(), filename, id)?],
            cwl_version: spec.cwl_version().cloned(),
        },
        CWLDocument::ExpressionTool(_) => PackedCWL {
            graph: vec![pack_tool(spec.clone(), filename, id)?],
            cwl_version: spec.cwl_version().cloned(),
        },
        CWLDocument::Workflow(wf) => todo!(),
        CWLDocument::Operation(_) => unimplemented!(),
    })
}

fn pack_tool(
    mut tool: CWLDocument,
    filename: impl AsRef<Path>,
    id: Option<&str>,
) -> anyhow::Result<CWLDocument> {
    ensure!(
        matches!(tool, CWLDocument::CommandLineTool(_))
            || matches!(tool, CWLDocument::ExpressionTool(_))
    );

    let tool_dir = filename.as_ref().parent().unwrap();
    let name = filename.as_ref().file_name().unwrap().to_string_lossy();

    if let Some(id) = id {
        tool.set_id(id);
    } else if let Some(id) = tool.get_id() {
        tool.set_id(&format!("#{id}"));
    } else {
        tool.set_id(&format!("#{name}"));
    }

    let id = tool.get_id().cloned().unwrap();
    match &mut tool {
        CWLDocument::CommandLineTool(clt) => {
            for input in &mut clt.inputs {
                pack_command_input(input, &id, tool_dir)?;
            }
            for output in &mut clt.outputs {
                output.id = Some(format!("{id}/{}", output.id.as_ref().unwrap()));
            }
        }
        CWLDocument::ExpressionTool(et) => {
            for input in &mut et.inputs {
                pack_workflow_input(input, &id, tool_dir)?;
            }
            for output in &mut et.outputs {
                output.id = Some(format!("{id}/{}", output.id.as_ref().unwrap()));
            }
        }
        _ => {}
    }

    Ok(tool)
}

fn pack_command_input(
    input: &mut CommandInputParameter,
    root_id: &str,
    doc_dir: impl AsRef<Path>,
) -> anyhow::Result<()> {
    input.id = Some(format!("{root_id}/{}", input.id.as_ref().unwrap()));

    if let Some(default) = &mut input.default {
        match default {
            DefaultValue::FileOrDirectory(FileOrDirectory::File(f)) => {
                if let Some(loc) = &mut f.location
                    && Url::parse(loc).is_err()
                {
                    let p = Path::new(&loc);
                    if p.is_absolute() {
                        *loc = Url::from_file_path(p)
                            .map_err(|()| anyhow::anyhow!("Could not convert path to URL"))?
                            .to_string();
                    } else {
                        let p = doc_dir.as_ref().join(p);
                        let p = if p.exists() {
                            p.canonicalize().unwrap_or(p).to_string_lossy().into_owned()
                        } else {
                            normalize_path(&p)
                                .unwrap_or(p)
                                .to_string_lossy()
                                .into_owned()
                        };
                        *loc = Url::from_file_path(p)
                            .map_err(|()| anyhow::anyhow!("Could not convert path to URL"))?
                            .to_string();
                    }
                }
            }
            DefaultValue::FileOrDirectory(FileOrDirectory::Directory(d)) => {
                if let Some(loc) = &mut d.location
                    && Url::parse(loc).is_err()
                {
                    let p = Path::new(&loc);
                    if p.is_absolute() {
                        *loc = Url::from_file_path(p)
                            .map_err(|()| anyhow::anyhow!("Could not convert path to URL"))?
                            .to_string();
                    } else {
                        let p = doc_dir.as_ref().join(p);
                        let p = if p.exists() {
                            p.canonicalize().unwrap_or(p).to_string_lossy().into_owned()
                        } else {
                            normalize_path(&p)
                                .unwrap_or(p)
                                .to_string_lossy()
                                .into_owned()
                        };
                        *loc = Url::from_file_path(p)
                            .map_err(|()| anyhow::anyhow!("Could not convert path to URL"))?
                            .to_string();
                    }
                }
            }
            DefaultValue::Any(_) => {}
        }
    }

    Ok(())
}

fn pack_workflow_input(
    input: &mut WorkflowInputParameter,
    root_id: &str,
    doc_dir: impl AsRef<Path>,
) -> anyhow::Result<()> {
    input.id = Some(format!("{root_id}/{}", input.id.as_ref().unwrap()));

    if let Some(default) = &mut input.default {
        match default {
            DefaultValue::FileOrDirectory(FileOrDirectory::File(f)) => {
                if let Some(loc) = &mut f.location
                    && Url::parse(&loc).is_err()
                {
                    let p = Path::new(&loc);
                    if p.is_absolute() {
                        *loc = Url::from_file_path(p)
                            .map_err(|()| anyhow::anyhow!("Could not convert path to URL"))?
                            .to_string();
                    } else {
                        let p = doc_dir.as_ref().join(p);
                        let p = if p.exists() {
                            p.canonicalize().unwrap_or(p).to_string_lossy().into_owned()
                        } else {
                            normalize_path(&p)
                                .unwrap_or(p)
                                .to_string_lossy()
                                .into_owned()
                        };
                        *loc = Url::from_file_path(p)
                            .map_err(|()| anyhow::anyhow!("Could not convert path to URL"))?
                            .to_string();
                    }
                }
            }
            DefaultValue::FileOrDirectory(FileOrDirectory::Directory(d)) => {
                if let Some(loc) = &mut d.location
                    && Url::parse(&loc).is_err()
                {
                    let p = Path::new(&loc);
                    if p.is_absolute() {
                        *loc = Url::from_file_path(p)
                            .map_err(|()| anyhow::anyhow!("Could not convert path to URL"))?
                            .to_string();
                    } else {
                        let p = doc_dir.as_ref().join(p);
                        let p = if p.exists() {
                            p.canonicalize().unwrap_or(p).to_string_lossy().into_owned()
                        } else {
                            normalize_path(&p)
                                .unwrap_or(p)
                                .to_string_lossy()
                                .into_owned()
                        };
                        *loc = Url::from_file_path(p)
                            .map_err(|()| anyhow::anyhow!("Could not convert path to URL"))?
                            .to_string();
                    }
                }
            }
            DefaultValue::Any(_) => {}
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        files::{File, FileOrDirectory},
        inputs::{CommandInputParameter, CommandLineBinding, DefaultValue},
        load_cwl_file,
        types::CWLType,
    };
    use std::path::{MAIN_SEPARATOR_STR, Path};

    pub fn normalize_json_newlines(val: &mut serde_json::Value) {
        match val {
            serde_json::Value::String(s) => {
                *s = s.replace("\r\n", "\n");
            }
            serde_json::Value::Array(arr) => {
                for item in arr {
                    normalize_json_newlines(item);
                }
            }
            serde_json::Value::Object(map) => {
                for value in map.values_mut() {
                    normalize_json_newlines(value);
                }
            }
            _ => {}
        }
    }

    #[test]
    fn test_pack_input() {
        let path = Path::new("../../testdata/packed/data/population.csv")
            .canonicalize()
            .unwrap();
        let mut input = CommandInputParameter::builder()
            .id("population")
            .r#type(CWLType::File)
            .default(DefaultValue::FileOrDirectory(FileOrDirectory::File(
                File::builder()
                    .location(Url::from_file_path(path).unwrap().as_str())
                    .build(),
            )))
            .input_binding(CommandLineBinding::builder().prefix("--population").build())
            .build();

        let base_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .canonicalize()
            .unwrap();

        let file_path = base_dir.join("testdata/packed/workflows/calculation");
        pack_command_input(&mut input, "#calculation.cwl", file_path).unwrap();

        let json = serde_json::json!(&input);
        let reference_json = r##"{
                    "id": "#calculation.cwl/population",
                    "type": "File",
                    "default": {
                        "class": "File",
                        "location": "file://XXX/testdata/packed/data/population.csv"
                    },
                    "inputBinding": {
                        "prefix": "--population"
                    }
                }"##
        .replace(
            "XXX",
            &base_dir.to_string_lossy().replace(MAIN_SEPARATOR_STR, "/"),
        )
        .replace("//?", "");

        let value: serde_json::Value = serde_json::from_str(&reference_json).unwrap();
        assert_eq!(json, value);
    }

    #[test]
    fn test_pack_commandlinetool() {
        let path = Path::new("../../testdata/packed/workflows/calculation/calculation.cwl");
        let tool = load_cwl_file(path, true).unwrap();
        let packed = &pack_cwl(&tool, path, Some("#main")).unwrap().graph[0];

        let base_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .canonicalize()
            .unwrap();

        let mut json = serde_json::json!(packed);
        let reference_json = include_str!("../../testdata/packed/calculation_packed.cwl")
            .replace(
                "/mnt/commonwl",
                &base_dir.to_string_lossy().replace(MAIN_SEPARATOR_STR, "/"),
            )
            .replace("//?", "");

        let mut reference: serde_json::Value = serde_json::from_str(&reference_json).unwrap();
        normalize_json_newlines(&mut json);
        normalize_json_newlines(&mut reference);
        assert_eq!(json, reference);
    }
}
