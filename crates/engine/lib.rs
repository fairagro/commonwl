use cwl_core::documents::CWLDocument;
use semver::Version;

pub mod backend;
pub mod command;
pub mod docker;
pub mod environment;
pub mod expression;
pub mod input;
pub mod io;
pub mod output;
pub mod request;
pub mod requirements;
pub mod scatter;
pub mod schema;
pub mod tree;
pub mod workflow;

pub(crate) fn cwl_version(doc: &CWLDocument) -> anyhow::Result<Version> {
    let default = "v1.2".to_string();
    let version = doc.cwl_version().unwrap_or(&default);
    let version = version.trim_start_matches('v');
    let version = if version.matches('.').count() == 1 {
        format!("{}.0", version)
    } else {
        version.to_owned()
    };
    Ok(Version::parse(&version)?)
}

pub(crate) const V1_2_0: Version = Version::new(1, 2, 0);
