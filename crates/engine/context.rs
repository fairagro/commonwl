use cwl_core::inputs::DefaultValue;
use std::{collections::HashMap, path::PathBuf};
use url::Url;

pub type PathMapper = HashMap<String, (PathBuf, Url)>;

pub struct TaskSpawnContext {
    pub work_dir: PathBuf,
    pub runtime: Runtime,
    pub env: HashMap<String, String>,
    pub inputs: HashMap<String, DefaultValue>,
    pub path_mapper: PathMapper,
}

// Runtime Environment like described in CWL Spec
pub struct Runtime {
    pub outdir: PathBuf,
    pub tmpdir: PathBuf,
    pub cores: i64,
    pub ram: i64,
    pub outdir_size: i64,
    pub tmpdir_size: i64,
}
