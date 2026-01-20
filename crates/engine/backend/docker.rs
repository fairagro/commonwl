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
        task::Execution,
    },
};
use cwl_core::{documents::CWLDocument, requirements::DockerRequirement};
use nonempty::{NonEmpty, nonempty};
use std::{
    path::Path,
    process::ExitStatus,
    sync::{Arc, Mutex},
};
use tokio_util::sync::CancellationToken;

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
        let inputs = collect_inputs(&request.specification, &request.inputs)?;
        let stage_dir = Path::new(CONTAINER_INPUT_DIR);

        let _path_mapper = PathMapper::new(&inputs, &request.working_dir, stage_dir)?;
        let CWLDocument::CommandLineTool(tool) = &request.specification else {
            panic!("Currently only CommandLineTool is supported in Docker backend");
        };

        let args = command::build_command(tool, &request.inputs)?;

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

        let task = Task::builder()
            .maybe_name(tool.label.clone())
            .executions(nonempty![
                Execution::builder()
                    .work_dir(CONTAINER_WORKDIR)
                    .program(&args[0])
                    .args(&args[1..])
                    .image(container)
                    .stdout(CONTAINER_STDOUT_FILE)
                    .stderr(CONTAINER_STDERR_FILE)
                    .build()
            ])
            .build();

        // need to correct input/output paths here for command
        // need to handle iwdr
        // need to stage inputs based on path mapper
        // need to collect outputs

        self.backend.run(task, token)?.await
    }
}

#[cfg(test)]
mod tests {
    use crate::backend::load_execution_context;

    use super::*;

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
        let specification_path = base_dir.join("cat3-tool-shortcut.cwl");
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
