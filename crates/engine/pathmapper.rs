use cwl_core::{
    files::{Directory, File, FileOrDirectory},
    inputs::DefaultValue,
};
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};

#[derive(Debug)]
pub struct PathMapper {
    mappings: HashMap<PathBuf, PathBuf>,
    local_mappings: HashMap<PathBuf, PathBuf>,
    base_dir: PathBuf,
    stage_dir: PathBuf,
}

impl PathMapper {
    //inputs need to be collected before creating the pathmapper
    pub fn new(
        inputs: &HashMap<String, DefaultValue>,
        base_dir: &Path,
        stage_dir: &Path,
    ) -> anyhow::Result<Self> {
        let mut mappings = HashMap::new();
        for value in inputs.values() {
            Self::collect_files(value, base_dir, stage_dir, &mut mappings)?
        }

        let locals = mappings
            .keys()
            .map(|host_path| {
                let relative_path = host_path.strip_prefix(base_dir).unwrap_or(host_path);
                (relative_path.to_path_buf(), mappings[host_path].clone())
            })
            .collect::<HashMap<_, _>>();

        Ok(Self {
            mappings,
            local_mappings: locals,
            base_dir: base_dir.to_path_buf(),
            stage_dir: stage_dir.to_path_buf(),
        })
    }

    pub fn correct_execution_path(&self, mut args: Vec<String>) -> Vec<String> {
        for arg in &mut args {
            let pb = PathBuf::from(arg.clone());
            *arg = self
                .get_guest(&pb)
                .unwrap_or(&pb)
                .to_string_lossy()
                .into_owned();
        }
        args
    }

    pub fn get_guest(&self, host_path: impl AsRef<Path>) -> Option<&PathBuf> {
        let host = host_path.as_ref();

        self.local_mappings
            .get(host)
            .or_else(|| self.mappings.get(host))
    }

    pub fn get_host(&self, guest_path: impl AsRef<Path>) -> Option<&PathBuf> {
        let guest = guest_path.as_ref();
        self.mappings
            .iter()
            .find(|(_, v)| v.as_path() == guest)
            .map(|(k, _)| k)
    }

    pub fn add(&mut self, new_path: impl AsRef<Path>) -> anyhow::Result<()> {
        let relative_source = if new_path.as_ref().is_absolute() {
            new_path.as_ref().strip_prefix(&self.base_dir)?
        } else {
            new_path.as_ref()
        }
        .to_path_buf();

        let dest = self.stage_dir.join(&relative_source);
        let abs_source = self.base_dir.join(&relative_source);

        self.local_mappings.insert(relative_source, dest.clone());
        self.mappings.insert(abs_source, dest);

        Ok(())
    }

    fn collect_files(
        value: &DefaultValue,
        base_dir: &Path,
        stage_dir: &Path,
        mappings: &mut HashMap<PathBuf, PathBuf>,
    ) -> anyhow::Result<()> {
        match value {
            DefaultValue::FileOrDirectory(fod) => match fod {
                FileOrDirectory::File(file) => {
                    Self::handle_file(file, base_dir, stage_dir, mappings)?
                }
                FileOrDirectory::Directory(dir) => {
                    Self::handle_directory(dir, base_dir, stage_dir, mappings)?
                }
            },
            DefaultValue::Any(value) => match value {
                serde_yaml::Value::Sequence(values) => {
                    for value in values {
                        let value = serde_yaml::from_value(value.clone())?;
                        Self::collect_files(&value, base_dir, stage_dir, mappings)?;
                    }
                }
                serde_yaml::Value::Mapping(mapping) => {
                    for (_, value) in mapping {
                        let value = serde_yaml::from_value(value.clone())?;
                        Self::collect_files(&value, base_dir, stage_dir, mappings)?;
                    }
                }
                _ => {}
            },
        }
        Ok(())
    }

    fn handle_file(
        file: &File,
        base_dir: &Path,
        stage_dir: &Path,
        mappings: &mut HashMap<PathBuf, PathBuf>,
    ) -> anyhow::Result<()> {
        //ensure path is filled
        let mut file = file.to_owned();
        file.dry_validation();
        let Some(path) = file.path else {
            anyhow::bail!("File is missing a path")
        };

        let host_path = Self::resolve_location(&path, base_dir)?;
        let Some(filename) = host_path.file_name() else {
            anyhow::bail!("File is missing a path")
        };
        let staged_path = stage_dir.join(filename);

        mappings.insert(host_path, staged_path);

        if let Some(secondary) = file.secondary_files {
            for item in secondary {
                Self::collect_files(
                    &DefaultValue::FileOrDirectory(item),
                    base_dir,
                    stage_dir,
                    mappings,
                )?;
            }
        }

        Ok(())
    }

