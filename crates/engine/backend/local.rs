use async_trait::async_trait;
use crankshaft::{
    config::backend::generic::{
        Config,
        driver::{self, Locale, Shell},
    },
    engine::{
        Task,
        service::{
            name::{GeneratorIterator, UniqueAlphanumeric},
            runner::{
                Backend,
                backend::{TaskRunError, generic},
            },
        },
        task::{
            Execution, Input, Output, Resources,
            input::{self, Contents},
            output::{self},
        },
    },
};
use cwl_core::IntegerOrExpression;
use nonempty::nonempty;
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio_util::sync::CancellationToken;
use tracing::error;
use url::Url;
use uuid::Uuid;

use crate::{
    backend::{
        TaskBackend, TaskExecutionRequest, TaskExecutionResult,
        mount::{mount_input, mount_workdir_item},
    },
    expression::{do_eval, do_eval_to_string},
};

#[derive(Debug, Clone)]
pub struct LocalBackend {
    //wrapper to crankshaft backend
    backend: Arc<generic::Backend>,
}

impl LocalBackend {
    pub async fn new() -> anyhow::Result<Self> {
        const NAME_BUFFER_LEN: usize = 4096;
        let names = Arc::new(Mutex::new(GeneratorIterator::new(
            UniqueAlphanumeric::default_with_expected_generations(NAME_BUFFER_LEN),
            NAME_BUFFER_LEN,
        )));
        let config = Config::builder()
            .driver(
                driver::Config::builder()
                    .locale(Locale::Local)
                    .max_attempts(1.into())
                    .shell(Shell::Bash)
                    .build(),
            )
            .monitor("")
            .submit("")
            .kill("kill ~{job_id}")
            .build();

        let backend = Arc::new(generic::Backend::initialize(config, None, names, None).await?);

        Ok(Self { backend })
    }
}

#[async_trait]
impl TaskBackend for LocalBackend {
    async fn run(
        &self,
        request: &TaskExecutionRequest<'_>,
        token: CancellationToken,
    ) -> anyhow::Result<TaskExecutionResult> {
        //handle docker requirement
        let container = "ubuntu".to_string(); //add config "default-container"
        //if let Some(dr) = request.docker {
        //    if let Some(df) = &dr.docker_file
        //        && let Some(dt) = &dr.docker_image_id
        //    {
        //        build_container(self.client.inner(), df, dt).await?;
        //        container = dt.to_string();
        //    } else if let Some(dp) = &dr.docker_pull {
        //        container = dp.to_string();
        //    }
        //}

        let stdout_file = if let Some(s) = request.stdout_file {
            &format!("{}/{s}", request.tmpdir.to_string_lossy())
        } else {
            &format!("{}/stdout", request.tmpdir.to_string_lossy())
        };

        let stderr_file = if let Some(s) = request.stderr_file {
            &format!("{}/{s}", request.tmpdir.to_string_lossy())
        } else {
            &format!("{}/stderr", request.tmpdir.to_string_lossy())
        };

        let args = request
            .command
            .iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>();

        //build crankshaft task object
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
                    .maybe_stdin(request.stdin_file)
                    .build()
            ])
            .resources(
                Resources::builder()
                    .cpu(request.runtime.cores as f64)
                    .disk(request.runtime.outdir_size as f64)
                    .ram(request.runtime.ram as f64)
                    .build(),
            )
            .build();

        //add file inputs to task
        for input in request.inputs {
            mount_input(&mut task, input)?;
        }

        for mount in request.mounts {
            mount_workdir_item(
                mount.clone(),
                request.outdir,
                request.use_container,
                &mut task,
            )?;
        }

        //add outdir mount
        task.add_input(
            Input::builder()
                .name("outdir")
                .contents(Contents::Path(request.outdir.to_path_buf()))
                .path(request.staged_dir)
                .ty(input::Type::Directory)
                .read_only(false)
                .build(),
        );

        //add tmpdir input
        task.add_input(
            Input::builder()
                .name("tmpdir")
                .contents(Contents::Path(request.tmpdir.to_path_buf()))
                .path(self.tmp_dir())
                .ty(input::Type::Directory)
                .read_only(false)
                .build(),
        );

        //handle stderr output
        let stderr_out_file = if let Some(stderr) = request.stderr_file {
            let stderr = do_eval_to_string(stderr, request.eval_context);
            request.outdir.join(stderr)
        } else {
            request
                .tmpdir
                .join(format!("stderr_{}", &Uuid::new_v4().to_string()[..8]))
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
            request.outdir.join(stdout)
        } else {
            request
                .tmpdir
                .join(format!("stdout_{}", &Uuid::new_v4().to_string()[..8]))
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
                IntegerOrExpression::Int(i) => Some(*i as i64),
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
                _ = tokio::time::sleep(limit) => {
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
            stdout_file: stdout_out_file,
            stderr_file: stderr_out_file,
        })
    }

    fn input_dir(&self) -> String {
        let uuid = &Uuid::new_v4().to_string()[..8];
        format!("/tmp/{uuid}/task")
    }

    fn work_dir(&self) -> String {
        let uuid = &Uuid::new_v4().to_string()[..8];
        format!("/tmp/{uuid}/work")
    }

    fn tmp_dir(&self) -> String {
        let uuid = &Uuid::new_v4().to_string()[..8];
        format!("/tmp/{uuid}/tmp")
    }
}
