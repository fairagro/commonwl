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
use cwl_engine_storage::{Storage, StorageBackend};
use futures_util::{FutureExt, future::BoxFuture};
use nonempty::NonEmpty;
use std::{
    path::Path,
    process::{ExitStatus, Stdio},
    sync::Arc,
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
pub struct CommandBackend {
    storage: Arc<StorageBackend>,
}

impl CommandBackend {
    #[must_use]
    pub fn new(storage: Arc<StorageBackend>) -> Self {
        Self { storage }
    }
}

impl crankshaft::engine::service::runner::backend::Backend for CommandBackend {
    fn default_name(&self) -> &'static str {
        "command"
    }

    fn run(
        &self,
        task: Task,
        token: CancellationToken,
    ) -> anyhow::Result<BoxFuture<'static, Result<NonEmpty<ExitStatus>, TaskRunError>>> {
        let storage = self.storage.clone();
        Ok(async move {
            let mut statuses = Vec::new();
            stage_inputs(storage, task.inputs())
                .await
                .map_err(|e| TaskRunError::Other(e.into()))?;

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
                    debug!("reading stdin {stdin_path}");
                    let file = File::open(stdin_path)
                        .await
                        .context("Could not open stdin path")?;
                    command.stdin(file.into_std().await);
                } else {
                    command.stdin(Stdio::null());
                }

                // stdout: open/create the file to write into
                if let Some(stdout_path) = execution.stdout() {
                    debug!("Redirecting stdout to {stdout_path}");
                    if let Some(parent) = Path::new(stdout_path).parent() {
                        fs::create_dir_all(parent)
                            .await
                            .with_context(|| format!("Could not create dir: {}", parent.display()))
                            .map_err(|e| TaskRunError::Other(e.into()))?;
                    }
                    let file = File::create(stdout_path)
                        .await
                        .map_err(|e| TaskRunError::Other(e.into()))?;
                    command.stdout(file.into_std().await);
                } else {
                    command.stdout(Stdio::inherit());
                }

                // stderr: open/create the file to write into
                if let Some(stderr_path) = execution.stderr() {
                    if let Some(parent) = Path::new(stderr_path).parent() {
                        fs::create_dir_all(parent)
                            .await
                            .with_context(|| format!("Could not create dir: {}", parent.display()))
                            .map_err(|e| TaskRunError::Other(e.into()))?;
                    }
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
                    () = token.cancelled() =>{
                        let _ = child.kill().await;
                        return Err(TaskRunError::Canceled);
                    }
                    result = child.wait() => {
                        result.map_err(|e| TaskRunError::Other(e.into()))?
                    }
                };
                statuses.push(status);
            }

            stage_outputs(task.outputs())
                .await
                .map_err(|e| TaskRunError::Other(e.into()))?;

            Ok(NonEmpty::from_vec(statuses).unwrap())
        }
        .boxed())
    }
}

async fn stage_inputs(
    storage: Arc<StorageBackend>,
    inputs: impl Iterator<Item = &Input>,
) -> crate::Result<()> {
    for input in inputs {
        let dest = Path::new(input.path());
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)
                .await
                .with_context(|| format!("Could not create dir: {}", parent.display()))?;
        }

        match input.contents() {
            Contents::Literal(src) => {
                debug!("Creating file {}", dest.display());
                fs::write(dest, src)
                    .await
                    .with_context(|| format!("Could not create file {}", dest.display()))?;
            }
            Contents::Path(src_path) => match input.ty() {
                input::Type::File => {
                    debug!("Linking/copying from {src_path:?} to {dest:?}");
                    link_or_copy_file(src_path, dest).await.with_context(|| {
                        format!(
                            "Could not copy from {} to {}",
                            src_path.display(),
                            dest.display()
                        )
                    })?;
                }
                input::Type::Directory => {
                    debug!("Linking/copying from {src_path:?} to {dest:?}");
                    link_or_copy_dir(src_path, dest).await.with_context(|| {
                        format!(
                            "Could not copy from {} to {}",
                            src_path.display(),
                            dest.display()
                        )
                    })?;
                }
            },
            Contents::Url(url) => {
                let dest = Path::new(input.path());
                storage
                    .download(url, dest)
                    .await
                    .with_context(|| format!("Could not download {url} to {}", dest.display()))?;
            }
        }
    }
    Ok(())
}

