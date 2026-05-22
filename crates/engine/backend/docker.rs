use crate::{
    backend::{
        TaskBackend, TaskExecutionRequest, TaskExecutionResult,
        mount::{MountStrategy, mount_input, mount_workdir_item},
    },
    docker::build_container,
    expression::{do_eval, do_eval_to_string},
};
use anyhow::ensure;
use async_trait::async_trait;
use crankshaft::{
    config::backend::docker::Config,
    docker::Docker,
    engine::{
        Task,
        service::{
            name::{GeneratorIterator, UniqueAlphanumeric},
            runner::{
                Backend,
                backend::{TaskRunError, docker},
            },
        },
        task::{
            Execution, Input, Output, Resources,
            input::{self, Contents},
            output,
        },
    },
};
use cwl_core::{IntegerOrExpression, requirements::StringOrInclude};
use cwl_engine_storage::{StorageBackend, StoragePath};
use nonempty::nonempty;
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio_util::sync::CancellationToken;
use tracing::error;
use url::Url;
use uuid::Uuid;

const CONTAINER_WORKDIR: &str = "/mnt/task/workdir";
const CONTAINER_TMPDIR: &str = "/mnt/task/tmp";
const CONTAINER_INPUT_DIR: &str = "/mnt/task/inputs";
const CONTAINER_STDOUT_FILE: &str = "/mnt/task/stdout";
const CONTAINER_STDERR_FILE: &str = "/mnt/task/stderr";

#[derive(Debug, Clone)]
pub struct DockerBackend {
    //wrapper to Bollard Docker client
    client: Docker,
    storage: Arc<StorageBackend>,
    data_store: StoragePath,
    //wrapper to crankshaft backend
    backend: Arc<docker::Backend>,
}

impl DockerBackend {
    /// Creates a new instance of `DockerBackend`
    /// # Errors
    /// Throws if the docker client is not reachable
    pub async fn new(
        config: Config,
        storage: Arc<StorageBackend>,
        data_store: StoragePath,
    ) -> anyhow::Result<Self> {
        const NAME_BUFFER_LEN: usize = 4096;
        let names = Arc::new(Mutex::new(GeneratorIterator::new(
            UniqueAlphanumeric::default_with_expected_generations(NAME_BUFFER_LEN),
            NAME_BUFFER_LEN,
        )));
        let backend =
            Arc::new(docker::Backend::initialize_default_with(config, names, None).await?);

        let client = Docker::with_defaults()?;

        Ok(Self {
            client,
            storage,
            data_store,
            backend,
        })
    }
}

