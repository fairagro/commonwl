use crate::ExtractFromEnum;
use crate::OneOrMany;
use crate::deserialize::FromShortHand;
use crate::inputs::InputDataProvider;
use crate::inputs::{
    CommandInputParameter, CommandLineBinding, OperationInputParameter, WorkflowInputParameter,
    WorkflowStepInput,
};
use crate::outputs::{
    ExpressionToolOutputParameter, OperationOutputParameter, StringOrWorkflowStepOutput,
    WorkflowOutputParameter,
};
use crate::requirements::{ToolHints, ToolRequirements, WorkflowHints, WorkflowRequirements};
use crate::{
    deserialize::deserialize_map_list_id, deserialize::deserialize_map_list_option_class,
    outputs::CommandOutputParameter,
};
use bon::Builder;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "class")]
pub enum CWLDocument {
    CommandLineTool(CommandLineTool),
    ExpressionTool(ExpressionTool),
    Operation(Operation),
    Workflow(Workflow),
}

impl CWLDocument {
    pub fn get_input_data_providers(&self) -> Vec<&dyn InputDataProvider> {
        match self {
            Self::CommandLineTool(clt) => clt
                .inputs
                .iter()
                .map(|i| i as &dyn InputDataProvider)
                .collect(),
            Self::ExpressionTool(et) => et
                .inputs
                .iter()
                .map(|i| i as &dyn InputDataProvider)
                .collect(),
            Self::Operation(o) => o
                .inputs
                .iter()
                .map(|i| i as &dyn InputDataProvider)
                .collect(),
            Self::Workflow(wf) => wf
                .inputs
                .iter()
                .map(|i| i as &dyn InputDataProvider)
                .collect(),
        }
    }
}

impl From<CommandLineTool> for CWLDocument {
    fn from(value: CommandLineTool) -> Self {
        CWLDocument::CommandLineTool(value)
    }
}

impl From<ExpressionTool> for CWLDocument {
    fn from(value: ExpressionTool) -> Self {
        CWLDocument::ExpressionTool(value)
    }
}

impl From<Workflow> for CWLDocument {
    fn from(value: Workflow) -> Self {
        CWLDocument::Workflow(value)
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(untagged)]
pub enum StringOrDocument {
    String(String),
    Document(Box<CWLDocument>),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(untagged)]
pub enum Argument {
    String(String),
    Binding(CommandLineBinding),
}

macro_rules! impl_document_defaults {
    ($class:ident, $req_enum:ident, $hint_enum:ident) => {
        impl $class {
            pub fn get_requirement<T>(&self) -> Option<&T>
            where
                T: ExtractFromEnum<$req_enum>,
            {
                self.requirements
                    .as_ref()
                    .and_then(|reqs| reqs.iter().find_map(|req| T::get(req)))
            }

            pub fn get_requirement_or_hint<T>(&self) -> Option<&T>
            where
                T: ExtractFromEnum<$req_enum>,
            {
                let maybe_req = self
                    .requirements
                    .as_ref()
                    .and_then(|reqs| reqs.iter().find_map(|req| T::get(req)));
                let maybe_hint = self.hints.as_ref().and_then(|hints| {
                    hints.iter().find_map(|hint| {
                        if let $hint_enum::Requirement(inner) = hint {
                            T::get(inner)
                        } else {
                            None
                        }
                    })
                });
                maybe_req.or(maybe_hint)
            }

            pub fn has_requirement<T>(&self) -> bool
            where
                T: ExtractFromEnum<$req_enum>,
            {
                self.get_requirement::<T>().is_some()
            }

            pub fn has_requirement_or_hint<T>(&self) -> bool
            where
                T: ExtractFromEnum<$req_enum>,
            {
                self.get_requirement_or_hint::<T>().is_some()
            }
        }
    };
}

impl_document_defaults!(CommandLineTool, ToolRequirements, ToolHints);
impl_document_defaults!(ExpressionTool, WorkflowRequirements, WorkflowHints);
impl_document_defaults!(Operation, WorkflowRequirements, WorkflowHints);
impl_document_defaults!(Workflow, WorkflowRequirements, WorkflowHints);

#[derive(Serialize, Deserialize, Debug, Clone, Default, Builder)]
#[serde(rename_all = "camelCase")]
pub struct CommandLineTool {
    #[serde(deserialize_with = "deserialize_map_list_id")]
    #[builder(default, into)]
    pub inputs: Vec<CommandInputParameter>,
    #[serde(deserialize_with = "deserialize_map_list_id")]
    #[builder(default, into)]
    pub outputs: Vec<CommandOutputParameter>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub doc: Option<OneOrMany<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    #[serde(default)]
    #[serde(deserialize_with = "deserialize_map_list_option_class")]
    pub requirements: Option<Vec<ToolRequirements>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    #[serde(default)]
    #[serde(deserialize_with = "deserialize_map_list_option_class")]
    pub hints: Option<Vec<ToolHints>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub cwl_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub intent: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub base_command: Option<OneOrMany<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub arguments: Option<Vec<Argument>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub stdin: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub stderr: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub stdout: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub success_codes: Option<Vec<i32>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub temporary_fail_codes: Option<Vec<i32>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub permanent_fail_codes: Option<Vec<i32>>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Builder, Default)]
