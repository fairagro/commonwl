use crate::Mapping;
use serde_json::Value;

#[must_use]
pub fn type_dsl(value: Value) -> Value {
    let mut value = value;
    while let Some(new_value) = type_dsl_impl(&value) {
        value = new_value;
    }
    value
}

fn type_dsl_impl(value: &Value) -> Option<Value> {
    match value {
        Value::String(value) => {
            if value.ends_with('?') {
                let inner = Value::String(value[..value.len() - 1].to_string());

                return Some(Value::Array(vec![
                    type_dsl(inner),
                    Value::String("null".to_string()),
                ]));
            }

            if value.ends_with("[]") {
                let inner = Value::String(value[..value.len() - 2].to_string());

                return Some(Value::Object(Mapping::from_iter([
                    ("type".into(), Value::String("array".into())),
                    ("items".into(), type_dsl(inner)),
                ])));
            }

            None
        }
        _ => None,
    }
}

pub fn secondary_files_dsl(value: Value) -> Value {
    match value {
        Value::String(value) => {
            if value.ends_with('?') {
                return Value::Object(Mapping::from_iter([
                    (
                        "pattern".into(),
                        Value::String(value[..value.len() - 1].to_string()),
                    ),
                    ("required".into(), Value::Bool(false)),
                ]));
            }
            Value::Object(Mapping::from_iter([(
                "pattern".into(),
                Value::String(value),
            )]))
        }
        Value::Array(seq) => Value::Array(seq.into_iter().map(secondary_files_dsl).collect()),
        _ => value,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_type_dsl() {
        let optional = "string?";
        let result = type_dsl(Value::String(optional.to_owned()));
        assert_eq!(
            result,
            Value::Array(vec![
                Value::String("string".to_string()),
                Value::String("null".to_string())
            ])
        );

        let array = "File[]";
        let result = type_dsl(Value::String(array.to_owned()));

        assert_eq!(
            result,
            Value::Object(Mapping::from_iter([
                ("type".into(), Value::String("array".into())),
                ("items".into(), Value::String("File".into())),
            ]))
        );

        let optional_array = "File[]?";
        let result = type_dsl(Value::String(optional_array.to_owned()));

        assert_eq!(
            result,
            Value::Array(vec![
                Value::Object(Mapping::from_iter([
                    ("type".into(), Value::String("array".into())),
                    ("items".into(), Value::String("File".into())),
                ])),
                Value::String("null".to_string())
            ])
        );

        let array_optional = "File?[]";
        let result = type_dsl(Value::String(array_optional.to_owned()));

        assert_eq!(
            result,
            Value::Object(Mapping::from_iter([
                ("type".into(), Value::String("array".into())),
                (
                    "items".into(),
                    Value::Array(vec![
                        Value::String("File".into()),
                        Value::String("null".to_string())
                    ])
                ),
            ])),
        );
    }

    #[test]
    fn test_secondary_files_dsl() {
        assert_eq!(
            secondary_files_dsl(Value::String("wamborambo".to_string())),
            Value::Object(Mapping::from_iter([(
                "pattern".into(),
                Value::String("wamborambo".into()),
            )]))
        );

        assert_eq!(
            secondary_files_dsl(Value::String("wamborambo?".to_string())),
            Value::Object(Mapping::from_iter([
                ("pattern".into(), Value::String("wamborambo".into()),),
                ("required".into(), Value::Bool(false),)
            ]))
        );
    }
}
