use anyhow::ensure;
use bollard::{Docker, body_full, query_parameters::BuildImageOptions};
use bon::Builder;
use cwl_core::files::FileOrDirectory;
use futures_util::TryStreamExt;
use indexmap::IndexMap;
use std::{
    fmt::Display,
    io::BufRead,
    path::Path,
    process::{Command, Stdio},
};

use crate::{environment::workdir::{self, WorkDirMount}, string_url_to_file_path};

/// builds a Docker container using the bollard Docker client
pub(crate) async fn build_container(
    client: &Docker,
    docker_file: impl AsRef<Path>,
    tag: &str,
) -> anyhow::Result<()> {
    let mut archive = tar::Builder::new(vec![]);

    archive.append_path_with_name(docker_file, "Dockerfile")?;
    let tarball = archive.into_inner()?;

    let options = BuildImageOptions {
        dockerfile: "Dockerfile".to_string(),
        t: Some(tag.to_string()),
        rm: true,
        ..Default::default()
    };

    let mut stream = client.build_image(options, None, Some(body_full(tarball.into())));

    while let Some(msg) = stream.try_next().await? {
        if let Some(stream) = msg.stream {
            tracing::info!("{stream}");
        }
        if let Some(error) = msg.error {
            anyhow::bail!("Docker build error: {error}");
        }
    }

    Ok(())
}

#[derive(Default, Clone, Debug, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ContainerEngine {
    #[default]
    Docker,
    Podman,
    Singularity,
    Apptainer,
}

impl Display for ContainerEngine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ContainerEngine::Docker => write!(f, "docker"),
            ContainerEngine::Podman => write!(f, "podman"),
            ContainerEngine::Singularity => write!(f, "singularity"),
            ContainerEngine::Apptainer => write!(f, "apptainer"),
        }
    }
}

#[derive(Clone, Debug, Builder)]
pub struct ContainerBuildOptions {
    #[builder(into)]
    pub engine: ContainerEngine,
    #[builder(into)]
    pub docker_image_id: String,
    #[builder(into)]
    pub docker_file: Option<String>,
    #[builder(into)]
    pub workdir: String,
    #[builder(into)]
    pub outdir: String,
    #[builder(into)]
    pub tmpdir: String,
    #[builder(into)]
    pub env: IndexMap<String, String>,
    #[builder(into)]
    pub network: bool,
    #[builder(into)]
    pub mounts: Vec<WorkDirMount>,
}

/// Builds a container command from the given raw command
/// # Panics
/// Building containers currently is unimplemented
/// # Errors
/// Throws if building dockerfile fails
pub fn build_container_command(
    raw_command: Vec<String>,
    inputs: &[FileOrDirectory],
    options: ContainerBuildOptions,
    specificationdir: &Path,
) -> anyhow::Result<Vec<String>> {
    if let Some(df) = &options.docker_file {
        build_docker_file(df, specificationdir, &options)?;
    }

    let outdir = options.outdir;
    let workdir = options.workdir;
    let tmpdir = options.tmpdir;

    let mut args = if options.engine == ContainerEngine::Singularity
        || options.engine == ContainerEngine::Apptainer
    {
        vec![
            "exec".to_string(),
            "-H".to_string(),
            format!("{outdir}:{workdir}"),
            "-B".to_string(),
            "/tmp/apptainer_t,p:/tmp".to_string(),
            "--pwd".to_string(),
            workdir.clone(),
            "--env".to_string(),
            "TMPDIR=/tmp".to_string(),
        ]
    } else {
        let workdir_mount = format!("--mount=type=bind,source={outdir},target={workdir}");
        let tmpdir_mount = format!("--mount=type=bind,source={tmpdir},target={tmpdir}");
        let workdir_arg = format!("--workdir={}", &workdir);

        vec![
            "run".to_string(),
            "-i".to_string(),
            workdir_mount,
            tmpdir_mount,
            workdir_arg,
            "--rm".to_string(),
        ]
    };

    for input in inputs {
        let loc = string_url_to_file_path(input.location().unwrap())?;
        let loc = loc.canonicalize()?; //resolve `..` to have an absolute path that can be safely mounted
        let mount = format!(
            "--mount=type=bind,source={},target={}",
            loc.display(),
            input.path().unwrap()
        );
        args.push(mount);
    }

    for mount in options.mounts {
        let workdir::Source::Url(loc) = mount.source else {
            continue;
        };
        ensure!(mount.target.scheme() == "file");
        let target = mount
            .target
            .to_file_path()
            .map_err(|()| anyhow::anyhow!("Not a local path!"))?;
        let mount = format!(
            "--mount=type=bind,source={},target={}",
            loc.path(), //dangerous
            target.to_string_lossy()
        );
        args.push(mount);
    }

    #[cfg(unix)]
    {
        args.push(get_user_flag());
    }

    args.push(format!("--env=HOME={}", &workdir));
    args.push(format!("--env=TMPDIR={}", &tmpdir));

    for (key, val) in options
        .env
        .into_iter()
        .skip_while(|(key, _)| *key == "HOME" || *key == "TMPDIR")
    {
        args.push(format!("--env={key}={val}"));
    }

    if !options.network {
        args.push("--net=none".to_string());
    }

    args.push(options.docker_image_id);

    args.extend(raw_command);

    args.splice(0..0, vec![options.engine.to_string()]);

    Ok(args)
}

fn build_docker_file(
    df: &str,
    specificationdir: &Path,
    options: &ContainerBuildOptions,
) -> anyhow::Result<()> {
    let path = specificationdir.join(df);
    let mut build = Command::new(options.engine.to_string());
    let mut process = build
        .args([
            "build",
            "-f",
            &path.to_string_lossy(),
            "-t",
            &options.docker_image_id,
            ".",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let stdout = process.stdout.take().expect("Failed to capture stdout");
    let stderr = process.stderr.take().expect("Failed to capture stderr");

    // Stream stdout in a thread
    let stdout_thread = std::thread::spawn(move || {
        let reader = std::io::BufReader::new(stdout);
        for line in reader.lines() {
            match line {
                Ok(line) => println!("{line}"),
                Err(e) => eprintln!("stdout read error: {e}"),
            }
        }
    });

    // Stream stderr in a thread
    let stderr_thread = std::thread::spawn(move || {
        let reader = std::io::BufReader::new(stderr);
        for line in reader.lines() {
            match line {
                Ok(line) => eprintln!("{line}"),
                Err(e) => eprintln!("stderr read error: {e}"),
            }
        }
    });

    // Wait for both streams to finish, then get exit status
    stdout_thread.join().expect("stdout thread panicked");
    stderr_thread.join().expect("stderr thread panicked");

    let status = process.wait()?;
    if status.success() {
        tracing::info!("Docker build successful");
        Ok(())
    } else {
        let mut stderr = String::new();
        if let Some(mut err) = process.stderr.take() {
            use std::io::Read;
            err.read_to_string(&mut stderr)?;
        }
        anyhow::bail!("Docker build failed: {stderr}");
    }
}

#[cfg(unix)]
fn get_user_flag() -> String {
    use nix::unistd::{getgid, getuid};
    format!("--user={}:{}", getuid().as_raw(), getgid().as_raw())
}
