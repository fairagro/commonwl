use crate::BoolOrExpression;
use crate::IntegerOrExpression;
use crate::NumberOrExpression;
use crate::deserialize::FromShortHand;
use crate::deserialize::deserialize_map_list_envname;
use crate::deserialize::deserialize_map_list_package;
use crate::deserialize::make_shorthand_impl;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Copy, PartialEq, Hash, Clone)]
#[serde(rename = "snake_case")]
pub enum LoadListingEnum {
    NoListing,
    ShallowListing,
    DeepListing,
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

#[derive(Serialize, Deserialize, Debug, Default, PartialEq, Hash, Clone)]
#[serde(rename = "camelCase")]
pub struct InlineJavascriptRequirement {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expression_lib: Option<Vec<String>>,
}

#[derive(Serialize, Deserialize, Debug, Default, PartialEq, Hash, Clone)]
#[serde(rename = "camelCase")]
pub struct SchemaDefRequirement {
    //TODO
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Hash, Clone)]
#[serde(rename = "camelCase")]
pub struct LoadListingRequirement {
    pub load_listing: LoadListingEnum,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Hash, Clone)]
#[serde(rename = "camelCase")]
pub struct DockerRequirement {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub docker_pull: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub docker_load: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub docker_file: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub docker_import: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub docker_image_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub docker_output_directory: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Hash, Clone)]
#[serde(rename = "camelCase")]
pub struct SoftwareRequirement {
    #[serde(deserialize_with = "deserialize_map_list_package")]
    pub packages: Vec<SoftwarePackage>,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Hash, Clone)]
#[serde(rename = "camelCase")]
pub struct SoftwarePackage {
    pub package: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub specs: Option<Vec<String>>,
}

impl FromShortHand for SoftwarePackage {}

#[derive(Serialize, Deserialize, Debug, PartialEq, Hash, Clone)]
#[serde(rename = "camelCase")]
pub struct InitialWorkDirRequirement {
    //TODO
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Hash, Clone)]
#[serde(rename = "camelCase")]
pub struct EnvVarRequirement {
    #[serde(deserialize_with = "deserialize_map_list_envname", rename = "envDef")]
    pub env_def: Vec<EnvironmentDef>,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Hash, Clone)]
#[serde(rename = "camelCase")]
pub struct EnvironmentDef {
    pub env_name: String,
    pub env_value: String,
}
make_shorthand_impl!(EnvironmentDef, "env_name", "env_value");

#[derive(Serialize, Deserialize, Debug, PartialEq, Hash, Clone)]
pub struct ShellCommandRequirement;

#[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
#[serde(rename = "camelCase")]
pub struct ResourceRequirement {
    pub cores_min: Option<NumberOrExpression>,
    pub cores_max: Option<NumberOrExpression>,
    pub ram_min: Option<NumberOrExpression>,
    pub ram_max: Option<NumberOrExpression>,
    pub tmpdir_min: Option<NumberOrExpression>,
    pub tmpdir_max: Option<NumberOrExpression>,
    pub outdir_min: Option<NumberOrExpression>,
    pub outdir_max: Option<NumberOrExpression>,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Hash, Clone)]
#[serde(rename = "camelCase")]
pub struct WorkReuse {
    pub enable_reuse: BoolOrExpression,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Hash, Clone)]
#[serde(rename = "camelCase")]
pub struct NetworkAccess {
    pub network_access: BoolOrExpression,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Hash, Clone)]
#[serde(rename = "camelCase")]
pub struct InplaceUpdateRequirement {
    pub inplace_update: bool,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Hash, Clone)]
#[serde(rename = "camelCase")]
pub struct ToolTimeLimit {
    pub timelimit: IntegerOrExpression,
}

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
    fn test_mapping_requirements() {
        let contents = include_str!("../testdata/tool_requirements.yaml");
        let res = serde_yaml::from_str::<RequirementsBag>(contents);
        assert!(res.is_ok());

        let contents = include_str!("../testdata/tool_requirements_list.yaml");
        let res = serde_yaml::from_str::<RequirementsBag>(contents);
        assert!(res.is_ok());
    }
}
