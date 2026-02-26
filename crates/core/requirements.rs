use std::collections::HashMap;

use crate::deserialize::{
    deserialize_map_list_envname, deserialize_map_list_package, make_shorthand_impl,
};
use crate::files::{Dirent, FileOrDirectory, LoadListingEnum};
use crate::{
    BoolOrExpression, IntegerOrExpression, NumberOrExpression, deserialize::FromShortHand,
};
use crate::{ExtractFromEnum, OneOrMany};
use bon::Builder;
use serde::{Deserialize, Serialize};

macro_rules! impl_conversion_methods {
    ($enum:ident, $variant:ident) => {
        impl From<$variant> for $enum {
            fn from(value: $variant) -> Self {
                $enum::$variant(value)
            }
        }

        impl ExtractFromEnum<$enum> for $variant {
            fn get(e: &$enum) -> Option<&Self> {
                if let $enum::$variant(v) = e {
                    Some(v)
                } else {
                    None
                }
            }
        }
    };
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "class")]
pub enum ToolRequirements {
    InlineJavascriptRequirement(InlineJavascriptRequirement),
    LoadListingRequirement(LoadListingRequirement),
    SchemaDefRequirement(SchemaDefRequirement),
    DockerRequirement(DockerRequirement),
    SoftwareRequirement(SoftwareRequirement),
    InitialWorkDirRequirement(InitialWorkDirRequirement),
    EnvVarRequirement(EnvVarRequirement),
    ShellCommandRequirement(ShellCommandRequirement),
    ResourceRequirement(ResourceRequirement),
    WorkReuse(WorkReuse),
    NetworkAccess(NetworkAccess),
    InplaceUpdateRequirement(InplaceUpdateRequirement),
    ToolTimeLimit(ToolTimeLimit),
}

impl FromShortHand for ToolRequirements {}
impl_conversion_methods!(ToolRequirements, InlineJavascriptRequirement);
impl_conversion_methods!(ToolRequirements, LoadListingRequirement);
impl_conversion_methods!(ToolRequirements, SchemaDefRequirement);
impl_conversion_methods!(ToolRequirements, DockerRequirement);
impl_conversion_methods!(ToolRequirements, SoftwareRequirement);
impl_conversion_methods!(ToolRequirements, InitialWorkDirRequirement);
impl_conversion_methods!(ToolRequirements, EnvVarRequirement);
impl_conversion_methods!(ToolRequirements, ShellCommandRequirement);
impl_conversion_methods!(ToolRequirements, ResourceRequirement);
impl_conversion_methods!(ToolRequirements, WorkReuse);
impl_conversion_methods!(ToolRequirements, NetworkAccess);
impl_conversion_methods!(ToolRequirements, InplaceUpdateRequirement);
impl_conversion_methods!(ToolRequirements, ToolTimeLimit);

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "class")]
pub enum WorkflowRequirements {
    InlineJavascriptRequirement(InlineJavascriptRequirement),
    SchemaDefRequirement(SchemaDefRequirement),
    LoadListingRequirement(LoadListingRequirement),
    DockerRequirement(DockerRequirement),
    SoftwareRequirement(SoftwareRequirement),
    InitialWorkDirRequirement(InitialWorkDirRequirement),
    EnvVarRequirement(EnvVarRequirement),
    ShellCommandRequirement(ShellCommandRequirement),
    ResourceRequirement(ResourceRequirement),
    NetworkAccess(NetworkAccess),
    InplaceUpdateRequirement(InplaceUpdateRequirement),
    ToolTimeLimit(ToolTimeLimit),
    SubworkflowFeatureRequirement(SubworkflowFeatureRequirement),
    ScatterFeatureRequirement(ScatterFeatureRequirement),
    MultipleInputFeatureRequirement(MultipleInputFeatureRequirement),
    StepInputExpressionRequirement(StepInputExpressionRequirement),
    WorkReuse(WorkReuse)
}

