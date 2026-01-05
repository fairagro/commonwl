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

#[derive(Serialize, Deserialize, Debug, Clone, Hash, PartialEq)]
#[serde(untagged)]
pub enum OneOrMany<T> {
    One(T),
    Many(Vec<T>),
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
