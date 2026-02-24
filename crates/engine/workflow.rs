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
        let mut value = if let Some(sources) = &workflow_step_input.source {
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

                serde_yaml::to_value(data)?
            } else {
                //no multiple feature input requirement branch
                let source = sources.as_one();
                if let Some(value) = completed_outputs.get(source) {
                    let yaml_value = serde_yaml::to_value(value)?;

                    match workflow_step_input.link_merge {
                        None | Some(LinkMergeMethod::MergeFlattened) => yaml_value,
                        Some(LinkMergeMethod::MergeNested) => {
                            serde_yaml::Value::Sequence(vec![yaml_value])
                        }
                    }
                } else {
                    serde_yaml::Value::Null
                }
            }
        } else {
            serde_yaml::Value::Null
        };

        //handle input defaults
        if let Some(default) = &workflow_step_input.default
            && value == serde_yaml::Value::Null
        {
            //use step default
            value = serde_yaml::to_value(default)?;
        }

        //handle load_contets
        if let Some(load_contents) = &workflow_step_input.load_contents
            && *load_contents
        {
            let mut dv: DefaultValue = serde_yaml::from_value(value)?;
            load_file_contents(&mut dv)?;
            value = serde_yaml::to_value(dv)?;
        }

        //handle value_from
        if let Some(value_from) = &workflow_step_input.value_from {
            if let Some(scatter) = &step.scatter
                && scatter.as_many().contains(step_input_id)
            {
                //item is array, get it!
                let serde_yaml::Value::Sequence(vals) = &value else {
                    anyhow::bail!("Expected array for scattered input")
                };
                //apply value from
                let transformed = vals
                    .iter()
                    .map(|v| {
                        let current_value = serde_json::to_value(v)?;
                        let eval_context = eval_context.clone().with_context(&current_value);
                        do_eval(value_from, &eval_context)
                    })
                    .collect::<anyhow::Result<Vec<_>>>()?;
                value = serde_yaml::to_value(transformed)?
            } else {
                let current_value = serde_json::to_value(value)?;
                let eval_context = eval_context.clone().with_context(&current_value);
                value = do_eval(value_from, &eval_context)?;
            }
        }

        inputs.insert(step_input_id.to_string(), value);
    }

    Ok(inputs)
}