impl FromShortHand for WorkflowRequirements {}
impl_conversion_methods!(WorkflowRequirements, InlineJavascriptRequirement);
impl_conversion_methods!(WorkflowRequirements, SchemaDefRequirement);
impl_conversion_methods!(WorkflowRequirements, LoadListingRequirement);
impl_conversion_methods!(WorkflowRequirements, DockerRequirement);
impl_conversion_methods!(WorkflowRequirements, SoftwareRequirement);
impl_conversion_methods!(WorkflowRequirements, InitialWorkDirRequirement);
impl_conversion_methods!(WorkflowRequirements, EnvVarRequirement);
impl_conversion_methods!(WorkflowRequirements, ShellCommandRequirement);
impl_conversion_methods!(WorkflowRequirements, ResourceRequirement);
impl_conversion_methods!(WorkflowRequirements, NetworkAccess);
impl_conversion_methods!(WorkflowRequirements, InplaceUpdateRequirement);
impl_conversion_methods!(WorkflowRequirements, ToolTimeLimit);
impl_conversion_methods!(WorkflowRequirements, SubworkflowFeatureRequirement);
impl_conversion_methods!(WorkflowRequirements, ScatterFeatureRequirement);
impl_conversion_methods!(WorkflowRequirements, MultipleInputFeatureRequirement);
impl_conversion_methods!(WorkflowRequirements, StepInputExpressionRequirement);
impl_conversion_methods!(WorkflowRequirements, WorkReuse);


#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(untagged)]
pub enum ToolHints {
    Requirement(ToolRequirements),
    Any(serde_yaml::Value),
}
impl FromShortHand for ToolHints {}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(untagged)]
pub enum WorkflowHints {
    Requirement(WorkflowRequirements),
    Any(serde_yaml::Value),
}
impl FromShortHand for WorkflowHints {}

#[derive(Serialize, Deserialize, Debug, Default, PartialEq, Hash, Clone)]
#[serde(rename_all = "camelCase")]
pub struct InlineJavascriptRequirement {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expression_lib: Option<Vec<ExpressionLibItem>>,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Hash, Clone)]
#[serde(untagged)]
pub enum ExpressionLibItem {
    Include(Include),
    Expression(String),
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Clone, Hash)]
pub struct Include {
    #[serde(rename = "$include")]
    pub include: String,
}

#[derive(Serialize, Deserialize, Debug, Default, PartialEq, Hash, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SchemaDefRequirement {
    pub types: serde_yaml::Value,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Hash, Clone)]
#[serde(rename_all = "camelCase")]
pub struct LoadListingRequirement {
    pub load_listing: LoadListingEnum,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Hash, Clone, Default, Builder)]
#[serde(rename_all = "camelCase")]
pub struct DockerRequirement {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub docker_pull: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub docker_load: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub docker_file: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub docker_import: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub docker_image_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub docker_output_directory: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Hash, Clone, Builder)]
#[serde(rename_all = "camelCase")]
pub struct SoftwareRequirement {
    #[serde(deserialize_with = "deserialize_map_list_package")]
    pub packages: Vec<SoftwarePackage>,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Hash, Clone, Builder)]
#[serde(rename_all = "camelCase")]
pub struct SoftwarePackage {
    pub package: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub specs: Option<Vec<String>>,
}

impl FromShortHand for SoftwarePackage {}

#[derive(Serialize, Deserialize, Debug, PartialEq, Hash, Clone)]
#[serde(untagged)]
pub enum ListingItems {
    Expression(String),
    Dirent(Dirent),
    FileOrDirectory(FileOrDirectory),
    Vec(Vec<FileOrDirectory>),
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Hash, Clone)]
#[serde(untagged)]
pub enum WorkDirItems {
    Expression(String),
    ListingItems(Box<OneOrMany<ListingItems>>),
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Hash, Clone, Builder)]
#[serde(rename_all = "camelCase")]
pub struct InitialWorkDirRequirement {
    pub listing: WorkDirItems,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Hash, Clone, Builder)]
#[serde(rename_all = "camelCase")]
pub struct EnvVarRequirement {
    #[serde(deserialize_with = "deserialize_map_list_envname", rename = "envDef")]
    pub env_def: Vec<EnvironmentDef>,
}

impl EnvVarRequirement {
    pub fn to_map(self) -> HashMap<String, String> {
        self.env_def
            .into_iter()
            .map(|e| (e.env_name, e.env_value))
            .collect()
    }
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Hash, Clone, Builder)]
#[serde(rename_all = "camelCase")]
pub struct EnvironmentDef {
    #[builder(into)]
    pub env_name: String,
    #[builder(into)]
    pub env_value: String,
}
make_shorthand_impl!(EnvironmentDef, "envName", "envValue");

