use crate::{
    OneOrMany, Result,
    documents::{
        CWLDocument, CommandLineTool, ExpressionTool, Operation, StringOrDocument, Workflow,
        WorkflowStep,
    },
    files::FileOrDirectory,
    guard,
    inputs::{CommandInputParameter, DefaultValue, WorkflowInputParameter},
    load_cwl_file, normalize_path,
    outputs::StringOrWorkflowStepOutput,
    requirements::{
        DockerRequirement, InitialWorkDirRequirement, InlineJavascriptRequirement, ListingItems,
        StringOrInclude, ToolRequirements, WorkDirItems, WorkflowRequirements,
    },
    validate::CWL_VERSION,
};
use anyhow::ensure;
use cwl_salad::Identifiable;
use serde::{Deserialize, Serialize};
use std::{fs, path::Path};
use url::Url;
use validator::Validate;

#[derive(Serialize, Deserialize, Debug, Default, Clone, Validate)]
#[serde(rename_all = "camelCase")]
pub struct PackedCWL {
    #[serde(rename = "$graph")]
    pub graph: Vec<CWLDocument>,
    #[validate(regex(path = *CWL_VERSION))]
    pub cwl_version: Option<String>,
}

impl PackedCWL {
    /// Tries to unpack `PackedCWL` to `CWLDocument`
    /// # Errors
    /// - If root entity is invalid (e.g. #main)
    pub fn unpack(self, root_entity: Option<&str>) -> Result<CWLDocument> {
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

        guard!(let Some(mut main) = main ,"Could not find root entity");

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

/// Generates a document graph from a given Document
/// # Errors
/// Can return error if packing of workflow
pub fn pack_cwl(
    spec: &CWLDocument,
    filename: impl AsRef<Path>,
    id: Option<&str>,
) -> Result<Vec<CWLDocument>> {
    Ok(match spec {
        CWLDocument::CommandLineTool(_) | CWLDocument::ExpressionTool(_) => {
            vec![pack_tool(spec.clone(), filename, id)?]
        }
        CWLDocument::Workflow(wf) => pack_workflow(wf, filename, id)?.graph,
        CWLDocument::Operation(_) => unimplemented!(),
    })
}

/// Generates a packed cwl file for a given Document
/// # Errors
/// Can return error if packing of workflow
/// # Panics
/// unwrap used on wf.id after definitvely setting it -> no panic
pub fn pack_workflow(
    wf: &Workflow,
    filename: impl AsRef<Path>,
    id: Option<&str>,
) -> Result<PackedCWL> {
    let mut wf = wf.clone();
    if let Some(id) = id {
        wf.id = Some(id.to_string());
    } else {
        wf.id = Some("#main".to_string());
    }

    let wf_dir = filename.as_ref().parent().unwrap_or(filename.as_ref());
    let wf_id = wf.id.clone().unwrap();

    let mut graph = vec![];

    for input in &mut wf.inputs {
        pack_workflow_input(input, &wf_id, wf_dir)?;
    }

    for output in &mut wf.outputs {
        output.id = Some(format!("{wf_id}/{}", output.id.as_ref().unwrap()));
        if let Some(output_source) = &mut output.output_source {
            match output_source {
                OneOrMany::One(item) => *item = format!("{wf_id}/{item}"),
                OneOrMany::Many(items) => {
                    for item in items {
                        *item = format!("{wf_id}/{item}");
                    }
                }
            }
        }
    }

    if let Some(reqs) = &mut wf.requirements {
        for req in reqs {
            match req {
                WorkflowRequirements::InitialWorkDirRequirement(iwdr) => {
                    pack_iwdr(iwdr, wf_dir)?;
                }
                WorkflowRequirements::DockerRequirement(dr) => {
                    pack_docker_requirement(dr, wf_dir)?;
                }
                WorkflowRequirements::InlineJavascriptRequirement(ijs) => {
                    pack_js_requirement(ijs, wf_dir)?;
                }
                _ => {}
            }
        }
    }

    for step in &mut wf.steps {
        graph.extend(pack_step(step, wf_dir, &wf_id)?);
    }

    let cwl_version = Some(
        wf.cwl_version
            .as_ref()
            .map_or("v1.2".to_string(), Clone::clone),
    );
    wf.cwl_version = None;

    graph.push(CWLDocument::Workflow(wf.clone()));
    graph.sort_by(|a, b| a.get_id().unwrap().cmp(b.get_id().unwrap()));

    Ok(PackedCWL { graph, cwl_version })
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
            if let Some(reqs) = &mut clt.requirements {
                for req in reqs {
                    match req {
                        ToolRequirements::InitialWorkDirRequirement(iwdr) => {
                            pack_iwdr(iwdr, tool_dir)?;
                        }
                        ToolRequirements::DockerRequirement(dr) => {
                            pack_docker_requirement(dr, tool_dir)?;
                        }
                        ToolRequirements::InlineJavascriptRequirement(ijs) => {
                            pack_js_requirement(ijs, tool_dir)?;
                        }
                        _ => {}
                    }
                }
            }
        }
        CWLDocument::ExpressionTool(et) => {
            for input in &mut et.inputs {
                pack_workflow_input(input, &id, tool_dir)?;
            }
            for output in &mut et.outputs {
                output.id = Some(format!("{id}/{}", output.id.as_ref().unwrap()));
            }
            if let Some(reqs) = &mut et.requirements {
                for req in reqs {
                    match req {
                        WorkflowRequirements::InitialWorkDirRequirement(iwdr) => {
                            pack_iwdr(iwdr, tool_dir)?;
                        }
                        WorkflowRequirements::DockerRequirement(dr) => {
                            pack_docker_requirement(dr, tool_dir)?;
                        }
                        WorkflowRequirements::InlineJavascriptRequirement(ijs) => {
                            pack_js_requirement(ijs, tool_dir)?;
                        }
                        _ => {}
                    }
                }
            }
        }
        _ => {}
    }

