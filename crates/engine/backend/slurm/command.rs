use crate::backend::local::command::{stage_inputs, stage_outputs};
use anyhow::Context;
use crankshaft::engine::{
    Task,
    service::runner::backend::TaskRunError,
    task::{Execution, execution::ExecutionResult},
};
use bon::Builder;
use crankshaft::events::Event;
use cwl_engine_storage::StorageBackend;
use futures_util::{FutureExt, future::BoxFuture};
use nonempty::NonEmpty;
use std::{fmt::Write as _, path::{Path, PathBuf}, process::{ExitStatus, Stdio}, sync::Arc};
use tokio::{
    fs,
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    process::Command,
    select,
    sync::broadcast,
};
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};

#[derive(Debug, Clone, Builder)]
pub struct SlurmConfig {
    #[builder(default)]
    pub sbatch_args: Vec<String>,
    #[builder(into)]
    pub log_dir: Option<PathBuf>,
    #[builder(into, default = String::from("sbatch"))]
    pub sbatch: String,
    #[builder(into, default = String::from("scancel"))]
    pub scancel: String,
}

impl Default for SlurmConfig {
    fn default() -> Self {
        Self::builder().build()
    }
}

#[derive(Debug, Clone)]
pub struct SlurmCommandBackend {
    storage: Arc<StorageBackend>,
    config: SlurmConfig,
}

impl SlurmCommandBackend {
    #[must_use]
    pub fn new(storage: Arc<StorageBackend>, config: SlurmConfig) -> Self {
        Self { storage, config }
    }
}

impl crankshaft::engine::service::runner::backend::Backend for SlurmCommandBackend {
    fn default_name(&self) -> &'static str {
        "slurm"
    }

    fn run(
        &self,
        task: Task,
        _events: Option<broadcast::Sender<Event>>,
        token: CancellationToken,
    ) -> anyhow::Result<BoxFuture<'static, Result<NonEmpty<ExecutionResult>, TaskRunError>>> {
        let storage = self.storage.clone();
        let config = self.config.clone();
        Ok(async move {
            let mut status_vec = Vec::new();
            stage_inputs(storage, task.inputs())
                .await
                .map_err(|e| TaskRunError::Other(e.into()))?;
            for exec in task.executions() {
                if token.is_cancelled() {
                    return Err(TaskRunError::Canceled);
                }
                for path in [exec.stdout(), exec.stderr()].into_iter().flatten() {
                    if let Some(parent) = Path::new(path).parent() {
                        fs::create_dir_all(parent)
                            .await
                            .with_context(|| format!("Could not create dir: {}", parent.display()))?;
                    }
                }
                let res = resource_args(&task);
                let status = submit(&config, &res, &job_script(exec), &token).await?;
                status_vec.push(ExecutionResult {
                    image: Some(exec.images().first().clone()),
                    status,
                });
            }
            stage_outputs(task.outputs())
                .await
                .map_err(|e| TaskRunError::Other(e.into()))?;
            Ok(NonEmpty::from_vec(status_vec).unwrap())
        }
        .boxed())
    }
}

