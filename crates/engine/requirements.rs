use cwl_core::{
    documents::CWLDocument,
    requirements::{
        DockerRequirement, EnvVarRequirement, InitialWorkDirRequirement,
        InlineJavascriptRequirement, InplaceUpdateRequirement, LoadListingRequirement,
        MultipleInputFeatureRequirement, NetworkAccess, ResourceRequirement,
        ScatterFeatureRequirement, SchemaDefRequirement, ShellCommandRequirement,
        SoftwareRequirement, StepInputExpressionRequirement, SubworkflowFeatureRequirement,
        ToolRequirements, ToolTimeLimit, WorkReuse, WorkflowRequirements,
    },
};
use serde::Deserialize;

use crate::input::InputObject;

// CWL does care about the distinction between tool requirements and workflow requirements,
// but we don't care about that distinction here.
#[derive(Deserialize, Debug, Clone)]
#[serde(tag = "class")]
pub enum ProcessRequirements {
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
    SubworkflowFeatureRequirement(SubworkflowFeatureRequirement),
    ScatterFeatureRequirement(ScatterFeatureRequirement),
    MultipleInputFeatureRequirement(MultipleInputFeatureRequirement),
    StepInputExpressionRequirement(StepInputExpressionRequirement),
}

impl From<ToolRequirements> for ProcessRequirements {
    fn from(req: ToolRequirements) -> Self {
        match req {
            ToolRequirements::InlineJavascriptRequirement(r) => {
                ProcessRequirements::InlineJavascriptRequirement(r)
            }
            ToolRequirements::LoadListingRequirement(r) => {
                ProcessRequirements::LoadListingRequirement(r)
            }
            ToolRequirements::SchemaDefRequirement(r) => {
                ProcessRequirements::SchemaDefRequirement(r)
            }
            ToolRequirements::DockerRequirement(r) => ProcessRequirements::DockerRequirement(r),
            ToolRequirements::SoftwareRequirement(r) => ProcessRequirements::SoftwareRequirement(r),
            ToolRequirements::InitialWorkDirRequirement(r) => {
                ProcessRequirements::InitialWorkDirRequirement(r)
            }
            ToolRequirements::EnvVarRequirement(r) => ProcessRequirements::EnvVarRequirement(r),
            ToolRequirements::ShellCommandRequirement(r) => {
                ProcessRequirements::ShellCommandRequirement(r)
            }
            ToolRequirements::ResourceRequirement(r) => ProcessRequirements::ResourceRequirement(r),
            ToolRequirements::WorkReuse(r) => ProcessRequirements::WorkReuse(r),
            ToolRequirements::NetworkAccess(r) => ProcessRequirements::NetworkAccess(r),
            ToolRequirements::InplaceUpdateRequirement(r) => {
                ProcessRequirements::InplaceUpdateRequirement(r)
            }
            ToolRequirements::ToolTimeLimit(r) => ProcessRequirements::ToolTimeLimit(r),
        }
    }
}

