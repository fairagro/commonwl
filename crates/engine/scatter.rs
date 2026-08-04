use cwl_core::{documents::ScatterMethod, inputs::DefaultValue};
use std::collections::HashMap;

use crate::request::InputObject;

pub(crate) fn gather_jobs(
    scatter_inputs: &[Vec<serde_json::Value>],
    scatter_keys: &[String],
    method: ScatterMethod,
) -> crate::Result<Vec<HashMap<String, serde_json::Value>>> {
    match method {
        ScatterMethod::Dotproduct => {
            //return empty if no scatter
            if scatter_inputs.is_empty() {
                return Ok(vec![]);
            }

            let len = scatter_inputs[0].len();
            if scatter_inputs.iter().any(|arr| arr.len() != len) {
                return Err(crate::RunnerError::Guard(
                    "All scatter inputs must be the same length for dotproduct.".to_string(),
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
                values: &[Vec<serde_json::Value>],
                index: usize,
                current: &mut HashMap<String, serde_json::Value>,
                jobs: &mut Vec<HashMap<String, serde_json::Value>>,
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

pub(crate) fn gather_inputs(
    scatter_keys: &[String],
    input_values: &InputObject,
) -> crate::Result<Vec<Vec<serde_json::Value>>> {
    scatter_keys
        .iter()
        .map(|k| {
            input_values
                .inputs
                .get(k)
                .and_then(|v| match v {
                    DefaultValue::Any(serde_json::Value::Array(arr)) => Some(arr.clone()),
                    _ => None,
                })
                .ok_or_else(|| {
                    crate::RunnerError::Guard(format!(
                        "Input {k} must be of type array to scatter!"
                    ))
                })
        })
        .collect::<crate::Result<Vec<Vec<serde_json::Value>>>>()
}

pub(crate) fn nest_results(flat: &[serde_json::Value], dims: &[usize]) -> serde_json::Value {
    fn chunk(flat: &[serde_json::Value], dims: &[usize]) -> serde_json::Value {
        if dims.len() == 1 {
            serde_json::Value::Array(flat.to_vec())
        } else {
            let inner_size: usize = dims[1..].iter().product();
            let chunks = flat
                .chunks(inner_size)
                .map(|c| chunk(c, &dims[1..]))
                .collect();
            serde_json::Value::Array(chunks)
        }
    }
    chunk(flat, dims)
}

pub(crate) fn empty(method: ScatterMethod, dims: &[usize]) -> serde_json::Value {
    match method {
        ScatterMethod::Dotproduct | ScatterMethod::FlatCrossproduct => {
            serde_json::Value::Array(vec![])
        }
        ScatterMethod::NestedCrossproduct => {
            fn build_empty(dims: &[usize]) -> serde_json::Value {
                if dims.len() == 1 {
                    serde_json::Value::Array(vec![])
                } else {
                    let inner = (0..dims[0]).map(|_| build_empty(&dims[1..])).collect();
                    serde_json::Value::Array(inner)
                }
            }
            build_empty(dims)
        }
    }
}
