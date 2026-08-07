use miette::Diagnostic;
use std::convert::Infallible;
use std::io;
use std::num::TryFromIntError;
use std::path::StripPrefixError;
use thiserror::Error;
use tokio::{sync::AcquireError, task::JoinError};

#[derive(Error, Debug, Diagnostic)]
pub enum RunnerError {
    #[error("{0}")]
    #[diagnostic(code(cwl_engine::RunnerError::Guard))]
    Guard(String),

    #[error("IO Error: {0}")]
    #[diagnostic(code(std::io::Error))]
    IOError(#[from] io::Error),

    #[error("URL Parsing Error: {0}")]
    #[diagnostic(code(url::ParseError))]
    UrlParseError(#[from] url::ParseError),

    #[error("JSON Error: {0}")]
    #[diagnostic(code(serde_json::Error))]
    JsonError(#[from] serde_json::Error),

    #[error("YAML parsing failed: {0}")]
    #[diagnostic(code(serde_saphyr::Error))]
    YamlParsingFailed(#[from] serde_saphyr::Error),

    #[error("YAML serialization failed: {0}")]
    #[diagnostic(code(serde_saphyr::ser::Error))]
    YamlSerialization(#[from] serde_saphyr::ser::Error),

    #[error(transparent)]
    #[diagnostic(transparent)]
    CoreError(#[from] cwl_core::Error),

    #[error("Docker client error: {0}")]
    #[diagnostic(code(cwl_engine::RunnerError::Docker))]
    DockerError(#[from] crankshaft::docker::Error),

    #[error("Docker daemon error: {0}")]
    #[diagnostic(code(cwl_engine::RunnerError::Bollard))]
    BollardError(#[from] bollard::errors::Error),

    #[error("Task execution failed: {0}")]
    #[diagnostic(code(cwl_engine::RunnerError::TaskRun))]
    TaskRunError(#[from] crankshaft::engine::service::runner::backend::TaskRunError),

    #[error("Task panicked or was cancelled: {0}")]
    #[diagnostic(code(cwl_engine::RunnerError::Join))]
    JoinError(#[from] JoinError),

    #[error("Failed to acquire concurrency permit: {0}")]
    #[diagnostic(code(cwl_engine::RunnerError::Acquire))]
    AcquireError(#[from] AcquireError),

    #[error("Integer conversion overflowed: {0}")]
    #[diagnostic(code(cwl_engine::RunnerError::TryFromInt))]
    TryFromIntError(#[from] TryFromIntError),

    #[error("Path is not a prefix of the given path: {0}")]
    #[diagnostic(code(cwl_engine::RunnerError::StripPrefix))]
    StripPrefixError(#[from] StripPrefixError),

    #[error("Infallible: {0}")]
    #[diagnostic(code(cwl_engine::RunnerError::Infallible))]
    Infallible(#[from] Infallible),

    #[error(transparent)]
    #[diagnostic(code(anyhow::error))]
    Other(#[from] anyhow::Error),
}

pub type Result<T, E = RunnerError> = std::result::Result<T, E>;

#[macro_export]
macro_rules! bail {
    ($($arg:tt)*) => {
        return Err($crate::error::RunnerError::Guard(format!($($arg)*)))
    };
}

#[macro_export]
macro_rules! ensure {
    ($cond:expr $(,)?) => {
        if !($cond) {
            return Err($crate::error::RunnerError::Guard(format!(
                "Condition failed: `{}`",
                stringify!($cond)
            )));
        }
    };
    ($cond:expr, $($arg:tt)*) => {
        if !($cond) {
            return Err($crate::error::RunnerError::Guard(format!($($arg)*)));
        }
    };
}
