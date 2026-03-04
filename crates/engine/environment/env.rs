use crate::{
    expression::{EvaluationContext, do_eval},
    request::InputObject,
};
use anyhow::Context;
use cwl_core::requirements::EnvVarRequirement;
use indexmap::IndexMap;

pub(crate) fn build_environment(input_values: &InputObject) -> IndexMap<String, String> {
    if let Some(env) = input_values.get_requirement_or_hint::<EnvVarRequirement>() {
        env.clone().to_map().into_iter().collect()
    } else {
        IndexMap::new()
    }
}

pub(crate) fn handle_environment(
    input_env: IndexMap<String, String>,
    evr: Option<&EnvVarRequirement>,
    eval_context: &EvaluationContext,
) -> anyhow::Result<IndexMap<String, String>> {
    let mut environment = input_env;

    for value in &mut environment.values_mut() {
        *value = eval_as_string(value, eval_context)?;
    }

    if let Some(evr) = evr {
        for item in &evr.env_def {
            //do not overwrite input requirements
            if !environment.contains_key(&item.env_name) {
                let parsed_value = eval_as_string(&item.env_value, eval_context)?;
                environment.insert(item.env_name.clone(), parsed_value.to_string());
            }
        }
    }

    Ok(environment)
}

fn eval_as_string(value: &str, context: &EvaluationContext) -> anyhow::Result<String> {
    if let Ok(value) = do_eval(value, context) {
        value
            .as_str()
            .context("Expected string value")
            .map(ToString::to_string)
    } else {
        Ok(value.to_string())
    }
}
