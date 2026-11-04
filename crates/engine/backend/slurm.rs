use crate::{
    backend::{
        DEFAULT_DOCKER_CONTAINER, TaskBackend, TaskExecutionRequest, TaskExecutionResult,
        mount::{MountStrategy, mount_input, mount_workdir_item},
    },
    docker::{ContainerBuildOptions, ContainerEngine, build_container_command},
};
use anyhow::Context;
use async_trait::async_trait;
use chrono::Utc;
use crankshaft::engine::{
    Task,
    task::{
        Execution, Input, Output, Resources,
        input::{self, Contents},
        output::{self},
    },
};
use cwl_core::{files::FileOrDirectory, requirements::StringOrInclude};
use cwl_engine_storage::{StorageBackend, StoragePath};
use nonempty::nonempty;
use std::{fs, path::PathBuf, sync::Arc};
use tokio_util::sync::CancellationToken;
use tracing::{debug, warn};
use url::Url;
use uuid::Uuid;

pub mod command;

pub use self::command::{SlurmCommandBackend, SlurmConfig};

#[derive(Debug)]
pub struct SlurmBackend {
    uuid: String,
    container_engine: ContainerEngine,
    storage: Arc<StorageBackend>,
    data_store: StoragePath,
    base_path: PathBuf,
    backend: Arc<SlurmCommandBackend>,
}

impl SlurmBackend {
    pub fn new(
        container_engine: ContainerEngine,
        storage: Arc<StorageBackend>,
        data_store: StoragePath,
        slurm_config: SlurmConfig,
    ) -> crate::Result<Self> {
        crate::ensure!(data_store.is_local(),"slurm requires a data store on a filesystem that is shared with the clusters compute nodes");
        let base_path = data_store.as_local_path()?;
        let backend = Arc::new(SlurmCommandBackend::new(storage.clone(), slurm_config));
        Ok(Self {
            uuid: Uuid::new_v4().to_string()[..8].to_string(),
            container_engine,
            storage,
            data_store,
            base_path,
            backend,
        })
    }

    fn step_dir(&self) -> PathBuf {
        self.base_path.join(&self.uuid)
    }
}