#[serde(rename_all = "camelCase")]
pub struct ExpressionTool {
    #[serde(deserialize_with = "deserialize_map_list_id")]
    #[builder(default, into)]
    pub inputs: Vec<WorkflowInputParameter>,
    #[serde(deserialize_with = "deserialize_map_list_id")]
    #[builder(default, into)]
    pub outputs: Vec<ExpressionToolOutputParameter>,
    #[builder(into)]
    pub expression: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub doc: Option<OneOrMany<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    #[serde(default)]
    #[serde(deserialize_with = "deserialize_map_list_option_class")]
    pub requirements: Option<Vec<WorkflowRequirements>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    #[serde(default)]
    #[serde(deserialize_with = "deserialize_map_list_option_class")]
    pub hints: Option<Vec<WorkflowHints>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub cwl_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub intent: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Builder, Default)]
#[serde(rename_all = "camelCase")]
pub struct Operation {
    #[serde(deserialize_with = "deserialize_map_list_id")]
    #[builder(default, into)]
    pub inputs: Vec<OperationInputParameter>,
    #[serde(deserialize_with = "deserialize_map_list_id")]
    #[builder(default, into)]
    pub outputs: Vec<OperationOutputParameter>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub doc: Option<OneOrMany<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    #[serde(default)]
    #[serde(deserialize_with = "deserialize_map_list_option_class")]
    pub requirements: Option<Vec<WorkflowRequirements>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    #[serde(default)]
    #[serde(deserialize_with = "deserialize_map_list_option_class")]
    pub hints: Option<Vec<WorkflowHints>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub cwl_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub intent: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Builder, Default)]
