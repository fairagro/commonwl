use crate::types::{CWLType, SecondaryFileSchema};
use crate::{OneOrMany, files::LoadListingEnum};
use bon::Builder;
use cwl_salad::Identifiable;
use cwl_salad::deserialize::{
    FromShortHand, deserialize_map_list_option_name, deserialize_with_secondary_files_dsl,
    deserialize_with_type_dsl,
};
use cwl_salad::make_shorthand_impl;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Serialize, Debug, PartialEq, Hash, Clone, Eq)]
#[serde(untagged)]
pub enum CommandOutputParameterType {
    #[serde(rename = "stdout")]
    Stdout,
    #[serde(rename = "stderr")]
    Stderr,
    CommandOutputType(OneOrMany<CommandOutputType>),
}

impl<'de> Deserialize<'de> for CommandOutputParameterType {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = Value::deserialize(deserializer)?;

        if value == Value::String("stderr".to_string()) {
            return Ok(Self::Stderr);
        }

        if value == Value::String("stdout".to_string()) {
            return Ok(Self::Stdout);
        }

        OneOrMany::<CommandOutputType>::deserialize(value)
            .map(Self::CommandOutputType)
            .map_err(serde::de::Error::custom)
    }
}

impl Default for CommandOutputParameterType {
    fn default() -> Self {
        CommandOutputParameterType::CommandOutputType(OneOrMany::One(CommandOutputType::default()))
    }
}

impl From<CWLType> for CommandOutputParameterType {
    fn from(value: CWLType) -> Self {
        CommandOutputParameterType::CommandOutputType(OneOrMany::One(CommandOutputType::CWLType(
            value,
        )))
    }
}

impl From<CommandOutputType> for CommandOutputParameterType {
    fn from(value: CommandOutputType) -> Self {
        CommandOutputParameterType::CommandOutputType(OneOrMany::One(value))
    }
}

impl From<OneOrMany<OutputType>> for CommandOutputParameterType {
    fn from(value: OneOrMany<OutputType>) -> Self {
        CommandOutputParameterType::CommandOutputType(match value {
            OneOrMany::One(item) => OneOrMany::One(item.into()),
            OneOrMany::Many(items) => OneOrMany::Many(items.into_iter().map(Into::into).collect()),
        })
    }
}

#[derive(
    Serialize, Deserialize, Debug, PartialEq, Hash, Clone, Default, Builder, Identifiable, Eq,
)]
#[serde(rename_all = "camelCase")]
pub struct CommandOutputParameter {
    #[serde(deserialize_with = "deserialize_with_type_dsl")]
    #[builder(into)]
    pub r#type: CommandOutputParameterType,
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
    pub output_binding: Option<CommandOutputBinding>,
}

make_shorthand_impl!(CommandOutputParameter, "id", "type");

#[derive(
    Serialize, Deserialize, Debug, PartialEq, Hash, Clone, Default, Builder, Identifiable, Eq,
)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowOutputParameter {
    #[serde(deserialize_with = "deserialize_with_type_dsl")]
    #[builder(into)]
    pub r#type: CommandOutputParameterType,
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
    pub output_source: Option<OneOrMany<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub link_merge: Option<LinkMergeMethod>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub pick_value: Option<PickValueMethod>,
}

make_shorthand_impl!(WorkflowOutputParameter, "id", "type");

#[derive(
    Serialize, Deserialize, Debug, PartialEq, Hash, Clone, Default, Builder, Identifiable, Eq,
)]
#[serde(rename_all = "camelCase")]
pub struct OperationOutputParameter {
    #[serde(deserialize_with = "deserialize_with_type_dsl")]
    #[builder(into)]
    pub r#type: OneOrMany<OutputType>,
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
}

make_shorthand_impl!(OperationOutputParameter, "id", "type");

#[derive(
    Serialize, Deserialize, Debug, PartialEq, Hash, Clone, Default, Builder, Identifiable, Eq,
)]
#[serde(rename_all = "camelCase")]
pub struct ExpressionToolOutputParameter {
    #[serde(deserialize_with = "deserialize_with_type_dsl")]
    #[builder(into)]
    pub r#type: OneOrMany<OutputType>,
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
}

