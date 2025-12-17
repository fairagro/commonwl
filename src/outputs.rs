use crate::deserialize::{
    FromShortHand, deserialize_map_list_option_name, deserialize_with_secondary_files_dsl,
    deserialize_with_type_dsl, make_shorthand_impl,
};
use crate::types::{CWLType, SecondaryFileSchema};
use crate::{
    OneOrMany,
    files::{FileOrDirectory, LoadListingEnum},
};

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, PartialEq, Hash, Clone)]
#[serde(untagged)]
pub enum CommandOutputParameterType {
    #[serde(rename = "stdout")]
    Stdout,
    #[serde(rename = "stderr")]
    Stderr,
    CommandInputType(OneOrMany<CommandOutputType>),
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Hash, Clone)]
#[serde(untagged)]
pub enum DefautltValue {
    FileOrDirectory(FileOrDirectory),
    Any(serde_yaml::Value),
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Hash, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CommandOutputParameter {
    #[serde(deserialize_with = "deserialize_with_type_dsl")]
    pub r#type: CommandOutputParameterType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    #[serde(deserialize_with = "deserialize_with_secondary_files_dsl")]
    pub secondary_files: Option<OneOrMany<SecondaryFileSchema>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub streamable: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub doc: Option<OneOrMany<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<OneOrMany<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_binding: Option<CommandOutputBinding>,
}

make_shorthand_impl!(CommandOutputParameter, "id", "type");

#[derive(Serialize, Deserialize, Debug, PartialEq, Hash, Clone)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowOutputParameter {
    #[serde(deserialize_with = "deserialize_with_type_dsl")]
    pub r#type: CommandOutputParameterType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    #[serde(deserialize_with = "deserialize_with_secondary_files_dsl")]
    pub secondary_files: Option<OneOrMany<SecondaryFileSchema>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub streamable: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub doc: Option<OneOrMany<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<OneOrMany<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_source: Option<OneOrMany<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub link_merge: Option<LinkMergeMethod>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pick_value: Option<PickValueMethod>,
}

make_shorthand_impl!(WorkflowOutputParameter, "id", "type");

#[derive(Serialize, Deserialize, Debug, PartialEq, Hash, Clone)]
#[serde(rename_all = "camelCase")]
pub struct OperationOutputParameter {
    #[serde(deserialize_with = "deserialize_with_type_dsl")]
    pub r#type: OneOrMany<OutputType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    #[serde(deserialize_with = "deserialize_with_secondary_files_dsl")]
    pub secondary_files: Option<OneOrMany<SecondaryFileSchema>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub streamable: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub doc: Option<OneOrMany<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<OneOrMany<String>>,
}

make_shorthand_impl!(OperationOutputParameter, "id", "type");

#[derive(Serialize, Deserialize, Debug, PartialEq, Hash, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ExpressionToolOutputParameter {
    #[serde(deserialize_with = "deserialize_with_type_dsl")]
    pub r#type: OneOrMany<OutputType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    #[serde(deserialize_with = "deserialize_with_secondary_files_dsl")]
    pub secondary_files: Option<OneOrMany<SecondaryFileSchema>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub streamable: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub doc: Option<OneOrMany<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<OneOrMany<String>>,
}

make_shorthand_impl!(ExpressionToolOutputParameter, "id", "type");

#[derive(Serialize, Deserialize, Debug, Copy, PartialEq, Hash, Clone)]
#[serde(rename_all = "snake_case")]
pub enum LinkMergeMethod {
    MergeNested,
    MergeFlattened,
}

#[derive(Serialize, Deserialize, Debug, Copy, PartialEq, Hash, Clone)]
#[serde(rename_all = "snake_case")]
pub enum PickValueMethod {
    FirstNonNull,
    TheOnlyNonNull,
    AllNonNull,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Hash, Clone)]
