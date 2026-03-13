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
use cwl_core::IntegerOrExpression;
use cwl_engine_storage::Storage;
use nonempty::nonempty;
use std::{
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
    backend: Arc<tes::Backend>,
}

impl TesBackend {
    /// Creates a new instance of `TesBackend`
    /// # Errors
    /// ??
    pub async fn new(config: Config) -> anyhow::Result<Self> {
        const NAME_BUFFER_LEN: usize = 4096;
        let names = Arc::new(Mutex::new(GeneratorIterator::new(
            UniqueAlphanumeric::default_with_expected_generations(NAME_BUFFER_LEN),
            NAME_BUFFER_LEN,
        )));
        let backend = Arc::new(tes::Backend::initialize(config, names, None).await);

        Ok(Self { backend })
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

        let mut args = request
            .command
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>();
        // manually check for an entypoint
        if let Some(entrypoint) = self.get_docker_entrypoint(&container).await? {
            args.splice(0..0, entrypoint);
        }

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
        for input in &mut inputs {
            if let Some(location) = input.location()
                && let Some(path) = location.strip_prefix("file://")
            {
                //file is local and needs to be uploaded
                let dest = Url::parse(&format!(
                    "s3://test-bucket/{}/{}{}",
                    request.id,
                    &Uuid::new_v4().to_string()[..8],
                    input.basename().unwrap()
                ))?;
                request.storage.upload(Path::new(path), &dest).await?;
                input.set_location(Some(dest.to_string()));
            }
            mount_input(&mut task, input)?;
        }

        let s3_workdir = Url::parse(&format!("s3://test-bucket/{}/workdir/", request.id))?;
        for mount in request.mounts {
            mount_workdir_item(
                mount.clone(),
                request.outdir,
                request.staged_dir,
                request.use_container,
                &mut task,
                request.storage.clone(),
                MountStrategy::Remote {
                    base_url: s3_workdir.clone(),
                },
            )
            .await?;
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
            (request.outdir.join(&filename), s3_workdir.join(&filename)?)
        } else {
            let filename = format!("stderr_{}", &Uuid::new_v4().to_string()[..8]);
            (request.outdir.join(&filename), bucket_url.join(&filename)?)
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
            (request.outdir.join(&filename), s3_workdir.join(&filename)?)
        } else {
            let filename = format!("stdout_{}", &Uuid::new_v4().to_string()[..8]);
            (request.outdir.join(&filename), bucket_url.join(&filename)?)
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
        request
            .storage
            .download(&stdout_remote, &stdout_local)
            .await?;
        request
            .storage
            .download(&stderr_remote, &stderr_local)
            .await?;
        request
            .storage
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

    fn input_dir(&self) -> String {
        CONTAINER_INPUT_DIR.to_string()
    }

    fn work_dir(&self) -> String {
        CONTAINER_WORKDIR.to_string()
    }

    fn tmp_dir(&self) -> String {
        CONTAINER_TMPDIR.to_string()
    }
}

impl TesBackend {
    ///Crankshaft backend overwrites docker entrypoint, so we need to get it beforehand and append it to the command
    async fn get_docker_entrypoint(&self, _container: &str) -> anyhow::Result<Option<Vec<String>>> {
        //ensure image
        //self.client.ensure_image(container).await?;
        //let info = self.client.inner().inspect_image(container).await?;
        //if let Some(cfg) = info.config
        //    && let Some(entrypoint) = cfg.entrypoint
        //{
        //    return Ok(Some(entrypoint));
        //}
        Ok(None)
    }
}
