use cwl_core::{documents::StringOrDocument, inputs::DefaultValue};
use anyhow::Context;
use base16ct::HexDisplay;
use serde_json::Value;
use sha1::{Digest, Sha1};
use std::{collections::{HashMap, HashSet}, fs, io::{self, Read}, path::{Path, PathBuf}};
#[cfg(unix)]
use std::os::unix::fs::MetadataExt;
use tracing::warn;
use url::Url;

const JOB_DIR: &str = "jobs";
const RESULT_FILE: &str = "result.json";
const OUT_DIR: &str = "out";
const MARKER_FILE: &str = ".s4n_jobstore";

#[derive(Debug)]
pub struct Jobstore {
    dir: PathBuf,
}

impl Jobstore {
    pub fn new(dir: impl AsRef<Path>, restart: bool) -> crate::Result<Self> {
        let job_dir = dir.as_ref().join(JOB_DIR);
        if restart {
            crate::ensure!(job_dir.is_dir() && dir.as_ref().join(MARKER_FILE).is_file(), "No job store at {} to restart from", dir.as_ref().display());
            #[cfg(unix)]
            {
                let meta = fs::metadata(dir.as_ref())?;
                crate::ensure!(meta.uid() == nix::unistd::Uid::current().as_raw(), "Jobstore {} is not owned by the current user", dir.as_ref().display());
                crate::ensure!(meta.mode() & 0o077 == 0, "Jobstore {} must not be accessible by other users", dir.as_ref().display());
            }
        } else {
            let is_empty = fs::read_dir(dir.as_ref()).map_or(true, |mut d| d.next().is_none());
            crate::ensure!(is_empty, "Jobstore {} already exists and is not empty, remove it, choose a new directory or restart the workflow",
                dir.as_ref().display());
        }
        create_private_dir(dir.as_ref()).with_context(|| format!("Could not create private dir {}", dir.as_ref().display()))?;
        fs::write(dir.as_ref().join(MARKER_FILE), b"")
            .with_context(|| format!("Could not create jobstore marker in {}", dir.as_ref().display()))?;
        fs::create_dir_all(&job_dir).with_context(|| format!("Could not create jobstore {}", job_dir.display()))?;
        Ok(Self { dir: dunce::canonicalize(dir.as_ref())? })
    }
    
    #[must_use]
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub(crate) fn reset(&self, key: &str) -> crate::Result<()> {
        let dir = self.job_dir(key);
        remove_job_dir(&dir).with_context(|| format!("Could not remove unfinished job {}", dir.display()))?;
        Ok(())
    }
    
    pub fn clean(&self) -> crate::Result<()> {
        crate::ensure!(self.dir.join(MARKER_FILE).is_file(), "{} is not a jobstore, not removing it", self.dir.display());
        remove_job_dir(&self.dir).with_context(|| format!("Could not remove jobstore {}", self.dir.display()))?;
        Ok(())
    }

    pub(crate) fn job_key(&self, parent: &Path, step_id: &str, job_idx: Option<usize>, run: &StringOrDocument,
        wd: &Path, inputs: &HashMap<String, DefaultValue>) -> crate::Result<String> {
        let mut h = Sha1::new();
        let parent = parent.strip_prefix(&self.dir).unwrap_or(Path::new(""));
        h.update(parent.to_string_lossy().as_bytes());
        h.update([0]);
        h.update(step_id.as_bytes());
        h.update([0]);
        h.update(job_idx.map(|i| i.to_string()).unwrap_or_default().as_bytes());
        h.update([0]);
        match run {
            StringOrDocument::String(path) => {
                let path = wd.join(path);
                h.update(fs::read(&path).with_context(|| format!("Could not read {}", path.display()))?);
            }
            StringOrDocument::Document(doc) => h.update(serde_json::to_vec(&serde_json::to_value(doc)?)?),
        }
        h.update([0]);
        let mut inputs = serde_json::to_value(inputs)?;
        remove_staging_paths(&mut inputs);
        h.update(serde_json::to_vec(&inputs)?);
        hash_file_stats(&inputs, &mut h)?;
        Ok(format!("{:x}", HexDisplay(h.finalize().as_slice())))
    }
    
    pub(crate) fn job_out_dir(&self, key: &str) -> PathBuf {
        self.job_dir(key).join(OUT_DIR)
    }
    
    pub(crate) fn finished(&self, key: &str, inputs: &HashMap<String, DefaultValue>) -> Option<HashMap<String, DefaultValue>> {
        let path = self.job_dir(key).join(RESULT_FILE);
        let contents = fs::read(&path).ok()?;
        let outputs: HashMap<String, DefaultValue> = match serde_json::from_slice(&contents) {
            Ok(outputs) => outputs,
            Err(e) => {
                warn!("Could not read job result {}, running job again: {e}", path.display());
                return None;
            }
        };
        let value = serde_json::to_value(&outputs).ok()?;
        if !files_exist(&value) {
            warn!("Output files of job {} are missing, running job again", path.display());
            return None;
        }
        let mut valid = Valid::default();
        valid.path.extend(dunce::canonicalize(self.job_out_dir(key)).ok());
        valid.add_inputs(&serde_json::to_value(inputs).ok()?);
        if let Err(e) = valid.check(&value) {
            warn!("Job result {} refers to {e}, running job again", path.display());
            return None;
        }
        Some(outputs)
    }

    pub(crate) fn finish(&self, key: &str, outputs: &HashMap<String, DefaultValue>) -> crate::Result<()> {
        let dir = self.job_dir(key);
        fs::create_dir_all(&dir)?;
        let tmp = dir.join(format!("{RESULT_FILE}.tmp"));
        fs::write(&tmp, serde_json::to_vec(outputs)?).with_context(|| format!("Could not write job result {}", tmp.display()))?;
        fs::rename(&tmp, dir.join(RESULT_FILE))?;
        Ok(())
    }