#[allow(clippy::too_many_lines)]
#[async_trait]
impl TaskBackend for DockerBackend {
    async fn run(
        &self,
        request: &TaskExecutionRequest<'_>,
        token: CancellationToken,
    ) -> anyhow::Result<TaskExecutionResult> {
        //handle docker requirement
        let mut container = "ubuntu:noble".to_string(); //add config "default-container"
        if let Some(dr) = request.docker {
            if let Some(df) = &dr.docker_file
                && let Some(dt) = &dr.docker_image_id
            {
                let df = match df {
                    StringOrInclude::Include(include) => include.include.clone(),
                    StringOrInclude::String(s) => {
                        let tmp = request.tmpdir.join("Dockerfile")?;
                        self.storage
                            .upload_bytes(s.as_bytes(), &tmp.as_url()?)
                            .await?;
                        tmp.as_local_path()?.to_string_lossy().into_owned()
                    }
                };
                build_container(self.client.inner(), df, dt).await?;
                container = dt.clone();
            } else if let Some(dp) = &dr.docker_pull {
                container = dp.clone();
            }
        }

        let stdout_file = if let Some(s) = request.stdout_file {
            &format!("/mnt/task/{s}")
        } else {
            CONTAINER_STDOUT_FILE
        };

        let stderr_file = if let Some(s) = request.stderr_file {
            &format!("/mnt/task/{s}")
        } else {
            CONTAINER_STDERR_FILE
        };

        //this is a local only backend so we assume outdir is local
        ensure!(request.outdir.is_local());
        let outdir = request.outdir.as_local_path()?;
        ensure!(request.tmpdir.is_local());
        let tmpdir = request.tmpdir.as_local_path()?;

        let mut args = request
            .command
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>();
        // manually check for an entypoint
        if let Some(entrypoint) = self
            .get_docker_entrypoint(&container, token.clone())
            .await?
        {
            args.splice(0..0, entrypoint);
        }

        //build crankshaft task object
        #[allow(clippy::cast_precision_loss)]
        let mut task = Task::builder()
            .name(request.id)
            .maybe_description(request.description)
            .executions(nonempty![
                Execution::builder()
                    .work_dir(request.execution_path)
                    .env(request.env.clone())
                    .program(&args[0])
                    .args(&args[1..])
                    .image(container)
                    .stdout(stdout_file)
                    .stderr(stderr_file)
                    .maybe_stdin(request.stdin_file)
                    .build()
            ])
            .resources(
                Resources::builder()
                    .cpu(request.runtime.cores as f64)
                    //.disk(request.runtime.outdir_size) //we don't use this currently
                    .ram(request.runtime.ram as f64)
                    .build(),
            )
            .build();

        //add file inputs to task
        for input in request.inputs {
            mount_input(&mut task, input)?;
        }

        for mount in request.mounts {
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

        //add outdir mount
        task.add_input(
            Input::builder()
                .name("outdir")
                .contents(Contents::Path(outdir.clone()))
                .path(request.execution_path)
                .ty(input::Type::Directory)
                .read_only(false)
                .build(),
        );

        //add tmpdir input
        task.add_input(
            Input::builder()
                .name("tmpdir")
                .contents(Contents::Path(tmpdir.clone()))
                .path(CONTAINER_TMPDIR)
                .ty(input::Type::Directory)
                .read_only(false)
                .build(),
        );

        //handle stderr output
        let stderr_out_file = if let Some(stderr) = request.stderr_file {
            let stderr = do_eval_to_string(stderr, request.eval_context);
            outdir.join(stderr)
        } else {
            tmpdir.join(format!("stderr_{}", &Uuid::new_v4().to_string()[..8]))
        };
        task.add_output(
            Output::builder()
                .name("stderr")
                .path(stderr_file)
                .url(Url::from_file_path(&stderr_out_file).unwrap())
                .ty(output::Type::File)
                .build(),
        );

        //handle stdout output
        let stdout_out_file = if let Some(stdout) = request.stdout_file {
            let stdout = do_eval_to_string(stdout, request.eval_context);
            outdir.join(stdout)
        } else {
            tmpdir.join(format!("stdout_{}", &Uuid::new_v4().to_string()[..8]))
        };
        task.add_output(
            Output::builder()
                .name("stdout")
                .path(stdout_file)
                .url(Url::from_file_path(&stdout_out_file).unwrap())
                .ty(output::Type::File)
                .build(),
        );

        let timelimit = request
            .timelimit
            .as_ref()
            .and_then(|ttl| match &ttl.timelimit {
                IntegerOrExpression::Int(i) => Some(i64::from(*i)),
                IntegerOrExpression::Long(i) => Some(*i),
                IntegerOrExpression::Expression(e) => {
                    let value = do_eval(e, request.eval_context).ok()?;
                    value.as_i64()
                }
            });
        let exit_status = if let Some(timeout) = timelimit
            && timeout != 0
        {
            let limit = Duration::from_secs(timeout.try_into()?); //this is intended to throw!
            let token_clone = token.clone();
            tokio::select! {
                result = self.backend.run(task, token)? => result?,
                () = tokio::time::sleep(limit) => {
                    token_clone.cancel();
                    error!("Timelimit reached: {timeout}");
                    return Err(TaskRunError::Canceled.into());
                }
            }
        } else {
            //no time constraint
            self.backend.run(task, token)?.await?
        };

        Ok(TaskExecutionResult {
            exit_status,
            stdout_file: StoragePath::Local(stdout_out_file),
            stderr_file: StoragePath::Local(stderr_out_file),
        })
    }

    fn storage(&self) -> Arc<StorageBackend> {
        self.storage.clone()
    }

    fn task_scoped(&self) -> Arc<dyn TaskBackend> {
        Arc::clone(&Arc::new(self.clone())) as Arc<dyn TaskBackend>
    }

    fn container_input_dir(&self) -> String {
        CONTAINER_INPUT_DIR.to_string()
    }

    fn container_work_dir(&self) -> String {
        CONTAINER_WORKDIR.to_string()
    }

    fn container_tmp_dir(&self) -> String {
        CONTAINER_TMPDIR.to_string()
    }

    fn data_store(&self) -> &StoragePath {
        &self.data_store
    }
}

impl DockerBackend {
    ///Crankshaft backend overwrites docker entrypoint, so we need to get it beforehand and append it to the command
    async fn get_docker_entrypoint(
        &self,
        container: &str,
        token: CancellationToken,
    ) -> anyhow::Result<Option<Vec<String>>> {
        //ensure image
        self.client.ensure_image(container, token).await?;
        let info = self.client.inner().inspect_image(container).await?;
        if let Some(cfg) = info.config
            && let Some(entrypoint) = cfg.entrypoint
        {
            return Ok(Some(entrypoint));
        }
        Ok(None)
    }
}

#[cfg(test)]
#[cfg(not(target_os = "macos"))] //ignore on macos because of CI issues with docker
mod tests {
    use super::*;
    use crate::{backend::execute_commandline_tool, request::create_execution_request};
    use cwl_engine_storage::StorageBackend;
    use std::{env, path::Path};
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_docker_backend_creation() {
        let config = Config::default();
        let storage = Arc::new(StorageBackend::new());
        let data_store = StoragePath::from_local(&env::temp_dir());
        let backend = DockerBackend::new(config, storage, data_store).await;
        assert!(backend.is_ok());
    }

