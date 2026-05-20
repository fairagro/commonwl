use crate::Result;
use crate::files::File;
use crate::outputs::{LinkMergeMethod, PickValueMethod};
use crate::types::{CWLType, SecondaryFileSchema};
use crate::{IntegerOrExpression, files};
use crate::{
    OneOrMany,
    files::{FileOrDirectory, LoadListingEnum},
};
use bon::Builder;
use cwl_salad::Identifiable;
use cwl_salad::deserialize::{
    FromShortHand, deserialize_map_list_option_name, deserialize_with_secondary_files_dsl,
    deserialize_with_type_dsl,
};
use cwl_salad::make_shorthand_impl;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use validator::Validate;

#[derive(Serialize, Debug, PartialEq, Hash, Clone, Eq)]
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
        let value = serde_json::Value::deserialize(deserializer)?;

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
#[allow(clippy::large_enum_variant)]
#[derive(Serialize, Deserialize, Debug, PartialEq, Hash, Clone, Eq)]
#[serde(untagged)]
pub enum DefaultValue {
    FileOrDirectory(FileOrDirectory),
    Any(serde_json::Value),
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
        DefaultValue::Any(serde_json::Value::String(value.to_string()))
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
    pub fn try_get_value_ref(&self) -> Option<&serde_json::Value> {
        match self {
            Self::FileOrDirectory(_) => None,
            Self::Any(value) => Some(value),
        }
    }

    #[must_use]
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::Any(serde_json::Value::String(s)) => Some(s),
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

#[doc = include_str!("../../docs/gen/CommandInputParameter.md")]
#[derive(
    Serialize,
    Deserialize,
    Debug,
    PartialEq,
    Hash,
    Clone,
    Default,
    Builder,
    Identifiable,
    Eq,
    Validate,
)]
#[serde(rename_all = "camelCase")]
/// - Reference: <https://www.commonwl.org/v1.2/CommandLineTool.html#CommandInputParameter>
pub struct CommandInputParameter {
    #[doc = include_str!("../../docs/gen/CommandInputParameter_type.md")]
    #[serde(deserialize_with = "deserialize_with_type_dsl")]
    #[builder(into)]
    pub r#type: CommandInputParameterType,
    #[doc = include_str!("../../docs/gen/CommandInputParameter_label.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub label: Option<String>,
    #[doc = include_str!("../../docs/gen/CommandInputParameter_secondaryFiles.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    #[serde(default)]
    #[serde(deserialize_with = "deserialize_with_secondary_files_dsl")]
    pub secondary_files: Option<OneOrMany<SecondaryFileSchema>>,
    #[doc = include_str!("../../docs/gen/CommandInputParameter_streamable.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub streamable: Option<bool>,
    #[doc = include_str!("../../docs/gen/CommandInputParameter_doc.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub doc: Option<OneOrMany<String>>,
    #[doc = include_str!("../../docs/gen/CommandInputParameter_id.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    #[validate(required)]
    pub id: Option<String>,
    #[doc = include_str!("../../docs/gen/CommandInputParameter_format.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub format: Option<OneOrMany<String>>,
    #[doc = include_str!("../../docs/gen/CommandInputParameter_loadContents.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub load_contents: Option<bool>,
    #[doc = include_str!("../../docs/gen/CommandInputParameter_loadListing.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub load_listing: Option<LoadListingEnum>,
    #[doc = include_str!("../../docs/gen/CommandInputParameter_default.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub default: Option<DefaultValue>,
    #[doc = include_str!("../../docs/gen/CommandInputParameter_inputBinding.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub input_binding: Option<CommandLineBinding>,
}

make_shorthand_impl!(CommandInputParameter, "id", "type");