    fn job_dir(&self, key: &str) -> PathBuf {
        self.dir.join(JOB_DIR).join(key)
    }
}

fn hash_path(p: &Path, h: &mut Sha1, visited_dirs: &mut HashSet<PathBuf>) -> io::Result<()> {
    if p.is_dir() {
        let canonical_path = dunce::canonicalize(p)?;
        if !visited_dirs.insert(canonical_path) {
            h.update([2]);
            return Ok(());
        }
        let mut entries = fs::read_dir(p)?.collect::<io::Result<Vec<_>>>()?;
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            h.update(entry.file_name().as_encoded_bytes());
            h.update([0]);
            hash_path(&entry.path(), h, visited_dirs)?;
        }
        h.update([1]);
    } else {
        let mut file = fs::File::open(p)?;
        let mut buffer = [0u8; 1 << 16];
        loop {
            let bytes_read = file.read(&mut buffer)?;
            if bytes_read == 0 {
                break;
            }
            h.update(&buffer[..bytes_read]);
        }
        h.update([0]);
    }
    Ok(())
}

#[derive(Default)]
struct Valid{
    path: Vec<PathBuf>,
    locations: Vec<String>,
}

impl Valid {
    fn add_inputs(&mut self, val: &Value){
        match val {
            Value::Array(items) => items.iter().for_each(|item| self.add_inputs(item)),
            Value::Object(map) => {
                if is_file_or_dir(map){
                    if let Some(p) = local_path(map) {
                        self.path.extend(dunce::canonicalize(p).ok());
                    } else if let Some(loc) = map.get("location").and_then(Value::as_str) && !self.locations.iter().any(|l| l ==loc){
                        self.locations.push(loc.to_owned());                  
                    }
                }
                map.values().for_each(|v| self.add_inputs(v));
            }
            _ => {}
        }
    }

    fn check(&self, val: &Value) -> Result<(), String> {
        match val {
            Value::Array(items) => {
                items.iter().try_for_each(|item| self.check(item))
            }
            Value::Object(object) => {
                if is_file_or_dir(object) {
                    if let Some(path) = local_path(object) {
                        self.check_path(&path, &mut HashSet::new())?;
                    } else if let Some(loc) =
                        object.get("location").and_then(Value::as_str) && !self.locations.iter().any(|allowed| allowed == loc) {
                            return Err(loc.to_owned());
                    }
                }

                object.values().try_for_each(|nested| self.check(nested))
            }
            _ => Ok(()),
        }
    }

     fn check_path(&self, p: &Path, visited: &mut HashSet<PathBuf>) -> Result<(), String> {
        let resolved = dunce::canonicalize(p).map_err(|_| p.display().to_string())?;
        if !self.path.iter().any(|root| resolved.starts_with(root)) {
            return Err(p.display().to_string());
        }
        if resolved.is_dir() && visited.insert(resolved.clone()) {
            let entries = fs::read_dir(&resolved).map_err(|_| p.display().to_string())?;
            for entry in entries {
                let entry = entry.map_err(|_| p.display().to_string())?;
                self.check_path(&entry.path(), visited)?;
            }
        }
        Ok(())
    }
}

pub(crate) fn remove_job_dir(dir: &Path) -> crate::Result<()> {
    match fs::remove_dir_all(dir){
        Err(e) if e.kind() != io::ErrorKind::NotFound => {
            Err(e).with_context(|| { format!("Could not remove job directory {}", dir.display())})?
        }
        _ => Ok(())
    }
}

fn is_file_or_dir(map: &serde_json::Map<String, Value>) -> bool {
    matches!(map.get("class").and_then(Value::as_str), Some("File" | "Directory"))
}

fn local_path(map: &serde_json::Map<String, Value>) -> Option<PathBuf> {
    let location = map.get("location").or_else(|| map.get("path"))?.as_str()?;
    if location.starts_with("file://") {
        Url::parse(location).ok()?.to_file_path().ok()
    } else if location.contains("://") {
        None
    } else {
        Some(PathBuf::from(location))
    }
}

fn remove_staging_paths(val: &mut Value) {
    match val {
        Value::Array(items) => items.iter_mut().for_each(remove_staging_paths),
        Value::Object(map) => {
            if is_file_or_dir(map) && map.contains_key("location") {
                map.remove("path");
                map.remove("dirname");
            }
            map.values_mut().for_each(remove_staging_paths);
        }
        _ => {}
    }
}

fn hash_file_stats(val: &Value, h: &mut Sha1) -> io::Result<()> {
    match val {
        Value::Array(items) => items.iter().try_for_each(|i| hash_file_stats(i, h)),
        Value::Object(map) => {
            if is_file_or_dir(map) && let Some(path) = local_path(map) && path.exists(){
                hash_path(&path, h, &mut HashSet::new())?;
            }
            map.values().try_for_each(|v| hash_file_stats(v, h))
        }
        _ => Ok(()),
    }
}

fn files_exist(val: &Value) -> bool {
     match val {
        Value::Array(items) => items.iter().all(files_exist),
        Value::Object(map) => {
            let exists = !is_file_or_dir(map) || local_path(map).is_none_or(|p| p.exists());
            exists && map.values().all(files_exist)
        }
        _ => true,
    }
}

pub(crate) fn create_private_dir(dir: impl AsRef<std::path::Path>) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
        fs::DirBuilder::new().recursive(true).mode(0o700).create(&dir)?;
        fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700))
    }
    #[cfg(not(unix))]
    std::fs::create_dir_all(dir)
}
