use cwl_core::{files::FileOrDirectory, inputs::DefaultValue};
use std::collections::HashMap;

pub fn create_flattened_inputs(
    inputs: &HashMap<String, DefaultValue>,
) -> anyhow::Result<Vec<FileOrDirectory>> {
    //handle synthethic directories
    Ok(flatten_inputs(inputs.values()))
}

//flattens inputs of any type to a list of file or directory
fn flatten_inputs<'a, I: Iterator<Item = &'a DefaultValue>>(inputs: I) -> Vec<FileOrDirectory> {
    let mut flattened = vec![];
    for input in inputs {
        flatten_inputs_impl(input, &mut flattened);
    }
    flattened
}

fn flatten_inputs_impl(dv: &DefaultValue, flattened: &mut Vec<FileOrDirectory>) {
    match dv {
        DefaultValue::FileOrDirectory(fod) => {
            flattened.push(fod.clone());
            if let FileOrDirectory::File(f) = fod
                && let Some(secondary_files) = &f.secondary_files
            {
                flattened.extend(secondary_files.clone());
            }
        }
        DefaultValue::Any(v) => match v {
            serde_yaml::Value::Sequence(values) => {
                for v in values {
                    if let Ok(dv) = serde_yaml::from_value(v.clone()) {
                        flatten_inputs_impl(&dv, flattened);
                    }
                }
            }
            serde_yaml::Value::Mapping(mapping) => {
                for v in mapping.values() {
                    if let Ok(dv) = serde_yaml::from_value(v.clone()) {
                        flatten_inputs_impl(&dv, flattened);
                    }
                }
            }
            _ => {}
        },
    }
}
