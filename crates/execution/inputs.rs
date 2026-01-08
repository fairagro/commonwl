use std::collections::HashMap;

use cwl_core::{
    documents::CWLDocument,
    inputs::{DefaultValue, InputDataProvider},
};

pub fn collect_inputs(
    doc: &CWLDocument,
    inputs: &HashMap<String, serde_yaml::Value>,
) -> anyhow::Result<HashMap<String, DefaultValue>> {
    let mut values = HashMap::new();
    for input in doc.get_input_data_providers() {
        values.insert(
            input.id().clone().unwrap_or_default(),
            get_input_value(input, inputs)?,
        );
    }
    Ok(values)
}

pub(crate) fn get_input_value(
    input: &dyn InputDataProvider,
    inputs: &HashMap<String, serde_yaml::Value>,
) -> anyhow::Result<DefaultValue> {
    let value = inputs.get(&input.id().clone().unwrap_or_default());
    Ok(
        if let Some(value) = value
            && !value.is_null()
        {
            serde_yaml::from_value::<DefaultValue>(value.clone())?
        } else if let Some(default) = input.default() {
            default.clone()
        } else {
            DefaultValue::Any(serde_yaml::Value::Null)
        },
    )
}