    fn handle_directory(
        dir: &Directory,
        base_dir: &Path,
        stage_dir: &Path,
        mappings: &mut HashMap<PathBuf, PathBuf>,
    ) -> anyhow::Result<()> {
        //ensure path is filled
        let mut dir = dir.to_owned();
        dir.dry_validation();
        let Some(path) = dir.path else {
            anyhow::bail!("File is missing a path")
        };

        let host_path = Self::resolve_location(&path, base_dir)?;
        let Some(filename) = host_path.file_name() else {
            anyhow::bail!("File is missing a path")
        };
        let staged_path = stage_dir.join(filename);

        //recursively walk dir with new roots
        for entry in fs::read_dir(&host_path)? {
            let entry = entry?;
            let value = if entry.file_type()?.is_dir() {
                DefaultValue::FileOrDirectory(FileOrDirectory::Directory(
                    Directory::builder()
                        .path(entry.path().to_string_lossy())
                        .build(),
                ))
            } else {
                DefaultValue::FileOrDirectory(FileOrDirectory::File(
                    File::builder().path(entry.path().to_string_lossy()).build(),
                ))
            };
            Self::collect_files(&value, &host_path, &staged_path, mappings)?
        }

        Ok(())
    }

    fn resolve_location(path: &str, base_dir: &Path) -> anyhow::Result<PathBuf> {
        let path = if Path::new(path).is_absolute() {
            PathBuf::from(path)
        } else {
            base_dir.join(path)
        };
        Ok(path.canonicalize()?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::collect_inputs;
    use cwl_core::load_cwl_file;
    use std::fs;

    #[test]
    fn test_pathmapper_init() {
        let base_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/cwl/tests");
        let cwl_file = base_dir.join("cat-tool.cwl");
        let input_file = base_dir.join("cat-job.json");

        let stage_dir = PathBuf::from("/mnt/69420/");

        let specification = load_cwl_file(cwl_file, true).unwrap();
        let inputs: HashMap<String, serde_yaml::Value> =
            serde_yaml::from_str(&fs::read_to_string(input_file).unwrap()).unwrap();
        let inputs = collect_inputs(&specification, &inputs).unwrap();

        let result = PathMapper::new(&inputs, &base_dir, &stage_dir);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().mappings.len(), 1);
    }

    #[test]
    fn test_pathmapper_init_recursively() {
        let base_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/cwl/tests");
        let cwl_file = base_dir.join("dir.cwl");
        let input_file = base_dir.join("dir-job.yml");

        let stage_dir = PathBuf::from("/mnt/69420/");

        let specification = load_cwl_file(cwl_file, true).unwrap();
        let inputs: HashMap<String, serde_yaml::Value> =
            serde_yaml::from_str(&fs::read_to_string(input_file).unwrap()).unwrap();
        let inputs = collect_inputs(&specification, &inputs).unwrap();

        let result = PathMapper::new(&inputs, &base_dir, &stage_dir);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().mappings.len(), 3);
    }

    #[test]
    fn test_pathmapper_init_array() {
        let base_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/cwl/tests");
        let cwl_file = base_dir.join("stage_file_array.cwl");
        let input_file = base_dir.join("stage_file_array.job.json");

        let stage_dir = PathBuf::from("/mnt/69420/");

        let specification = load_cwl_file(cwl_file, true).unwrap();
        let inputs: HashMap<String, serde_yaml::Value> =
            serde_yaml::from_str(&fs::read_to_string(input_file).unwrap()).unwrap();
        let inputs = collect_inputs(&specification, &inputs).unwrap();

        let result = PathMapper::new(&inputs, &base_dir, &stage_dir);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().mappings.len(), 2);
    }

    #[test]
    fn test_pathmapper_init_secondary_files() {
        let base_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/cwl/tests");
        let cwl_file = base_dir.join("dir4.cwl");
        let input_file = base_dir.join("dir4-job.yml");

        let stage_dir = PathBuf::from("/mnt/69420/");

        let specification = load_cwl_file(cwl_file, true).unwrap();
        let inputs: HashMap<String, serde_yaml::Value> =
            serde_yaml::from_str(&fs::read_to_string(input_file).unwrap()).unwrap();
        let inputs = collect_inputs(&specification, &inputs).unwrap();

        let result = PathMapper::new(&inputs, &base_dir, &stage_dir);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().mappings.len(), 5);
    }
}
