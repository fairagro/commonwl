use sha1::{Digest, Sha1};

pub mod backend;
pub mod command;
pub mod context;
pub mod docker;
pub mod environment;
pub mod expression;
pub mod input;
pub mod output;
pub mod pathmapper;
pub mod requirements;
pub mod workdir;

pub fn checksum(str: &str) -> String {
    let mut hasher = Sha1::new();

    hasher.update(str);
    let hash = hasher.finalize();
    format!("sha1${hash:x}")
}
