use miette::Diagnostic;
use std::io;
use thiserror::Error;

#[derive(Error, Debug, Diagnostic)]
pub enum Error {
    #[error("YAML parsing failed")]
    #[diagnostic(code(commonwl::Error::ParsingFailed))]
    ParsingFailed(#[from] serde_saphyr::Error),

    #[error("IO Error")]
    #[diagnostic(code(std::io::Error))]
    IOError(#[from] io::Error),

    #[error("Serialization Error")]
    #[diagnostic(code(serde_saphyr::ser::Error))]
    Serialization(#[from] serde_saphyr::ser::Error),

    #[error("Serialization Error")]
    #[diagnostic(code(commonwl::Error::Guard))]
    Guard(&'static str),

    #[error("URL Parsing Error")]
    #[diagnostic(code(commonwl::Error::Guard))]
    UrlParseError(#[from] url::ParseError),

    #[error("CWL document failed validation")]
    #[diagnostic(code(commonwl::Error::Validation))]
    Validation(#[from] validator::ValidationErrors),

    #[error(transparent)]
    #[diagnostic(code(anyhow::error))]
    Unknown(#[from] anyhow::Error),
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

#[macro_export]
macro_rules! guard {
    (let $pat:pat = $expr:expr, $err:expr) => {
        let $pat = $expr else {
            return Err($crate::Error::Guard($err).into());
        };
    };
}
