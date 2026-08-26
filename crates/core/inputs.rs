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
use std::fmt::Display;
use validator::Validate;

#[derive(Debug, PartialEq, Hash, Clone, Eq)]
pub enum CommandInputParameterType {
    #[doc = include_str!("docs/stdin.md")]
    Stdin,
    CommandInputType(OneOrMany<CommandInputType>),
}

impl Serialize for CommandInputParameterType {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        // `#[serde(untagged)]` would silently serialize the unit variant as `null`,
        // ignoring any `#[serde(rename = "...")]` on it -- so this is written by hand.
        match self {
            Self::Stdin => serializer.serialize_str("stdin"),
            Self::CommandInputType(t) => t.serialize(serializer),
        }
    }
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

impl Display for DefaultValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            // location preferred, path and basename as fallback - all three are optional
            DefaultValue::FileOrDirectory(item) => f.write_str(
                item.location()
                    .or_else(|| item.path())
                    .or_else(|| item.basename())
                    .map_or("", String::as_str),
            ),
            // strings are printed as-is, everything else as json
            DefaultValue::Any(Value::String(s)) => f.write_str(s),
            DefaultValue::Any(value) => {
                f.write_str(&serde_json::to_string(value).unwrap_or_default())
            }
        }
    }
}

#[doc = include_str!("docs/CommandInputParameter.md")]
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
    #[doc = include_str!("docs/CommandInputParameter_type.md")]
    #[serde(deserialize_with = "deserialize_with_type_dsl")]
    #[builder(into)]
    pub r#type: CommandInputParameterType,
    #[doc = include_str!("docs/CommandInputParameter_label.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub label: Option<String>,
    #[doc = include_str!("docs/CommandInputParameter_secondaryFiles.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    #[serde(default)]
    #[serde(deserialize_with = "deserialize_with_secondary_files_dsl")]
    pub secondary_files: Option<OneOrMany<SecondaryFileSchema>>,
    #[doc = include_str!("docs/CommandInputParameter_streamable.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub streamable: Option<bool>,
    #[doc = include_str!("docs/CommandInputParameter_doc.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub doc: Option<OneOrMany<String>>,
    #[doc = include_str!("docs/CommandInputParameter_id.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    #[validate(required)]
    pub id: Option<String>,
    #[doc = include_str!("docs/CommandInputParameter_format.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub format: Option<OneOrMany<String>>,
    #[doc = include_str!("docs/CommandInputParameter_loadContents.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub load_contents: Option<bool>,
    #[doc = include_str!("docs/CommandInputParameter_loadListing.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub load_listing: Option<LoadListingEnum>,
    #[doc = include_str!("docs/CommandInputParameter_default.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub default: Option<DefaultValue>,
    #[doc = include_str!("docs/CommandInputParameter_inputBinding.md")]
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
    Builder,
    Identifiable,
    Eq,
    Validate,
)]
#[serde(rename_all = "camelCase")]
/// - Reference: <https://www.commonwl.org/v1.2/Workflow.html#WorkflowInputParameter>
pub struct WorkflowInputParameter {
    #[doc = include_str!("docs/WorkflowInputParameter_type.md")]
    #[serde(deserialize_with = "deserialize_with_type_dsl")]
    #[builder(into)]
    pub r#type: OneOrMany<InputType>,
    #[doc = include_str!("docs/WorkflowInputParameter_label.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub label: Option<String>,
    #[doc = include_str!("docs/WorkflowInputParameter_secondaryFiles.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    #[serde(default)]
    #[serde(deserialize_with = "deserialize_with_secondary_files_dsl")]
    pub secondary_files: Option<OneOrMany<SecondaryFileSchema>>,
    #[doc = include_str!("docs/WorkflowInputParameter_streamable.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub streamable: Option<bool>,
    #[doc = include_str!("docs/WorkflowInputParameter_doc.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub doc: Option<OneOrMany<String>>,
    #[doc = include_str!("docs/WorkflowInputParameter_id.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    #[validate(required)]
    pub id: Option<String>,
    #[doc = include_str!("docs/WorkflowInputParameter_format.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub format: Option<OneOrMany<String>>,
    #[doc = include_str!("docs/WorkflowInputParameter_loadContents.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub load_contents: Option<bool>,
    #[doc = include_str!("docs/WorkflowInputParameter_loadListing.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub load_listing: Option<LoadListingEnum>,
    #[doc = include_str!("docs/WorkflowInputParameter_default.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub default: Option<DefaultValue>,
    #[doc = include_str!("docs/WorkflowInputParameter_inputBinding.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    #[deprecated(since = "1.2.0", note = "Will be removed in CWL 2.0")]
    pub input_binding: Option<CommandLineBinding>,
}