#[allow(clippy::too_many_lines)]
#[async_trait]
impl TaskBackend for SlurmBackend {
    async fn run(
        &self,
        request: &TaskExecutionRequest<'_>,
        token: CancellationToken,
    ) -> crate::Result<TaskExecutionResult> {        
        let started_at = Utc::now().naive_utc();
        let stdout_file = if let Some(s) = request.stdout_file {
            &format!("{}/{s}", self.container_tmp_dir())
        } else {
            &format!("{}/stdout", self.container_tmp_dir())
        };
        let stderr_file = if let Some(s) = request.stderr_file {
            &format!("{}/{s}", self.container_tmp_dir())
        } else {
            &format!("{}/stderr", self.container_tmp_dir())
        };
        let mut inputs = request.inputs.to_vec();
        crate::ensure!(request.outdir.is_local(), "slurm backend requires a local outdir");
        let outdir = request.outdir.as_local_path()?;
        crate::ensure!(request.tmpdir.is_local(), "slurm backend requires a local tempdir");
        let tmpdir = request.tmpdir.as_local_path()?;
        for file in &mut inputs
            .iter_mut()
            .filter_map(|f| {
                if let FileOrDirectory::File(f) = f {
                    Some(f)
                } else {
                    None
                }
            })
            .filter(|f| f.location.is_none() && f.contents.is_some())
        {
            let file_uuid = &Uuid::new_v4().to_string()[0..8];
            let location = tmpdir.join(file_uuid);
            debug!("Writing File literal to {:?}", location);
            fs::write(&location, file.contents.as_ref().unwrap()).with_context(|| {
                format!("Could not lock in File Literal at {}", location.display())
            })?;
            file.location = Some(
                Url::from_file_path(&location)
                    .map_err(|()| {
                        anyhow::anyhow!("Could not create URL from {}", location.display())
                    })?
                    .to_string(),
            );
        }
        let mut args = request.command.iter().map(ToString::to_string).collect::<Vec<_>>();

        let (extras, mounts): (Vec<_>, Vec<_>) = request.mounts.iter().cloned().partition(|m| {
            !m.target.to_file_path().unwrap().starts_with(&outdir) && request.use_container
        });
        if request.docker.is_some() {
            let resolved = crate::backend::resolve_docker_requirement(
                request.docker,
                DEFAULT_DOCKER_CONTAINER,
            );

            let df = match &resolved.dockerfile {
                Some(StringOrInclude::Include(include)) => Some(include.include.clone()),
                Some(StringOrInclude::String(s)) => {
                    let tmp = request.tmpdir.join("Dockerfile")?;
                    self.storage
                        .upload_bytes(s.as_bytes(), &tmp.as_url()?)
                        .await?;
                    Some(tmp.as_local_path()?.to_string_lossy().into_owned())
                }
                None => None,
            };

            let options = ContainerBuildOptions::builder()
                .docker_image_id(resolved.image_id)
                .network(request.network)
                .engine(self.container_engine)
                .env(request.env.clone())
                .outdir(self.container_work_dir())
                .tmpdir(request.runtime.tmpdir.to_string_lossy())
                .workdir(request.execution_path)
                .maybe_docker_file(df)
                .mounts(extras)
                .build();
            args = build_container_command(args, &inputs, options, request.specificationdir)?;
        }
        //build crankshaft task object
        #[allow(clippy::cast_precision_loss)]
        let mut task = Task::builder()
            .name(request.id)
            .maybe_description(request.description)
            .executions(nonempty![
                Execution::builder()
                    .work_dir(self.container_work_dir())
                    .env(request.env.clone())
                    .program(&args[0])
                    .args(&args[1..])
                    .images(["unsupported"])
                    .expect("image list must not be empty")
                    .stdout(stdout_file)
                    .stderr(stderr_file)
                    .maybe_stdin(request.stdin_file)
                    .build()
            ])
            .resources(
                Resources::builder()
                    .cpu(request.runtime.cores as f64)
                    .disk(request.runtime.outdir_size as f64)
                    .ram(request.runtime.ram as f64 / 1024.0)
                    .build(),
            )
            .build();

        for input in &inputs {
            mount_input(&mut task, input)?;
        }

        for mount in mounts {
            let inputs = mount_workdir_item(
                mount.clone(),
                &request.outdir.as_url()?,
                request.execution_path,
                request.use_container,
                self.storage(),
                MountStrategy::Local,
            )
            .await?;
            for input in inputs {
                task.add_input(input);
            }
        }

        task.add_input(
            Input::builder()
                .name("outdir")
                .contents(Contents::Path(outdir.clone()))
                .path(self.container_work_dir())
                .ty(input::Type::Directory)
                .read_only(false)
                .build(),
        );

        task.add_input(
            Input::builder()
                .name("tmpdir")
                .contents(Contents::Path(tmpdir.clone()))
                .path(self.container_tmp_dir())
                .ty(input::Type::Directory)
                .read_only(false)
                .build(),
        );

        let stderr_out = crate::backend::resolve_output_location(
            request.stderr_file,
            "stderr",
            request.outdir,
            request.tmpdir,
            request.eval_context,
        )?;
        task.add_output(
            Output::builder()
                .name("stderr")
                .path(stderr_file)
                .url(stderr_out.as_url()?)
                .ty(output::Type::File)
                .build(),
        );

        let stdout_out = crate::backend::resolve_output_location(
            request.stdout_file,
            "stdout",
            request.outdir,
            request.tmpdir,
            request.eval_context,
        )?;
        task.add_output(
            Output::builder()
                .name("stdout")
                .path(stdout_file)
                .url(stdout_out.as_url()?)
                .ty(output::Type::File)
                .build(),
        );

        task.add_output(
            Output::builder()
                .name("workdir")
                .path(self.container_work_dir())
                .url(Url::from_file_path(outdir).unwrap())
                .ty(output::Type::Directory)
                .build(),
        );

        let exit_status = crate::backend::run_with_timelimit(
            self.backend.as_ref(),
            task,
            token,
            request.timelimit,
            request.eval_context,
        )
        .await?;

        let finished_at = Utc::now().naive_utc();
        Ok(TaskExecutionResult {
            exit_status,
            stdout_file: stdout_out,
            stderr_file: stderr_out,
            started_at: Some(started_at),
            finished_at: Some(finished_at),
        })
    }