#[derive(Serialize, Deserialize, Debug, PartialEq, Hash, Clone)]
pub struct ShellCommandRequirement;

#[derive(Serialize, Deserialize, Debug, PartialEq, Clone, Default, Builder)]
#[serde(rename_all = "camelCase")]
pub struct ResourceRequirement {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub cores_min: Option<NumberOrExpression>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub cores_max: Option<NumberOrExpression>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub ram_min: Option<NumberOrExpression>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub ram_max: Option<NumberOrExpression>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub tmpdir_min: Option<NumberOrExpression>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub tmpdir_max: Option<NumberOrExpression>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub outdir_min: Option<NumberOrExpression>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub outdir_max: Option<NumberOrExpression>,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Hash, Clone, Builder)]
#[serde(rename_all = "camelCase")]
pub struct WorkReuse {
    pub enable_reuse: BoolOrExpression,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Hash, Clone, Builder)]
#[serde(rename_all = "camelCase")]
pub struct NetworkAccess {
    pub network_access: BoolOrExpression,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Hash, Clone, Builder)]
#[serde(rename_all = "camelCase")]
pub struct InplaceUpdateRequirement {
    pub inplace_update: bool,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Hash, Clone, Builder)]
#[serde(rename_all = "camelCase")]
pub struct ToolTimeLimit {
    pub timelimit: IntegerOrExpression,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Hash, Clone)]
pub struct SubworkflowFeatureRequirement;

#[derive(Serialize, Deserialize, Debug, PartialEq, Hash, Clone)]
pub struct ScatterFeatureRequirement;

#[derive(Serialize, Deserialize, Debug, PartialEq, Hash, Clone)]
pub struct MultipleInputFeatureRequirement;

#[derive(Serialize, Deserialize, Debug, PartialEq, Hash, Clone)]
pub struct StepInputExpressionRequirement;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::deserialize::deserialize_map_list_class;

    #[derive(Serialize, Deserialize, Debug)]
    struct RequirementsBag {
        #[serde(deserialize_with = "deserialize_map_list_class")]
        requirements: Vec<ToolRequirements>,
    }

    #[test]
    fn test_inline_js_req() {
        let bare_by_class = r#"
        requirements:
          - class: InlineJavascriptRequirement
        "#;
        let res = serde_yaml::from_str::<RequirementsBag>(bare_by_class);
        assert!(res.is_ok());

        let bare_by_map = r#"
        requirements:
          InlineJavascriptRequirement: {}
        "#;
        let res = serde_yaml::from_str::<RequirementsBag>(bare_by_map);
        assert!(res.is_ok());
    }

    #[test]
    fn test_iwdr() {
        let contents = include_str!("../../testdata/iwdr.yaml");
        let res = serde_yaml::from_str::<RequirementsBag>(contents);
        dbg!(&res);
        assert!(res.is_ok());
    }

    #[test]
    fn test_mapping_requirements() {
        let contents = include_str!("../../testdata/tool_requirements.yaml");
        let res = serde_yaml::from_str::<RequirementsBag>(contents);
        assert!(res.is_ok());

        let contents = include_str!("../../testdata/tool_requirements_list.yaml");
        let res = serde_yaml::from_str::<RequirementsBag>(contents);
        assert!(res.is_ok());
    }

    #[test]
    fn test_mixed() {
        let contents = r#"
        requirements:
          - class: InitialWorkDirRequirement
            listing:
              - entryname: foo.txt
                entry: $(t("The file is <%= data.inputs.file1.path.split('/').slice(-1)[0] %>\n"))
          - class: InlineJavascriptRequirement
            expressionLib:
              - { $include: underscore.js }
              - "var t = function(s) { return _.template(s, {variable: 'data'})({'inputs': inputs}); };"
        "#;
        let res = serde_yaml::from_str::<RequirementsBag>(contents);
        assert!(res.is_ok());
    }

    #[test]
    fn test_ijsr_mixed_items() {
        let contents = r#"        
        expressionLib:
          - { $include: underscore.js }
          - "var t = function(s) { return _.template(s, {variable: 'data'})({'inputs': inputs}); };"
        "#;
        let res = serde_yaml::from_str::<InlineJavascriptRequirement>(contents);
        assert!(res.is_ok());
    }
}
