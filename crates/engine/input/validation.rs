use cwl_core::{
    OneOrMany,
    files::FileOrDirectory,
    inputs::{
        CommandInputParameterType, DefaultValue, InputArraySchema, InputEnumSchema,
        InputRecordSchema, InputSchema, InputType,
    },
    types::CWLType,
};
use tracing::error;

use crate::format::FormatValidator;

pub fn validate_command_input(
    schema: &CommandInputParameterType,
    value: &DefaultValue,
    format: Option<&String>,
    fv: Option<&FormatValidator>,
) -> bool {
    match schema {
        CommandInputParameterType::Stdin => !value.is_null(), // for stdin we accept any existing value
        CommandInputParameterType::CommandInputType(one_or_many) => match one_or_many {
            OneOrMany::One(item) => validate_input_type(&item.clone().into(), value, format, fv),
            OneOrMany::Many(items) => items
                .iter()
                .any(|i| validate_input_type(&i.clone().into(), value, format, fv)),
        },
    }
}

pub fn validate_input_type(
    r#type: &InputType,
    value: &DefaultValue,
    format: Option<&String>,
    fv: Option<&FormatValidator>,
) -> bool {
    match r#type {
        InputType::CWLType(ty) => validate_cwl_type(*ty, value, format, fv),
        InputType::InputSchema(schema) => validate_schema(schema, value, format, fv),
        InputType::String(_) => {
            if let Some(val) = value.try_get_value_ref() {
                val.is_string()
            } else {
                false
            }
        }
    }
}

fn validate_cwl_type(
    r#type: CWLType,
    value: &DefaultValue,
    format: Option<&String>,
    fv: Option<&FormatValidator>,
) -> bool {
    match value {
        DefaultValue::FileOrDirectory(fod) => match r#type {
            CWLType::File => {
                if fod.is_file() {
                    if let FileOrDirectory::File(file) = &fod
                        && let Some(file_format) = &file.format
                        && let Some(fv) = fv
                    {
                        let expected_resolved = fv.handle(format, None);
                        let actual_resolved = fv.handle(Some(file_format), None);
                        if let Some(actual_format) = actual_resolved
                            && let Some(expected_format) = expected_resolved
                            && !fv.validate(&actual_format, &expected_format)
                        {
                            error!(
                                "Format could not be validated: {actual_format} vs. {expected_format}"
                            );
                            return false;
                        }
                    }
                    true
                } else {
                    false
                }
            }
            CWLType::Directory => fod.is_dir(),
            CWLType::Any => true,
            _ => false,
        },
        DefaultValue::Any(value) => match r#type {
            CWLType::Null => value.is_null(),
            CWLType::Boolean => value.is_bool(),
            CWLType::Int => value.is_i64(),
            CWLType::Long => value.is_i64(),
            CWLType::Float => value.is_f64(),
            CWLType::Double => value.is_f64(),
            CWLType::String => value.is_string(),
            CWLType::Any => true,
            _ => false,
        },
    }
}

fn validate_schema(
    schema: &InputSchema,
    value: &DefaultValue,
    format: Option<&String>,
    fv: Option<&FormatValidator>,
) -> bool {
    match schema {
        InputSchema::Record(rec) => validate_record_schema(rec, value, fv),
        InputSchema::Enum(enu) => validate_enum_schema(enu, value),
        InputSchema::Array(arr) => validate_array_schema(arr, value, format, fv),
    }
}

fn validate_record_schema(
    schema: &InputRecordSchema,
    value: &DefaultValue,
    fv: Option<&FormatValidator>,
) -> bool {
    let mapping = match value {
        DefaultValue::Any(serde_yaml::Value::Mapping(map)) => map,
        _ => return false,
    };

    if let Some(fields) = &schema.fields {
        return fields.iter().all(|f| {
            let key = serde_yaml::Value::String(f.name.clone());
            if let Some(field_value) = mapping.get(&key) {
                match &f.r#type {
                    OneOrMany::One(item) => validate_input_type(
                        item,
                        &serde_yaml::from_value(field_value.clone())
                            .expect("DefaultValue violates itself"),
                        f.format.as_ref().map(|f| f.as_one()),
                        fv,
                    ),
                    OneOrMany::Many(items) => items.iter().any(|item| {
                        validate_input_type(
                            item,
                            &serde_yaml::from_value(field_value.clone())
                                .expect("DefaultValue violates itself"),
                            f.format.as_ref().map(|f| f.as_one()),
                            fv,
                        )
                    }),
                }
            } else {
                // Field is missing - check if it's optional (has null in union type)
                match &f.r#type {
                    OneOrMany::One(InputType::CWLType(CWLType::Null)) => true,
                    OneOrMany::Many(items) => items
                        .iter()
                        .any(|item| matches!(item, InputType::CWLType(CWLType::Null))),
                    _ => false,
                }
            }
        });
    }
    false
}

fn validate_array_schema(
    schema: &InputArraySchema,
    value: &DefaultValue,
    format: Option<&String>,
    fv: Option<&FormatValidator>,
) -> bool {
    //check whether we have a sequence!
    if let DefaultValue::Any(serde_yaml::Value::Sequence(seq)) = value {
        seq.iter().all(|item| {
            let item_value: DefaultValue =
                serde_yaml::from_value(item.clone()).expect("DefaultValue violates itself");
            match &schema.items {
                OneOrMany::One(t) => validate_input_type(&t.clone(), &item_value, format, fv),
                OneOrMany::Many(ts) => ts
                    .iter()
                    .any(|t| validate_input_type(&t.clone(), &item_value, format, fv)),
            }
        })
    } else {
        false
    }
}

fn validate_enum_schema(schema: &InputEnumSchema, value: &DefaultValue) -> bool {
    if let DefaultValue::Any(val) = value {
        if let Some(s) = val.as_str() {
            schema.symbols.contains(&s.to_string())
        } else {
            false
        }
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    use cwl_core::{documents::CommandLineTool, files::File};
    use std::collections::HashMap;

    use super::*;
    #[test]
    fn test_input_validation_complex() {
        let tool: CommandLineTool =
            serde_yaml::from_str(include_str!("../../../testdata/cwl/tests/binding-test.cwl"))
                .unwrap();
        let mut inputs_values: HashMap<String, DefaultValue> =
            serde_yaml::from_str(include_str!("../../../testdata/cwl/tests/bwa-mem-job.json"))
                .unwrap();

        //append the default value as we do not test that here
        inputs_values.insert(
            "#args.py".to_string(),
            File::builder().path("args.py").build().into(),
        );

        for input in tool.inputs {
            let id = input.id.unwrap();
            let fetched_value = &inputs_values[&id];
            assert!(
                validate_command_input(&input.r#type, fetched_value, None, None),
                "failed in {id}"
            )
        }
    }

    #[test]
    fn test_input_validation_enum() {
        let tool: CommandLineTool = serde_yaml::from_str(include_str!(
            "../../../testdata/cwl/tests/anon_enum_inside_array.cwl"
        ))
        .unwrap();
        let inputs_values: HashMap<String, DefaultValue> = serde_yaml::from_str(include_str!(
            "../../../testdata/cwl/tests/anon_enum_inside_array.yml"
        ))
        .unwrap();

        for input in tool.inputs {
            let id = input.id.unwrap();
            let fetched_value = &inputs_values[&id];
            assert!(
                validate_command_input(&input.r#type, fetched_value, None, None),
                "failed in {id}"
            )
        }
    }
}
