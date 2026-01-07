use crate::IntegerOrExpression;
use crate::deserialize::{
    FromShortHand, deserialize_map_list_option_name, deserialize_with_secondary_files_dsl,
    deserialize_with_type_dsl, make_shorthand_impl,
};
use crate::outputs::{LinkMergeMethod, PickValueMethod};
use crate::types::{CWLType, SecondaryFileSchema};
use crate::{
    OneOrMany,
    files::{FileOrDirectory, LoadListingEnum},
};
use bon::Builder;
use serde::{Deserialize, Serialize};
use serde_yaml::Value;
use std::fmt::{self, Display};

#[derive(Serialize, Deserialize, Debug, PartialEq, Hash, Clone)]
#[serde(untagged)]
pub enum CommandInputParameterType {
    #[serde(rename = "stdin")]
    Stdin,
    CommandInputType(OneOrMany<CommandInputType>),
}

impl Default for CommandInputParameterType {
    fn default() -> Self {
        CommandInputParameterType::CommandInputType(OneOrMany::One(CommandInputType::default()))
    }
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Hash, Clone)]
#[serde(untagged)]
pub enum DefaultValue {
    FileOrDirectory(FileOrDirectory),
    Any(serde_yaml::Value),
}

impl Display for DefaultValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DefaultValue::FileOrDirectory(fd) => match fd.path() {
                Some(path) => write!(f, "{path}"),
                None => Err(fmt::Error),
            },
            DefaultValue::Any(value) => match value {
                Value::String(s) => write!(f, "{s}"),
                Value::Number(n) => write!(f, "{n}"),
                Value::Bool(b) => write!(f, "{b}"),
                _ => Err(fmt::Error),
            },
        }
    }
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Hash, Clone, Default, Builder)]
#[serde(rename_all = "camelCase")]
pub struct CommandInputParameter {
    #[serde(deserialize_with = "deserialize_with_type_dsl")]
    #[builder(into)]
    pub r#type: CommandInputParameterType,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    #[serde(default)]
    #[serde(deserialize_with = "deserialize_with_secondary_files_dsl")]
    pub secondary_files: Option<OneOrMany<SecondaryFileSchema>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub streamable: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub doc: Option<OneOrMany<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub format: Option<OneOrMany<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub load_contents: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub load_listing: Option<LoadListingEnum>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub default: Option<DefaultValue>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub input_binding: Option<CommandLineBinding>,
}

make_shorthand_impl!(CommandInputParameter, "id", "type");

#[derive(Serialize, Deserialize, Debug, PartialEq, Hash, Clone, Default, Builder)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowInputParameter {
    #[serde(deserialize_with = "deserialize_with_type_dsl")]
    #[builder(into)]
    pub r#type: InputType,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    #[serde(default)]
    #[serde(deserialize_with = "deserialize_with_secondary_files_dsl")]
    pub secondary_files: Option<OneOrMany<SecondaryFileSchema>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub streamable: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub doc: Option<OneOrMany<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub format: Option<OneOrMany<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub load_contents: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub load_listing: Option<LoadListingEnum>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub default: Option<DefaultValue>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    #[deprecated(since = "1.2.0", note = "Will be removed in CWL 2.0")]
    pub input_binding: Option<CommandLineBinding>,
}

make_shorthand_impl!(WorkflowInputParameter, "id", "type");

#[derive(Serialize, Deserialize, Debug, PartialEq, Hash, Clone, Default, Builder)]
#[serde(rename_all = "camelCase")]
pub struct OperationInputParameter {
    #[serde(deserialize_with = "deserialize_with_type_dsl")]
    #[builder(into)]
    pub r#type: InputType,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    #[serde(default)]
    #[serde(deserialize_with = "deserialize_with_secondary_files_dsl")]
    pub secondary_files: Option<OneOrMany<SecondaryFileSchema>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub streamable: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub doc: Option<OneOrMany<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub format: Option<OneOrMany<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub load_contents: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub load_listing: Option<LoadListingEnum>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub default: Option<DefaultValue>,
}

make_shorthand_impl!(OperationInputParameter, "id", "type");

#[derive(Serialize, Deserialize, Debug, PartialEq, Hash, Clone)]
#[serde(tag = "type")]
#[serde(rename_all = "snake_case")]
pub enum CommandInputSchema {
    Record(CommandInputRecordSchema),
    Enum(CommandInputEnumSchema),
    Array(CommandInputArraySchema),
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Hash, Clone)]
#[serde(tag = "type")]
#[serde(rename_all = "snake_case")]
pub enum InputSchema {
    Record(InputRecordSchema),
    Enum(InputEnumSchema),
    Array(InputArraySchema),
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Hash, Clone)]
#[serde(untagged)]
pub enum CommandInputType {
    CWLType(CWLType),
    CommandInputSchema(Box<CommandInputSchema>),
    String(String),
}

