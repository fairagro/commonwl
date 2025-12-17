use crate::deserialize::{FromShortHand, deserialize_map_list_option_name, make_shorthand_impl};
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
    pub r#type: CommandOutputParameterType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
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
#[serde(tag = "type")]
#[serde(rename_all = "snake_case")]
pub enum CommandOutputSchema {
    Record(CommandOutputRecordSchema),
    Enum(CommandOutputEnumSchema),
    Array(CommandOutputArraySchema),
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Hash, Clone)]
#[serde(untagged)]
pub enum CommandOutputType {
    CWLType(CWLType),
    CommandOutputSchema(Box<CommandOutputSchema>),
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
pub struct CommandOutputRecordField {
    pub name: String,
    pub r#type: OneOrMany<CommandOutputType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub doc: Option<OneOrMany<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
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
