use crate::files::{Dirent, FileOrDirectory, LoadListingEnum};
use crate::{BoolOrExpression, IntegerOrExpression, NumberOrExpression};
use crate::{ExtractFromEnum, OneOrMany};
use bon::Builder;
use cwl_salad::deserialize::{
    FromShortHand, deserialize_map_list_envname, deserialize_map_list_package,
};
use cwl_salad::make_shorthand_impl;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

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

            fn get_mut(e: &mut $enum) -> Option<&mut Self> {
                if let $enum::$variant(v) = e {
                    Some(v)
                } else {
                    None
                }
            }
        }
    };
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
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

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
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
    WorkReuse(WorkReuse),
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

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(untagged)]
pub enum ToolHints {
    Requirement(ToolRequirements),
    Any(Value),
}
impl FromShortHand for ToolHints {}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(untagged)]
pub enum WorkflowHints {
    Requirement(WorkflowRequirements),
    Any(Value),
}
impl FromShortHand for WorkflowHints {}

#[derive(Serialize, Deserialize, Debug, Default, PartialEq, Hash, Clone)]
#[serde(rename_all = "camelCase")]
pub struct InlineJavascriptRequirement {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expression_lib: Option<Vec<StringOrInclude>>,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Hash, Clone)]
#[serde(untagged)]
pub enum StringOrInclude {
    Include(Include),
    String(String),
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Clone, Hash, Builder)]
pub struct Include {
    #[serde(rename = "$include")]
    pub include: String,
}

#[doc = include_str!("docs/SchemaDefRequirement.md")]
#[derive(Serialize, Deserialize, Debug, Default, PartialEq, Hash, Clone)]
#[serde(rename_all = "camelCase")]
/// - Reference: <https://www.commonwl.org/v1.2/CommandLineTool.html#SchemaDefRequirement>
pub struct SchemaDefRequirement {
    #[doc = include_str!("docs/SchemaDefRequirement_types.md")]
    pub types: Value,
}

#[doc = include_str!("docs/LoadListingRequirement.md")]
#[derive(Serialize, Deserialize, Debug, PartialEq, Hash, Clone)]
#[serde(rename_all = "camelCase")]
/// - Reference: <https://www.commonwl.org/v1.2/CommandLineTool.html#LoadListingRequirement>
pub struct LoadListingRequirement {
    pub load_listing: LoadListingEnum,
}

#[doc = include_str!("docs/DockerRequirement.md")]
#[derive(Serialize, Deserialize, Debug, PartialEq, Hash, Clone, Default, Builder)]
#[serde(rename_all = "camelCase")]
/// - Reference: <https://www.commonwl.org/v1.2/CommandLineTool.html#DockerRequirement>
pub struct DockerRequirement {
    #[doc = include_str!("docs/DockerRequirement_dockerPull.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub docker_pull: Option<String>,
    #[doc = include_str!("docs/DockerRequirement_dockerLoad.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub docker_load: Option<String>,
    #[doc = include_str!("docs/DockerRequirement_dockerFile.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub docker_file: Option<StringOrInclude>,
    #[doc = include_str!("docs/DockerRequirement_dockerImport.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub docker_import: Option<String>,
    #[doc = include_str!("docs/DockerRequirement_dockerImageId.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub docker_image_id: Option<String>,
    #[doc = include_str!("docs/DockerRequirement_dockerOutputDirectory.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub docker_output_directory: Option<String>,
}

#[doc = include_str!("docs/SoftwareRequirement.md")]
#[derive(Serialize, Deserialize, Debug, PartialEq, Hash, Clone, Builder)]
#[serde(rename_all = "camelCase")]
/// - Reference: <https://www.commonwl.org/v1.2/CommandLineTool.html#SoftwareRequirement>
pub struct SoftwareRequirement {
    #[doc = include_str!("docs/SoftwareRequirement_packages.md")]
    #[serde(deserialize_with = "deserialize_map_list_package")]
    pub packages: Vec<SoftwarePackage>,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Hash, Clone, Builder)]