impl From<WorkflowRequirements> for ProcessRequirements {
    fn from(req: WorkflowRequirements) -> Self {
        match req {
            WorkflowRequirements::InlineJavascriptRequirement(r) => {
                ProcessRequirements::InlineJavascriptRequirement(r)
            }
            WorkflowRequirements::SchemaDefRequirement(r) => {
                ProcessRequirements::SchemaDefRequirement(r)
            }
            WorkflowRequirements::LoadListingRequirement(r) => {
                ProcessRequirements::LoadListingRequirement(r)
            }
            WorkflowRequirements::DockerRequirement(r) => ProcessRequirements::DockerRequirement(r),
            WorkflowRequirements::SoftwareRequirement(r) => {
                ProcessRequirements::SoftwareRequirement(r)
            }
            WorkflowRequirements::InitialWorkDirRequirement(r) => {
                ProcessRequirements::InitialWorkDirRequirement(r)
            }
            WorkflowRequirements::EnvVarRequirement(r) => ProcessRequirements::EnvVarRequirement(r),
            WorkflowRequirements::ShellCommandRequirement(r) => {
                ProcessRequirements::ShellCommandRequirement(r)
            }
            WorkflowRequirements::ResourceRequirement(r) => {
                ProcessRequirements::ResourceRequirement(r)
            }
            WorkflowRequirements::NetworkAccess(r) => ProcessRequirements::NetworkAccess(r),
            WorkflowRequirements::InplaceUpdateRequirement(r) => {
                ProcessRequirements::InplaceUpdateRequirement(r)
            }
            WorkflowRequirements::ToolTimeLimit(r) => ProcessRequirements::ToolTimeLimit(r),
            WorkflowRequirements::SubworkflowFeatureRequirement(r) => {
                ProcessRequirements::SubworkflowFeatureRequirement(r)
            }
            WorkflowRequirements::ScatterFeatureRequirement(r) => {
                ProcessRequirements::ScatterFeatureRequirement(r)
            }
            WorkflowRequirements::MultipleInputFeatureRequirement(r) => {
                ProcessRequirements::MultipleInputFeatureRequirement(r)
            }
            WorkflowRequirements::StepInputExpressionRequirement(r) => {
                ProcessRequirements::StepInputExpressionRequirement(r)
            }
        }
    }
}

pub fn collect_requirements(
    specification: &CWLDocument,
    inputs: &InputObject,
) -> Vec<ProcessRequirements> {
    let mut requirements: Vec<ProcessRequirements> = match &specification {
        CWLDocument::CommandLineTool(tool) => tool
            .requirements
            .clone()
            .unwrap_or_default()
            .into_iter()
            .map(ProcessRequirements::from)
            .collect(),
        CWLDocument::Workflow(workflow) => workflow
            .requirements
            .clone()
            .unwrap_or_default()
            .into_iter()
            .map(ProcessRequirements::from)
            .collect(),
        CWLDocument::ExpressionTool(tool) => tool
            .requirements
            .clone()
            .unwrap_or_default()
            .into_iter()
            .map(ProcessRequirements::from)
            .collect(),
        CWLDocument::Operation(operation) => operation
            .requirements
            .clone()
            .unwrap_or_default()
            .into_iter()
            .map(ProcessRequirements::from)
            .collect(),
    };

    merge(&mut requirements, &inputs.requirements);
    requirements
}

fn merge(dst: &mut Vec<ProcessRequirements>, src: &[ProcessRequirements]) {
    for req in src {
        if let Some(r) = dst.iter_mut().find(|r| same_variant(r, req)) {
            *r = req.clone();
        } else {
            dst.push(req.clone());
        }
    }
}

fn same_variant(a: &ProcessRequirements, b: &ProcessRequirements) -> bool {
    std::mem::discriminant(a) == std::mem::discriminant(b)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::load_input_file_from_file;
    use std::path::Path;

    #[test]
    fn test_collect_requirements() {
        let yaml = include_str!("../../testdata/cwl/tests/env-tool4.cwl");
        let tool: CWLDocument = serde_yaml::from_str(yaml).unwrap();

        let base_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/cwl/tests");
        let inputs = base_path.join("env-job4.yaml");
        let input_values = load_input_file_from_file(inputs, base_path).unwrap();

        let requirements = collect_requirements(&tool, &input_values);
        assert_eq!(requirements.len(), 1);
        assert!(matches!(
            &requirements[0],
            ProcessRequirements::EnvVarRequirement(_)
        ));

        //confirm input overwrites tool requirement. tool value is
        if let ProcessRequirements::EnvVarRequirement(r) = &requirements[0] {
            let vars = &r.env_def;
            let var = vars.iter().find(|v| v.env_name == "TEST_ENV").unwrap();
            assert_eq!(var.env_value, "conflict_user_override");
        } else {
            panic!("Expected EnvVarRequirement");
        }
    }
}
