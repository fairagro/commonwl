use crate::{
    backend::{
        DEFAULT_DOCKER_CONTAINER, TaskBackend, TaskExecutionRequest, TaskExecutionResult,
        mount::{MountStrategy, mount_input, mount_workdir_item},
    },
    string_url_to_path_string,
};
use async_trait::async_trait;
use chrono::Utc;
use crankshaft::{
    config::backend::tes::Config,
    engine::{
        Task,
        service::{
            name::{GeneratorIterator, UniqueAlphanumeric},
            runner::backend::tes,
        },
        task::{
            Execution, Input, Output, Resources,
            input::{self, Contents},
            output,
        },
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

const CONTAINER_TASK_DIR: &str = "/mnt/task";
const CONTAINER_WORKDIR: &str = "/mnt/task/workdir";
const CONTAINER_TMPDIR: &str = "/mnt/task/tmp";
const CONTAINER_INPUT_DIR: &str = "/mnt/task/inputs";
const CONTAINER_STDOUT_FILE: &str = "/mnt/task/stdout";
const CONTAINER_STDERR_FILE: &str = "/mnt/task/stderr";
/// Written by the scan executor below the tmpdir output, outside of the tool's results
const EMPTY_DIRS_FILE: &str = "cwl_empty_dirs";

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
    ) -> crate::Result<Self> {
        const NAME_BUFFER_LEN: usize = 4096;
        let names = Arc::new(Mutex::new(GeneratorIterator::new(
            UniqueAlphanumeric::default_with_expected_generations(NAME_BUFFER_LEN),
            NAME_BUFFER_LEN,
        )));
        let backend = Arc::new(tes::Backend::initialize(config, names).await);

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
    ) -> crate::Result<TaskExecutionResult> {
        let started_at = Utc::now().naive_utc();
        //handle docker requirement
        let resolved =
            crate::backend::resolve_docker_requirement(request.docker, DEFAULT_DOCKER_CONTAINER);
        if resolved.dockerfile.is_some() {
            crate::bail!(
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
        let executions = nonempty![
            Execution::builder()
                .work_dir(request.execution_path)
                .env(request.env.clone())
                .program(&args[0])
                .args(&args[1..])
                .images([&container])
                .expect("image list must not be empty")
                .stdout(stdout_file)
                .stderr(stderr_file)
                .build(),
            // TES only uploads files, so report the directories left empty
            Execution::builder()
                .work_dir(request.execution_path)
                .program("find")
                .args([
                    ".".to_string(),
                    "-type".to_string(),
                    "d".to_string(),
                    "-empty".to_string(),
                    "-print0".to_string(),
                ])
                .images([DEFAULT_DOCKER_CONTAINER])
                .expect("image list must not be empty")
                .stdout(format!("{CONTAINER_TMPDIR}/{EMPTY_DIRS_FILE}"))
                .build()
        ];

        // crankshaft only takes volumes at build time, so collect the inputs on a staging task
        let mut staging = Task::builder().executions(executions.clone()).build();

        //add file inputs to task
        let mut inputs = request.inputs.to_vec();
        let mut new_destinations = self.upload_files_parallel(&inputs, &outdir).await?;
        for (i, input) in inputs.iter_mut().enumerate() {
            if let Some(dest) = new_destinations.remove(&i) {
                input.set_location(Some(dest.to_string()));
            }
            mount_input(&mut staging, input)?;
        }

        let mut set = tokio::task::JoinSet::new();
        let sem = Arc::new(tokio::sync::Semaphore::new(32));

        for mount in request.mounts.iter().cloned() {
            let outdir = request.outdir.to_owned();
            let workdir = request.execution_path.to_owned();
            // represents if there is a docker requirement
            // a container in tes is always used but cwl e.g. only allows absolute path in iwdr when docker requirement is specified
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
                staging.add_input(input);
            }
        }

        let (task_inputs, volumes) = self
            .empty_dirs_to_volumes(staging.inputs().cloned().collect())
            .await?;

        #[allow(clippy::cast_precision_loss)]
        let mut task = Task::builder()
            .name(request.id)
            .maybe_description(request.description)
            .executions(executions)
            .inputs(task_inputs)
            .volumes(volumes)
            .resources(
                Resources::builder()
                    .cpu(request.runtime.cores as f64)
                    //.disk(request.runtime.outdir_size) //we don't use this currently
                    .ram(request.runtime.ram as f64 / 1024.0)
                    .build(),
            )
            .build();

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

        // evaluate side task exit status
        if let Some(scan_status) = exit_status.get(1)
            && !scan_status.success()
        {
            tracing::warn!(
                "empty-directory scan exited with {scan_status}; \
                 empty output directories may be missing from results"
            );
        }

        if let Err(e) = self.store_empty_dirs(request).await {
            tracing::warn!("could not store empty output directories: {e}");
        }

        let finished_at = Utc::now().naive_utc();
        Ok(TaskExecutionResult {
            exit_status,
            stdout_file: stdout_path,
            stderr_file: stderr_path,
            started_at: Some(started_at),
            finished_at: Some(finished_at),
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
    /// Funnel only stages objects, so folder keys below a directory input never reach the task.
    /// Those directories become volumes, and an input that is empty as a whole is dropped.
    async fn empty_dirs_to_volumes(
        &self,
        inputs: Vec<Input>,
    ) -> crate::Result<(Vec<Input>, Vec<String>)> {
        let mut kept = Vec::with_capacity(inputs.len());
        let mut volumes = vec![];

        for input in inputs {
            let empty_dirs = match (input.ty(), input.contents()) {
                (input::Type::Directory, Contents::Url(url)) if url.scheme() == "s3" => {
                    self.storage.list_empty_dirs(url).await?
                }
                _ => vec![],
            };

            let base = input.path().trim_end_matches('/');
            volumes.extend(empty_dirs.iter().map(|dir| {
                if dir.is_empty() {
                    base.to_string()
                } else {
                    format!("{base}/{dir}")
                }
            }));

            if !empty_dirs.iter().any(String::is_empty) {
                kept.push(input);
            }
        }

        // Funnel folds only one nested volume into a parent mount, the rest stay docker arguments
        // that it does not quote. Listing the task root first folds them all.
        if !volumes.is_empty() {
            volumes.insert(0, CONTAINER_TASK_DIR.to_string());
        }

        Ok((kept, volumes))
    }

    /// Stores the directories the scan found empty, as Funnel only uploads files.
    async fn store_empty_dirs(&self, request: &TaskExecutionRequest<'_>) -> crate::Result<()> {
        let file = request.tmpdir.join(EMPTY_DIRS_FILE)?.as_url()?;
        let empty_dirs = self.storage.read_file(&file).await?;

        let mut set = tokio::task::JoinSet::new();
        let sem = Arc::new(tokio::sync::Semaphore::new(32));
        for dir in empty_dirs.split('\0').filter(|dir| !dir.is_empty()) {
            let dest = request.outdir.join(dir)?.as_url()?;
            let permit = sem.clone().acquire_owned().await?;
            let storage = self.storage.clone();
            set.spawn(async move {
                let _permit = permit; //reference semaphore
                storage.create_dir(&dest).await
            });
        }

        while let Some(res) = set.join_next().await {
            res??;
        }

        Ok(())
    }

    async fn upload_files_parallel(
        &self,
        inputs: &[FileOrDirectory],
        outdir: &Url,
    ) -> crate::Result<HashMap<usize, Url>> {
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