#[serde(rename_all = "camelCase")]
/// - Reference: <https://www.commonwl.org/v1.2/CommandLineTool.html#SoftwarePackage>
pub struct SoftwarePackage {
    #[doc = include_str!("docs/SoftwarePackage_package.md")]
    pub package: String,
    #[doc = include_str!("docs/SoftwarePackage_version.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<Vec<String>>,
    #[doc = include_str!("docs/SoftwarePackage_specs.md")]
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

#[doc = include_str!("docs/InitialWorkDirRequirement.md")]
#[derive(Serialize, Deserialize, Debug, PartialEq, Hash, Clone, Builder)]
#[serde(rename_all = "camelCase")]
/// - Reference: <https://www.commonwl.org/v1.2/CommandLineTool.html#InitialWorkDirRequirement>
pub struct InitialWorkDirRequirement {
    #[doc = include_str!("docs/InitialWorkDirRequirement_listing.md")]
    pub listing: WorkDirItems,
}

#[doc = include_str!("docs/EnvVarRequirement.md")]
#[derive(Serialize, Deserialize, Debug, PartialEq, Hash, Clone, Builder)]
#[serde(rename_all = "camelCase")]
/// - Reference: <https://www.commonwl.org/v1.2/CommandLineTool.html#EnvVarRequirement>
pub struct EnvVarRequirement {
    #[doc = include_str!("docs/EnvVarRequirement_envDef.md")]
    #[serde(deserialize_with = "deserialize_map_list_envname", rename = "envDef")]
    pub env_def: Vec<EnvironmentDef>,
}

impl EnvVarRequirement {
    #[must_use]
    pub fn to_map(self) -> HashMap<String, String> {
        self.env_def
            .into_iter()
            .map(|e| (e.env_name, e.env_value))
            .collect()
    }
}

#[doc = include_str!("docs/EnvironmentDef.md")]
#[derive(Serialize, Deserialize, Debug, PartialEq, Hash, Clone, Builder)]
#[serde(rename_all = "camelCase")]
/// - Reference: <https://www.commonwl.org/v1.2/CommandLineTool.html#EnvironmentDef>
pub struct EnvironmentDef {
    #[doc = include_str!("docs/EnvironmentDef_envName.md")]
    #[builder(into)]
    pub env_name: String,
    #[doc = include_str!("docs/EnvironmentDef_envValue.md")]
    #[builder(into)]
    pub env_value: String,
}
make_shorthand_impl!(EnvironmentDef, "envName", "envValue");

#[doc = include_str!("docs/ShellCommandRequirement.md")]
#[derive(Serialize, Deserialize, Debug, PartialEq, Hash, Clone)]
/// - Reference: <https://www.commonwl.org/v1.2/CommandLineTool.html#ShellCommandRequirement>
pub struct ShellCommandRequirement;

#[doc = include_str!("docs/ResourceRequirement.md")]
#[derive(Serialize, Deserialize, Debug, PartialEq, Clone, Default, Builder)]
#[serde(rename_all = "camelCase")]
/// - Reference: <https://www.commonwl.org/v1.2/CommandLineTool.html#ResourceRequirement>
pub struct ResourceRequirement {
    #[doc = include_str!("docs/ResourceRequirement_coresMin.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub cores_min: Option<NumberOrExpression>,
    #[doc = include_str!("docs/ResourceRequirement_coresMax.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub cores_max: Option<NumberOrExpression>,
    #[doc = include_str!("docs/ResourceRequirement_ramMin.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub ram_min: Option<NumberOrExpression>,
    #[doc = include_str!("docs/ResourceRequirement_ramMax.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub ram_max: Option<NumberOrExpression>,
    #[doc = include_str!("docs/ResourceRequirement_tmpdirMin.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub tmpdir_min: Option<NumberOrExpression>,
    #[doc = include_str!("docs/ResourceRequirement_tmpdirMax.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub tmpdir_max: Option<NumberOrExpression>,
    #[doc = include_str!("docs/ResourceRequirement_outdirMin.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub outdir_min: Option<NumberOrExpression>,
    #[doc = include_str!("docs/ResourceRequirement_outdirMax.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub outdir_max: Option<NumberOrExpression>,
}

#[doc = include_str!("docs/WorkReuse.md")]
#[derive(Serialize, Deserialize, Debug, PartialEq, Hash, Clone, Builder)]
#[serde(rename_all = "camelCase")]
/// - Reference: <https://www.commonwl.org/v1.2/Workflow.html#WorkReuse>
pub struct WorkReuse {
    pub enable_reuse: BoolOrExpression,
}

#[doc = include_str!("docs/NetworkAccess.md")]
#[derive(Serialize, Deserialize, Debug, PartialEq, Hash, Clone, Builder)]
#[serde(rename_all = "camelCase")]
/// - Reference: <https://www.commonwl.org/v1.2/Workflow.html#NetworkAccess>
pub struct NetworkAccess {
    pub network_access: BoolOrExpression,
}

#[doc = include_str!("docs/InplaceUpdateRequirement.md")]
#[derive(Serialize, Deserialize, Debug, PartialEq, Hash, Clone, Builder)]
#[serde(rename_all = "camelCase")]
/// - Reference: <https://www.commonwl.org/v1.2/Workflow.html#InplaceUpdateRequirement>
pub struct InplaceUpdateRequirement {
    pub inplace_update: bool,
}

#[doc = include_str!("docs/ToolTimeLimit.md")]
#[derive(Serialize, Deserialize, Debug, PartialEq, Hash, Clone, Builder)]
#[serde(rename_all = "camelCase")]
/// - Reference: <https://www.commonwl.org/v1.2/Workflow.html#ToolTimeLimit>
pub struct ToolTimeLimit {
    #[doc = include_str!("docs/ToolTimeLimit_timelimit.md")]
    pub timelimit: IntegerOrExpression,
}

#[doc = include_str!("docs/SubworkflowFeatureRequirement.md")]
#[derive(Serialize, Deserialize, Debug, PartialEq, Hash, Clone)]
/// - Reference: <https://www.commonwl.org/v1.2/Workflow.html#SubworkflowFeatureRequirement>
pub struct SubworkflowFeatureRequirement;

#[doc = include_str!("docs/ScatterFeatureRequirement.md")]
#[derive(Serialize, Deserialize, Debug, PartialEq, Hash, Clone)]
/// - Reference: <https://www.commonwl.org/v1.2/Workflow.html#ScatterFeatureRequirement>
pub struct ScatterFeatureRequirement;

#[doc = include_str!("docs/MultipleInputFeatureRequirement.md")]
#[derive(Serialize, Deserialize, Debug, PartialEq, Hash, Clone)]
/// - Reference: <https://www.commonwl.org/v1.2/Workflow.html#MultipleInputFeatureRequirement>
pub struct MultipleInputFeatureRequirement;

#[doc = include_str!("docs/StepInputExpressionRequirement.md")]
#[derive(Serialize, Deserialize, Debug, PartialEq, Hash, Clone)]
/// - Reference: <https://www.commonwl.org/v1.2/Workflow.html#StepInputExpressionRequirement>
pub struct StepInputExpressionRequirement;

#[cfg(test)]
mod tests {
    use super::*;
    use cwl_salad::deserialize::deserialize_map_list_class;

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
        let res = serde_saphyr::from_str::<RequirementsBag>(bare_by_class);
        assert!(res.is_ok());