make_shorthand_impl!(ExpressionToolOutputParameter, "id", "type");

#[derive(Serialize, Deserialize, Debug, Copy, PartialEq, Hash, Clone, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LinkMergeMethod {
    MergeNested,
    MergeFlattened,
}

#[derive(Serialize, Deserialize, Debug, Copy, PartialEq, Hash, Clone, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PickValueMethod {
    FirstNonNull,
    TheOnlyNonNull,
    AllNonNull,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Hash, Clone, Eq)]
#[serde(tag = "type")]
#[serde(rename_all = "snake_case")]
pub enum CommandOutputSchema {
    Record(CommandOutputRecordSchema),
    Enum(CommandOutputEnumSchema),
    Array(CommandOutputArraySchema),
}

impl From<OutputSchema> for CommandOutputSchema {
    fn from(value: OutputSchema) -> Self {
        match value {
            OutputSchema::Record(rec) => CommandOutputSchema::Record(rec.into()),
            OutputSchema::Enum(enu) => CommandOutputSchema::Enum(enu.into()),
            OutputSchema::Array(arr) => CommandOutputSchema::Array(arr.into()),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Hash, Clone, Eq)]
#[serde(tag = "type")]
#[serde(rename_all = "snake_case")]
pub enum OutputSchema {
    Record(OutputRecordSchema),
    Enum(OutputEnumSchema),
    Array(OutputArraySchema),
}

impl From<CommandOutputSchema> for OutputSchema {
    fn from(value: CommandOutputSchema) -> Self {
        match value {
            CommandOutputSchema::Record(rec) => OutputSchema::Record(rec.into()),
            CommandOutputSchema::Enum(enu) => OutputSchema::Enum(enu.into()),
            CommandOutputSchema::Array(arr) => OutputSchema::Array(arr.into()),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Hash, Clone, Eq)]
#[serde(untagged)]
pub enum CommandOutputType {
    CWLType(CWLType),
    CommandOutputSchema(Box<CommandOutputSchema>),
    String(String),
}

impl Default for CommandOutputType {
    fn default() -> Self {
        CommandOutputType::CWLType(CWLType::Null)
    }
}

impl From<OutputType> for CommandOutputType {
    fn from(value: OutputType) -> Self {
        match value {
            OutputType::CWLType(cwltype) => CommandOutputType::CWLType(cwltype),
            OutputType::OutputSchema(schema) => {
                CommandOutputType::CommandOutputSchema(Box::new((*schema).into()))
            }
            OutputType::String(s) => CommandOutputType::String(s),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Hash, Clone, Eq)]
#[serde(untagged)]
pub enum OutputType {
    CWLType(CWLType),
    OutputSchema(Box<OutputSchema>),
    String(String),
}

impl Default for OutputType {
    fn default() -> Self {
        OutputType::CWLType(CWLType::Null)
    }
}

impl Default for OneOrMany<OutputType> {
    fn default() -> Self {
        OneOrMany::One(OutputType::default())
    }
}

impl From<CommandOutputType> for OutputType {
    fn from(value: CommandOutputType) -> Self {
        match value {
            CommandOutputType::CWLType(t) => OutputType::CWLType(t),
            CommandOutputType::CommandOutputSchema(schema) => {
                OutputType::OutputSchema(Box::new((*schema).into()))
            }
            CommandOutputType::String(s) => OutputType::String(s),
        }
    }
}

impl From<OneOrMany<CommandOutputType>> for OneOrMany<OutputType> {
    fn from(value: OneOrMany<CommandOutputType>) -> Self {
        match value {
            OneOrMany::One(t) => OneOrMany::One(t.into()),
            OneOrMany::Many(v) => OneOrMany::Many(v.into_iter().map(Into::into).collect()),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Hash, Clone, Eq, Builder)]
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

impl From<OutputRecordSchema> for CommandOutputRecordSchema {
    fn from(value: OutputRecordSchema) -> Self {
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

#[derive(Serialize, Deserialize, Debug, PartialEq, Hash, Clone, Eq, Builder)]
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

impl From<CommandOutputRecordSchema> for OutputRecordSchema {
    fn from(value: CommandOutputRecordSchema) -> Self {
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

#[derive(Serialize, Deserialize, Debug, PartialEq, Hash, Clone, Eq, Builder)]
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

impl From<OutputRecordField> for CommandOutputRecordField {
    fn from(value: OutputRecordField) -> Self {
        Self {
            name: value.name,
            r#type: value.r#type.map(Into::into),
            doc: value.doc,
            label: value.label,
            secondary_files: value.secondary_files,
            streamable: value.streamable,
            format: value.format,
            output_binding: None,
        }
    }
}

make_shorthand_impl!(CommandOutputRecordField, "name", "type");

#[derive(Serialize, Deserialize, Debug, PartialEq, Hash, Clone, Eq, Builder)]
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

impl From<CommandOutputRecordField> for OutputRecordField {
    fn from(value: CommandOutputRecordField) -> Self {
        Self {
            name: value.name,
            r#type: value.r#type.map(Into::into),
            doc: value.doc,
            label: value.label,
            secondary_files: value.secondary_files,
            streamable: value.streamable,
            format: value.format,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Hash, Clone, Eq, Builder)]
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

impl From<OutputEnumSchema> for CommandOutputEnumSchema {
    fn from(value: OutputEnumSchema) -> Self {
        Self {
            symbols: value.symbols,
            name: value.name,
            label: value.label,
            doc: value.doc,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Hash, Clone, Eq, Builder)]
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

impl From<CommandOutputEnumSchema> for OutputEnumSchema {
    fn from(value: CommandOutputEnumSchema) -> Self {
        Self {
            symbols: value.symbols,
            name: value.name,
            label: value.label,
            doc: value.doc,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Hash, Clone, Eq, Builder)]
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

impl From<OutputArraySchema> for CommandOutputArraySchema {
    fn from(value: OutputArraySchema) -> Self {
        Self {
            items: value.items.map(Into::into),
            name: value.name,
            label: value.label,
            doc: value.doc,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Hash, Clone, Eq, Builder)]
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

impl From<CommandOutputArraySchema> for OutputArraySchema {
    fn from(value: CommandOutputArraySchema) -> Self {
        Self {
            items: value.items.map(Into::into),
            name: value.name,
            label: value.label,
            doc: value.doc,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Hash, Clone, Default, Builder, Eq)]
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

#[derive(Serialize, Deserialize, Debug, PartialEq, Hash, Clone)]
#[serde(untagged)]
pub enum StringOrWorkflowStepOutput {
    String(String),
    WorkflowStepOutput(WorkflowStepOutput),
}

impl StringOrWorkflowStepOutput {
    /// Gets the `WorkflowStepOutput` ID
    /// # Panics
    /// if id is null (never!)
    #[must_use]
    pub fn id(&self) -> String {
        match self {
            Self::String(s) => s,
            Self::WorkflowStepOutput(wfs) => wfs.id.as_ref().unwrap(),
        }
        .clone()
    }
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Hash, Clone, Identifiable)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowStepOutput {
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use cwl_salad::deserialize::deserialize_map_list_id;

    #[test]
    #[allow(unused)]
    fn test_command_output_params() {
        #[derive(Deserialize, Debug)]
        struct OutputHolder {
            #[serde(deserialize_with = "deserialize_map_list_id")]
            outputs: Vec<CommandOutputParameter>,
        }

        let contents = include_str!("../../testdata/command_outputs.yaml");
        let res = serde_saphyr::from_str::<OutputHolder>(contents);
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

        let contents = include_str!("../../testdata/command_out_schemas.yaml");
        let res = serde_saphyr::from_str::<Bag>(contents);
        dbg!(&res);
        assert!(res.is_ok());
    }
}
