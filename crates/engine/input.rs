use crate::{
    command::to_str,
    io::{directory::locate_dir, file::locate_file},
    schema::format_validation::FormatValidator,
    schema::validation::{validate_command_input, validate_input_type},
};
use cwl_core::{
    OneOrMany,
    documents::{CWLDocument, CommandLineTool},
    files::{FileOrDirectory, LoadListingEnum},
    inputs::{CommandInputParameterType, DefaultValue, OperationInputParameter},
    requirements::LoadListingRequirement,
};
use std::{collections::HashMap, path::Path};

pub fn collect_inputs(
    doc: &CWLDocument,
    inputs: &HashMap<String, serde_yaml::Value>,
    work_dir: &Path,
    stage_dir: &Path,
    llr: Option<&LoadListingRequirement>,
    fv: Option<&FormatValidator>,
) -> anyhow::Result<HashMap<String, DefaultValue>> {
    let mut values = HashMap::new();
    for input in &doc.get_inputs() {
        // collect the actual value
        let mut value = get_input_value(input, inputs)?;
        let format = input.format.as_ref().map(|f| f.as_one());
        //do some validation
        let valid = match doc {
            CWLDocument::CommandLineTool(clt) => {
                //can have stdin...
                let Some(command_input) = clt.inputs.iter().find(|i| i.id == input.id) else {
                    anyhow::bail!(
                        "Could not find input `{}`",
                        input.id.clone().unwrap_or_default()
                    )
                };
                validate_command_input(&command_input.r#type, &value, format, fv)
            }

            _ => match &input.r#type {
                OneOrMany::One(item) => {
                    validate_input_type(&item.clone().into(), &value, format, fv)
                }
                OneOrMany::Many(items) => items
                    .iter()
                    .any(|i| validate_input_type(&i.clone().into(), &value, format, fv)),
            },
        };
        //error if validity can not be confirmed
        if !valid {
            anyhow::bail!(
                "Value {value:?} is not valid for `{}`, expected {:?}",
                input.id.clone().unwrap_or_default(),
                input.r#type
            )
        }
        let load_listing = input.load_listing.or_else(|| llr.map(|r| r.load_listing));
        let load_contents = input.load_contents.unwrap_or_default();
        let stage_dir = stage_dir.join(input.id.as_ref().unwrap());
        load_input(
            &mut value,
            work_dir,
            &stage_dir,
            load_listing,
            load_contents,
        )?;
        values.insert(input.id.clone().unwrap_or_default(), value);
    }

    Ok(values)
}

fn load_input(
    value: &mut DefaultValue,
    work_dir: &Path,
    stage_dir: &Path,
    load_listing: Option<LoadListingEnum>,
    load_contents: bool,
) -> anyhow::Result<()> {
    match value {
        DefaultValue::FileOrDirectory(FileOrDirectory::File(file)) => {
            locate_file(file, work_dir, stage_dir, load_contents)?;
        }
        DefaultValue::FileOrDirectory(FileOrDirectory::Directory(dir)) => {
            locate_dir(dir, work_dir, stage_dir, load_listing)?;
        }
        DefaultValue::Any(serde_yaml::Value::Sequence(vec)) => {
            for item in vec {
                let mut dv = serde_yaml::from_value(item.clone())?;
                load_input(&mut dv, work_dir, stage_dir, load_listing, load_contents)?;
                *item = serde_yaml::to_value(&dv)?;
            }
        }
        DefaultValue::Any(serde_yaml::Value::Mapping(map)) => {
            for item in map.values_mut() {
                let mut dv = serde_yaml::from_value(item.clone())?;
                load_input(&mut dv, work_dir, stage_dir, None, false)?;
                *item = serde_yaml::to_value(&dv)?;
            }
        }
        _ => {}
    }
    Ok(())
}

pub(crate) fn get_input_value(
    input: &OperationInputParameter,
    inputs: &HashMap<String, serde_yaml::Value>,
) -> anyhow::Result<DefaultValue> {
    let value = inputs.get(&input.id.clone().unwrap_or_default());
    Ok(
        if let Some(value) = value
            && !value.is_null()
        {
            serde_yaml::from_value::<DefaultValue>(value.clone())?
        } else if let Some(default) = &input.default {
            default.clone()
        } else {
            DefaultValue::Any(serde_yaml::Value::Null)
        },
    )
}

pub fn get_stdin(tool: &CommandLineTool, inputs: &HashMap<String, DefaultValue>) -> Option<String> {
    if let Some(stdin) = &tool.stdin {
        return Some(stdin.to_string());
    }

    if let Some(input) = tool
        .inputs
        .iter()
        .find(|i| matches!(i.r#type, CommandInputParameterType::Stdin))
    {
        return inputs
            .get(&input.id.clone().unwrap_or_default())
            .map(to_str);
    }
    None
}

//flattens inputs of any type to a list of file or directory
pub fn flatten_inputs(
    inputs: &HashMap<String, DefaultValue>,
) -> anyhow::Result<Vec<FileOrDirectory>> {
    let mut flattened = vec![];
    for input in inputs.values() {
        flatten_inputs_impl(input, &mut flattened);
    }
    Ok(flattened)
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

#[cfg(test)]
mod tests {
    use crate::request::load_input_file_from_file;

    use super::*;
    use cwl_core::{documents::CommandLineTool, load_cwl_file};
    use std::collections::HashMap;

    #[test]
    fn test_collect_inputs() {
        let tool: CommandLineTool = serde_yaml::from_str(include_str!(
            "../../testdata/cwl/tests/anon_enum_inside_array.cwl"
        ))
        .unwrap();
        let inputs_values: HashMap<String, serde_yaml::Value> = serde_yaml::from_str(include_str!(
            "../../testdata/cwl/tests/anon_enum_inside_array.yml"
        ))
        .unwrap();

        let inputs = collect_inputs(
            &CWLDocument::CommandLineTool(tool),
            &inputs_values,
            Path::new("../../testdata/cwl/tests"),
            Path::new("."),
            None,
            None,
        );
        assert!(inputs.is_ok());

        assert_eq!(inputs.unwrap().len(), 2);
    }

    #[test]
    fn test_get_stdin() {
        let base_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../testdata/cwl/tests")
            .canonicalize()
            .unwrap();
        let specification_path = base_dir.join("cat-tool-shortcut.cwl");
        let inputs_path = base_dir.join("cat-job.json");

        let inputs = load_input_file_from_file(&inputs_path, base_dir).unwrap();
        let doc = load_cwl_file(specification_path, false).unwrap();
        let inputs = collect_inputs(
            &doc,
            &inputs.inputs,
            Path::new("../../testdata/cwl/tests"),
            Path::new("."),
            None,
            None,
        )
        .unwrap();

        let CWLDocument::CommandLineTool(tool) = doc else {
            panic!("Oh no!")
        };
        let stdin = get_stdin(&tool, &inputs);
        assert_eq!(stdin, Some("./file1/hello.txt".into()));
    }
}
