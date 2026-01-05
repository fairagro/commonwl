use crate::BoolOrExpression;
use serde::{Deserialize, Serialize};
use serde_yaml::Mapping;

#[derive(Serialize, Deserialize, Debug, Copy, PartialEq, Hash, Clone)]
#[serde(rename_all = "snake_case")]
pub enum CWLType {
    Null,
    Boolean,
    Int,
    Long,
    Float,
    Double,
    String,
    #[serde(rename = "File")]
    File,
    #[serde(rename = "Directory")]
    Directory,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Hash, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SecondaryFileSchema {
    pub pattern: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required: Option<BoolOrExpression>,
}

pub fn type_dsl(value: serde_yaml::Value) -> serde_yaml::Value {
    let mut value = value;
    while let Some(new_value) = type_dsl_impl(&value) {
        value = new_value;
    }
    value
}

fn type_dsl_impl(value: &serde_yaml::Value) -> Option<serde_yaml::Value> {
    match value {
        serde_yaml::Value::String(value) => {
            if value.ends_with("?") {
                let inner = serde_yaml::Value::String(value[..value.len() - 1].to_string());

                return Some(serde_yaml::Value::Sequence(vec![
                    type_dsl(inner),
                    serde_yaml::Value::String("null".to_string()),
                ]));
            }

            if value.ends_with("[]") {
                let inner = serde_yaml::Value::String(value[..value.len() - 2].to_string());

                return Some(serde_yaml::Value::Mapping(Mapping::from_iter([
                    (
                        serde_yaml::Value::String("type".into()),
                        serde_yaml::Value::String("array".into()),
                    ),
                    (serde_yaml::Value::String("items".into()), type_dsl(inner)),
                ])));
            }

            None
        }
        _ => None,
    }
}

pub fn secondary_files_dsl(value: serde_yaml::Value) -> serde_yaml::Value {
    match value {
        serde_yaml::Value::String(value) => {
            if value.ends_with("?") {
                return serde_yaml::Value::Mapping(Mapping::from_iter([
                    (
                        serde_yaml::Value::String("pattern".into()),
                        serde_yaml::Value::String(value[..value.len() - 1].to_string()),
                    ),
                    (
                        serde_yaml::Value::String("required".into()),
                        serde_yaml::Value::Bool(false),
                    ),
                ]));
            }
            serde_yaml::Value::Mapping(Mapping::from_iter([(
                serde_yaml::Value::String("pattern".into()),
                serde_yaml::Value::String(value),
            )]))
        }
        serde_yaml::Value::Sequence(seq) => {
            serde_yaml::Value::Sequence(seq.into_iter().map(secondary_files_dsl).collect())
        }
        _ => value,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::OneOrMany;
    use crate::deserialize::deserialize_with_secondary_files_dsl;

    #[test]
    fn test_type_dsl() {
        let optional = "string?";
        let result = type_dsl(serde_yaml::Value::String(optional.to_owned()));
        assert_eq!(
            result,
            serde_yaml::Value::Sequence(vec![
                serde_yaml::Value::String("string".to_string()),
                serde_yaml::Value::String("null".to_string())
            ])
        );

        let array = "File[]";
        let result = type_dsl(serde_yaml::Value::String(array.to_owned()));

        assert_eq!(
            result,
            serde_yaml::Value::Mapping(Mapping::from_iter([
                (
                    serde_yaml::Value::String("type".into()),
                    serde_yaml::Value::String("array".into())
                ),
                (
                    serde_yaml::Value::String("items".into()),
                    serde_yaml::Value::String("File".into())
                ),
            ]))
        );

        let optional_array = "File[]?";
        let result = type_dsl(serde_yaml::Value::String(optional_array.to_owned()));

        assert_eq!(
            result,
            serde_yaml::Value::Sequence(vec![
                serde_yaml::Value::Mapping(Mapping::from_iter([
                    (
                        serde_yaml::Value::String("type".into()),
                        serde_yaml::Value::String("array".into())
                    ),
                    (
                        serde_yaml::Value::String("items".into()),
                        serde_yaml::Value::String("File".into())
                    ),
                ])),
                serde_yaml::Value::String("null".to_string())
            ])
        );

        let array_optional = "File?[]";
        let result = type_dsl(serde_yaml::Value::String(array_optional.to_owned()));

        assert_eq!(
            result,
            serde_yaml::Value::Mapping(Mapping::from_iter([
                (
                    serde_yaml::Value::String("type".into()),
                    serde_yaml::Value::String("array".into())
                ),
                (
                    serde_yaml::Value::String("items".into()),
                    serde_yaml::Value::Sequence(vec![
                        serde_yaml::Value::String("File".into()),
                        serde_yaml::Value::String("null".to_string())
                    ])
                ),
            ])),
        );
    }

    #[test]
    fn test_secondary_files_dsl() {
        assert_eq!(
            secondary_files_dsl(serde_yaml::Value::String("wamborambo".to_string())),
            serde_yaml::Value::Mapping(Mapping::from_iter([(
                serde_yaml::Value::String("pattern".into()),
                serde_yaml::Value::String("wamborambo".into()),
            )]))
        );

        assert_eq!(
            secondary_files_dsl(serde_yaml::Value::String("wamborambo?".to_string())),
            serde_yaml::Value::Mapping(Mapping::from_iter([
                (
                    serde_yaml::Value::String("pattern".into()),
                    serde_yaml::Value::String("wamborambo".into()),
                ),
                (
                    serde_yaml::Value::String("required".into()),
                    serde_yaml::Value::Bool(false),
                )
            ]))
        );
    }

    #[test]
    #[allow(unused)]
    fn test_secondary_files_dsl_deserialization() {
        #[derive(Deserialize, Debug)]
        struct SecondaryBag {
            #[serde(rename = "secondaryFiles")]
            #[serde(deserialize_with = "deserialize_with_secondary_files_dsl")]
            secondary_files: OneOrMany<SecondaryFileSchema>,
        }

        let contents = r#"
        secondaryFiles:
            - bai
            - rampawampa?
            - wamborambo
            - alerta"#;
        let sec_files = serde_yaml::from_str::<SecondaryBag>(contents);
        dbg!(&sec_files);
        assert!(sec_files.is_ok())
    }
}