    Ok(tool)
}

fn pack_step(
    step: &mut WorkflowStep,
    wf_dir: impl AsRef<Path>,
    wf_id: &str,
) -> Result<Vec<CWLDocument>> {
    let step_id = format!("{wf_id}/{}", step.id.as_ref().unwrap());
    step.id = Some(step_id.clone());

    let mut packed_graph = match &mut step.run {
        StringOrDocument::String(filename) => {
            let path = Path::new(filename);
            let path = if path.is_absolute() {
                path
            } else {
                &wf_dir.as_ref().join(path)
            };
            let filename = if let Some(filename) = path.file_name() {
                filename.to_string_lossy().into_owned()
            } else {
                format!("{step_id}.cwl")
            };
            let step_hash = format!("#{filename}");
            let cwl = load_cwl_file(path, true)?;
            let graph = pack_cwl(&cwl, path, Some(&step_hash))?;

            step.run = StringOrDocument::String(step_hash);
            graph
        }
        StringOrDocument::Document(doc) => {
            let step_hash = format!("#{step_id}.cwl");
            let graph = pack_cwl(
                doc,
                wf_dir.as_ref().join(step.id.as_ref().unwrap()),
                Some(&step_hash),
            )?;

            step.run = StringOrDocument::String(step_hash);
            graph
        }
    };

    //clear versions from child docs
    for item in &mut packed_graph {
        match item {
            CWLDocument::CommandLineTool(clt) => clt.cwl_version = None,
            CWLDocument::ExpressionTool(et) => et.cwl_version = None,
            CWLDocument::Operation(op) => op.cwl_version = None,
            CWLDocument::Workflow(wf) => wf.cwl_version = None,
        }
    }

    for input in &mut step.r#in {
        input.id = Some(format!("{step_id}/{}", input.id.as_ref().unwrap()));
        if let Some(src) = &mut input.source {
            match src {
                OneOrMany::One(item) => *item = format!("{wf_id}/{item}"),
                OneOrMany::Many(items) => {
                    for item in items {
                        *item = format!("{wf_id}/{item}");
                    }
                }
            }
        }
    }

    for output in &mut step.out {
        match output {
            StringOrWorkflowStepOutput::String(o_string) => {
                *o_string = format!("{step_id}/{o_string}");
            }
            StringOrWorkflowStepOutput::WorkflowStepOutput(wf_out) => {
                wf_out.set_id(&format!("{step_id}/{}", wf_out.get_id().unwrap()));
            }
        }
    }

    Ok(packed_graph.clone())
}

