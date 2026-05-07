use crate::{
    expression::{EvaluationContext, do_eval},
    io::load_file_contents,
    request::InputObject,
};
use cwl_core::{
    documents::WorkflowStep,
    inputs::DefaultValue,
    outputs::{LinkMergeMethod, PickValueMethod},
    requirements::MultipleInputFeatureRequirement,
};
use std::collections::HashMap;

pub(crate) fn collect_raw_inputs(
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
                let resolved = sources
                    .iter()
                    .map(|s| {
                        completed_outputs
                            .get(s)
                            .cloned()
                            .ok_or_else(|| anyhow::anyhow!("Could not find input {s}"))
                    })
                    .collect::<anyhow::Result<Vec<_>>>()?;
                let link_merge = workflow_step_input
                    .link_merge
                    .unwrap_or(LinkMergeMethod::MergeNested);
                let data = handle_link_merge(link_merge, resolved)?;
                //according to spec :
                //If both linkMerge and pickValue are null or not specified, and there is only a single element in the source,
                //then the input parameter takes the scalar value from the single input link (it is not wrapped in a single-list).
                if workflow_step_input.link_merge.is_none()
                    && workflow_step_input.pick_value.is_none()
                    && data.len() == 1
                {
                    data[0].clone()
                } else {
                    //sequence
                    DefaultValue::Any(serde_json::to_value(data)?)
                }
            } else {
                //no multiple feature input requirement branch
                let source = sources.as_one();
                if let Some(value) = completed_outputs.get(source) {
                    match workflow_step_input.link_merge {
                        None | Some(LinkMergeMethod::MergeFlattened) => value.clone(),
                        Some(LinkMergeMethod::MergeNested) => {
                            let yaml_value = serde_json::to_value(value)?;
                            DefaultValue::Any(serde_json::Value::Array(vec![yaml_value]))
                        }
                    }
                } else {
                    DefaultValue::Any(serde_json::Value::Null)
                }
            }
        } else {
            DefaultValue::Any(serde_json::Value::Null)
        };

        //handle input defaults
        if let Some(default) = &workflow_step_input.default
            && value == DefaultValue::Any(serde_json::Value::Null)
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

        inputs.insert(step_input_id.clone(), value);
    }

    Ok(inputs)
}

pub(crate) fn eval_inputs(
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
            .unwrap_or(DefaultValue::Any(serde_json::Value::Null));
        //handle value_from
        if let Some(value_from) = &workflow_step_input.value_from {
            let current_value = serde_json::to_value(value)?;
            let eval_context = eval_context
                .clone()
                .with_context(&current_value)
                .with_inputs(inputs);
            value = serde_json::from_value(do_eval(value_from, &eval_context)?)?;
        }
        transformed.insert(step_input_id.clone(), value);
    }
    Ok(InputObject {
        inputs: transformed,
        ..raw_inputs
    })
}

pub(crate) fn handle_link_merge(
    link_merge: LinkMergeMethod,
    values: Vec<DefaultValue>,
) -> anyhow::Result<Vec<DefaultValue>> {
    match link_merge {
        LinkMergeMethod::MergeNested => Ok(values),
        LinkMergeMethod::MergeFlattened => {
            let mut flattened = vec![];
            for value in values {
                match value {
                    DefaultValue::Any(serde_json::Value::Null) => {}
                    DefaultValue::Any(serde_json::Value::Array(arr)) => {
                        for item in arr {
                            flattened.push(serde_json::from_value(item)?);
                        }
                    }
                    other => flattened.push(other),
                }
            }
            Ok(flattened)
        }
    }
}

pub(crate) fn handle_pick_value(
    output_id: &str,
    pick_value: PickValueMethod,
    values: Vec<DefaultValue>,
) -> anyhow::Result<DefaultValue> {
    match pick_value {
        PickValueMethod::AllNonNull => {
            let filtered = values.into_iter().filter(|v| !v.is_null()).collect::<Vec<_>>();
            Ok(DefaultValue::Any(serde_json::to_value(filtered)?))
        }
        PickValueMethod::FirstNonNull => {
            values
                .into_iter()
                .find(|v| !v.is_null())
                .ok_or_else(|| anyhow::anyhow!(
                    "No output for {output_id}, pick value `FirstNonNull` requires at least one non-null value"
                ))
        }
        PickValueMethod::TheOnlyNonNull => {
            let non_null: Vec<_> = values.into_iter().filter(|v| !v.is_null()).collect();
            if non_null.len() != 1 {
                anyhow::bail!(
                    "Output {output_id}: pick value `TheOnlyNonNull` requires exactly one non-null value, found {}",
                    non_null.len()
                );
            }
            Ok(non_null.into_iter().next().unwrap())
        }
    }
}
