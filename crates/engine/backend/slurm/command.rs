use crate::backend::local::command::{stage_inputs, stage_outputs};
use anyhow::Context;
use crankshaft::engine::{
    Task,
    service::runner::backend::TaskRunError,
    task::{Execution, execution::ExecutionResult, Input, input::{self, Contents}},
};
use bon::Builder;
use crankshaft::events::Event;
use cwl_engine_storage::StorageBackend;
use futures_util::{FutureExt, future::BoxFuture};
use nonempty::NonEmpty;
use std::{fmt::Write as _, path::{Path, PathBuf}, process::{ExitStatus, Stdio}, sync::Arc, time::Duration};
use tokio::{
    fs,
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    process::{Command, Child},
    select,
    sync::broadcast,
    time::timeout
};
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};

const CANCEL_JOB_TIMEOUT: Duration = Duration::from_secs(60);

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
    #[builder(into)]
    pub image_path: Option<PathBuf>,
    #[builder(into)]
    pub time_limit: Option<String>,
    #[builder(into)]
    pub jobstore: Option<PathBuf>,
    #[builder(default = false)]
    pub restart: bool,
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
    link_inputs: bool,
    time_limit: Option<u64>,
}

impl SlurmCommandBackend {
    #[must_use]
    pub fn new(storage: Arc<StorageBackend>, config: SlurmConfig) -> Self {
        Self { storage, config, link_inputs: false, time_limit: None }
    }
    #[must_use]
    pub fn with_linked_inputs(&self, link_inputs: bool) -> Self {
        Self {link_inputs, ..self.clone()}
    }
    #[must_use]
    pub fn with_time_limit(&self, seconds: Option<u64>) -> Self {
        Self {time_limit: seconds, ..self.clone()}
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
        let link_inputs = self.link_inputs;
        let time_limit = self.time_limit;
        Ok(async move {
            let mut status_vec = Vec::new();
            stage_job_inputs(storage, task.inputs(), link_inputs)
                .await
                .map_err(|e| TaskRunError::Other(e.into()))?;
            let name = job_name(&task);
            let disk_limit = disk_limit(&task);
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
                let mut res = resource_args(&task);
                res.extend(time_limit.map(time_arg));
                let status = submit(&config, &name, &res, &job_script(exec, time_limit, disk_limit.as_ref()), &token).await?;
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

async fn submit(slurm_config: &SlurmConfig, name: &str, res: &[String], script: &str, token: &CancellationToken) -> Result<ExitStatus, TaskRunError> {
    let log = slurm_config.log_dir.as_ref().map_or_else( || "/dev/null".to_string(),
        |d| d.join("cwl-%j.out").to_string_lossy().into_owned());
    let mut cmd = Command::new(&slurm_config.sbatch);
    cmd
        .arg(format!("--job-name={name}"))
        .args(&slurm_config.sbatch_args)
        .args(res)
        .arg("--parsable")
        .arg("--wait")
        .arg(format!("--output={log}"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    #[cfg(unix)]
    cmd.process_group(0);
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
    if token.is_cancelled() {
        return Err(cancel_job(&mut j, &mut child).await);
    }
    let status = select! {
        biased;
        () = token.cancelled() => return Err(TaskRunError::Canceled),
        status = child.wait() => status.context("Unable to wait for sbatch")?,
    };
    if j.job_id.is_some() && status.code().is_none() {
        return Err(TaskRunError::Other(anyhow::anyhow!("sbatch was terminated: {status}")));
    }
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
        if let Some(gpu) = resources.gpu(){
            args.push(format!("--gres=gpu:{gpu}"));
        }
    }
    args
}

fn job_script(execution: &Execution, time_limit: Option<u64>, disk_limit: Option<&DiskSpace>) -> String {
    let mut script = String::from("#!/bin/sh\n");
    let redirect = execution.stderr().map_or_else(|| ">&2".to_owned(), |stderr| format!(">> {}", quote(stderr)));
    for (key, value) in execution.env() {
        debug_assert!(is_valid_env_name(key));
        writeln!(script, "export {key}={}", quote(value)).expect("writing to a String cannot fail");
    }
    if let Some(secs) = time_limit {
        let _ = writeln!(script, "( sleep {secs}; echo \"Stopping job, it exceeded the time limit of {secs}s\" {redirect}; \
            kill -TERM $$; sleep 10; kill -KILL $$ ) &");
    }
    if let Some(limit) = disk_limit {
        let dirs = limit.dirs.iter().map(|d| quote(d)).collect::<Vec<_>>().join(" ");
        let _ = writeln!(script, "( while sleep {secs}; do used=$(du -skH {dirs} 2>/dev/null | awk '{{s+=$1}} END {{print s+0}}'); \
            if [ \"$used\" -gt {kib} ]; then echo \"Stopping job, it uses $used KiB of disk, more than the requested {kib} KiB\" {redirect}; \
            kill -TERM $$; sleep {secs}; kill -KILL $$; exit; fi; done ) &", secs = Duration::from_secs(30).as_secs(), kib = limit.kib);
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

async fn stage_job_inputs(storage: Arc<StorageBackend>, inputs: impl Iterator<Item = &Input>, link_inputs: bool) -> crate::Result<()> {
    if link_inputs {
        return stage_inputs(storage, inputs).await;
    }
    let mut other = Vec::new();
    for i in inputs {
        match i.contents() {
            Contents::Path(src) => {
                let dest = Path::new(i.path());
                if let Some(parent) = dest.parent() {
                    fs::create_dir_all(parent).await.with_context(|| format!("Could not create dir: {}", parent.display()))?;
                }
                let result = match i.ty() {
                    input::Type::File => copy_file(src, dest).await,
                    input::Type::Directory => copy_dir(src, dest).await,
                };
                result.with_context(|| {
                    format!("Could not copy from {} to {}", src.display(), dest.display())
                })?;
            }
            _ => other.push(i),
        }
    }
    stage_inputs(storage, other.into_iter()).await?;
    Ok(())
}

async fn copy_file(src: &Path, dest: &Path) -> anyhow::Result<()> {
    if src == dest {
        return Ok(());
    }
    let _ = fs::remove_file(dest).await;
    fs::copy(src, dest).await?;
    Ok(())
}

fn copy_dir<'a>(src: &'a Path, dest: &'a Path) -> BoxFuture<'a, anyhow::Result<()>> {
    async move {
        fs::create_dir_all(dest).await?;
        let mut entries = fs::read_dir(src).await?;
        while let Some(entry) = entries.next_entry().await? {
            let src = entry.path();
            let dest = dest.join(entry.file_name());
            match entry.file_type().await?.is_dir() {
                true => copy_dir(&src, &dest).await?,
                false => copy_file(&src, &dest).await?,
            }
        }
        Ok(())
    }
    .boxed()
}

async fn cancel_job(job: &mut JobGuard, sbatch: &mut Child) -> TaskRunError {
    let Some(id) = job.job_id.as_deref() else {
        return TaskRunError::Canceled;
    };
    info!("Cancelling Slurm job with {id}");
    match Command::new(&job.scancel).arg(id).status().await {
        Ok(status) if !status.success() => warn!("`{} {id}` failed with {status}", job.scancel),
        Err(e) => warn!("Could not run `{} {id}`: {e}", job.scancel),
        Ok(_) => {}
    }
    if timeout(CANCEL_JOB_TIMEOUT, sbatch.wait()).await.is_ok() {
        info!("Slurm job {id} cancelled");
        job.job_id = None;
    } else {
        warn!("Slurm job {id} did not stop within {}s after cancelling it", CANCEL_JOB_TIMEOUT.as_secs());
    }
    TaskRunError::Canceled
}

fn job_name(task: &Task) -> String {
    let name = task.name().unwrap_or_default().chars().map(|c| {
        if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.') {
            c
        } else {
            '_'
        }
    })
    .take(64).collect::<String>();
    format!("s4n-{}", name.trim_matches('_'))
}

fn time_arg(seconds: u64) -> String {
    format!("--time={}", seconds.div_ceil(60) + 1)
}

#[derive(Debug)]
struct DiskSpace {
    dirs: Vec<String>,
    kib: u64,
}

fn disk_limit(task: &Task) -> Option<DiskSpace> {
    let gib = task.resources()?.disk()?;
    let mut dirs: Vec<_> = task.inputs().filter(|input| !input.read_only()).map(|input| input.path().to_owned()).collect();
    dirs.sort();
    dirs.dedup_by(|dir, parent| {
        Path::new(dir).starts_with(Path::new(parent))
    });
    if dirs.is_empty() {
        return None;
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let kib = (gib * 1024.0 * 1024.0).ceil() as u64;
    Some(DiskSpace { dirs, kib })
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
        let status = submit(&config, "s4n-test", &["--mem=1024M".to_string()], "#!/bin/sh\necho hello\n",
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