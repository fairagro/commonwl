use crate::deserialize::{
    FromShortHand, deserialize_map_list_option_name, deserialize_with_secondary_files_dsl,
    deserialize_with_type_dsl, make_shorthand_impl,
};
use crate::files::File;
use crate::outputs::{LinkMergeMethod, PickValueMethod};
use crate::types::{CWLType, SecondaryFileSchema};
use crate::{IntegerOrExpression, files};
use crate::{
    OneOrMany,
    files::{FileOrDirectory, LoadListingEnum},
};
use bon::Builder;
use salad::Identifiable;
use serde::{Deserialize, Serialize};
use serde_yaml::Value;

#[derive(Serialize, Debug, PartialEq, Hash, Clone)]
#[serde(untagged)]
pub enum CommandInputParameterType {
    #[serde(rename = "stdin")]
    Stdin,
    CommandInputType(OneOrMany<CommandInputType>),
}

impl<'de> Deserialize<'de> for CommandInputParameterType {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = serde_yaml::Value::deserialize(deserializer)?;

        if value == Value::String("stdin".to_string()) {
            return Ok(Self::Stdin);
        }

        OneOrMany::<CommandInputType>::deserialize(value)
            .map(Self::CommandInputType)
            .map_err(serde::de::Error::custom)
    }
}

impl From<CWLType> for CommandInputParameterType {
    fn from(value: CWLType) -> Self {
        CommandInputParameterType::CommandInputType(OneOrMany::One(CommandInputType::CWLType(
            value,
        )))
    }
}

impl From<CommandInputSchema> for CommandInputParameterType {
    fn from(value: CommandInputSchema) -> Self {
        CommandInputParameterType::CommandInputType(OneOrMany::One(
            CommandInputType::CommandInputSchema(Box::new(value)),
        ))
    }
}

impl From<CommandInputType> for CommandInputParameterType {
    fn from(value: CommandInputType) -> Self {
        CommandInputParameterType::CommandInputType(OneOrMany::One(value))
    }
}

impl Default for CommandInputParameterType {
    fn default() -> Self {
        CommandInputParameterType::CommandInputType(OneOrMany::One(CommandInputType::default()))
    }
}

impl CommandInputParameterType {
    /// specifies if for this input parameter a singular null value is allowed
    #[must_use]
    pub fn is_null_allowed(&self) -> bool {
        matches!(
            self,
            CommandInputParameterType::CommandInputType(OneOrMany::One(CommandInputType::CWLType(
                CWLType::Null
            )))
        ) || matches!(
            self,
            CommandInputParameterType::CommandInputType(
                OneOrMany::Many(v)
            ) if v.iter().any(|t| matches!(
                t,
                CommandInputType::CWLType(CWLType::Null)
            ))
        ) || matches!(
            self,
            CommandInputParameterType::CommandInputType(
                OneOrMany::One(CommandInputType::CommandInputSchema(schema))
            ) if schema.is_null_allowed()
        )
    }
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Hash, Clone)]
#[serde(untagged)]
pub enum DefaultValue {
    FileOrDirectory(FileOrDirectory),
    Any(serde_yaml::Value),
}

impl From<files::File> for DefaultValue {
    fn from(value: files::File) -> Self {
        DefaultValue::FileOrDirectory(FileOrDirectory::File(value))
    }
}

impl From<files::Directory> for DefaultValue {
    fn from(value: files::Directory) -> Self {
        DefaultValue::FileOrDirectory(FileOrDirectory::Directory(value))
    }
}

impl From<&str> for DefaultValue {
    fn from(value: &str) -> Self {
        DefaultValue::Any(serde_yaml::Value::String(value.to_string()))
    }
}

impl DefaultValue {
    #[must_use]
    pub fn is_null(&self) -> bool {
        match self {
            Self::Any(value) => value.is_null(),
            Self::FileOrDirectory(_) => false,
        }
    }

    #[must_use]
    pub fn try_get_value_ref(&self) -> Option<&Value> {
        match self {
            Self::FileOrDirectory(_) => None,
            Self::Any(value) => Some(value),
        }
    }

    #[must_use]
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::Any(serde_yaml::Value::String(s)) => Some(s),
            _ => None,
        }
    }

    #[must_use]
    pub fn as_file(&self) -> Option<&File> {
        match self {
            Self::FileOrDirectory(FileOrDirectory::File(f)) => Some(f),
            _ => None,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Hash, Clone, Default, Builder, Identifiable)]
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

