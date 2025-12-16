use serde::{Deserialize, Serialize};

pub mod deserialize;
pub mod io;
pub mod requirements;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(untagged)]
pub enum NumberOrExpression {
    Int(i32),
    Long(i64),
    Float(f32),
    Expression(String),
}

#[derive(Serialize, Deserialize, Debug, Clone, Hash, PartialEq)]
#[serde(untagged)]
pub enum IntegerOrExpression {
    Int(i32),
    Long(i64),
    Expression(String),
}

#[derive(Serialize, Deserialize, Debug, Clone, Hash, PartialEq)]
#[serde(untagged)]
pub enum BoolOrExpression {
    Bool(bool),
    Expression(String),
}

#[derive(Serialize, Deserialize, Debug, Clone, Hash, Copy, PartialEq)]
#[serde(untagged)]
pub enum Integer {
    Int(i32),
    Long(i64),
}
