use std::collections::HashMap;

use cwl_core::{
    documents::WorkflowStep, inputs::DefaultValue, outputs::LinkMergeMethod,
    requirements::MultipleInputFeatureRequirement,
};

use crate::{
    expression::{EvaluationContext, do_eval},
    request::InputObject,
};

pub fn build_step_input_object(
    step: &WorkflowStep,
    completed_outputs: &HashMap<String, DefaultValue>,
    mir: Option<&MultipleInputFeatureRequirement>,
    eval_context: &EvaluationContext,
) -> anyhow::Result<InputObject> {
    let step_inputs = collect_workflow_step_inputs(completed_outputs, step, mir, eval_context)?;
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
    step: &WorkflowStep,
    mir: Option<&MultipleInputFeatureRequirement>,
    eval_context: &EvaluationContext,
) -> anyhow::Result<HashMap<String, serde_yaml::Value>> {
    let mut inputs = HashMap::new();

    for workflow_step_input in &step.r#in {
        let step_input_id = workflow_step_input.id.as_ref().unwrap();
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

                //handle value from
                if let Some(value_from) = &workflow_step_input.value_from {
                    for item in &mut data {
                        let current_value = serde_json::to_value(&item)?;
                        let eval_context = eval_context.clone().with_context(&current_value);
                        let value = do_eval(value_from, &eval_context)?;
                        *item = serde_yaml::from_value(value)?;
                    }
                }

                let yaml_value = serde_yaml::to_value(data)?;
                inputs.insert(step_input_id.clone(), yaml_value);
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

                        //handle value from
                        if let Some(value_from) = &workflow_step_input.value_from {
                            let value = if let Some(scatter) = &step.scatter
                                && scatter.as_many().contains(step_input_id)
                            {
                                //item is array, get it!
                                let serde_yaml::Value::Sequence(vals) = &yaml_value else {
                                    anyhow::bail!("Expected array for scattered input")
                                };
                                //apply value from
                                let transformed = vals
                                    .iter()
                                    .map(|v| {
                                        let current_value = serde_json::to_value(v)?;
                                        let eval_context =
                                            eval_context.clone().with_context(&current_value);
                                        do_eval(value_from, &eval_context)
                                    })
                                    .collect::<anyhow::Result<Vec<_>>>()?;
                                serde_yaml::to_value(transformed)?
                                //insert array converted back to value
                            } else {
                                let current_value = serde_json::to_value(&yaml_value)?;
                                let eval_context =
                                    eval_context.clone().with_context(&current_value);
                                do_eval(value_from, &eval_context)?
                            };
                            inputs.insert(step_input_id.clone(), value);
                        } else {
                            inputs.insert(step_input_id.clone(), yaml_value);
                        }
                    }
                }
            }
        }

        //handle input defaults
        if let Some(default) = &workflow_step_input.default
            && (!inputs.contains_key(step_input_id)
                || matches!(inputs.get(step_input_id), Some(serde_yaml::Value::Null)))
        {
            //use step default
            let yaml_value = serde_yaml::to_value(default)?;
            inputs.insert(step_input_id.clone(), yaml_value);
        }
    }

    Ok(inputs)
}