impl Default for CommandInputType {
    fn default() -> Self {
        CommandInputType::CWLType(CWLType::Null)
    }
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Hash, Clone)]
#[serde(untagged)]
pub enum InputType {
    CWLType(CWLType),
    InputSchema(Box<InputSchema>),
    String(String),
}

impl Default for InputType {
    fn default() -> Self {
        InputType::CWLType(CWLType::Null)
    }
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Hash, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CommandInputRecordSchema {
    #[serde(deserialize_with = "deserialize_map_list_option_name")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub fields: Option<Vec<CommandInputRecordField>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub doc: Option<OneOrMany<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_binding: Option<CommandLineBinding>,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Hash, Clone)]
#[serde(rename_all = "camelCase")]
pub struct InputRecordSchema {
    #[serde(deserialize_with = "deserialize_map_list_option_name")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub fields: Option<Vec<InputRecordField>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub doc: Option<OneOrMany<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Hash, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CommandInputRecordField {
    pub name: String,
    #[serde(deserialize_with = "deserialize_with_type_dsl")]
    pub r#type: OneOrMany<CommandInputType>,
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
    pub load_contents: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub load_listing: Option<LoadListingEnum>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_binding: Option<CommandLineBinding>,
}

make_shorthand_impl!(CommandInputRecordField, "name", "type");

#[derive(Serialize, Deserialize, Debug, PartialEq, Hash, Clone)]
#[serde(rename_all = "camelCase")]
pub struct InputRecordField {
    pub name: String,
    #[serde(deserialize_with = "deserialize_with_type_dsl")]
    pub r#type: OneOrMany<InputType>,
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
    pub load_contents: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub load_listing: Option<LoadListingEnum>,
}

make_shorthand_impl!(InputRecordField, "name", "type");

#[derive(Serialize, Deserialize, Debug, PartialEq, Hash, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CommandInputEnumSchema {
    pub symbols: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub doc: Option<OneOrMany<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_binding: Option<CommandLineBinding>,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Hash, Clone)]
#[serde(rename_all = "camelCase")]
pub struct InputEnumSchema {
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
pub struct CommandInputArraySchema {
    pub items: OneOrMany<CommandInputType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub doc: Option<OneOrMany<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_binding: Option<CommandLineBinding>,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Hash, Clone)]
#[serde(rename_all = "camelCase")]
pub struct InputArraySchema {
    pub items: OneOrMany<CommandInputType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub doc: Option<OneOrMany<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Hash, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct CommandLineBinding {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub load_contents: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub position: Option<IntegerOrExpression>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prefix: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub separate: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub item_separator: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value_from: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shell_quote: Option<bool>,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Hash, Clone)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowStepInput {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<OneOrMany<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub link_merge: Option<LinkMergeMethod>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pick_value: Option<PickValueMethod>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub load_contents: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub load_listing: Option<LoadListingEnum>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<DefaultValue>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value_from: Option<String>,
}

make_shorthand_impl!(WorkflowStepInput, "id", "source");

#[cfg(test)]
mod tests {
    use super::*;
    use crate::deserialize::deserialize_map_list_id;

    #[test]
    #[allow(unused)]
    fn test_command_input_params() {
        #[derive(Deserialize, Debug)]
        struct InputHolder {
            #[serde(deserialize_with = "deserialize_map_list_id")]
            inputs: Vec<CommandInputParameter>,
        }

        let contents = include_str!("../../testdata/command_inputs.yaml");
        let res = serde_yaml::from_str::<InputHolder>(contents);
        dbg!(&res);
        assert!(res.is_ok());
        assert_eq!(res.unwrap().inputs.len(), 14);
    }

    #[test]
    #[allow(unused)]
    fn test_command_input_type_dsl() {
        #[derive(Deserialize, Debug)]
        struct InputHolder {
            #[serde(deserialize_with = "deserialize_map_list_id")]
            inputs: Vec<CommandInputParameter>,
        }

        let contents = include_str!("../../testdata/command_input_typedsl.yaml");
        let res = serde_yaml::from_str::<InputHolder>(contents);
        dbg!(&res);
        assert!(res.is_ok());
        assert_eq!(res.unwrap().inputs.len(), 3);
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
            r#type: OneOrMany<CommandInputType>,
        }

        let contents = include_str!("../../testdata/command_schemas.yaml");
        let res = serde_yaml::from_str::<Bag>(contents);
        dbg!(&res);
        assert!(res.is_ok());
    }
}
