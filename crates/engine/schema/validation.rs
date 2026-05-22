use cwl_core::{
    OneOrMany,
    files::FileOrDirectory,
    inputs::{CommandInputParameterType, DefaultValue, InputType},
    types::CWLType,
};
use tracing::error;

use crate::{
    schema::format_validation::FormatValidator,
    schema::validation_types::{
        ValidationArraySchema, ValidationEnumSchema, ValidationRecordSchema, ValidationSchema,
        ValidationType,
    },
};

pub(crate) fn validate_command_input(
    schema: &CommandInputParameterType,
    value: &DefaultValue,
    format: Option<&String>,
    fv: Option<&FormatValidator>,
) -> bool {
    match schema {
        CommandInputParameterType::Stdin => !value.is_null(), // for stdin we accept any existing value
        CommandInputParameterType::CommandInputType(one_or_many) => match one_or_many {
            OneOrMany::One(item) => validate_type(
                &Into::<InputType>::into(item.clone()).into(),
                value,
                format,
                fv,
            ),
            OneOrMany::Many(items) => items.iter().any(|i| {
                validate_type(
                    &Into::<InputType>::into(i.clone()).into(),
                    value,
                    format,
                    fv,
                )
            }),
        },
    }
}

pub(crate) fn validate_type(
    r#type: &ValidationType,
    value: &DefaultValue,
    format: Option<&String>,
    fv: Option<&FormatValidator>,
) -> bool {
    match r#type {
        ValidationType::CWLType(ty) => validate_cwl_type(*ty, value, format, fv),
        ValidationType::Schema(schema) => validate_schema(schema, value, format, fv),
        ValidationType::String(_) => {
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
            CWLType::File if fod.is_file() => {
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
            }
            CWLType::Directory => fod.is_dir(),
            CWLType::Any => true,
            _ => false,
        },
        DefaultValue::Any(value) => match r#type {
            CWLType::Null => value.is_null(),
            CWLType::Boolean => value.is_boolean(),
            CWLType::Int | CWLType::Long => value.is_i64() || value.is_u64(),
            CWLType::Float => value.is_f64() || value.is_i64() || value.is_u64(),
            CWLType::Double => value.is_f64(),
            CWLType::String => value.is_string(),
            CWLType::Any => !value.is_null(),
            _ => false,
        },
    }
}

fn validate_schema(
    schema: &ValidationSchema,
    value: &DefaultValue,
    format: Option<&String>,
    fv: Option<&FormatValidator>,
) -> bool {
    match schema {
        ValidationSchema::Record(rec) => validate_record_schema(rec, value, fv),
        ValidationSchema::Enum(enu) => validate_enum_schema(enu, value),
        ValidationSchema::Array(arr) => validate_array_schema(arr, value, format, fv),
    }
}

fn validate_record_schema(
    schema: &ValidationRecordSchema,
    value: &DefaultValue,
    fv: Option<&FormatValidator>,
) -> bool {
    let DefaultValue::Any(serde_json::Value::Object(mapping)) = value else {
        return false;
    };

    if let Some(fields) = &schema.fields {
        return fields.iter().all(|f| {
            if let Some(field_value) = mapping.get(&f.name) {
                match &f.r#type {
                    OneOrMany::One(item) => validate_type(
                        item,
                        &serde_json::from_value(field_value.clone())
                            .expect("DefaultValue violates itself"),
                        f.format.as_ref().map(OneOrMany::as_one),
                        fv,
                    ),
                    OneOrMany::Many(items) => items.iter().any(|item| {
                        validate_type(
                            item,
                            &serde_json::from_value(field_value.clone())
                                .expect("DefaultValue violates itself"),
                            f.format.as_ref().map(OneOrMany::as_one),
                            fv,
                        )
                    }),
                }
            } else {
                // Field is missing - check if it's optional (has null in union type)
                match &f.r#type {
                    OneOrMany::One(ValidationType::CWLType(CWLType::Null)) => true,
                    OneOrMany::Many(items) => items
                        .iter()
                        .any(|item| matches!(item, ValidationType::CWLType(CWLType::Null))),
                    OneOrMany::One(_) => false,
                }
            }
        });
    }
    false
}

fn validate_array_schema(
    schema: &ValidationArraySchema,
    value: &DefaultValue,
    format: Option<&String>,
    fv: Option<&FormatValidator>,
) -> bool {
    //check whether we have a sequence!
    if let DefaultValue::Any(serde_json::Value::Array(seq)) = value {
        seq.iter().all(|item| {
            let item_value: DefaultValue =
                serde_json::from_value(item.clone()).expect("DefaultValue violates itself");
            match &schema.items {
                OneOrMany::One(t) => validate_type(t, &item_value, format, fv),
                OneOrMany::Many(ts) => ts.iter().any(|t| validate_type(t, &item_value, format, fv)),
            }
        })
    } else {
        false
    }
}

fn validate_enum_schema(schema: &ValidationEnumSchema, value: &DefaultValue) -> bool {
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
    use super::*;
    use cwl_core::{documents::CommandLineTool, files::File};
    use std::collections::HashMap;

    #[test]
    #[cfg(not(target_os = "windows"))] //cwl submodule is not available on windows
    fn test_input_validation_complex() {
        let tool: CommandLineTool =
            serde_saphyr::from_str(include_str!("../../../testdata/cwl/tests/binding-test.cwl"))
                .unwrap();
        let mut inputs_values: HashMap<String, DefaultValue> =
            serde_saphyr::from_str(include_str!("../../../testdata/cwl/tests/bwa-mem-job.json"))
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
    #[cfg(not(target_os = "windows"))] //cwl submodule is not available on windows
    fn test_input_validation_enum() {
        let tool: CommandLineTool = serde_saphyr::from_str(include_str!(
            "../../../testdata/cwl/tests/anon_enum_inside_array.cwl"
        ))
        .unwrap();
        let inputs_values: HashMap<String, DefaultValue> = serde_saphyr::from_str(include_str!(
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
