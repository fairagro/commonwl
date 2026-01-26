use crate::{
    backend::{ExecutionRequest, TaskBackend, handle_synthetic_directories},
    command,
    context::build_runtime,
    docker::build_container,
    input::{collect_inputs, flatten_inputs, get_stdin},
    output::collect_command_outputs,
    pathmapper::PathMapper,
};
use crankshaft::{
    config::backend::docker::Config,
    docker::Docker,
    engine::{
        Task,
        service::{
            name::{GeneratorIterator, UniqueAlphanumeric},
            runner::{Backend, backend::docker},
        },
        task::{
            Execution, Input, Output, Resources,
            input::{self, Contents},
            output,
        },
    },
};
use cwl_core::{
    docstring,
    documents::CWLDocument,
    files::FileOrDirectory,
    requirements::{DockerRequirement, ResourceRequirement},
};
use nonempty::{NonEmpty, nonempty};
use std::{
    fs,
    path::Path,
    process::ExitStatus,
    sync::{Arc, Mutex},
};
use tempfile::tempdir;
use tokio_util::sync::CancellationToken;
use tracing::info;
use url::Url;

const CONTAINER_WORKDIR: &str = "/mnt/task/workdir";
const CONTAINER_INPUT_DIR: &str = "/mnt/task/inputs";
const CONTAINER_STDOUT_FILE: &str = "/mnt/task/stdout";
const CONTAINER_STDERR_FILE: &str = "/mnt/task/stderr";

pub struct DockerBackend {
    //wrapper to Bollard Docker client
    client: Docker,
    //wrapper to crankshaft backend
    backend: Arc<docker::Backend>,
}

impl DockerBackend {
    pub async fn new(config: Config) -> anyhow::Result<Self> {
        const NAME_BUFFER_LEN: usize = 4096;
        let names = Arc::new(Mutex::new(GeneratorIterator::new(
            UniqueAlphanumeric::default_with_expected_generations(NAME_BUFFER_LEN),
            NAME_BUFFER_LEN,
        )));
        let backend =
            Arc::new(docker::Backend::initialize_default_with(config, names, None).await?);

        let client = Docker::with_defaults()?;

        Ok(Self { backend, client })
    }
}