#[derive(Serialize, Deserialize, Debug, PartialEq, Hash, Clone, Default, Builder, Identifiable)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowInputParameter {
    #[serde(deserialize_with = "deserialize_with_type_dsl")]
    #[builder(into)]
    pub r#type: OneOrMany<InputType>,
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

impl Default for OneOrMany<InputType> {
    fn default() -> Self {
        OneOrMany::One(InputType::CWLType(CWLType::Null))
    }
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Hash, Clone, Default, Builder, Identifiable)]
#[serde(rename_all = "camelCase")]
pub struct OperationInputParameter {
    #[serde(deserialize_with = "deserialize_with_type_dsl")]
    #[builder(into)]
    pub r#type: OneOrMany<InputType>,
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

impl From<WorkflowInputParameter> for OperationInputParameter {
    fn from(value: WorkflowInputParameter) -> Self {
        Self {
            r#type: value.r#type,
            label: value.label,
            secondary_files: value.secondary_files,
            streamable: value.streamable,
            doc: value.doc,
            id: value.id,
            format: value.format,
            load_contents: value.load_contents,
            load_listing: value.load_listing,
            default: value.default,
        }
    }
}

impl From<CommandInputParameter> for OperationInputParameter {
    fn from(value: CommandInputParameter) -> Self {
        let ty = match value.r#type {
            CommandInputParameterType::Stdin => {
                OneOrMany::One(InputType::String("stdin".to_string()))
            }
            CommandInputParameterType::CommandInputType(types) => types.map(Into::into),
        };
        Self {
            r#type: ty,
            label: value.label,
            secondary_files: value.secondary_files,
            streamable: value.streamable,
            doc: value.doc,
            id: value.id,
            format: value.format,
            load_contents: value.load_contents,
            load_listing: value.load_listing,
            default: value.default,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Hash, Clone)]
#[serde(tag = "type")]
#[serde(rename_all = "snake_case")]
pub enum CommandInputSchema {
    Record(CommandInputRecordSchema),
    Enum(CommandInputEnumSchema),
    Array(CommandInputArraySchema),
}

impl CommandInputSchema {
    pub fn is_null_allowed(&self) -> bool {
        match self {
            Self::Array(array) => match &array.items {
                OneOrMany::Many(items) => items.iter().any(CommandInputType::is_null_allowed),
                OneOrMany::One(item) => item.is_null_allowed(),
            },
            _ => false,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Hash, Clone)]
#[serde(tag = "type")]
#[serde(rename_all = "snake_case")]
pub enum InputSchema {
    Record(InputRecordSchema),
    Enum(InputEnumSchema),
    Array(InputArraySchema),
}

impl From<CommandInputSchema> for InputSchema {
    fn from(value: CommandInputSchema) -> Self {
        match value {
            CommandInputSchema::Record(rec) => InputSchema::Record(rec.into()),
            CommandInputSchema::Enum(enu) => InputSchema::Enum(enu.into()),
            CommandInputSchema::Array(arr) => InputSchema::Array(arr.into()),
        }
    }
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

impl CommandInputType {
    #[must_use]
    pub fn is_null(&self) -> bool {
        matches!(self, CommandInputType::CWLType(CWLType::Null))
    }