make_shorthand_impl!(WorkflowInputParameter, "id", "type");

#[allow(deprecated)]
impl Default for WorkflowInputParameter {
    fn default() -> Self {
        Self {
            r#type: default_input_type(),
            label: None,
            secondary_files: None,
            streamable: None,
            doc: None,
            id: None,
            format: None,
            load_contents: None,
            load_listing: None,
            default: None,
            input_binding: None,
        }
    }
}

#[doc = include_str!("docs/OperationInputParameter.md")]
#[derive(
    Serialize,
    Deserialize,
    Debug,
    PartialEq,
    Hash,
    Clone,
    Builder,
    Identifiable,
    Eq,
    Validate,
)]
#[serde(rename_all = "camelCase")]
/// - Reference: <https://www.commonwl.org/v1.2/Workflow.html#OperationInputParameter>
pub struct OperationInputParameter {
    #[doc = include_str!("docs/OperationInputParameter_type.md")]
    #[serde(deserialize_with = "deserialize_with_type_dsl")]
    #[builder(into)]
    pub r#type: OneOrMany<InputType>,
    #[doc = include_str!("docs/OperationInputParameter_label.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub label: Option<String>,
    #[doc = include_str!("docs/OperationInputParameter_secondaryFiles.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    #[serde(default)]
    #[serde(deserialize_with = "deserialize_with_secondary_files_dsl")]
    pub secondary_files: Option<OneOrMany<SecondaryFileSchema>>,
    #[doc = include_str!("docs/OperationInputParameter_streamable.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub streamable: Option<bool>,
    #[doc = include_str!("docs/OperationInputParameter_doc.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub doc: Option<OneOrMany<String>>,
    #[doc = include_str!("docs/OperationInputParameter_id.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    #[validate(required)]
    pub id: Option<String>,
    #[doc = include_str!("docs/OperationInputParameter_format.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub format: Option<OneOrMany<String>>,
    #[doc = include_str!("docs/OperationInputParameter_loadContents.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub load_contents: Option<bool>,
    #[doc = include_str!("docs/OperationInputParameter_loadListing.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub load_listing: Option<LoadListingEnum>,
    #[doc = include_str!("docs/OperationInputParameter_default.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub default: Option<DefaultValue>,
}

make_shorthand_impl!(OperationInputParameter, "id", "type");

impl Default for OperationInputParameter {
    fn default() -> Self {
        Self {
            r#type: default_input_type(),
            label: None,
            secondary_files: None,
            streamable: None,
            doc: None,
            id: None,
            format: None,
            load_contents: None,
            load_listing: None,
            default: None,
        }
    }
}

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

