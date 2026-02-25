use crate::{
    expression::{EvaluationContext, do_eval},
    io::load_file_contents,
    request::InputObject,
};
use cwl_core::{
    documents::WorkflowStep, inputs::DefaultValue, outputs::LinkMergeMethod,
    requirements::MultipleInputFeatureRequirement,
};
use std::collections::HashMap;

pub fn collect_raw_inputs(
    step: &WorkflowStep,
    completed_outputs: &HashMap<String, DefaultValue>,
    mir: Option<&MultipleInputFeatureRequirement>,
) -> anyhow::Result<InputObject> {
    Ok(input_object(
        step,
        collect_workflow_step_inputs(completed_outputs, step, mir)?,
    ))
}

fn input_object(step: &WorkflowStep, inputs: HashMap<String, DefaultValue>) -> InputObject {
    InputObject {
        inputs,
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
    }
}

fn collect_workflow_step_inputs(
    completed_outputs: &HashMap<String, DefaultValue>,
    step: &WorkflowStep,
    mir: Option<&MultipleInputFeatureRequirement>,
) -> anyhow::Result<HashMap<String, DefaultValue>> {
    let mut inputs: HashMap<String, DefaultValue> = HashMap::with_capacity(step.r#in.len());

    for workflow_step_input in &step.r#in {
        let step_input_id = workflow_step_input.id.as_ref().unwrap();
        let mut value: DefaultValue = if let Some(sources) = &workflow_step_input.source {
            //handle multiple input feature requirement
            if mir.is_some() {
                let sources = sources.as_many();
                let mut data = Vec::with_capacity(sources.len());
                for s in sources {
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

                DefaultValue::Any(serde_yaml::to_value(data)?) //sequence
            } else {
                //no multiple feature input requirement branch
                let source = sources.as_one();
                if let Some(value) = completed_outputs.get(source) {
                    match workflow_step_input.link_merge {
                        None | Some(LinkMergeMethod::MergeFlattened) => value.clone(),
                        Some(LinkMergeMethod::MergeNested) => {
                            let yaml_value = serde_yaml::to_value(value)?;
                            DefaultValue::Any(serde_yaml::Value::Sequence(vec![yaml_value]))
                        }
                    }
                } else {
                    DefaultValue::Any(serde_yaml::Value::Null)
                }
            }
        } else {
            DefaultValue::Any(serde_yaml::Value::Null)
        };

        //handle input defaults
        if let Some(default) = &workflow_step_input.default
            && value == DefaultValue::Any(serde_yaml::Value::Null)
        {
            //use step default
            value = default.clone();
        }

        //handle load_contets
        if let Some(load_contents) = &workflow_step_input.load_contents
            && *load_contents
        {
            load_file_contents(&mut value)?;
        }

        inputs.insert(step_input_id.to_string(), value);
    }

    Ok(inputs)
}

pub fn eval_inputs(
    step: &WorkflowStep,
    raw_inputs: InputObject,
    eval_context: &EvaluationContext,
) -> anyhow::Result<InputObject> {
    let mut transformed = HashMap::with_capacity(step.r#in.len());
    let inputs = &raw_inputs.inputs;
    for workflow_step_input in &step.r#in {
        let step_input_id = workflow_step_input.id.as_ref().unwrap();
        let mut value = raw_inputs
            .inputs
            .get(step_input_id)
            .cloned()
            .unwrap_or(DefaultValue::Any(serde_yaml::Value::Null));
        //handle value_from
        if let Some(value_from) = &workflow_step_input.value_from {
            let current_value = serde_json::to_value(value)?;
            let eval_context = eval_context
                .clone()
                .with_context(&current_value)
                .with_inputs(inputs);
            value = serde_yaml::from_value(do_eval(value_from, &eval_context)?)?;
        }
        transformed.insert(step_input_id.clone(), value);
    }
    Ok(InputObject {
        inputs: transformed,
        ..raw_inputs
    })
}
