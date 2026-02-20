use sha1::{Digest, Sha1};

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
pub mod schema;
pub mod tree;
pub mod scatter;
pub mod workflow;

pub fn checksum(str: &str) -> String {
    let mut hasher = Sha1::new();

    hasher.update(str);
    let hash = hasher.finalize();
    format!("sha1${hash:x}")
}
