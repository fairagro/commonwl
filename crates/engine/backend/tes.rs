use crate::{
    backend::{
        DEFAULT_DOCKER_CONTAINER, TaskBackend, TaskExecutionRequest, TaskExecutionResult,
        mount::{MountStrategy, mount_input, mount_workdir_item},
    },
    string_url_to_path_string,
};
use async_trait::async_trait;
use crankshaft::{
    config::backend::tes::Config,
    engine::{
        Task,
        service::{
            name::{GeneratorIterator, UniqueAlphanumeric},
            runner::backend::tes,
        },
        task::{Execution, Output, Resources, output},
    },
};
use cwl_core::files::FileOrDirectory;
use cwl_engine_storage::{Storage, StorageBackend, StoragePath};
use nonempty::nonempty;
use std::{
    collections::HashMap,
    path::Path,
    sync::{Arc, Mutex},
};
use tokio_util::sync::CancellationToken;
use url::Url;
use uuid::Uuid;

const CONTAINER_WORKDIR: &str = "/mnt/task/workdir";
const CONTAINER_TMPDIR: &str = "/mnt/task/tmp";
const CONTAINER_INPUT_DIR: &str = "/mnt/task/inputs";
const CONTAINER_STDOUT_FILE: &str = "/mnt/task/stdout";
const CONTAINER_STDERR_FILE: &str = "/mnt/task/stderr";

#[derive(Debug, Clone)]
pub struct TesBackend {
    storage: Arc<StorageBackend>,
    data_store: StoragePath,
    backend: Arc<tes::Backend>,
}

impl TesBackend {
    /// Creates a new instance of `TesBackend`
    /// # Errors
    /// ??
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
        let backend = Arc::new(tes::Backend::initialize(config, names, None).await);

        Ok(Self {
            storage,
            data_store,
            backend,
        })
    }
}

#[allow(clippy::too_many_lines)]
#[async_trait]
impl TaskBackend for TesBackend {
    async fn run(
        &self,
        request: &TaskExecutionRequest<'_>,
        token: CancellationToken,
    ) -> anyhow::Result<TaskExecutionResult> {
        //handle docker requirement
        let resolved =
            crate::backend::resolve_docker_requirement(request.docker, DEFAULT_DOCKER_CONTAINER);
        if resolved.dockerfile.is_some() {
            anyhow::bail!(
                "TES backend does not support building images from a Dockerfile; provide dockerPull or a pre-built dockerImageId"
            );
        }
        let container = resolved.image_id;

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

        let outdir = request.outdir.as_url()?;

        let args = request
            .command
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>();

        //build crankshaft task object
        // Deliberately not wired to `Execution::stdin` as TES implementations may truncate the file
        // appended to args as trailing in backend.rs
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
                    .build(),
                // mark all created empty dirs with the s3 marker
                Execution::builder()
                    .work_dir(request.execution_path)
                    .program("find")
                    .args([
                        ".".to_string(),
                        "-type".to_string(),
                        "d".to_string(),
                        "-empty".to_string(),
                        "-exec".to_string(),
                        "touch".to_string(),
                        format!("{{}}/{}", crate::backend::mount::S3_EMPTY_DIR_MARKER),
                        ";".to_string(),
                    ])
                    .image(DEFAULT_DOCKER_CONTAINER)
                    .build()
            ])
            .resources(
                Resources::builder()
                    .cpu(request.runtime.cores as f64)
                    //.disk(request.runtime.outdir_size) //we don't use this currently
                    .ram(request.runtime.ram as f64 / 1024.0)
                    .build(),
            )
            .build();

        //add file inputs to task
        let mut inputs = request.inputs.to_vec();
        let mut new_destinations = self.upload_files_parallel(&inputs, &outdir).await?;
        for (i, input) in inputs.iter_mut().enumerate() {
            if let Some(dest) = new_destinations.remove(&i) {
                input.set_location(Some(dest.to_string()));
            }
            mount_input(&mut task, input)?;
        }

        let mut set = tokio::task::JoinSet::new();
        let sem = Arc::new(tokio::sync::Semaphore::new(32));

        for mount in request.mounts.iter().cloned() {
            let outdir = request.outdir.to_owned();
            let workdir = request.execution_path.to_owned();
            let use_container = request.use_container;
            let storage = self.storage();
            let permit = sem.clone().acquire_owned().await?;

            set.spawn(async move {
                let _permit = permit;
                mount_workdir_item(
                    mount,
                    &outdir.as_url()?,
                    &workdir,
                    use_container,
                    storage,
                    MountStrategy::Remote {
                        base_url: outdir.as_url()?,
                    },
                )
                .await
            });
        }

        while let Some(res) = set.join_next().await {
            for input in res?? {
                task.add_input(input);
            }
        }

        task.add_output(
            Output::builder()
                .name("workdir")
                .path(request.execution_path)
                .url(outdir.clone())
                .ty(output::Type::Directory)
                .build(),
        );

        task.add_output(
            Output::builder()
                .name("tmpdir")
                .path(self.container_tmp_dir())
                .url(request.tmpdir.as_url()?)
                .ty(output::Type::Directory)
                .build(),
        );

        //handle stderr output
        let stderr_path = crate::backend::resolve_output_location(
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
                .url(stderr_path.as_url()?)
                .ty(output::Type::File)
                .build(),
        );
        //handle stdout output
        let stdout_path = crate::backend::resolve_output_location(
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
                .url(stdout_path.as_url()?)
                .ty(output::Type::File)
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

        Ok(TaskExecutionResult {
            exit_status,
            stdout_file: stdout_path,
            stderr_file: stderr_path,
        })
    }

    fn task_scoped(&self) -> Arc<dyn TaskBackend> {
        Arc::clone(&Arc::new(self.clone())) as Arc<dyn TaskBackend>
    }

    fn storage(&self) -> Arc<StorageBackend> {
        self.storage.clone()
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

impl TesBackend {
    async fn upload_files_parallel(
        &self,
        inputs: &[FileOrDirectory],
        outdir: &Url,
    ) -> anyhow::Result<HashMap<usize, Url>> {
        //create a list of upload tasks (the new urls)
        let upload_tasks: Vec<(usize, String, Url)> = inputs
            .iter()
            .enumerate()
            .filter_map(|(i, input)| {
                let location = input.location()?;
                let path = string_url_to_path_string(location).ok()?;
                let dest = outdir
                    .join(&format!(
                        "inputs/{}{}",
                        &Uuid::new_v4().to_string()[..8],
                        input.basename()?
                    ))
                    .ok()?;
                Some((i, path, dest))
            })
            .collect();

        //run uploads
        let mut set = tokio::task::JoinSet::new();
        let sem = Arc::new(tokio::sync::Semaphore::new(32));
        for (i, path, dest) in upload_tasks {
            let permit = sem.clone().acquire_owned().await?;
            let storage = self.storage.clone();
            set.spawn(async move {
                let _permit = permit; //reference semaphore
                storage.upload(Path::new(&path), &dest).await?;
                anyhow::Ok((i, dest))
            });
        }

        //fech results
        let mut dest_updates: HashMap<usize, Url> = HashMap::new();
        while let Some(res) = set.join_next().await {
            let (i, dest) = res??;
            dest_updates.insert(i, dest);
        }

        Ok(dest_updates)
    }
}