        let bare_by_map = r#"
        requirements:
          InlineJavascriptRequirement: {}
        "#;
        let res = serde_saphyr::from_str::<RequirementsBag>(bare_by_map);
        assert!(res.is_ok());
    }

    #[test]
    fn test_iwdr() {
        let contents = include_str!("../../testdata/iwdr.yaml");
        let res = serde_saphyr::from_str::<RequirementsBag>(contents);
        dbg!(&res);
        assert!(res.is_ok());
    }

    #[test]
    fn test_mapping_requirements() {
        let contents = include_str!("../../testdata/tool_requirements.yaml");
        let res = serde_saphyr::from_str::<RequirementsBag>(contents);
        assert!(res.is_ok());

        let contents = include_str!("../../testdata/tool_requirements_list.yaml");
        let res = serde_saphyr::from_str::<RequirementsBag>(contents);
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
        let res = serde_saphyr::from_str::<RequirementsBag>(contents);
        assert!(res.is_ok());
    }

    #[test]
    fn test_ijsr_mixed_items() {
        let contents = r#"        
        expressionLib:
          - { $include: underscore.js }
          - "var t = function(s) { return _.template(s, {variable: 'data'})({'inputs': inputs}); };"
        "#;
        let res = serde_saphyr::from_str::<InlineJavascriptRequirement>(contents);
        assert!(res.is_ok());
    }

    #[test]
    fn test_iwdr_include_entry() {
        let contents = r#"
        requirements:
          - class: InitialWorkDirRequirement
            listing:
              - entryname: foo.txt
                entry: 
                    $include: calculation.py
    "#;
        let res = serde_saphyr::from_str::<RequirementsBag>(contents);
        assert!(res.is_ok());
    }
}
