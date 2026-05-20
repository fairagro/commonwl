use crate::BoolOrExpression;
use serde::{Deserialize, Serialize};
use std::fmt::Display;

#[doc = include_str!("../../docs/gen/CWLType.md")]
#[derive(Serialize, Deserialize, Debug, Copy, PartialEq, Hash, Clone, Eq)]
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
    #[doc = include_str!("../../docs/gen/Any.md")]
    #[serde(rename = "Any")]
    Any,
}

impl Display for CWLType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            CWLType::Null => "null",
            CWLType::Boolean => "boolean",
            CWLType::Int => "int",
            CWLType::Long => "long",
            CWLType::Float => "float",
            CWLType::Double => "double",
            CWLType::String => "string",
            CWLType::File => "File",
            CWLType::Directory => "Directory",
            CWLType::Any => "Any",
        };
        write!(f, "{s}")
    }
}

#[doc = include_str!("../../docs/gen/SecondaryFileSchema.md")]
#[derive(Serialize, Deserialize, Debug, PartialEq, Hash, Clone, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SecondaryFileSchema {
    #[doc = include_str!("../../docs/gen/SecondaryFileSchema_pattern.md")]
    pub pattern: String,
    #[doc = include_str!("../../docs/gen/SecondaryFileSchema_required.md")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required: Option<BoolOrExpression>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::OneOrMany;
    use cwl_salad::deserialize::deserialize_with_secondary_files_dsl;

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
        let sec_files = serde_saphyr::from_str::<SecondaryBag>(contents);
        dbg!(&sec_files);
        assert!(sec_files.is_ok())
    }
}