    fn task_scoped(&self) -> Arc<dyn TaskBackend> {
        Arc::new(SlurmBackend {
            uuid: Uuid::new_v4().to_string()[..8].to_string(),
            container_engine: self.container_engine,
            storage: self.storage(),
            data_store: self.data_store.clone(),
            base_path: self.base_path.clone(),
            backend: self.backend.clone(),
        }) as Arc<dyn TaskBackend>
    }

    fn storage(&self) -> Arc<StorageBackend> {
        self.storage.clone()
    }

    fn container_input_dir(&self) -> String {
        self.step_dir().join("inputs").to_string_lossy().into_owned()
    }
    fn container_work_dir(&self) -> String {
        self.step_dir().join("work").to_string_lossy().into_owned()
    }
    fn container_tmp_dir(&self) -> String {
        self.step_dir().join("tmp").to_string_lossy().into_owned()
    }

    fn data_store(&self) -> &StoragePath {
        &self.data_store
    }
}

impl Drop for SlurmBackend {
    fn drop(&mut self) {
        if fs::exists(self.container_input_dir()).unwrap_or_default() && let Err(e) = fs::remove_dir_all(self.container_input_dir()) {
            warn!("Could not remove backend's input directory: {e}");
        }
        if fs::exists(self.container_tmp_dir()).unwrap_or_default() && let Err(e) = fs::remove_dir_all(self.container_tmp_dir()) {
            warn!("Could not remove backend's tmp directory: {e}");
        }
        if fs::exists(self.container_work_dir()).unwrap_or_default() && let Err(e) = fs::remove_dir_all(self.container_work_dir()) {
            warn!("Could not remove backend's working directory: {e}");
        }
        if fs::exists(self.step_dir()).unwrap_or_default() && let Err(e) = fs::remove_dir_all(self.step_dir()){
            warn!("Could not remove backend's directory {}: {e}", self.step_dir().display());
        }
    }
}

#[cfg(test)]
#[cfg(unix)]
mod tests {
    use super::*;
    use crate::{create_execution_request, execute};
    use std::{os::unix::fs::PermissionsExt, path::Path};
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_slurm_backend_runs_tool_as_job() {
        let dir = dunce::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata")).unwrap();
        let cluster = tempdir().unwrap();
        let shared = tempdir().unwrap();
        let sbatch = cluster.path().join("sbatch");
        std::fs::write(
        &sbatch,
    r#"#!/bin/sh
                echo "$@" >> "$(dirname "$0")/sbatch.calls"
                script=$(mktemp)
                cat > "$script"
                echo 42
                sh "$script"
                status=$?
                rm -f "$script"
                exit $status
                "#,
        ).unwrap();
        std::fs::set_permissions(&sbatch, std::fs::Permissions::from_mode(0o755)).unwrap();
        let config = SlurmConfig::builder().sbatch(sbatch.to_string_lossy().into_owned())
            .sbatch_args(vec!["--partition=test".to_string()]).build();
        let backend = Arc::new(
            SlurmBackend::new(ContainerEngine::Apptainer, Arc::new(StorageBackend::new()),
                StoragePath::from_local(shared.path()), config).unwrap()
        );
        let outdir = tempdir().unwrap();
        let request = create_execution_request(dir.join("cat-tool.cwl"), dir.join("cat-job.json"), Some(outdir.path())).unwrap();
        let result = execute(backend, &request, CancellationToken::new()).await;
        assert!(result.is_ok(), "{result:?}");
        assert_eq!(fs::read_to_string(outdir.path().join("output")).unwrap(), fs::read_to_string(dir.join("hello.txt")).unwrap());
        let calls = fs::read_to_string(cluster.path().join("sbatch.calls")).unwrap();
        assert!(calls.contains("--partition=test"), "{calls}");
        assert!(calls.contains("--wait"), "{calls}");
    }

    #[test]
    fn test_slurm_backend_requires_local_data_store() {
        let url = Url::parse("s3://bucket/prefix/").unwrap();
        let backend = SlurmBackend::new(ContainerEngine::Apptainer, Arc::new(StorageBackend::new()),
            StoragePath::from_url(url), SlurmConfig::default());
        assert!(backend.is_err());
    }
}