impl TaskBackend for DockerBackend {
    async fn run(
        &self,
        request: &ExecutionRequest,
        token: CancellationToken,
    ) -> anyhow::Result<NonEmpty<ExitStatus>> {
        let inputs = collect_inputs(&request.specification, &request.inputs)?;
        let stage_dir = Path::new(CONTAINER_INPUT_DIR);

        let outdir = tempdir()?;
        let tmpdir = tempdir()?;

        let mut path_mapper = PathMapper::new(&inputs, &request.working_dir, stage_dir)?;
        let CWLDocument::CommandLineTool(tool) = &request.specification else {
            panic!("Currently only CommandLineTool is supported in Docker backend");
        };

        let req = tool.get_requirement_or_hint::<ResourceRequirement>();
        let mut runtime = build_runtime(req);
        runtime.outdir = request.out_dir.clone();

        //handle synthethic directories
        let mut flattened_inputs = flatten_inputs(inputs.values());
        handle_synthetic_directories(
            &mut flattened_inputs,
            &mut path_mapper,
            &request.working_dir,
            tmpdir.path(),
        )?;

        //collect command string and correct args for staged paths
        let mut args = path_mapper.correct_execution_path(command::build_command(
            tool,
            &request.inputs,
            &runtime,
        )?);

        //handle docker requirement
        let mut container = "alpine".to_string();
        if let Some(dr) = tool.get_requirement_or_hint::<DockerRequirement>() {
            if let Some(df) = &dr.docker_file
                && let Some(dt) = &dr.docker_image_id
            {
                build_container(self.client.inner(), df, dt).await?;
                container = dt.to_string();
            } else if let Some(dp) = &dr.docker_pull {
                container = dp.to_string();
            }
        }

        let stdout_file = if let Some(s) = &tool.stdout {
            &format!("/mnt/task/{s}")
        } else {
            CONTAINER_STDOUT_FILE
        };

        let stderr_file = if let Some(s) = &tool.stderr {
            &format!("/mnt/task/{s}")
        } else {
            CONTAINER_STDERR_FILE
        };
        //correct and add the stdin value
        let mut stdin = get_stdin(tool, &inputs);
        if let Some(stdin) = &mut stdin {
            path_mapper.add(&stdin)?;
            *stdin = path_mapper
                .get_guest(&stdin)
                .unwrap() //allowed as we just added it!
                .to_string_lossy()
                .into_owned();
            args.push(stdin.to_string());
        }

        info!("Executing: {}", args.join(" "));

        //build crankshaft task object
        let mut task = Task::builder()
            .maybe_name(tool.label.clone())
            .maybe_description(tool.doc.as_ref().map(|d| docstring(d.clone())))
            .executions(nonempty![
                Execution::builder()
                    .work_dir(CONTAINER_WORKDIR)
                    .env(request.environment.clone())
                    .program(&args[0])
                    .args(&args[1..])
                    .image(container)
                    .stdout(stdout_file)
                    .stderr(stderr_file)
                    .maybe_stdin(stdin)
                    .build()
            ])
            .resources(
                Resources::builder()
                    .cpu(runtime.cores as f64)
                    //.disk(request.runtime.outdir_size) //we don't use this currently
                    .ram(runtime.ram as f64)
                    .build(),
            )
            .build();

        //add file inputs to task
        for mut input in flattened_inputs {
            input.dry_validation();
            let path = input.path().cloned();

            let ty = match input {
                FileOrDirectory::File(_) => input::Type::File,
                FileOrDirectory::Directory(_) => input::Type::Directory,
            };

            if let Some(path) = path {
                let guest_path = path_mapper.get_guest(path).unwrap();
                let host_path = path_mapper.get_host(guest_path).unwrap();
                task.add_input(
                    Input::builder()
                        .contents(Contents::Path(host_path.to_path_buf()))
                        .path(guest_path.to_string_lossy())
                        .ty(ty)
                        .build(),
                );
            }
        }

        //add outdir mount
        task.add_input(
            Input::builder()
                .name("outdir")
                .contents(Contents::Path(outdir.path().to_path_buf()))
                .path(CONTAINER_WORKDIR)
                .ty(input::Type::Directory)
                .read_only(false)
                .build(),
        );

        //handle stderr output
        let stderr_out_file = if let Some(stderr) = &tool.stderr {
            outdir.path().join(stderr)
        } else {
            tmpdir.path().join("stderr")
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
        let stdout_out_file = if let Some(stdout) = &tool.stdout {
            outdir.path().join(stdout)
        } else {
            tmpdir.path().join("stdout")
        };
        task.add_output(
            Output::builder()
                .name("stdout")
                .path(stdout_file)
                .url(Url::from_file_path(&stdout_out_file).unwrap())
                .ty(output::Type::File)
                .build(),
        );

        // need to handle iwdr

        let status = self.backend.run(task, token)?.await?;

        //evaluate stderr/stdout
        let stdout = fs::read_to_string(&stdout_out_file)?;
        if !stdout.is_empty() {
            eprintln!("{stdout}");
        }
        let stderr = fs::read_to_string(&stderr_out_file)?;
        if !stderr.is_empty() {
            eprintln!("{stderr}");
        }

        // need to collect outputs
        let outputs = collect_command_outputs(
            &tool.outputs,
            outdir.path(),
            &runtime.outdir,
            &stdout_out_file,
            &stderr_out_file,
        )?;
        let json = serde_json::to_string_pretty(&outputs)?;
        println!("{json}");

        //evaluate exitstatus based on tool's expected exit codes

        Ok(status)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::load_execution_context;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_docker_backend_creation() {
        let config = Config::default();
        let backend = DockerBackend::new(config).await;
        assert!(backend.is_ok());
    }

    #[tokio::test]
    async fn test_docker_backend_run_simple() {
        let base_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../testdata/cwl/tests")
            .canonicalize()
            .unwrap();
        let specification_path = base_dir.join("cat-tool-shortcut.cwl");
        let inputs_path = base_dir.join("cat-job.json");

        let config = Config::default();
        let backend = DockerBackend::new(config).await.unwrap();
        let tmpdir = tempdir().unwrap();
        let request =
            load_execution_context(specification_path, inputs_path, Some(tmpdir.path())).unwrap();
        let cancellation_token = CancellationToken::new();
        let result = backend.run(&request, cancellation_token).await;
        assert!(result.is_ok());

        //check if output file exists
        let out_file = tmpdir.path().join("output");
        assert!(out_file.exists());
    }

    #[tokio::test]
    async fn test_docker_backend_run_simple_with_dir() {
        let base_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../testdata/cwl/tests")
            .canonicalize()
            .unwrap();
        let specification_path = base_dir.join("dir3.cwl");
        let inputs_path = base_dir.join("dir3-job.yml");

        let config = Config::default();
        let backend = DockerBackend::new(config).await.unwrap();
        let tmpdir = tempdir().unwrap();
        let request =
            load_execution_context(specification_path, inputs_path, Some(tmpdir.path())).unwrap();
        let cancellation_token = CancellationToken::new();
        let result = backend.run(&request, cancellation_token).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_docker_backend_run_simple_with_value_from() {
        let base_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../testdata/cwl/tests")
            .canonicalize()
            .unwrap();
        let specification_path = base_dir.join("cat3-from-dir.cwl");
        let inputs_path = base_dir.join("cat-from-dir-job.yaml");

        let config = Config::default();
        let backend = DockerBackend::new(config).await.unwrap();
        let tmpdir = tempdir().unwrap();
        let request =
            load_execution_context(specification_path, inputs_path, Some(tmpdir.path())).unwrap();
        let cancellation_token = CancellationToken::new();
        let result = backend.run(&request, cancellation_token).await;

        assert!(result.is_ok());
        //check if output file exists
        let out_file = tmpdir.path().join("output.txt");

        assert!(out_file.exists());
    }
}
