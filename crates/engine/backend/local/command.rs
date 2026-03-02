use anyhow::Context;
use crankshaft::engine::{
    Task,
    service::runner::backend::TaskRunError,
    task::{
        Input, Output,
        input::{self, Contents},
        output,
    },
};
use dircpy::copy_dir;
use futures_util::{FutureExt, future::BoxFuture};
use nonempty::NonEmpty;
use std::{
    path::Path,
    process::{ExitStatus, Stdio},
};
use tokio::{
    fs::{self, File},
    process::Command,
    select,
};
use tokio_util::sync::CancellationToken;
use tracing::debug;
use url::Url;

#[derive(Debug, Clone)]
pub struct CommandBackend;
impl crankshaft::engine::service::runner::backend::Backend for CommandBackend {
    fn default_name(&self) -> &'static str {
        "command"
    }

    fn run(
        &self,
        task: Task,
        token: CancellationToken,
    ) -> anyhow::Result<BoxFuture<'static, Result<NonEmpty<ExitStatus>, TaskRunError>>> {
        Ok(async move {
            let mut statuses = Vec::new();
            stage_inputs(task.inputs()).await?;

            for execution in task.executions() {
                if token.is_cancelled() {
                    return Err(TaskRunError::Canceled);
                }

                let mut command = Command::new(execution.program());
                command.args(execution.args());
                command.envs(execution.env());

                if let Some(cwd) = execution.work_dir() {
                    command.current_dir(cwd);
                }

                // stdin: open the file and pipe its contents in
                if let Some(stdin_path) = execution.stdin() {
                    let file = File::open(stdin_path)
                        .await
                        .map_err(|e| TaskRunError::Other(e.into()))?;
                    command.stdin(file.into_std().await);
                } else {
                    command.stdin(Stdio::null());
                }

                // stdout: open/create the file to write into
                if let Some(stdout_path) = execution.stdout() {
                    debug!("Redirecting stdout to {stdout_path}");
                    let file = File::create(stdout_path)
                        .await
                        .map_err(|e| TaskRunError::Other(e.into()))?;
                    command.stdout(file.into_std().await);
                } else {
                    command.stdout(Stdio::inherit());
                }

                // stderr: open/create the file to write into
                if let Some(stderr_path) = execution.stderr() {
                    let file = File::create(stderr_path)
                        .await
                        .map_err(|e| TaskRunError::Other(e.into()))?;
                    command.stderr(file.into_std().await);
                } else {
                    command.stderr(Stdio::inherit());
                }

                command.kill_on_drop(true);

                debug!("Command will spawn as {command:?}");

                let mut child = command
                    .spawn()
                    .map_err(|e| TaskRunError::Other(e.into()))
                    .context("Task could not run")?;

                let status = select! {
                    biased;
                    _ = token.cancelled() =>{
                        let _ = child.kill().await;
                        return Err(TaskRunError::Canceled);
                    }
                    result = child.wait() => {
                        result.map_err(|e| TaskRunError::Other(e.into()))?
                    }
                };
                statuses.push(status);
            }

            stage_outputs(task.outputs()).await?;

            Ok(NonEmpty::from_vec(statuses).unwrap())
        }
        .boxed())
    }
}

async fn stage_inputs(inputs: impl Iterator<Item = &Input>) -> anyhow::Result<()> {
    for input in inputs {
        let dest = Path::new(input.path());
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)
                .await
                .with_context(|| format!("Could not create dir: {parent:?}"))?;
        }

        match input.contents() {
            Contents::Literal(src) => {
                fs::write(dest, src)
                    .await
                    .with_context(|| format!("Could not create file {dest:#?}"))?;
            }
            Contents::Path(src_path) => match input.ty() {
                input::Type::File => {
                    fs::copy(src_path, dest)
                        .await
                        .with_context(|| format!("Could not copy from {src_path:?} to {dest:?}"))?;
                }
                input::Type::Directory => {
                    copy_dir(src_path, dest)
                        .with_context(|| format!("Could not copy from {src_path:?} to {dest:?}"))?;
                }
            },
            Contents::Url(url) => {
                anyhow::bail!("URL inputs are not yet supported: {url}");
            }
        }
    }
    Ok(())
}

async fn stage_outputs(outputs: impl Iterator<Item = &Output>) -> anyhow::Result<()> {
    for output in outputs {
        let src = Path::new(output.path());
        if let Some(parent) = src.parent() {
            fs::create_dir_all(parent)
                .await
                .with_context(|| format!("Could not create dir: {parent:?}"))?;
        }

        let dest = output.url();
        let dest_url = Url::parse(dest)?;
        if dest_url.scheme() != "file" {
            anyhow::bail!(
                "Schema {} is not yet supported for outputs",
                dest_url.scheme()
            );
        }

        let dest_path = dest_url.path();
        match output.ty() {
            output::Type::File => {
                fs::copy(src, dest_path)
                    .await
                    .with_context(|| format!("Could not copy from {src:?} to {dest_path:?}"))?;
            }
            output::Type::Directory => {
                copy_dir(src, dest_path)
                    .with_context(|| format!("Could not copy from {src:?} to {dest_path:?}"))?;
            }
        }
    }
    Ok(())
}
