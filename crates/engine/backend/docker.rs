use crate::{
    backend::TaskBackend, command, docker::build_container, input::collect_inputs,
    pathmapper::PathMapper,
};
use crankshaft::{
    config::backend::docker::Config,
    docker::Docker,
    engine::{
        Task,
        service::{
            name::{GeneratorIterator, UniqueAlphanumeric},
            runner::{
                Backend,
                backend::{TaskRunError, docker},
            },
        },
        task::{
            Execution, Input, Output,
            input::{self, Contents},
            output,
        },
    },
};
use cwl_core::{
    docstring, documents::CWLDocument, files::FileOrDirectory, inputs::DefaultValue,
    requirements::DockerRequirement,
};
use nonempty::{NonEmpty, nonempty};
use std::{
    path::Path,
    process::ExitStatus,
    sync::{Arc, Mutex},
};
use tokio_util::sync::CancellationToken;
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
        request: &super::ExecutionRequest,
        token: CancellationToken,
    ) -> Result<NonEmpty<ExitStatus>, TaskRunError> {
        let mut inputs = collect_inputs(&request.specification, &request.inputs)?;
        let stage_dir = Path::new(CONTAINER_INPUT_DIR);

        let path_mapper = PathMapper::new(&inputs, &request.working_dir, stage_dir)?;
        let CWLDocument::CommandLineTool(tool) = &request.specification else {
            panic!("Currently only CommandLineTool is supported in Docker backend");
        };

        //collect command string and correct args for staged paths
        let args =
            path_mapper.correct_execution_path(command::build_command(tool, &request.inputs)?);

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

        let stderr_file = if let Some(s) = &tool.stdout {
            &format!("/mnt/task/{s}")
        } else {
            CONTAINER_STDERR_FILE
        };

        //build crankshaft task object
        let mut task = Task::builder()
            .maybe_name(tool.label.clone())
            .maybe_description(tool.doc.as_ref().map(|d| docstring(d.clone())))
            .executions(nonempty![
                Execution::builder()
                    .work_dir(CONTAINER_WORKDIR)
                    .program(&args[0])
                    .args(&args[1..])
                    .image(container)
                    .stdout(stdout_file)
                    .stderr(stderr_file)
                    .build()
            ])
            .build();

        //add file inputs to task
        for input in inputs.values_mut() {
            add_input_to_task(input, &mut task, &path_mapper)?;
        }

        //handle stdout/stderr outputs if wanted
        if let Some(stderr) = &tool.stderr {
            task.add_output(
                Output::builder()
                    .name("stderr")
                    .path(stderr_file)
                    .url(Url::from_file_path(request.working_dir.join(stderr)).unwrap())
                    .ty(output::Type::File)
                    .build(),
            );
        }
        if let Some(stdout) = &tool.stdout {
            task.add_output(
                Output::builder()
                    .name("stderr")
                    .path(stdout_file)
                    .url(Url::from_file_path(request.working_dir.join(stdout)).unwrap())
                    .ty(output::Type::File)
                    .build(),
            );
        }

        dbg!(&task);
        // need to handle iwdr
        // need to collect outputs

        self.backend.run(task, token)?.await
    }
}

fn add_input_to_task(
    df: &mut DefaultValue,
    task: &mut Task,
    path_mapper: &PathMapper,
) -> anyhow::Result<()> {
    match df {
        DefaultValue::FileOrDirectory(fod) => {
            let input_type = match fod {
                FileOrDirectory::File(_) => input::Type::File,
                FileOrDirectory::Directory(_) => input::Type::Directory,
            };
            fod.dry_validation();
            let p = fod.path().unwrap();
            let guest_path = path_mapper.get_guest(p).unwrap();
            let host_path = path_mapper.get_host(guest_path).unwrap();

            task.add_input(
                Input::builder()
                    .path(guest_path.to_string_lossy())
                    .contents(Contents::Path(host_path.into()))
                    .ty(input_type)
                    .build(),
            );
        }
        DefaultValue::Any(v) => match v {
            serde_yaml::Value::Sequence(values) => {
                for v in values {
                    let mut dv = serde_yaml::from_value(v.clone())?;
                    add_input_to_task(&mut dv, task, path_mapper)?;
                }
            }
            serde_yaml::Value::Mapping(mapping) => {
                for v in mapping.values() {
                    let mut dv = serde_yaml::from_value(v.clone())?;
                    add_input_to_task(&mut dv, task, path_mapper)?;
                }
            }
            _ => {}
        },
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::load_execution_context;

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
        let specification_path = base_dir.join("cat3-tool-mediumcut.cwl");
        let inputs_path = base_dir.join("cat-job.json");

        let config = Config::default();
        let backend = DockerBackend::new(config).await.unwrap();
        let request = load_execution_context(specification_path, inputs_path).unwrap();
        let cancellation_token = CancellationToken::new();
        let result = backend.run(&request, cancellation_token).await;

        assert!(result.is_ok());
        //add check for exit status and outputs
    }
}