    #[tokio::test]
    async fn test_docker_backend_run_simple() {
        let base_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../testdata/cwl/tests")
            .canonicalize()
            .unwrap();
        let specification_path = base_dir.join("cat-tool-shortcut.cwl");
        let inputs_path = base_dir.join("cat-job.json");

        let config = Config::default();
        let storage = Arc::new(StorageBackend::new());
        let data_store = StoragePath::from_local(&env::temp_dir());
        let backend = Arc::new(
            DockerBackend::new(config, storage, data_store)
                .await
                .unwrap(),
        );
        let tmpdir = tempdir().unwrap();
        let request =
            create_execution_request(specification_path, inputs_path, Some(tmpdir.path())).unwrap();
        let cancellation_token = CancellationToken::new();
        let result = execute_commandline_tool(backend, &request, cancellation_token).await;
        assert!(result.is_ok());

        //check if output file exists
        let out_file = tmpdir.path().join("output");
        assert!(out_file.exists());
    }

    #[tokio::test]
    async fn test_docker_backend_run_simple_with_dir() {
        let base_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../testdata/cwl/tests")
            .canonicalize()
            .unwrap();
        let specification_path = base_dir.join("dir3.cwl");
        let inputs_path = base_dir.join("dir3-job.yml");

        let config = Config::default();
        let storage = Arc::new(StorageBackend::new());
        let data_store = StoragePath::from_local(&env::temp_dir());
        let backend = Arc::new(
            DockerBackend::new(config, storage, data_store)
                .await
                .unwrap(),
        );
        let tmpdir = tempdir().unwrap();
        let request =
            create_execution_request(specification_path, inputs_path, Some(tmpdir.path())).unwrap();
        let cancellation_token = CancellationToken::new();
        let result = execute_commandline_tool(backend, &request, cancellation_token).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_docker_backend_run_simple_with_value_from() {
        let base_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../testdata/cwl/tests")
            .canonicalize()
            .unwrap();
        let specification_path = base_dir.join("cat3-from-dir.cwl");
        let inputs_path = base_dir.join("cat-from-dir-job.yaml");

        let config = Config::default();
        let storage = Arc::new(StorageBackend::new());
        let data_store = StoragePath::from_local(&env::temp_dir());
        let backend = Arc::new(
            DockerBackend::new(config, storage, data_store)
                .await
                .unwrap(),
        );
        let tmpdir = tempdir().unwrap();
        let request =
            create_execution_request(specification_path, inputs_path, Some(tmpdir.path())).unwrap();
        let cancellation_token = CancellationToken::new();
        let result = execute_commandline_tool(backend, &request, cancellation_token).await;

        assert!(result.is_ok());
        //check if output file exists
        let out_file = tmpdir.path().join("output.txt");

        assert!(out_file.exists());
    }
}