#[serde(tag = "type")]
#[serde(rename_all = "snake_case")]
pub enum CommandOutputSchema {
    Record(CommandOutputRecordSchema),
    Enum(CommandOutputEnumSchema),
    Array(CommandOutputArraySchema),
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Hash, Clone)]
#[serde(tag = "type")]
#[serde(rename_all = "snake_case")]
pub enum OutputSchema {
    Record(OutputRecordSchema),
    Enum(OutputEnumSchema),
    Array(OutputArraySchema),
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Hash, Clone)]
#[serde(untagged)]
pub enum CommandOutputType {
    CWLType(CWLType),
    CommandOutputSchema(Box<CommandOutputSchema>),
    String(String),
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Hash, Clone)]
#[serde(untagged)]
pub enum OutputType {
    CWLType(CWLType),
    OutputSchema(Box<OutputSchema>),
    String(String),
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Hash, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CommandOutputRecordSchema {
    #[serde(deserialize_with = "deserialize_map_list_option_name")]
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fields: Option<Vec<CommandOutputRecordField>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub doc: Option<OneOrMany<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Hash, Clone)]
#[serde(rename_all = "camelCase")]
pub struct OutputRecordSchema {
    #[serde(deserialize_with = "deserialize_map_list_option_name")]
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fields: Option<Vec<OutputRecordField>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub doc: Option<OneOrMany<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Hash, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CommandOutputRecordField {
    pub name: String,
    pub r#type: OneOrMany<CommandOutputType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub doc: Option<OneOrMany<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    #[serde(deserialize_with = "deserialize_with_secondary_files_dsl")]
    pub secondary_files: Option<OneOrMany<SecondaryFileSchema>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub streamable: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<OneOrMany<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_binding: Option<CommandOutputBinding>,
}

make_shorthand_impl!(CommandOutputRecordField, "name", "type");

#[derive(Serialize, Deserialize, Debug, PartialEq, Hash, Clone)]
#[serde(rename_all = "camelCase")]
pub struct OutputRecordField {
    pub name: String,
    pub r#type: OneOrMany<OutputType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub doc: Option<OneOrMany<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    #[serde(deserialize_with = "deserialize_with_secondary_files_dsl")]
    pub secondary_files: Option<OneOrMany<SecondaryFileSchema>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub streamable: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<OneOrMany<String>>,
}

make_shorthand_impl!(OutputRecordField, "name", "type");

#[derive(Serialize, Deserialize, Debug, PartialEq, Hash, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CommandOutputEnumSchema {
    pub symbols: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub doc: Option<OneOrMany<String>>,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Hash, Clone)]
#[serde(rename_all = "camelCase")]
pub struct OutputEnumSchema {
    pub symbols: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub doc: Option<OneOrMany<String>>,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Hash, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CommandOutputArraySchema {
    pub items: OneOrMany<CommandOutputType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub doc: Option<OneOrMany<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Hash, Clone)]
#[serde(rename_all = "camelCase")]
pub struct OutputArraySchema {
    pub items: OneOrMany<OutputType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub doc: Option<OneOrMany<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Hash, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CommandOutputBinding {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub load_contents: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub load_listing: Option<LoadListingEnum>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub glob: Option<OneOrMany<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_eval: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::deserialize::deserialize_map_list_id;

    #[test]
    #[allow(unused)]
    fn test_command_output_params() {
        #[derive(Deserialize, Debug)]
        struct OutputHolder {
            #[serde(deserialize_with = "deserialize_map_list_id")]
            outputs: Vec<CommandOutputParameter>,
        }

        let contents = include_str!("../testdata/command_outputs.yaml");
        let res = serde_yaml::from_str::<OutputHolder>(contents);
        dbg!(&res);
        assert!(res.is_ok());
        assert_eq!(res.unwrap().outputs.len(), 11);
    }

    #[test]
    #[allow(unused)]
    fn test_command_schemas() {
        #[derive(Deserialize, Debug)]
        struct Bag {
            bag: Vec<BagHolder>,
        }
        #[derive(Deserialize, Debug)]
        struct BagHolder {
            id: String,
            r#type: OneOrMany<CommandOutputType>,
        }

        let contents = include_str!("../testdata/command_out_schemas.yaml");
        let res = serde_yaml::from_str::<Bag>(contents);
        dbg!(&res);
        assert!(res.is_ok());
    }
}