    #[must_use]
    pub fn is_null_allowed(&self) -> bool {
        if self.is_null() {
            return true;
        }

        match self {
            Self::CommandInputSchema(schema) => schema.is_null_allowed(),
            _ => false,
        }
    }
}

impl From<CWLType> for CommandInputType {
    fn from(value: CWLType) -> Self {
        CommandInputType::CWLType(value)
    }
}

impl From<CommandInputSchema> for CommandInputType {
    fn from(value: CommandInputSchema) -> Self {
        CommandInputType::CommandInputSchema(Box::new(value))
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

impl From<CommandInputType> for InputType {
    fn from(value: CommandInputType) -> Self {
        match value {
            CommandInputType::CWLType(t) => InputType::CWLType(t),
            CommandInputType::CommandInputSchema(schema) => {
                InputType::InputSchema(Box::new((*schema).into()))
            }
            CommandInputType::String(s) => InputType::String(s),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Hash, Clone, Builder)]
#[serde(rename_all = "camelCase")]
pub struct CommandInputRecordSchema {
    #[serde(deserialize_with = "deserialize_map_list_option_name")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    #[builder(into)]
    pub fields: Option<Vec<CommandInputRecordField>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub doc: Option<OneOrMany<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub input_binding: Option<CommandLineBinding>,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Hash, Clone, Builder)]
#[serde(rename_all = "camelCase")]
pub struct InputRecordSchema {
    #[serde(deserialize_with = "deserialize_map_list_option_name")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    #[builder(into)]
    pub fields: Option<Vec<InputRecordField>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub doc: Option<OneOrMany<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub name: Option<String>,
}

impl From<CommandInputRecordSchema> for InputRecordSchema {
    fn from(value: CommandInputRecordSchema) -> Self {
        Self {
            fields: value
                .fields
                .map(|fields| fields.into_iter().map(Into::into).collect()),
            label: value.label,
            doc: value.doc,
            name: value.name,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Hash, Clone, Builder)]
#[serde(rename_all = "camelCase")]
pub struct CommandInputRecordField {
    #[builder(into)]
    pub name: String,
    #[serde(deserialize_with = "deserialize_with_type_dsl")]
    #[builder(into)]
    pub r#type: OneOrMany<CommandInputType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub doc: Option<OneOrMany<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    #[serde(deserialize_with = "deserialize_with_secondary_files_dsl")]
    #[builder(into)]
    pub secondary_files: Option<OneOrMany<SecondaryFileSchema>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub streamable: Option<bool>,
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
    pub input_binding: Option<CommandLineBinding>,
}

make_shorthand_impl!(CommandInputRecordField, "name", "type");

#[derive(Serialize, Deserialize, Debug, PartialEq, Hash, Clone, Builder)]
#[serde(rename_all = "camelCase")]
pub struct InputRecordField {
    #[builder(into)]
    pub name: String,
    #[serde(deserialize_with = "deserialize_with_type_dsl")]
    #[builder(into)]
    pub r#type: OneOrMany<InputType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub doc: Option<OneOrMany<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    #[serde(deserialize_with = "deserialize_with_secondary_files_dsl")]
    #[builder(into)]
    pub secondary_files: Option<OneOrMany<SecondaryFileSchema>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub streamable: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub format: Option<OneOrMany<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub load_contents: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub load_listing: Option<LoadListingEnum>,
}

make_shorthand_impl!(InputRecordField, "name", "type");

impl From<CommandInputRecordField> for InputRecordField {
    fn from(value: CommandInputRecordField) -> Self {
        Self {
            name: value.name,
            r#type: value.r#type.map(Into::into),
            doc: value.doc,
            label: value.label,
            secondary_files: value.secondary_files,
            streamable: value.streamable,
            format: value.format,
            load_contents: value.load_contents,
            load_listing: value.load_listing,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Hash, Clone, Builder)]
#[serde(rename_all = "camelCase")]
pub struct CommandInputEnumSchema {
    #[builder(into)]
    pub symbols: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub doc: Option<OneOrMany<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub input_binding: Option<CommandLineBinding>,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Hash, Clone, Builder)]
#[serde(rename_all = "camelCase")]
pub struct InputEnumSchema {
    #[builder(into)]
    pub symbols: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub doc: Option<OneOrMany<String>>,
}

impl From<CommandInputEnumSchema> for InputEnumSchema {
    fn from(value: CommandInputEnumSchema) -> Self {
        Self {
            symbols: value.symbols,
            name: value.name,
            label: value.label,
            doc: value.doc,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Hash, Clone, Builder)]
#[serde(rename_all = "camelCase")]
pub struct CommandInputArraySchema {
    #[builder(into)]
    pub items: OneOrMany<CommandInputType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub doc: Option<OneOrMany<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub input_binding: Option<CommandLineBinding>,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Hash, Clone, Builder)]
#[serde(rename_all = "camelCase")]
pub struct InputArraySchema {
    #[builder(into)]
    pub items: OneOrMany<InputType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub doc: Option<OneOrMany<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub name: Option<String>,
}

impl From<CommandInputArraySchema> for InputArraySchema {
    fn from(value: CommandInputArraySchema) -> Self {
        Self {
            items: value.items.map(Into::into),
            name: value.name,
            label: value.label,
            doc: value.doc,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Hash, Clone, Default, Builder)]
#[serde(rename_all = "camelCase")]
pub struct CommandLineBinding {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub load_contents: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub position: Option<IntegerOrExpression>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub prefix: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub separate: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub item_separator: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub value_from: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub shell_quote: Option<bool>,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Hash, Clone, Identifiable)]
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
