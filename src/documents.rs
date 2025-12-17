use crate::OneOrMany;
use crate::inputs::{
    CommandInputParameter, CommandLineBinding, OperationInputParameter, WorkflowInputParameter,
};
use crate::outputs::{ExpressionToolOutputParameter, OperationOutputParameter};
use crate::requirements::{ToolHints, ToolRequirements, WorkflowHints, WorkflowRequirements};
use crate::{
    deserialize::deserialize_map_list_id, deserialize::deserialize_map_list_option_class,
    outputs::CommandOutputParameter,
};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "class")]
pub enum CWLDocument {
    CommandLineTool(CommandLineTool),
    ExpressionTool(ExpressionTool),
    Operation(Operation),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(untagged)]
pub enum Argument {
    String(String),
    Binding(CommandLineBinding),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CommandLineTool {
    #[serde(deserialize_with = "deserialize_map_list_id")]
    pub inputs: Vec<CommandInputParameter>,
    #[serde(deserialize_with = "deserialize_map_list_id")]
    pub outputs: Vec<CommandOutputParameter>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub doc: Option<OneOrMany<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    #[serde(deserialize_with = "deserialize_map_list_option_class")]
    pub requirements: Option<Vec<ToolRequirements>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    #[serde(deserialize_with = "deserialize_map_list_option_class")]
    pub hints: Option<Vec<ToolHints>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwl_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub intent: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_command: Option<OneOrMany<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arguments: Option<Vec<Argument>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stdin: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stderr: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stdout: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub success_codes: Option<Vec<i32>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temporary_fail_codes: Option<Vec<i32>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub permanent_fail_codes: Option<Vec<i32>>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ExpressionTool {
    #[serde(deserialize_with = "deserialize_map_list_id")]
    pub inputs: Vec<WorkflowInputParameter>,
    #[serde(deserialize_with = "deserialize_map_list_id")]
    pub outputs: Vec<ExpressionToolOutputParameter>,
    pub expression: String,
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
    pub cwl_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub intent: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Operation {
    #[serde(deserialize_with = "deserialize_map_list_id")]
    pub inputs: Vec<OperationInputParameter>,
    #[serde(deserialize_with = "deserialize_map_list_id")]
    pub outputs: Vec<OperationOutputParameter>,
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
    pub cwl_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub intent: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{env, fs, path::Path};

    #[test]
    fn test_command_line_tools() {
        let cwl_path = Path::new(&env::var("CARGO_MANIFEST_DIR").unwrap_or(".".to_string()))
            .join("testdata")
            .join("commandlinetools");
        let mut count = 0;
        for entry in cwl_path.read_dir().unwrap() {
            let entry = entry.unwrap();
            if entry.file_type().unwrap().is_file() && entry.path().extension().unwrap() == "cwl" {
                let contents = fs::read_to_string(entry.path()).unwrap();
                let result_doc = serde_yaml::from_str::<CWLDocument>(&contents);
                dbg!(&result_doc);
                assert!(result_doc.is_ok());
                assert!(matches!(result_doc.unwrap(), CWLDocument::CommandLineTool(_)));
                count += 1;
            }
        }
        assert_eq!(count, 7)
    }

    #[test]
    fn test_expression_tools() {
        let cwl_path = Path::new(&env::var("CARGO_MANIFEST_DIR").unwrap_or(".".to_string()))
            .join("testdata")
            .join("expressiontools");
        let mut count = 0;
        for entry in cwl_path.read_dir().unwrap() {
            let entry = entry.unwrap();
            if entry.file_type().unwrap().is_file() && entry.path().extension().unwrap() == "cwl" {
                let contents = fs::read_to_string(entry.path()).unwrap();
                let result_doc = serde_yaml::from_str::<CWLDocument>(&contents);
                dbg!(&result_doc);
                assert!(result_doc.is_ok());
                assert!(matches!(result_doc.unwrap(), CWLDocument::ExpressionTool(_)));
                count += 1;
            }
        }
        assert_eq!(count, 3)
    }
}
