use crate::input::InputObject;
use cwl_core::requirements::EnvVarRequirement;
use indexmap::IndexMap;

pub fn build_environment(input_values: &InputObject) -> IndexMap<String, String> {
    if let Some(env) = input_values.get_requirement_or_hint::<EnvVarRequirement>() {
        env.clone().to_map().into_iter().collect()
    } else {
        IndexMap::new()
    }
}