#[derive(
    Serialize,
    Deserialize,
    Debug,
    PartialEq,
    Hash,
    Clone,
    Default,
    Builder,
    Identifiable,
    Eq,
    Validate,
)]
#[serde(rename_all = "camelCase")]
/// - Reference: <https://www.commonwl.org/v1.2/Workflow.html#WorkflowInputParameter>
pub struct WorkflowInputParameter {
    #[doc = include_str!("../../docs/gen/WorkflowInputParameter_type.md")]
    #[serde(deserialize_with = "deserialize_with_type_dsl")]
    #[builder(into)]
    pub r#type: OneOrMany<InputType>,
    #[doc = include_str!("../../docs/gen/WorkflowInputParameter_label.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub label: Option<String>,
    #[doc = include_str!("../../docs/gen/WorkflowInputParameter_secondaryFiles.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    #[serde(default)]
    #[serde(deserialize_with = "deserialize_with_secondary_files_dsl")]
    pub secondary_files: Option<OneOrMany<SecondaryFileSchema>>,
    #[doc = include_str!("../../docs/gen/WorkflowInputParameter_streamable.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub streamable: Option<bool>,
    #[doc = include_str!("../../docs/gen/WorkflowInputParameter_doc.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub doc: Option<OneOrMany<String>>,
    #[doc = include_str!("../../docs/gen/WorkflowInputParameter_id.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    #[validate(required)]
    pub id: Option<String>,
    #[doc = include_str!("../../docs/gen/WorkflowInputParameter_format.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub format: Option<OneOrMany<String>>,
    #[doc = include_str!("../../docs/gen/WorkflowInputParameter_loadContents.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub load_contents: Option<bool>,
    #[doc = include_str!("../../docs/gen/WorkflowInputParameter_loadListing.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub load_listing: Option<LoadListingEnum>,
    #[doc = include_str!("../../docs/gen/WorkflowInputParameter_default.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub default: Option<DefaultValue>,
    #[doc = include_str!("../../docs/gen/WorkflowInputParameter_inputBinding.md")]
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

#[doc = include_str!("../../docs/gen/OperationInputParameter.md")]
#[derive(
    Serialize,
    Deserialize,
    Debug,
    PartialEq,
    Hash,
    Clone,
    Default,
    Builder,
    Identifiable,
    Eq,
    Validate,
)]
#[serde(rename_all = "camelCase")]
/// - Reference: <https://www.commonwl.org/v1.2/Workflow.html#OperationInputParameter>
pub struct OperationInputParameter {
    #[doc = include_str!("../../docs/gen/OperationInputParameter_type.md")]
    #[serde(deserialize_with = "deserialize_with_type_dsl")]
    #[builder(into)]
    pub r#type: OneOrMany<InputType>,
    #[doc = include_str!("../../docs/gen/OperationInputParameter_label.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub label: Option<String>,
    #[doc = include_str!("../../docs/gen/OperationInputParameter_secondaryFiles.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    #[serde(default)]
    #[serde(deserialize_with = "deserialize_with_secondary_files_dsl")]
    pub secondary_files: Option<OneOrMany<SecondaryFileSchema>>,
    #[doc = include_str!("../../docs/gen/OperationInputParameter_streamable.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub streamable: Option<bool>,
    #[doc = include_str!("../../docs/gen/OperationInputParameter_doc.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub doc: Option<OneOrMany<String>>,
    #[doc = include_str!("../../docs/gen/OperationInputParameter_id.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    #[validate(required)]
    pub id: Option<String>,
    #[doc = include_str!("../../docs/gen/OperationInputParameter_format.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub format: Option<OneOrMany<String>>,
    #[doc = include_str!("../../docs/gen/OperationInputParameter_loadContents.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub load_contents: Option<bool>,
    #[doc = include_str!("../../docs/gen/OperationInputParameter_loadListing.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub load_listing: Option<LoadListingEnum>,
    #[doc = include_str!("../../docs/gen/OperationInputParameter_default.md")]
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

#[derive(Serialize, Deserialize, Debug, PartialEq, Hash, Clone, Eq)]
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

#[derive(Serialize, Deserialize, Debug, PartialEq, Hash, Clone, Eq)]
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

#[derive(Serialize, Deserialize, Debug, PartialEq, Hash, Clone, Eq)]
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

#[derive(Serialize, Deserialize, Debug, PartialEq, Hash, Clone, Eq)]
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

#[derive(Serialize, Deserialize, Debug, PartialEq, Hash, Clone, Builder, Eq)]
#[serde(rename_all = "camelCase")]
/// - Reference: <https://www.commonwl.org/v1.2/CommandLineTool.html#CommandInputRecordSchema>
pub struct CommandInputRecordSchema {
    #[doc = include_str!("../../docs/gen/CommandInputRecordSchema_fields.md")]
    #[serde(deserialize_with = "deserialize_map_list_option_name")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    #[builder(into)]
    pub fields: Option<Vec<CommandInputRecordField>>,
    #[doc = include_str!("../../docs/gen/CommandInputRecordSchema_label.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub label: Option<String>,
    #[doc = include_str!("../../docs/gen/CommandInputRecordSchema_doc.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub doc: Option<OneOrMany<String>>,
    #[doc = include_str!("../../docs/gen/CommandInputRecordSchema_name.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub name: Option<String>,
    #[doc = include_str!("../../docs/gen/CommandInputRecordSchema_inputBinding.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub input_binding: Option<CommandLineBinding>,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Hash, Clone, Builder, Eq)]
#[serde(rename_all = "camelCase")]
/// - Reference: <https://www.commonwl.org/v1.2/Workflow.html#InputRecordSchema>
pub struct InputRecordSchema {
    #[doc = include_str!("../../docs/gen/InputRecordSchema_fields.md")]
    #[serde(deserialize_with = "deserialize_map_list_option_name")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    #[builder(into)]
    pub fields: Option<Vec<InputRecordField>>,
    #[doc = include_str!("../../docs/gen/InputRecordSchema_label.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub label: Option<String>,
    #[doc = include_str!("../../docs/gen/InputRecordSchema_doc.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub doc: Option<OneOrMany<String>>,
    #[doc = include_str!("../../docs/gen/InputRecordSchema_name.md")]
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

#[derive(Serialize, Deserialize, Debug, PartialEq, Hash, Clone, Builder, Eq)]
#[serde(rename_all = "camelCase")]
/// - Reference: <https://www.commonwl.org/v1.2/CommandLineTool.html#CommandInputRecordField>
pub struct CommandInputRecordField {
    #[doc = include_str!("../../docs/gen/CommandInputRecordField_name.md")]
    #[builder(into)]
    pub name: String,
    #[doc = include_str!("../../docs/gen/CommandInputRecordField_type.md")]
    #[serde(deserialize_with = "deserialize_with_type_dsl")]
    #[builder(into)]
    pub r#type: OneOrMany<CommandInputType>,
    #[doc = include_str!("../../docs/gen/CommandInputRecordField_doc.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub doc: Option<OneOrMany<String>>,
    #[doc = include_str!("../../docs/gen/CommandInputRecordField_label.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub label: Option<String>,
    #[doc = include_str!("../../docs/gen/CommandInputRecordField_secondaryFiles.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    #[serde(deserialize_with = "deserialize_with_secondary_files_dsl")]
    #[builder(into)]
    pub secondary_files: Option<OneOrMany<SecondaryFileSchema>>,
    #[doc = include_str!("../../docs/gen/CommandInputRecordField_streamable.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub streamable: Option<bool>,
    #[doc = include_str!("../../docs/gen/CommandInputRecordField_format.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub format: Option<OneOrMany<String>>,
    #[doc = include_str!("../../docs/gen/CommandInputRecordField_loadContents.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub load_contents: Option<bool>,
    #[doc = include_str!("../../docs/gen/CommandInputRecordField_loadListing.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub load_listing: Option<LoadListingEnum>,
    #[doc = include_str!("../../docs/gen/CommandInputRecordField_inputBinding.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub input_binding: Option<CommandLineBinding>,
}

make_shorthand_impl!(CommandInputRecordField, "name", "type");

#[derive(Serialize, Deserialize, Debug, PartialEq, Hash, Clone, Builder, Eq)]
#[serde(rename_all = "camelCase")]
/// - Reference: <https://www.commonwl.org/v1.2/Workflow.html#InputRecordField>
pub struct InputRecordField {
    #[doc = include_str!("../../docs/gen/InputRecordField_name.md")]
    #[builder(into)]
    pub name: String,
    #[doc = include_str!("../../docs/gen/InputRecordField_type.md")]
    #[serde(deserialize_with = "deserialize_with_type_dsl")]
    #[builder(into)]
    pub r#type: OneOrMany<InputType>,
    #[doc = include_str!("../../docs/gen/InputRecordField_doc.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub doc: Option<OneOrMany<String>>,
    #[doc = include_str!("../../docs/gen/InputRecordField_label.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub label: Option<String>,
    #[doc = include_str!("../../docs/gen/InputRecordField_secondaryFiles.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    #[serde(deserialize_with = "deserialize_with_secondary_files_dsl")]
    #[builder(into)]
    pub secondary_files: Option<OneOrMany<SecondaryFileSchema>>,
    #[doc = include_str!("../../docs/gen/InputRecordField_streamable.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub streamable: Option<bool>,
    #[doc = include_str!("../../docs/gen/InputRecordField_format.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub format: Option<OneOrMany<String>>,
    #[doc = include_str!("../../docs/gen/InputRecordField_loadContents.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub load_contents: Option<bool>,
    #[doc = include_str!("../../docs/gen/InputRecordField_loadListing.md")]
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

#[derive(Serialize, Deserialize, Debug, PartialEq, Hash, Clone, Builder, Eq)]
#[serde(rename_all = "camelCase")]
/// - Reference: <https://www.commonwl.org/v1.2/CommandLineTool.html#CommandInputEnumSchema>
pub struct CommandInputEnumSchema {
    #[doc = include_str!("../../docs/gen/CommandInputEnumSchema_symbols.md")]
    #[builder(into)]
    pub symbols: Vec<String>,
    #[doc = include_str!("../../docs/gen/CommandInputEnumSchema_name.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub name: Option<String>,
    #[doc = include_str!("../../docs/gen/CommandInputEnumSchema_label.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub label: Option<String>,
    #[doc = include_str!("../../docs/gen/CommandInputEnumSchema_doc.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub doc: Option<OneOrMany<String>>,
    #[doc = include_str!("../../docs/gen/CommandInputEnumSchema_inputBinding.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub input_binding: Option<CommandLineBinding>,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Hash, Clone, Builder, Eq)]
#[serde(rename_all = "camelCase")]
/// - Reference: <https://www.commonwl.org/v1.2/Workflow.html#InputEnumSchema>
pub struct InputEnumSchema {
    #[doc = include_str!("../../docs/gen/InputEnumSchema_symbols.md")]
    #[builder(into)]
    pub symbols: Vec<String>,
    #[doc = include_str!("../../docs/gen/InputEnumSchema_name.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub name: Option<String>,
    #[doc = include_str!("../../docs/gen/InputEnumSchema_label.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub label: Option<String>,
    #[doc = include_str!("../../docs/gen/InputEnumSchema_doc.md")]
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

#[derive(Serialize, Deserialize, Debug, PartialEq, Hash, Clone, Builder, Eq)]
#[serde(rename_all = "camelCase")]
/// - Reference: <https://www.commonwl.org/v1.2/CommandLineTool.html#CommandInputArraySchema>
pub struct CommandInputArraySchema {
    #[doc = include_str!("../../docs/gen/CommandInputArraySchema_items.md")]
    #[builder(into)]
    pub items: OneOrMany<CommandInputType>,
    #[doc = include_str!("../../docs/gen/CommandInputArraySchema_label.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub label: Option<String>,
    #[doc = include_str!("../../docs/gen/CommandInputArraySchema_doc.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub doc: Option<OneOrMany<String>>,
    #[doc = include_str!("../../docs/gen/CommandInputArraySchema_name.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub name: Option<String>,
    #[doc = include_str!("../../docs/gen/CommandInputArraySchema_inputBinding.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub input_binding: Option<CommandLineBinding>,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Hash, Clone, Builder, Eq)]
#[serde(rename_all = "camelCase")]
/// - Reference: <https://www.commonwl.org/v1.2/Workflow.html#InputArraySchema>
pub struct InputArraySchema {
    #[doc = include_str!("../../docs/gen/InputArraySchema_items.md")]
    #[builder(into)]
    pub items: OneOrMany<InputType>,
    #[doc = include_str!("../../docs/gen/InputArraySchema_label.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub label: Option<String>,
    #[doc = include_str!("../../docs/gen/InputArraySchema_doc.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub doc: Option<OneOrMany<String>>,
    #[doc = include_str!("../../docs/gen/InputArraySchema_name.md")]
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

#[doc = include_str!("../../docs/gen/CommandLineBinding.md")]
#[derive(Serialize, Deserialize, Debug, PartialEq, Hash, Clone, Default, Builder, Eq)]
#[serde(rename_all = "camelCase")]
/// - Reference: <https://www.commonwl.org/v1.2/CommandLineTool.html#CommandLineBinding>
pub struct CommandLineBinding {
    #[doc = include_str!("../../docs/gen/CommandLineBinding_loadContents.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub load_contents: Option<bool>,
    #[doc = include_str!("../../docs/gen/CommandLineBinding_position.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub position: Option<IntegerOrExpression>,
    #[doc = include_str!("../../docs/gen/CommandLineBinding_prefix.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub prefix: Option<String>,
    #[doc = include_str!("../../docs/gen/CommandLineBinding_separate.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub separate: Option<bool>,
    #[doc = include_str!("../../docs/gen/CommandLineBinding_itemSeparator.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub item_separator: Option<String>,
    #[doc = include_str!("../../docs/gen/CommandLineBinding_valueFrom.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub value_from: Option<String>,
    #[doc = include_str!("../../docs/gen/CommandLineBinding_shellQuote.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub shell_quote: Option<bool>,
}

#[doc = include_str!("../../docs/gen/WorkflowStepInput.md")]
#[derive(Serialize, Deserialize, Debug, PartialEq, Hash, Clone, Identifiable, Eq, Builder)]
#[serde(rename_all = "camelCase")]
/// - Reference: <https://www.commonwl.org/v1.2/Workflow.html#WorkflowStepInput>
pub struct WorkflowStepInput {
    #[doc = include_str!("../../docs/gen/WorkflowStepInput_id.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[doc = include_str!("../../docs/gen/WorkflowStepInput_source.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<OneOrMany<String>>,
    #[doc = include_str!("../../docs/gen/WorkflowStepInput_linkMerge.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub link_merge: Option<LinkMergeMethod>,
    #[doc = include_str!("../../docs/gen/WorkflowStepInput_pickValue.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pick_value: Option<PickValueMethod>,
    #[doc = include_str!("../../docs/gen/WorkflowStepInput_loadContents.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub load_contents: Option<bool>,
    #[doc = include_str!("../../docs/gen/WorkflowStepInput_loadListing.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub load_listing: Option<LoadListingEnum>,
    #[doc = include_str!("../../docs/gen/WorkflowStepInput_label.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[doc = include_str!("../../docs/gen/WorkflowStepInput_default.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<DefaultValue>,
    #[doc = include_str!("../../docs/gen/WorkflowStepInput_valueFrom.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value_from: Option<String>,
}

make_shorthand_impl!(WorkflowStepInput, "id", "source");

#[cfg(test)]
mod tests {
    use super::*;
    use cwl_salad::deserialize::deserialize_map_list_id;

    #[test]
    #[allow(unused)]
    fn test_command_input_params() {
        #[derive(Deserialize, Debug)]
        struct InputHolder {
            #[serde(deserialize_with = "deserialize_map_list_id")]
            inputs: Vec<CommandInputParameter>,
        }

        let contents = include_str!("../../testdata/command_inputs.yaml");
        let res = serde_saphyr::from_str::<InputHolder>(contents);
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
        let res = serde_saphyr::from_str::<InputHolder>(contents);
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
        let res = serde_saphyr::from_str::<Bag>(contents);
        dbg!(&res);
        assert!(res.is_ok());
    }
}