async fn submit(slurm_config: &SlurmConfig, res: &[String], script: &str, token: &CancellationToken) -> Result<ExitStatus, TaskRunError> {
    let log = slurm_config.log_dir.as_ref().map_or_else( || "/dev/null".to_string(),
        |d| d.join("cwl-%j.out").to_string_lossy().into_owned());
    let mut cmd = Command::new(&slurm_config.sbatch);
    cmd
        .args(&slurm_config.sbatch_args)
        .args(res)
        .arg("--parsable")
        .arg("--wait")
        .arg(format!("--output={log}"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    debug!("Slurm job with command {:?} submitted", cmd);
    let mut child = cmd.spawn().with_context(|| format!("Not able to run {:?}", slurm_config.sbatch))?;
    let mut stdin = child.stdin.take().expect("stdin is piped");
    stdin.write_all(script.as_bytes()).await.context("Not able to pass script")?;
    drop(stdin);
    let mut stdout = BufReader::new(child.stdout.take().expect("stdout is piped")).lines();
    let mut j = JobGuard {
        scancel: slurm_config.scancel.clone(),
        job_id: None,
    };
    select! {
        biased;
        () = token.cancelled() => return Err(TaskRunError::Canceled),
        line = stdout.next_line() => {
            j.job_id = line.context("Unable to read sbatch")?.and_then(|l| l.split(';').next().map(|i| i.trim().to_string())).filter(|i| !i.is_empty());
        }
    }
    if let Some(id) = &j.job_id {
        info!("A new slurm job with id {id} was submitted");
    }
    let status = select! {
        biased;
        () = token.cancelled() => return Err(TaskRunError::Canceled),
        status = child.wait() => status.context("Unable to wait for sbatch")?,
    };
    if j.job_id.take().is_none() {
        let mut stderr = String::new();
        if let Some(mut pipe) = child.stderr.take() {
            let _ = pipe.read_to_string(&mut stderr).await;
        }
        return Err(TaskRunError::Other(anyhow::anyhow!("sbatch failed: {}", stderr.trim())));
    }
    Ok(status)
}

struct JobGuard {
    scancel: String,
    job_id: Option<String>,
}

impl Drop for JobGuard {
    fn drop(&mut self) {
        if let Some(id) = self.job_id.take() {
            warn!("Cancelling Slurm job {id}");
            if let Err(e) = std::process::Command::new(&self.scancel).arg(&id).status() {
                warn!("Could not run `{} {id}`: {e}", self.scancel);
            }
        }
    }
}

fn resource_args(task: &Task) -> Vec<String> {
    let mut args = Vec::new();
    if let Some(resources) = task.resources() {
        if let Some(cpu) = resources.cpu() {
            args.push(format!("--cpus-per-task={}", cpu.ceil().max(1.0) as u64));
        }
        // convert ram
        if let Some(ram) = resources.ram() {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            args.push(format!("--mem={}M", (ram * 1024.0).ceil().max(1.0) as u64));
        }
    }
    args
}

fn job_script(execution: &Execution) -> String {
    let mut script = String::from("#!/bin/sh\n");
    for (key, value) in execution.env() {
        debug_assert!(is_valid_env_name(key));
        writeln!(script, "export {key}={}", quote(value)).expect("writing to a String cannot fail");
    }
    if let Some(dir) = execution.work_dir() {
        writeln!(script, "cd {} || exit 1", quote(dir)).expect("writing to a String cannot fail");
    }
    write!(script, "exec {}", quote(execution.program())).expect("writing to a String cannot fail");
    for arg in execution.args() {
        write!(script, " {}", quote(arg)).expect("writing to a String cannot fail");
    }
    match execution.stdin() {
        Some(stdin) => {
            write!(script, " < {}", quote(stdin)).expect("writing to a String cannot fail");
        }
        None => script.push_str(" < /dev/null"),
    }
    if let Some(stdout) = execution.stdout() {
        write!(script, " > {}", quote(stdout)).expect("writing to a String cannot fail");
    }
    if let Some(stderr) = execution.stderr() {
        write!(script, " 2> {}", quote(stderr)).expect("writing to a String cannot fail");
    }
    script.push('\n');
    script
}

fn is_valid_env_name(name: &str) -> bool {
    let mut c = name.chars();
    matches!(c.next(), Some('A'..='Z' | 'a'..='z' | '_')) && c.all(|c| matches!(c, 'A'..='Z' | 'a'..='z' | '0'..='9' | '_'))
}

fn quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', r"'\''"))
}

#[cfg(test)]
#[cfg(unix)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_args_slurm_backend() {
        let dir = tempdir().unwrap();
        let sbatch = dir.path().join("sbatch");
        std::fs::write(
        &sbatch,
    r#"#!/bin/sh
                echo "$@" > "$(dirname "$0")/sbatch.calls"
                cat > "$(dirname "$0")/sbatch.script"
                echo 42
                "#,
        ).unwrap();
        std::fs::set_permissions(&sbatch, std::fs::Permissions::from_mode(0o755)).unwrap();
        let config = SlurmConfig::builder().sbatch(sbatch.to_string_lossy().into_owned())
            .sbatch_args(vec!["--partition=test".to_string()]).log_dir("/logs").build();
        let status = submit(&config, &["--mem=1024M".to_string()], "#!/bin/sh\necho hello\n",
            &CancellationToken::new()).await.unwrap();
        assert!(status.success());
        let calls = std::fs::read_to_string(dir.path().join("sbatch.calls")).unwrap();
        for arg in ["--partition=test", "--mem=1024M", "--parsable", "--wait", "--output=/logs/cwl-%j.out"] {
            assert!(calls.contains(arg), "missing {arg} in {calls}");
        }
        let script = std::fs::read_to_string(dir.path().join("sbatch.script")).unwrap();
        assert_eq!(script, "#!/bin/sh\necho hello\n");
    }
}