#[serde(rename_all = "camelCase")]
pub struct Workflow {
    #[serde(deserialize_with = "deserialize_map_list_id")]
    #[builder(default, into)]
    pub inputs: Vec<WorkflowInputParameter>,
    #[serde(deserialize_with = "deserialize_map_list_id")]
    #[builder(default, into)]
    pub outputs: Vec<WorkflowOutputParameter>,
    #[serde(deserialize_with = "deserialize_map_list_id")]
    #[builder(into)]
    pub steps: Vec<WorkflowStep>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub doc: Option<OneOrMany<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    #[serde(default)]
    #[serde(deserialize_with = "deserialize_map_list_option_class")]
    pub requirements: Option<Vec<WorkflowRequirements>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    #[serde(default)]
    #[serde(deserialize_with = "deserialize_map_list_option_class")]
    pub hints: Option<Vec<WorkflowHints>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub cwl_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub intent: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowStep {
    #[serde(deserialize_with = "deserialize_map_list_id")]
    pub r#in: Vec<WorkflowStepInput>,
    pub out: Vec<StringOrWorkflowStepOutput>,
    pub run: StringOrDocument,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub doc: Option<OneOrMany<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    #[serde(deserialize_with = "deserialize_map_list_option_class")]
    pub requirements: Option<Vec<WorkflowRequirements>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    #[serde(deserialize_with = "deserialize_map_list_option_class")]
    pub hints: Option<Vec<WorkflowHints>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub when: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scatter: Option<OneOrMany<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scatter_method: Option<ScatterMethod>,
}
impl FromShortHand for WorkflowStep {}

#[derive(Serialize, Deserialize, Debug, Copy, PartialEq, Hash, Clone)]
#[serde(rename_all = "snake_case")]
pub enum ScatterMethod {
    Dotproduct,
    NestedCrossproduct,
    FlatCrossproduct,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{env, fs, path::Path};

    #[test]
    fn test_command_line_tools() {
        let cwl_path = Path::new(&env::var("CARGO_MANIFEST_DIR").unwrap_or(".".to_string()))
            .join("..")
            .join("..")
            .join("testdata")
            .join("smoke")
            .join("commandlinetools");
        let mut count = 0;
        for entry in cwl_path.read_dir().unwrap() {
            let entry = entry.unwrap();
            if entry.file_type().unwrap().is_file() && entry.path().extension().unwrap() == "cwl" {
                let contents = fs::read_to_string(entry.path()).unwrap();
                let result_doc = serde_yaml::from_str::<CWLDocument>(&contents);
                dbg!(&result_doc);
                assert!(result_doc.is_ok());
                assert!(matches!(
                    result_doc.unwrap(),
                    CWLDocument::CommandLineTool(_)
                ));
                count += 1;
            }
        }
        assert_eq!(count, 7)
    }

    #[test]
    fn test_expression_tools() {
        let cwl_path = Path::new(&env::var("CARGO_MANIFEST_DIR").unwrap_or(".".to_string()))
            .join("..")
            .join("..")
            .join("testdata")
            .join("smoke")
            .join("expressiontools");
        let mut count = 0;
        for entry in cwl_path.read_dir().unwrap() {
            let entry = entry.unwrap();
            if entry.file_type().unwrap().is_file() && entry.path().extension().unwrap() == "cwl" {
                let contents = fs::read_to_string(entry.path()).unwrap();
                let result_doc = serde_yaml::from_str::<CWLDocument>(&contents);
                dbg!(&result_doc);
                assert!(result_doc.is_ok());
                assert!(matches!(
                    result_doc.unwrap(),
                    CWLDocument::ExpressionTool(_)
                ));
                count += 1;
            }
        }
        assert_eq!(count, 3)
    }

    #[test]
    fn test_workflows() {
        let cwl_path = Path::new(&env::var("CARGO_MANIFEST_DIR").unwrap_or(".".to_string()))
            .join("..")
            .join("..")
            .join("testdata")
            .join("smoke")
            .join("workflows");
        let mut count = 0;
        for entry in cwl_path.read_dir().unwrap() {
            let entry = entry.unwrap();
            if entry.file_type().unwrap().is_file() && entry.path().extension().unwrap() == "cwl" {
                let contents = fs::read_to_string(entry.path()).unwrap();
                let result_doc = serde_yaml::from_str::<CWLDocument>(&contents);
                dbg!(&result_doc);
                assert!(result_doc.is_ok());
                assert!(matches!(result_doc.unwrap(), CWLDocument::Workflow(_)));
                count += 1;
            }
        }
        assert_eq!(count, 3)
    }

    #[test]
    fn test_doc_builder() {
        let tool = CommandLineTool::builder()
            .id("example_tool".to_string())
            .build();
        assert_eq!(tool.id.unwrap(), "example_tool");
    }
}
