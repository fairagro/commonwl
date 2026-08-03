#![deny(unused_crate_dependencies)]
use anyhow::Context;
use base16ct::HexDisplay;
use serde::{Deserialize, Serialize};
use sha1::Digest;
use sha1::Sha1;
use std::fmt::Display;
use std::fs;
use std::hash::Hash;
use std::path::Component;
use std::path::Path;
use std::path::PathBuf;

pub mod documents;
mod error;
pub mod files;
pub mod format;
pub mod inputs;
pub mod outputs;
pub mod packed;
pub mod requirements;
pub mod types;
pub mod validate;

mod load;
pub use crate::error::*;
pub use cwl_salad::Identifiable;
pub use load::from_str;
pub use load::load_cwl_file;
pub use load::preprocess_cwl_file;
pub use oneormany::OneOrMany;

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

impl Display for NumberOrExpression {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NumberOrExpression::Int(i) => write!(f, "{i}"),
            NumberOrExpression::Long(l) => write!(f, "{l}"),
            NumberOrExpression::Float(fl) => write!(f, "{fl}"),
            NumberOrExpression::Expression(e) => write!(f, "{e}"),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Hash, PartialEq, Eq)]
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

impl Display for IntegerOrExpression {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IntegerOrExpression::Int(i) => write!(f, "{i}"),
            IntegerOrExpression::Long(l) => write!(f, "{l}"),
            IntegerOrExpression::Expression(e) => write!(f, "{e}"),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Hash, PartialEq, Eq)]
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

#[derive(Serialize, Deserialize, Debug, Clone, Hash, Copy, PartialEq, Eq)]
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

impl From<u32> for Integer {
    fn from(value: u32) -> Self {
        Integer::Int(value.cast_signed())
    }
}

impl From<u64> for Integer {
    fn from(value: u64) -> Self {
        Integer::Long(value.cast_signed())
    }
}

impl Integer {
    #[must_use]
    pub fn as_i64(&self) -> i64 {
        match self {
            Self::Int(i) => i64::from(*i),
            Self::Long(l) => *l,
        }
    }
}

pub trait ExtractFromEnum<E> {
    fn get(e: &E) -> Option<&Self>
    where
        Self: Sized;

    fn get_mut(e: &mut E) -> Option<&mut Self>
    where
        Self: Sized;
}

/// Returns `String`, `Number` or `Bool` as String
/// # Errors
/// otherwise
pub fn value_as_string(value: &serde_json::Value) -> Result<String> {
    match value {
        serde_json::Value::String(s) => Ok(s.clone()),
        serde_json::Value::Number(n) => Ok(n.to_string()),
        serde_json::Value::Bool(b) => Ok(b.to_string()),
        _ => Err(Error::Guard(
            "Value is not a string, number, or bool".to_string(),
        )),
    }
}

#[must_use]
pub fn docstring(doc: OneOrMany<String>) -> String {
    match doc {
        OneOrMany::One(s) => s,
        OneOrMany::Many(vec) => vec.join("\n"),
    }
}

#[derive(Debug)]
pub struct FilePathMetaData {
    pub basename: Option<String>,
    pub nameroot: Option<String>,
    pub nameext: Option<String>,
    pub dirname: Option<String>,
}

#[must_use]
pub fn get_path_metadata(path: &Path) -> FilePathMetaData {
    let basename = path.file_name().map(|f| f.to_string_lossy().into_owned());
    let nameroot = path.file_stem().map(|s| s.to_string_lossy().into_owned());
    let nameext = path
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy()));
    let dirname = path.parent().map(|p| p.to_string_lossy().into_owned());

    FilePathMetaData {
        basename,
        nameroot,
        nameext,
        dirname,
    }
}

#[derive(Debug)]
pub struct FileMetaData {
    pub size: u64,
    pub checksum: Option<String>,
}

/// Tries to retrieve `FileMetaData` for given `Path`
/// # Errors
/// if file does no exist
pub fn get_file_metadata(path: &Path) -> Result<FileMetaData> {
    let metadata = fs::metadata(path)
        .with_context(|| format!("Can not retrieve File metadata for {}", path.display()))?;
    let size = metadata.len();
    let hash = fs::read(path).ok().map(|f| {
        let hash = Sha1::digest(&f);
        let dp = HexDisplay(hash.as_slice());
        format!("sha1${dp:x}")
    });
    Ok(FileMetaData {
        size,
        checksum: hash,
    })
}

pub(crate) fn normalize_path<P: AsRef<Path>>(input: P) -> std::io::Result<PathBuf> {
    let full_path = std::path::absolute(input)?;
    Ok(full_path
        .components()
        .fold(PathBuf::new(), |mut acc, comp| {
            match comp {
                Component::RootDir => acc.push(comp),
                Component::Prefix(prefix) => acc.push(prefix.as_os_str()),
                Component::CurDir => {}
                Component::ParentDir => {
                    acc.pop();
                }
                Component::Normal(c) => acc.push(c),
            }
            acc
        }))
}
