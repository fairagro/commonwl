use crate::{
    command::to_str,
    io::{directory::locate_dir, file::locate_file},
    schema::format_validation::FormatValidator,
    schema::validation::{validate_command_input, validate_type},
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
    inputs: &HashMap<String, DefaultValue>,
    work_dir: &Path,
    stage_dir: &Path,
    llr: Option<&LoadListingRequirement>,
    fv: Option<&FormatValidator>,
) -> anyhow::Result<HashMap<String, DefaultValue>> {
    let mut values = HashMap::new();
    for input in &doc.get_inputs() {
        // collect the actual value
        let mut value = get_input_value(input, inputs);
        let format = input.format.as_ref().map(OneOrMany::as_one);
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
                OneOrMany::One(item) => validate_type(&item.clone().into(), &value, format, fv),
                OneOrMany::Many(items) => items
                    .iter()
                    .any(|i| validate_type(&i.clone().into(), &value, format, fv)),
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
        let subdir = format!(
            "{}-stg{}",
            input.id.as_ref().unwrap(),
            &uuid::Uuid::new_v4().to_string()[..8]
        );
        let stage_dir = stage_dir.join(subdir);
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
        DefaultValue::Any(serde_json::Value::Array(vec)) => {
            for item in vec {
                let mut dv = serde_json::from_value(item.clone())?;
                let stage_dir =
                    stage_dir.join(format!("stg-{}", &uuid::Uuid::new_v4().to_string()[..8]));
                load_input(&mut dv, work_dir, &stage_dir, load_listing, load_contents)?;
                *item = serde_json::to_value(&dv)?;
            }
        }
        DefaultValue::Any(serde_json::Value::Object(map)) => {
            for item in map.values_mut() {
                let mut dv = serde_json::from_value(item.clone())?;
                load_input(&mut dv, work_dir, stage_dir, None, false)?;
                *item = serde_json::to_value(&dv)?;
            }
        }
        #[allow(clippy::cast_possible_truncation)]
        DefaultValue::Any(serde_json::Value::Number(n)) => {
            //the spec wants floats with .0 to be represented as integer
            const MAX_I64_AS_FLOAT: f64 = 9_223_372_036_854_775_808.0;
            const MIN_I64_AS_FLOAT: f64 = -9_223_372_036_854_775_808.0;
            if let Some(f) = n.as_f64()
                && f.is_finite()
                && f.fract() == 0.0
                && (MIN_I64_AS_FLOAT..MAX_I64_AS_FLOAT).contains(&f)
            {
                *value = DefaultValue::Any(serde_json::Value::Number(serde_json::Number::from(
                    //guarded!
                    f as i64,
                )));
            }
        }
        _ => {}
    }
    Ok(())
}

pub(crate) fn get_input_value(
    input: &OperationInputParameter,
    inputs: &HashMap<String, DefaultValue>,
) -> DefaultValue {
    let value = inputs.get(&input.id.clone().unwrap_or_default());

    if let Some(value) = value
        && !value.is_null()
    {
        value.clone()
    } else if let Some(default) = &input.default {
        default.clone()
    } else {
        DefaultValue::Any(serde_json::Value::Null)
    }
}

pub(crate) fn get_stdin(
    tool: &CommandLineTool,
    inputs: &HashMap<String, DefaultValue>,
) -> Option<String> {
    if let Some(stdin) = &tool.stdin {
        return Some(stdin.clone());
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
#[must_use]
pub fn flatten_inputs(inputs: &HashMap<String, DefaultValue>) -> Vec<FileOrDirectory> {
    let mut flattened = vec![];
    for input in inputs.values() {
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
            serde_json::Value::Array(values) => {
                for v in values {
                    if let Ok(dv) = serde_json::from_value(v.clone()) {
                        flatten_inputs_impl(&dv, flattened);
                    }
                }
            }
            serde_json::Value::Object(mapping) => {
                for v in mapping.values() {
                    if let Ok(dv) = serde_json::from_value(v.clone()) {
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
    use super::*;
    use crate::request::load_input_file_from_file;
    use cwl_core::load_cwl_file;

    #[test]
    #[cfg(not(target_os = "windows"))] //cwl submodule is not available on windows
    fn test_collect_inputs() {
        use cwl_core::documents::CommandLineTool;
        use std::collections::HashMap;

        let tool: CommandLineTool = serde_saphyr::from_str(include_str!(
            "../../testdata/cwl/tests/anon_enum_inside_array.cwl"
        ))
        .unwrap();
        let inputs_values: HashMap<String, DefaultValue> = serde_saphyr::from_str(include_str!(
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
    fn strip_ids(val: &str) -> String {
        // Matches segments like "stg21d5f9d2" - adjust regex to your ID format
        let re = regex::Regex::new(r"-[a-z0-9]{8,}").unwrap();
        re.replace_all(val, "-<ID>").to_string()
    }

    #[test]
    fn test_get_stdin() {
        use std::path::MAIN_SEPARATOR_STR;

        let base_dir =
            dunce::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata"))
                .unwrap();
        let specification_path = base_dir.join("cat-tool-shortcut.cwl");
        let inputs_path = base_dir.join("cat-job.json");

        let inputs = load_input_file_from_file(&inputs_path, base_dir).unwrap();
        let doc = load_cwl_file(specification_path, false).unwrap();
        let inputs = collect_inputs(
            &doc,
            &inputs.inputs,
            Path::new("../../testdata"),
            Path::new("."),
            None,
            None,
        )
        .unwrap();

        let CWLDocument::CommandLineTool(tool) = doc else {
            panic!("Oh no!")
        };
        let stdin = get_stdin(&tool, &inputs).unwrap();
        assert_eq!(
            strip_ids(&stdin),
            format!(".{MAIN_SEPARATOR_STR}file1-<ID>{MAIN_SEPARATOR_STR}hello.txt")
        );
    }
}
