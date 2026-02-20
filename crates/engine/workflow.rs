use std::collections::HashMap;

use cwl_core::{
    documents::WorkflowStep,
    inputs::{DefaultValue, WorkflowStepInput},
    outputs::LinkMergeMethod,
    requirements::MultipleInputFeatureRequirement,
};

use crate::request::InputObject;

pub fn build_step_input_object(
    step: &WorkflowStep,
    completed_outputs: &HashMap<String, DefaultValue>,
    mir: Option<&MultipleInputFeatureRequirement>,
) -> anyhow::Result<InputObject> {
    let step_inputs = collect_workflow_step_inputs(completed_outputs, &step.r#in, mir)?;
    let inputs = InputObject {
        inputs: step_inputs,
        requirements: step
            .requirements
            .clone()
            .unwrap_or_default()
            .into_iter()
            .map(Into::into)
            .collect(),
        hints: step
            .hints
            .clone()
            .unwrap_or_default()
            .into_iter()
            .map(Into::into)
            .collect(),
    };

    Ok(inputs)
}

fn collect_workflow_step_inputs(
    completed_outputs: &HashMap<String, DefaultValue>,
    step_inputs: &Vec<WorkflowStepInput>,
    mir: Option<&MultipleInputFeatureRequirement>,
) -> anyhow::Result<HashMap<String, serde_yaml::Value>> {
    let mut inputs = HashMap::new();

    for workflow_step_input in step_inputs {
        let step_id = workflow_step_input.id.as_ref().unwrap();
        if let Some(sources) = &workflow_step_input.source {
            //handle multiple input feature requirement
            if mir.is_some() {
                let mut data = vec![];
                for s in sources.as_many() {
                    let val = completed_outputs.get(&s);

                    //handle link merge
                    if let Some(val) = val {
                        match workflow_step_input.link_merge {
                            None | Some(LinkMergeMethod::MergeNested) => {
                                data.push(val.clone());
                            }
                            Some(LinkMergeMethod::MergeFlattened) => {
                                if let DefaultValue::Any(serde_yaml::Value::Sequence(arr)) = &val {
                                    let dv_arr = arr
                                        .iter()
                                        .filter_map(|i| {
                                            serde_yaml::from_value::<DefaultValue>(i.clone()).ok()
                                        })
                                        .collect::<Vec<_>>();
                                    data.extend(dv_arr);
                                } else {
                                    anyhow::bail!("Input needs to be of type array: {s}")
                                }
                            }
                        }
                    } else {
                        anyhow::bail!("Could not find input {s}")
                    }
                }
                let yaml_value = serde_yaml::to_value(data)?;
                inputs.insert(step_id.clone(), yaml_value);
            } else {
                //no multiple feature input requirement branch
                for source in &sources.as_many() {
                    if let Some(value) = completed_outputs.get(source) {
                        let yaml_value = serde_yaml::to_value(value)?;
                        let yaml_value = match workflow_step_input.link_merge {
                            None | Some(LinkMergeMethod::MergeFlattened) => yaml_value,
                            Some(LinkMergeMethod::MergeNested) => {
                                serde_yaml::Value::Sequence(vec![yaml_value])
                            }
                        };
                        inputs.insert(step_id.clone(), yaml_value);
                    }
                }
            }
        }

        if let Some(default) = &workflow_step_input.default
            && (!inputs.contains_key(step_id)
                || matches!(inputs.get(step_id), Some(serde_yaml::Value::Null)))
        {
            //use step default
            let yaml_value = serde_yaml::to_value(default)?;
            inputs.insert(step_id.clone(), yaml_value);
        }
    }

    Ok(inputs)
}
