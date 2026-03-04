use anyhow::Context;
use serde::{Deserialize, Serialize};
use serde_yaml::Value;
use sha1::Digest;
use sha1::Sha1;
use std::fs;
use std::path::Path;

pub mod deserialize;
pub mod documents;
pub mod files;
pub mod inputs;
pub mod outputs;
pub mod packed;
pub mod requirements;
pub mod types;

mod load;
pub use load::load_cwl_file;
pub use load::preprocess_cwl_file;

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
        OneOrMany::Many(value.iter().map(ToString::to_string).collect())
    }
}

impl<'a, const N: usize> From<&'a [&'a str; N]> for OneOrMany<String> {
    fn from(value: &'a [&'a str; N]) -> Self {
        value.as_ref().into()
    }
}

impl<T: Clone> OneOrMany<T> {
    pub fn map<U, F>(self, mut f: F) -> OneOrMany<U>
    where
        F: FnMut(T) -> U,
    {
        match self {
            OneOrMany::One(t) => OneOrMany::One(f(t)),
            OneOrMany::Many(ts) => OneOrMany::Many(ts.into_iter().map(f).collect()),
        }
    }

    /// Returns `OneOrMany` as One
    /// # Panics
    /// if many
    pub fn as_one(&self) -> &T {
        match self {
            OneOrMany::One(t) => t,
            OneOrMany::Many(v) => v.first().expect("Called as_one on an empty Many"),
        }
    }

    /// Returns `OneOrMany` as Many
    pub fn as_many(&self) -> Vec<T> {
        match self {
            OneOrMany::One(t) => vec![t.clone()],
            OneOrMany::Many(v) => v.clone(),
        }
    }
}

pub trait ExtractFromEnum<E> {
    fn get(e: &E) -> Option<&Self>
    where
        Self: Sized;
}

/// Returns `String`, `Number` or `Bool` as String
/// # Errors
/// otherwise
pub fn value_as_string(value: &Value) -> anyhow::Result<String> {
    match value {
        Value::String(s) => Ok(s.clone()),
        Value::Number(n) => Ok(n.to_string()),
        Value::Bool(b) => Ok(b.to_string()),
        _ => anyhow::bail!("Value is not a string, number, or bool"),
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
pub fn get_file_metadata(path: &Path) -> anyhow::Result<FileMetaData> {
    let metadata = fs::metadata(path)
        .with_context(|| format!("Can not retrieve File metadata for {}", path.display()))?;
    let size = metadata.len();

    let mut hasher = Sha1::new();
    let hash = fs::read(path).ok().map(|f| {
        hasher.update(&f);
        let hash = hasher.finalize();
        format!("sha1${hash:x}")
    });
    Ok(FileMetaData {
        size,
        checksum: hash,
    })
}
