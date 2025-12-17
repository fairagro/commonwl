use crate::{
    OneOrMany,
    deserialize::FromShortHand,
    deserialize::make_shorthand_impl,
    files::{FileOrDirectory, LoadListingEnum},
    types::{CommandInputType, CommandLineBinding, SecondaryFileSchema},
};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, PartialEq, Hash, Clone)]
#[serde(untagged)]
pub enum CommandInputParameterType {
    #[serde(rename = "stdin")]
    Stdin,
    CommandInputType(OneOrMany<CommandInputType>),
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Hash, Clone)]
#[serde(untagged)]
pub enum DefautltValue {
    FileOrDirectory(FileOrDirectory),
    Any(serde_yaml::Value),
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Hash, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CommandInputParameter {
    pub r#type: CommandInputParameterType,
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
    pub load_contents: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub load_listing: Option<LoadListingEnum>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<DefautltValue>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_binding: Option<CommandLineBinding>,
}

make_shorthand_impl!(CommandInputParameter, "id", "type");

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

        let contents = include_str!("../testdata/command_inputs.yaml");
        let res = serde_yaml::from_str::<InputHolder>(contents);
        dbg!(&res);
        assert!(res.is_ok());
        assert_eq!(res.unwrap().inputs.len(), 14);
    }
}
