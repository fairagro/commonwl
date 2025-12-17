use crate::BoolOrExpression;
use serde::{Deserialize, Serialize};

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