async fn stage_outputs(outputs: impl Iterator<Item = &Output>) -> crate::Result<()> {
    for output in outputs {
        let src = Path::new(output.path());
        if let Some(parent) = src.parent() {
            fs::create_dir_all(parent)
                .await
                .with_context(|| format!("Could not create dir: {}", parent.display()))?;
        }

        let dest = output.url();
        let dest_url = Url::parse(dest)?;
        if dest_url.scheme() != "file" {
            crate::bail!(
                "Schema {} is not yet supported for outputs",
                dest_url.scheme()
            );
        }

        let dest_path = dest_url
            .to_file_path()
            .map_err(|()| anyhow::anyhow!("Could not convert output url to file path: {dest}"))?;
        match output.ty() {
            output::Type::File => {
                link_or_copy_file(src, &dest_path).await.with_context(|| {
                    format!(
                        "Could not copy from {} to {}",
                        src.display(),
                        dest_path.display()
                    )
                })?;
            }
            output::Type::Directory => {
                link_or_copy_dir(src, &dest_path).await.with_context(|| {
                    format!(
                        "Could not copy from {} to {}",
                        src.display(),
                        dest_path.display()
                    )
                })?;
            }
        }
    }
    Ok(())
}

/// Hardlinks `src` to `dest` when possible, falling back to a real copy when linking isn't possibru
async fn link_or_copy_file(src: &Path, dest: &Path) -> crate::Result<()> {
    if is_same_file(src, dest) {
        return Ok(());
    }

    if fs::hard_link(src, dest).await.is_ok() {
        return Ok(());
    }

    // If `dest` exists but wasnt the same file, clean vefore copy
    if fs::metadata(dest).await.is_ok() {
        let _ = fs::remove_file(dest).await;
        if fs::hard_link(src, dest).await.is_ok() {
            return Ok(());
        }
    }

    Ok(fs::copy(src, dest).await.map(|_| ()).with_context(|| {
        format!(
            "Could not copy from {} to {}",
            src.display(),
            dest.display()
        )
    })?)
}

/// Directory analogue of [`link_or_copy_file`]
fn link_or_copy_dir<'a>(src: &'a Path, dest: &'a Path) -> BoxFuture<'a, crate::Result<()>> {
    async move {
        fs::create_dir_all(dest)
            .await
            .with_context(|| format!("Could not create dir: {}", dest.display()))?;

        let mut entries = fs::read_dir(src)
            .await
            .with_context(|| format!("Could not read dir: {}", src.display()))?;
        while let Some(entry) = entries
            .next_entry()
            .await
            .with_context(|| format!("Could not read dir: {}", src.display()))?
        {
            let file_type = entry.file_type().await?;
            let from = entry.path();
            let to = dest.join(entry.file_name());
            if file_type.is_dir() {
                link_or_copy_dir(&from, &to).await?;
            } else {
                link_or_copy_file(&from, &to).await?;
            }
        }
        Ok(())
    }
    .boxed()
}

fn is_same_file(src: &Path, dest: &Path) -> bool {
    same_file::is_same_file(src, dest).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crankshaft::engine::{Task, service::runner::backend::Backend as _, task::Execution};
    use nonempty::nonempty;

    #[tokio::test]
    async fn test_stdout_creates_missing_parent_dir() {
        let dir = tempfile::tempdir().unwrap();
        let stdout_path = dir.path().join("nested/sub/out.txt");

        let task = Task::builder()
            .name("test")
            .executions(nonempty![
                Execution::builder()
                    .image("unsupported")
                    .program("echo")
                    .args(vec!["hello".to_string()])
                    .stdout(stdout_path.to_string_lossy().into_owned())
                    .build()
            ])
            .build();

        let backend = CommandBackend::new(Arc::new(StorageBackend::new()));
        let result = backend.run(task, CancellationToken::new()).unwrap().await;

        assert!(result.is_ok(), "expected success, got {:?}", result.err());
        assert_eq!(fs::read_to_string(&stdout_path).await.unwrap().trim(), "hello");
    }

    #[tokio::test]
    async fn test_link_or_copy_file_survives_source_removal() {
        // `link_or_copy_file` must never leave `dest` as a symlink into `src`
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("src.txt");
        let dest = dir.path().join("dest.txt");
        fs::write(&src, b"hello").await.unwrap();

        link_or_copy_file(&src, &dest).await.unwrap();
        fs::remove_file(&src).await.unwrap();

        assert!(
            !fs::symlink_metadata(&dest)
                .await
                .unwrap()
                .file_type()
                .is_symlink(),
            "dest must not be a symlink"
        );
        assert_eq!(fs::read(&dest).await.unwrap(), b"hello");
    }
}
