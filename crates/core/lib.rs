use std::{fs, path::Path};

use serde::{Deserialize, Serialize};
use serde_yaml::Value;

use crate::documents::CWLDocument;

pub mod deserialize;
pub mod documents;
pub mod files;
pub mod inputs;
pub mod outputs;
pub mod requirements;
pub mod types;

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

pub fn load_cwl_file<P: AsRef<Path>>(path: P, preprocess: bool) -> anyhow::Result<CWLDocument> {
    let contents = if preprocess {
        preprocess_cwl_file(&path)?
    } else {
        fs::read_to_string(&path)?
    };
    serde_yaml::from_str::<CWLDocument>(&contents).map_err(|e| e.into())
}

pub fn preprocess_cwl_file<P: AsRef<Path>>(path: P) -> anyhow::Result<String> {
    let contents = fs::read_to_string(&path)?;
    let mut yaml: Value = serde_yaml::from_str(&contents)?;
    let path = path.as_ref().parent().unwrap_or_else(|| Path::new("."));

    resolve_imports(&mut yaml, path)?;
    Ok(serde_yaml::to_string(&yaml)?)
}

fn resolve_imports(value: &mut Value, base_path: &Path) -> anyhow::Result<()> {
    match value {
        Value::Mapping(map) => {
            if map.len() == 1
                && let Some(Value::String(file)) = map.get(Value::String("$import".to_string()))
            {
                let path = base_path.join(file);
                let contents = fs::read_to_string(&path)?;
                let mut imported_value: Value = serde_yaml::from_str(&contents)?;
                resolve_imports(&mut imported_value, path.parent().unwrap_or(base_path))?;
                *value = imported_value;
                return Ok(());
            }
            for val in map.values_mut() {
                resolve_imports(val, base_path)?;
            }
        }
        Value::Sequence(seq) => {
            for val in seq.iter_mut() {
                resolve_imports(val, base_path)?;
            }
        }
        _ => {}
    }
    Ok(())
}
