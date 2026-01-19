use crate::input::load_input_file_from_file;
use anyhow::Ok;
use cwl_core::load_cwl_file;
use std::{collections::HashMap, env, path::Path};

pub mod command;
pub mod context;
pub mod input;
pub mod pathmapper;

pub fn load_execution_context(
    specification_path: impl AsRef<Path>,
    inputs_path: impl AsRef<Path>,
) -> anyhow::Result<()> {
    let working_dir = env::current_dir()?;
    let base_path = specification_path.as_ref().parent().unwrap_or(&working_dir);

    let inputs = load_input_file_from_file(inputs_path, base_path)?;
    load_execution_context_with_inputs(specification_path, inputs)
}

pub fn load_execution_context_with_inputs(
    specification_path: impl AsRef<Path>,
    inputs: HashMap<String, serde_yaml::Value>,
) -> anyhow::Result<()> {
    let doc = load_cwl_file(&specification_path, true)?;

    //ToDo: build context

    Ok(())
}
