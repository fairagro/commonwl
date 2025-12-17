use crate::IntegerOrExpression;
use crate::deserialize::{FromShortHand, deserialize_map_list_option_name, make_shorthand_impl};
use crate::{BoolOrExpression, OneOrMany, files::LoadListingEnum};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Copy, PartialEq, Hash, Clone)]
#[serde(rename_all = "snake_case")]
pub enum CWLType {
    Null,
    Boolean,
    Int,
    Long,
    Float,
    Double,
    String,
    #[serde(rename = "File")]
    File,
    #[serde(rename = "Directory")]
    Directory,
}
#[derive(Serialize, Deserialize, Debug, PartialEq, Hash, Clone)]
#[serde(tag = "type")]
#[serde(rename_all = "snake_case")]
pub enum CommandInputSchema {
    Record(CommandInputRecordSchema),
    Enum(CommandInputEnumSchema),
    Array(CommandInputArraySchema),
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Hash, Clone)]
#[serde(untagged)]
pub enum CommandInputType {
    CWLType(CWLType),
    CommandInputSchema(Box<CommandInputSchema>),
    String(String),
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Hash, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CommandInputRecordSchema {
    #[serde(deserialize_with = "deserialize_map_list_option_name")]
    #[serde(skip_serializing_if = "Option::is_none")]
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
pub struct CommandInputRecordField {
    pub name: String,
    pub r#type: OneOrMany<CommandInputType>,
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
    pub load_contents: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub load_listing: Option<LoadListingEnum>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_binding: Option<CommandLineBinding>,
}

make_shorthand_impl!(CommandInputRecordField, "name", "type");

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
pub struct SecondaryFileSchema {
    pub pattern: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required: Option<BoolOrExpression>,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Hash, Clone)]
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

#[cfg(test)]
mod tests {
    use super::*;

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

        let contents = include_str!("../testdata/command_schemas.yaml");
        let res = serde_yaml::from_str::<Bag>(contents);
        dbg!(&res);
        assert!(res.is_ok());
    }
}