fn default_input_type() -> OneOrMany<InputType> {
    OneOrMany::One(InputType::default())
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
    #[doc = include_str!("docs/CommandInputRecordSchema_fields.md")]
    #[serde(deserialize_with = "deserialize_map_list_option_name")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    #[builder(into)]
    pub fields: Option<Vec<CommandInputRecordField>>,
    #[doc = include_str!("docs/CommandInputRecordSchema_label.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub label: Option<String>,
    #[doc = include_str!("docs/CommandInputRecordSchema_doc.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub doc: Option<OneOrMany<String>>,
    #[doc = include_str!("docs/CommandInputRecordSchema_name.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub name: Option<String>,
    #[doc = include_str!("docs/CommandInputRecordSchema_inputBinding.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub input_binding: Option<CommandLineBinding>,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Hash, Clone, Builder, Eq)]
#[serde(rename_all = "camelCase")]
/// - Reference: <https://www.commonwl.org/v1.2/Workflow.html#InputRecordSchema>
pub struct InputRecordSchema {
    #[doc = include_str!("docs/InputRecordSchema_fields.md")]
    #[serde(deserialize_with = "deserialize_map_list_option_name")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    #[builder(into)]
    pub fields: Option<Vec<InputRecordField>>,
    #[doc = include_str!("docs/InputRecordSchema_label.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub label: Option<String>,
    #[doc = include_str!("docs/InputRecordSchema_doc.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub doc: Option<OneOrMany<String>>,
    #[doc = include_str!("docs/InputRecordSchema_name.md")]
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
    #[doc = include_str!("docs/CommandInputRecordField_name.md")]
    #[builder(into)]
    pub name: String,
    #[doc = include_str!("docs/CommandInputRecordField_type.md")]
    #[serde(deserialize_with = "deserialize_with_type_dsl")]
    #[builder(into)]
    pub r#type: OneOrMany<CommandInputType>,
    #[doc = include_str!("docs/CommandInputRecordField_doc.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub doc: Option<OneOrMany<String>>,
    #[doc = include_str!("docs/CommandInputRecordField_label.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub label: Option<String>,
    #[doc = include_str!("docs/CommandInputRecordField_secondaryFiles.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    #[serde(deserialize_with = "deserialize_with_secondary_files_dsl")]
    #[builder(into)]
    pub secondary_files: Option<OneOrMany<SecondaryFileSchema>>,
    #[doc = include_str!("docs/CommandInputRecordField_streamable.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub streamable: Option<bool>,
    #[doc = include_str!("docs/CommandInputRecordField_format.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub format: Option<OneOrMany<String>>,
    #[doc = include_str!("docs/CommandInputRecordField_loadContents.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub load_contents: Option<bool>,
    #[doc = include_str!("docs/CommandInputRecordField_loadListing.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub load_listing: Option<LoadListingEnum>,
    #[doc = include_str!("docs/CommandInputRecordField_inputBinding.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub input_binding: Option<CommandLineBinding>,
}

make_shorthand_impl!(CommandInputRecordField, "name", "type");

#[derive(Serialize, Deserialize, Debug, PartialEq, Hash, Clone, Builder, Eq)]
#[serde(rename_all = "camelCase")]
/// - Reference: <https://www.commonwl.org/v1.2/Workflow.html#InputRecordField>
pub struct InputRecordField {
    #[doc = include_str!("docs/InputRecordField_name.md")]
    #[builder(into)]
    pub name: String,
    #[doc = include_str!("docs/InputRecordField_type.md")]
    #[serde(deserialize_with = "deserialize_with_type_dsl")]
    #[builder(into)]
    pub r#type: OneOrMany<InputType>,
    #[doc = include_str!("docs/InputRecordField_doc.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub doc: Option<OneOrMany<String>>,
    #[doc = include_str!("docs/InputRecordField_label.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub label: Option<String>,
    #[doc = include_str!("docs/InputRecordField_secondaryFiles.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    #[serde(deserialize_with = "deserialize_with_secondary_files_dsl")]
    #[builder(into)]
    pub secondary_files: Option<OneOrMany<SecondaryFileSchema>>,
    #[doc = include_str!("docs/InputRecordField_streamable.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub streamable: Option<bool>,
    #[doc = include_str!("docs/InputRecordField_format.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub format: Option<OneOrMany<String>>,
    #[doc = include_str!("docs/InputRecordField_loadContents.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub load_contents: Option<bool>,
    #[doc = include_str!("docs/InputRecordField_loadListing.md")]
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
    #[doc = include_str!("docs/CommandInputEnumSchema_symbols.md")]
    #[builder(into)]
    pub symbols: Vec<String>,
    #[doc = include_str!("docs/CommandInputEnumSchema_name.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub name: Option<String>,
    #[doc = include_str!("docs/CommandInputEnumSchema_label.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub label: Option<String>,
    #[doc = include_str!("docs/CommandInputEnumSchema_doc.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub doc: Option<OneOrMany<String>>,
    #[doc = include_str!("docs/CommandInputEnumSchema_inputBinding.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub input_binding: Option<CommandLineBinding>,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Hash, Clone, Builder, Eq)]
#[serde(rename_all = "camelCase")]
/// - Reference: <https://www.commonwl.org/v1.2/Workflow.html#InputEnumSchema>
pub struct InputEnumSchema {
    #[doc = include_str!("docs/InputEnumSchema_symbols.md")]
    #[builder(into)]
    pub symbols: Vec<String>,
    #[doc = include_str!("docs/InputEnumSchema_name.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub name: Option<String>,
    #[doc = include_str!("docs/InputEnumSchema_label.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub label: Option<String>,
    #[doc = include_str!("docs/InputEnumSchema_doc.md")]
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
    #[doc = include_str!("docs/CommandInputArraySchema_items.md")]
    #[builder(into)]
    pub items: OneOrMany<CommandInputType>,
    #[doc = include_str!("docs/CommandInputArraySchema_label.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub label: Option<String>,
    #[doc = include_str!("docs/CommandInputArraySchema_doc.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub doc: Option<OneOrMany<String>>,
    #[doc = include_str!("docs/CommandInputArraySchema_name.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub name: Option<String>,
    #[doc = include_str!("docs/CommandInputArraySchema_inputBinding.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub input_binding: Option<CommandLineBinding>,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Hash, Clone, Builder, Eq)]
#[serde(rename_all = "camelCase")]
/// - Reference: <https://www.commonwl.org/v1.2/Workflow.html#InputArraySchema>
pub struct InputArraySchema {
    #[doc = include_str!("docs/InputArraySchema_items.md")]
    #[builder(into)]
    pub items: OneOrMany<InputType>,
    #[doc = include_str!("docs/InputArraySchema_label.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub label: Option<String>,
    #[doc = include_str!("docs/InputArraySchema_doc.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub doc: Option<OneOrMany<String>>,
    #[doc = include_str!("docs/InputArraySchema_name.md")]
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

#[doc = include_str!("docs/CommandLineBinding.md")]
#[derive(Serialize, Deserialize, Debug, PartialEq, Hash, Clone, Default, Builder, Eq)]
#[serde(rename_all = "camelCase")]
/// - Reference: <https://www.commonwl.org/v1.2/CommandLineTool.html#CommandLineBinding>
pub struct CommandLineBinding {
    #[doc = include_str!("docs/CommandLineBinding_loadContents.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub load_contents: Option<bool>,
    #[doc = include_str!("docs/CommandLineBinding_position.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub position: Option<IntegerOrExpression>,
    #[doc = include_str!("docs/CommandLineBinding_prefix.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub prefix: Option<String>,
    #[doc = include_str!("docs/CommandLineBinding_separate.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub separate: Option<bool>,
    #[doc = include_str!("docs/CommandLineBinding_itemSeparator.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub item_separator: Option<String>,
    #[doc = include_str!("docs/CommandLineBinding_valueFrom.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub value_from: Option<String>,
    #[doc = include_str!("docs/CommandLineBinding_shellQuote.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub shell_quote: Option<bool>,
}

#[doc = include_str!("docs/WorkflowStepInput.md")]
#[derive(
    Serialize, Deserialize, Debug, PartialEq, Hash, Clone, Identifiable, Eq, Builder, Validate,
)]
#[serde(rename_all = "camelCase")]
/// - Reference: <https://www.commonwl.org/v1.2/Workflow.html#WorkflowStepInput>
pub struct WorkflowStepInput {
    #[doc = include_str!("docs/WorkflowStepInput_id.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[validate(required)]
    pub id: Option<String>,
    #[doc = include_str!("docs/WorkflowStepInput_source.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<OneOrMany<String>>,
    #[doc = include_str!("docs/WorkflowStepInput_linkMerge.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub link_merge: Option<LinkMergeMethod>,
    #[doc = include_str!("docs/WorkflowStepInput_pickValue.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pick_value: Option<PickValueMethod>,
    #[doc = include_str!("docs/WorkflowStepInput_loadContents.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub load_contents: Option<bool>,
    #[doc = include_str!("docs/WorkflowStepInput_loadListing.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub load_listing: Option<LoadListingEnum>,
    #[doc = include_str!("docs/WorkflowStepInput_label.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[doc = include_str!("docs/WorkflowStepInput_default.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<DefaultValue>,
    #[doc = include_str!("docs/WorkflowStepInput_valueFrom.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value_from: Option<String>,
}

make_shorthand_impl!(WorkflowStepInput, "id", "source");

#[cfg(test)]
mod tests {
    use super::*;
    use crate::files::Directory;
    use cwl_salad::deserialize::deserialize_map_list_id;

    #[test]
    fn test_display_file_prefers_location() {
        let file = File::builder()
            .location("file:///data/in.txt")
            .path("/data/in.txt")
            .basename("in.txt")
            .build();
        assert_eq!(DefaultValue::from(file).to_string(), "file:///data/in.txt");
    }

    #[test]
    fn test_display_file_falls_back_to_path() {
        let file = File::builder()
            .path("/data/in.txt")
            .basename("in.txt")
            .build();
        assert_eq!(DefaultValue::from(file).to_string(), "/data/in.txt");
    }

    #[test]
    fn test_display_file_falls_back_to_basename() {
        // literal file: only contents + basename, no location or path
        let file = File::builder().contents("hello").basename("in.txt").build();
        assert_eq!(DefaultValue::from(file).to_string(), "in.txt");
    }

    #[test]
    fn test_display_file_without_any_name() {
        assert_eq!(DefaultValue::from(File::default()).to_string(), "");
    }

    #[test]
    fn test_display_directory() {
        let dir = Directory::builder()
            .location("file:///data")
            .path("/data")
            .build();
        assert_eq!(DefaultValue::from(dir).to_string(), "file:///data");

        let dir = Directory::builder().path("/data").build();
        assert_eq!(DefaultValue::from(dir).to_string(), "/data");

        let dir = Directory::builder().basename("data").build();
        assert_eq!(DefaultValue::from(dir).to_string(), "data");

        assert_eq!(DefaultValue::from(Directory::default()).to_string(), "");
    }

    #[test]
    fn test_display_any_string_is_unquoted() {
        assert_eq!(DefaultValue::from("hello world").to_string(), "hello world");
    }

    #[test]
    fn test_display_any_scalars() {
        assert_eq!(DefaultValue::Any(serde_json::json!(42)).to_string(), "42");
        assert_eq!(DefaultValue::Any(serde_json::json!(1.5)).to_string(), "1.5");
        assert_eq!(
            DefaultValue::Any(serde_json::json!(true)).to_string(),
            "true"
        );
        assert_eq!(
            DefaultValue::Any(serde_json::Value::Null).to_string(),
            "null"
        );
    }

    #[test]
    fn test_display_any_collections() {
        assert_eq!(
            DefaultValue::Any(serde_json::json!([1, "a"])).to_string(),
            r#"[1,"a"]"#
        );
        assert_eq!(
            DefaultValue::Any(serde_json::json!({"k": 1})).to_string(),
            r#"{"k":1}"#
        );
    }

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

    #[test]
    fn test_stdin_type_round_trips_through_json() {
        let variant = CommandInputParameterType::Stdin;
        let json = serde_json::to_value(&variant).unwrap();
        assert_eq!(json, serde_json::Value::String("stdin".to_string()));

        let round_tripped: CommandInputParameterType = serde_json::from_value(json).unwrap();
        assert_eq!(round_tripped, variant);
    }
}
