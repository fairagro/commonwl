use crate::{
    backend::{
        TaskBackend, TaskExecutionRequest, TaskExecutionResult,
        mount::{MountStrategy, mount_input, mount_workdir_item},
    },
    expression::{do_eval, do_eval_to_string},
};
use async_trait::async_trait;
use crankshaft::{
    config::backend::tes::Config,
    engine::{
        Task,
        service::{
            name::{GeneratorIterator, UniqueAlphanumeric},
            runner::{
                Backend,
                backend::{TaskRunError, tes},
            },
        },
        task::{Execution, Output, Resources, output},
    },
};
use cwl_core::{IntegerOrExpression, files::FileOrDirectory};
use cwl_engine_storage::{Storage, StorageBackend};
use nonempty::nonempty;
use std::{
    collections::HashMap,
    path::Path,
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
pub struct TesBackend {
    storage: Arc<StorageBackend>,
    backend: Arc<tes::Backend>,
}

impl TesBackend {
    /// Creates a new instance of `TesBackend`
    /// # Errors
    /// ??
    pub async fn new(config: Config, storage: Arc<StorageBackend>) -> anyhow::Result<Self> {
        const NAME_BUFFER_LEN: usize = 4096;
        let names = Arc::new(Mutex::new(GeneratorIterator::new(
            UniqueAlphanumeric::default_with_expected_generations(NAME_BUFFER_LEN),
            NAME_BUFFER_LEN,
        )));
        let backend = Arc::new(tes::Backend::initialize(config, names, None).await);

        Ok(Self { storage, backend })
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
        let mut container = "ubuntu".to_string(); //add config "default-container"
        if let Some(dr) = request.docker {
            if let Some(_df) = &dr.docker_file
                && let Some(dt) = &dr.docker_image_id
            {
                //build_container(self.client.inner(), df, dt).await?;
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

        let args = request
            .command
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>();

        //build crankshaft task object
        #[allow(clippy::cast_precision_loss)]
        let mut task = Task::builder()
            .name(request.id)
            .maybe_description(request.description)
            .executions(nonempty![
                Execution::builder()
                    .work_dir(request.staged_dir)
                    .env(request.env.clone())
                    .program(&args[0])
                    .args(&args[1..])
                    .image(container)
                    .stdout(stdout_file)
                    .stderr(stderr_file)
                    //.maybe_stdin(request.stdin_file)
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
        let mut new_destinations = self.upload_files_parallel(&inputs, request.id).await?;
        for (i, input) in inputs.iter_mut().enumerate() {
            if let Some(dest) = new_destinations.remove(&i) {
                input.set_location(Some(dest.to_string()));
            }
            mount_input(&mut task, input)?;
        }

        let s3_workdir = Url::parse(&format!("s3://test-bucket/{}/workdir/", request.id))?;
        let mut set = tokio::task::JoinSet::new();
        let sem = Arc::new(tokio::sync::Semaphore::new(32));
        for mount in request.mounts.iter().cloned() {
            let outdir = request.outdir.to_owned();
            let workdir = request.staged_dir.to_owned();
            let use_container = request.use_container;
            let storage = self.storage();
            let base_url = s3_workdir.clone();
            let permit = sem.clone().acquire_owned().await?;
            set.spawn(async move {
                let _permit = permit;
                mount_workdir_item(
                    mount,
                    &outdir,
                    &workdir,
                    use_container,
                    storage,
                    MountStrategy::Remote { base_url },
                )
                .await
            });
        }

        while let Some(res) = set.join_next().await {
            for input in res?? {
                task.add_input(input);
            }
        }

        //add outdir mount
        //task.add_input(
        //Input::builder()
        //        .name("outdir")
        //        .contents(Contents::Path(request.outdir.to_path_buf()))
        //        .path(request.staged_dir)
        //        .ty(input::Type::Directory)
        //        .read_only(false)
        //        .build(),
        //);
        //
        //add tmpdir input
        //task.add_input(
        //    Input::builder()
        //        .name("tmpdir")
        //        .contents(Contents::Path(request.tmpdir.to_path_buf()))
        //        .path(CONTAINER_TMPDIR)
        //        .ty(input::Type::Directory)
        //        .read_only(false)
        //        .build(),
        //);

        //handle stderr output
        let bucket_url = Url::parse(&format!("s3://test-bucket/{}/", request.id))?;
        let (stderr_local, stderr_remote) = if let Some(stderr) = request.stderr_file {
            let filename = do_eval_to_string(stderr, request.eval_context);
            (request.outdir.join(&filename), bucket_url.join(&filename)?)
        } else {
            let filename = format!("stderr_{}", &Uuid::new_v4().to_string()[..8]);
            (request.tmpdir.join(&filename), bucket_url.join(&filename)?)
        };
        task.add_output(
            Output::builder()
                .name("stderr")
                .path(stderr_file)
                .url(stderr_remote.clone())
                .ty(output::Type::File)
                .build(),
        );

        task.add_output(
            Output::builder()
                .name("workdir")
                .path(CONTAINER_WORKDIR)
                .url(s3_workdir.clone())
                .ty(output::Type::Directory)
                .build(),
        );

        //handle stdout output
        let (stdout_local, stdout_remote) = if let Some(stdout) = request.stdout_file {
            let filename = do_eval_to_string(stdout, request.eval_context);
            (request.outdir.join(&filename), bucket_url.join(&filename)?)
        } else {
            let filename = format!("stdout_{}", &Uuid::new_v4().to_string()[..8]);
            (request.tmpdir.join(&filename), bucket_url.join(&filename)?)
        };
        task.add_output(
            Output::builder()
                .name("stdout")
                .path(stdout_file)
                .url(stdout_remote.clone())
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

        //download results
        tokio::try_join!(
            self.storage.download(&stdout_remote, &stdout_local),
            self.storage.download(&stderr_remote, &stderr_local),
        )?;

        self.storage
            .download(&s3_workdir, request.outdir)
            .await
            .ok(); //errors if workdir is empty as such thing as empty does not exist in s3

        Ok(TaskExecutionResult {
            exit_status,
            stdout_file: stdout_local,
            stderr_file: stderr_local,
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
}

impl TesBackend {
    async fn upload_files_parallel(
        &self,
        inputs: &[FileOrDirectory],
        request_id: &str,
    ) -> anyhow::Result<HashMap<usize, Url>> {
        //create a list of upload tasks (the new urls)
        let upload_tasks: Vec<(usize, String, Url)> = inputs
            .iter()
            .enumerate()
            .filter_map(|(i, input)| {
                let location = input.location()?;
                let path = location.strip_prefix("file://")?.to_owned();
                let dest = Url::parse(&format!(
                    "s3://test-bucket/{}/{}{}",
                    request_id,
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
