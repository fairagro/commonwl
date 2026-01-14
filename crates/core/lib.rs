use serde::{Deserialize, Serialize};

pub mod deserialize;
pub mod documents;
pub mod files;
pub mod inputs;
pub mod outputs;
pub mod requirements;
pub mod types;

mod load;
pub use load::load_cwl_file;
pub use load::preprocess_cwl_file;
use serde_yaml::Value;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(untagged)]
pub enum NumberOrExpression {
    Int(i32),
    Long(i64),
    Float(f32),
    Expression(String),
}

impl From<i32> for NumberOrExpression {
    fn from(value: i32) -> Self {
        NumberOrExpression::Int(value)
    }
}

impl From<i64> for NumberOrExpression {
    fn from(value: i64) -> Self {
        NumberOrExpression::Long(value)
    }
}

impl From<f32> for NumberOrExpression {
    fn from(value: f32) -> Self {
        NumberOrExpression::Float(value)
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Hash, PartialEq)]
#[serde(untagged)]
pub enum IntegerOrExpression {
    Int(i32),
    Long(i64),
    Expression(String),
}

impl From<i32> for IntegerOrExpression {
    fn from(value: i32) -> Self {
        IntegerOrExpression::Int(value)
    }
}

impl From<i64> for IntegerOrExpression {
    fn from(value: i64) -> Self {
        IntegerOrExpression::Long(value)
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Hash, PartialEq)]
#[serde(untagged)]
pub enum BoolOrExpression {
    Bool(bool),
    Expression(String),
}

impl From<bool> for BoolOrExpression {
    fn from(value: bool) -> Self {
        BoolOrExpression::Bool(value)
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Hash, Copy, PartialEq)]
#[serde(untagged)]
pub enum Integer {
    Int(i32),
    Long(i64),
}

impl From<i32> for Integer {
    fn from(value: i32) -> Self {
        Integer::Int(value)
    }
}

impl From<i64> for Integer {
    fn from(value: i64) -> Self {
        Integer::Long(value)
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Hash, PartialEq)]
#[serde(untagged)]
pub enum OneOrMany<T> {
    One(T),
    Many(Vec<T>),
}

impl<T> From<T> for OneOrMany<T> {
    fn from(value: T) -> Self {
        OneOrMany::One(value)
    }
}

impl<T> From<Vec<T>> for OneOrMany<T> {
    fn from(value: Vec<T>) -> Self {
        OneOrMany::Many(value)
    }
}

impl<'a> From<&'a str> for OneOrMany<String> {
    fn from(value: &'a str) -> Self {
        OneOrMany::One(value.into())
    }
}

impl<'a> From<&'a [&'a str]> for OneOrMany<String> {
    fn from(value: &[&str]) -> Self {
        if value.len() == 1 {
            return OneOrMany::One(value[0].to_string());
        }
        OneOrMany::Many(value.iter().map(|s| s.to_string()).collect())
    }
}

impl<'a, const N: usize> From<&'a [&'a str; N]> for OneOrMany<String> {
    fn from(value: &'a [&'a str; N]) -> Self {
        value.as_ref().into()
    }
}

impl<T> OneOrMany<T> {
    pub fn map<U, F>(self, mut f: F) -> OneOrMany<U>
    where
        F: FnMut(T) -> U,
    {
        match self {
            OneOrMany::One(t) => OneOrMany::One(f(t)),
            OneOrMany::Many(ts) => OneOrMany::Many(ts.into_iter().map(f).collect()),
        }
    }
}

pub trait ExtractFromEnum<E> {
    fn get(e: &E) -> Option<&Self>
    where
        Self: Sized;
}

pub fn value_as_string(value: &Value) -> anyhow::Result<String> {
    match value {
        Value::String(s) => Ok(s.to_string()),
        Value::Number(n) => Ok(n.to_string()),
        Value::Bool(b) => Ok(b.to_string()),
        _ => anyhow::bail!("Value is not a string, number, or bool"),
    }
}