fn pack_command_input(
    input: &mut CommandInputParameter,
    root_id: &str,
    doc_dir: impl AsRef<Path>,
) -> Result<()> {
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
                            dunce::canonicalize(&p)
                                .unwrap_or(p)
                                .to_string_lossy()
                                .into_owned()
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
                            dunce::canonicalize(&p)
                                .unwrap_or(p)
                                .to_string_lossy()
                                .into_owned()
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
) -> Result<()> {
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
                            dunce::canonicalize(&p)
                                .unwrap_or(p)
                                .to_string_lossy()
                                .into_owned()
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
                            dunce::canonicalize(&p)
                                .unwrap_or(p)
                                .to_string_lossy()
                                .into_owned()
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

fn pack_iwdr(iwdr: &mut InitialWorkDirRequirement, doc_dir: impl AsRef<Path>) -> Result<()> {
    match &mut iwdr.listing {
        WorkDirItems::Expression(_) => {}
        WorkDirItems::ListingItems(items) => {
            for item in items {
                pack_iwdr_item(item, &doc_dir)?;
            }
        }
    }

    Ok(())
}

fn pack_iwdr_item(item: &mut ListingItems, doc_dir: impl AsRef<Path>) -> Result<()> {
    if let ListingItems::Dirent(dirent) = item {
        pack_entry(&mut dirent.entry, &doc_dir)?;
    }

    Ok(())
}

fn pack_docker_requirement(dr: &mut DockerRequirement, doc_dir: impl AsRef<Path>) -> Result<()> {
    if let Some(df) = &mut dr.docker_file {
        pack_entry(df, doc_dir)?;
    }

    Ok(())
}

fn pack_js_requirement(
    ijs: &mut InlineJavascriptRequirement,
    doc_dir: impl AsRef<Path>,
) -> Result<()> {
    if let Some(lib) = &mut ijs.expression_lib {
        for item in lib {
            pack_entry(item, &doc_dir)?;
        }
    }

    Ok(())
}

fn pack_entry(entry: &mut StringOrInclude, doc_dir: impl AsRef<Path>) -> Result<()> {
    if let StringOrInclude::Include(include) = &entry {
        let path = &include.include;
        let contents = fs::read_to_string(doc_dir.as_ref().join(path))?;

        *entry = StringOrInclude::String(contents);
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
        let path = dunce::canonicalize("../../testdata/packed/data/population.csv").unwrap();
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

        let base_dir =
            dunce::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")).unwrap();

        let file_path = base_dir.join("testdata/packed/workflows/calculation");
        pack_command_input(&mut input, "#calculation.cwl", file_path).unwrap();

        let json = serde_json::json!(&input);
        let mut base_dir_str = base_dir.to_string_lossy().replace(MAIN_SEPARATOR_STR, "/");
        if !base_dir_str.starts_with('/') {
            base_dir_str = format!("/{}", base_dir_str);
        }
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
        .replace("XXX", &base_dir_str);

        let value: serde_json::Value = serde_json::from_str(&reference_json).unwrap();
        assert_eq!(json, value);
    }

    #[test]
    fn test_pack_commandlinetool() {
        let path = Path::new("../../testdata/packed/workflows/calculation/calculation.cwl");
        let tool = load_cwl_file(path, true).unwrap();
        let packed = &pack_cwl(&tool, path, Some("#main")).unwrap()[0];

        let base_dir =
            dunce::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")).unwrap();

        let mut json = serde_json::json!(packed);
        let mut base_dir_str = base_dir.to_string_lossy().replace(MAIN_SEPARATOR_STR, "/");
        if !base_dir_str.starts_with('/') {
            base_dir_str = format!("/{}", base_dir_str);
        }
        let reference_json = include_str!("../../testdata/packed/calculation_packed.cwl")
            .replace("/mnt/commonwl", &base_dir_str);

        let mut reference: serde_json::Value = serde_json::from_str(&reference_json).unwrap();
        normalize_json_newlines(&mut json);
        normalize_json_newlines(&mut reference);
        assert_eq!(json, reference);
    }

    #[test]
    fn test_pack_workflow() {
        let path = "../../testdata/packed/workflows/main/main.cwl";
        let CWLDocument::Workflow(wf) = load_cwl_file(path, true).unwrap() else {
            panic!()
        };

        let packed = pack_workflow(&wf, path, None).unwrap();
        let mut json = serde_json::json!(&packed);

        let base_dir =
            dunce::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")).unwrap();
        let mut base_dir_str = base_dir.to_string_lossy().replace(MAIN_SEPARATOR_STR, "/");
        if !base_dir_str.starts_with('/') {
            base_dir_str = format!("/{}", base_dir_str);
        }
        let reference_json = include_str!("../../testdata/packed/main_packed.cwl")
            .replace("/mnt/commonwl", &base_dir_str);

        let mut reference: serde_json::Value = serde_json::from_str(&reference_json).unwrap();
        normalize_json_newlines(&mut json);
        normalize_json_newlines(&mut reference);
        assert_eq!(json, reference);
    }
}
