use cwl_core::documents::ScatterMethod;
use std::collections::HashMap;

use crate::request::InputObject;

pub fn gather_jobs(
    scatter_inputs: &[Vec<serde_yaml::Value>],
    scatter_keys: &[String],
    method: &ScatterMethod,
) -> anyhow::Result<Vec<HashMap<String, serde_yaml::Value>>> {
    match method {
        ScatterMethod::Dotproduct => {
            let len = scatter_inputs[0].len();
            if scatter_inputs.iter().any(|arr| arr.len() != len) {
                return Err(anyhow::anyhow!(
                    "All scatter inputs must be the same length for dotproduct."
                ));
            }

            let jobs = (0..len)
                .map(|i| {
                    scatter_keys
                        .iter()
                        .cloned()
                        .zip(scatter_inputs.iter().map(|arr| arr[i].clone()))
                        .collect::<HashMap<_, _>>()
                })
                .collect::<Vec<_>>();
            Ok(jobs)
        }
        // a little Chad Gippity was used to get what the Docu was sayin' about the Flat CP
        ScatterMethod::FlatCrossproduct => {
            let mut jobs = vec![HashMap::new()];
            for (key, values) in scatter_keys.iter().zip(scatter_inputs.iter()) {
                let mut new_jobs = Vec::new();
                for job in &jobs {
                    for value in values {
                        let mut new_job = job.clone();
                        new_job.insert(key.clone(), value.clone());
                        new_jobs.push(new_job);
                    }
                }
                jobs = new_jobs;
            }
            Ok(jobs)
        }
        ScatterMethod::NestedCrossproduct => {
            fn nest(
                keys: &[String],
                values: &[Vec<serde_yaml::Value>],
                index: usize,
                current: &mut HashMap<String, serde_yaml::Value>,
                jobs: &mut Vec<HashMap<String, serde_yaml::Value>>,
            ) {
                if index == keys.len() {
                    jobs.push(current.clone());
                } else {
                    for v in &values[index] {
                        current.insert(keys[index].clone(), v.clone());
                        nest(keys, values, index + 1, current, jobs);
                    }
                }
            }

            let mut jobs = vec![];
            let mut current = HashMap::new();
            nest(scatter_keys, scatter_inputs, 0, &mut current, &mut jobs);
            Ok(jobs)
        }
    }
}

pub fn gather_inputs(
    scatter_keys: &[String],
    input_values: &InputObject,
) -> anyhow::Result<Vec<Vec<serde_yaml::Value>>> {
    scatter_keys
        .iter()
        .map(|k| {
            input_values
                .inputs
                .get(k)
                .and_then(|v| match v {
                    serde_yaml::Value::Sequence(arr) => Some(arr.clone()),
                    _ => None,
                })
                .ok_or_else(|| anyhow::anyhow!("Input {k} must be of type array to scatter!"))
        })
        .collect::<anyhow::Result<Vec<Vec<serde_yaml::Value>>>>()
}
