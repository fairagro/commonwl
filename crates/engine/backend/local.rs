use crate::{
    backend::{
        TaskBackend, TaskExecutionRequest, TaskExecutionResult,
        local::command::CommandBackend,
        mount::{mount_input, mount_workdir_item},
    },
    docker::{ContainerBuildOptions, ContainerEngine, build_container_command},
    expression::{do_eval, do_eval_to_string},
};
use async_trait::async_trait;
use crankshaft::engine::{
    Task,
    service::runner::{Backend, backend::TaskRunError},
    task::{
        Execution, Input, Output, Resources,
        input::{self, Contents},
        output::{self},
    },
};
use cwl_core::IntegerOrExpression;
use nonempty::nonempty;
use std::{sync::Arc, time::Duration};
use tokio_util::sync::CancellationToken;
use tracing::error;
use url::Url;
use uuid::Uuid;

pub mod command;

#[derive(Debug, Clone)]
pub struct LocalBackend {
    uuid: String,
    //wrapper to crankshaft backend
    backend: Arc<CommandBackend>,
}

impl Default for LocalBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl LocalBackend {
    pub fn new() -> Self {
        let backend = Arc::new(CommandBackend {});

        Self {
            uuid: Uuid::new_v4().to_string()[..8].to_string(),
            backend,
        }
    }
}

#[async_trait]
impl TaskBackend for LocalBackend {
    async fn run(
        &self,
        request: &TaskExecutionRequest<'_>,
        token: CancellationToken,
    ) -> anyhow::Result<TaskExecutionResult> {
        let stdout_file = if let Some(s) = request.stdout_file {
            &format!("{}/{s}", self.tmp_dir())
        } else {
            &format!("{}/stdout", self.tmp_dir())
        };

        let stderr_file = if let Some(s) = request.stderr_file {
            &format!("{}/{s}", self.tmp_dir())
        } else {
            &format!("{}/stderr", self.tmp_dir())
        };

        let mut args = request
            .command
            .iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>();

        //handle docker requirement
        if let Some(dr) = &request.docker {
            let dr = (*dr).clone();
            let image_id = dr.docker_pull.or(dr.docker_image_id).unwrap();
            let options = ContainerBuildOptions::builder()
                .docker_image_id(image_id)
                .network(request.network)
                .engine(ContainerEngine::Docker) //need options for that
                .env(request.env.clone())
                .outdir(request.outdir.to_string_lossy())
                .tmpdir(request.tmpdir.to_string_lossy())
                .workdir(request.staged_dir)
                .maybe_docker_file(dr.docker_file)
                .build();
            args = build_container_command(args, request.inputs, options)?;
        }

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
                    .image("unsupported")
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

        //the backend needs to copy back workdir
        task.add_output(
            Output::builder()
                .name("workdir")
                .path(request.staged_dir)
                .url(Url::from_file_path(request.outdir).unwrap())
                .ty(output::Type::Directory)
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

    fn task_scoped(&self) -> Arc<dyn TaskBackend> {
        Arc::new(LocalBackend::new()) as Arc<dyn TaskBackend>
    }

    fn input_dir(&self) -> String {
        format!("/tmp/{}/inputs", self.uuid)
    }
    fn work_dir(&self) -> String {
        format!("/tmp/{}/work", self.uuid)
    }
    fn tmp_dir(&self) -> String {
        format!("/tmp/{}/tmp", self.uuid)
    }
}
