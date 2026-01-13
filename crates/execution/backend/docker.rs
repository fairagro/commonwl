use crate::docker::build_container;
use crate::inputs::collect_inputs;
use crate::{
    backend::{TaskBackend, TaskRequest},
    command,
};
use crankshaft::docker::Docker;
use crankshaft::{
    config::backend::docker::Config,
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
            Execution, Input,
            input::{self, Contents},
        },
    },
};
use cwl_core::requirements::DockerRequirement;
use cwl_core::{
    documents::CWLDocument,
    files::FileOrDirectory,
    inputs::{DefaultValue, InputDataProvider},
};
use nonempty::NonEmpty;
use nonempty::nonempty;
use tracing::info;
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

    async fn create_docker_task(&self, req: &TaskRequest<'_>) -> anyhow::Result<Task> {
        let CWLDocument::CommandLineTool(tool) = req.definition else {
            panic!("damn");
        };

        let args = command::build_command(tool, req.inputs)?;

        info!("resolved command to: {}", args.join(" "));

        let mut container = "alpine".to_string();
        if let Some(dr) = tool.get_requirement_or_hint::<DockerRequirement>() {
            if let Some(df) = &dr.docker_file
                && let Some(dt) = &dr.docker_image_id
            {
                self.build_container(df, dt).await?;
                container = dt.to_string();
            } else if let Some(dp) = &dr.docker_pull {
                container = dp.to_string();
            }
        }

        let mut task = Task::builder()
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

        //collect inputs and use staging mechanisms for file in dir
        let input_values = collect_inputs(&tool.clone().into(), req.inputs)?;

        for input in &tool.inputs {
            let value = input_values.get(&input.id().clone().unwrap()).unwrap();
            //we stage here, so we want to only get file or dir
            if let DefaultValue::FileOrDirectory(fod) = value {
                let str = value.to_string();
                let ty = if matches!(fod, FileOrDirectory::File(_)) {
                    input::Type::File
                } else {
                    input::Type::Directory
                };
                task.add_input(
                    Input::builder()
                        .name(input.id().clone().unwrap())
                        .contents(Contents::Path(
                            Path::new(&str).canonicalize()?.to_path_buf(),
                        ))
                        .path(format!("{CONTAINER_INPUT_DIR}/{str}"))
                        .ty(ty)
                        .build(),
                );
            }
        }

        Ok(task)
    }

    async fn build_container(&self, docker_file: &str, docker_image: &str) -> anyhow::Result<()> {
        build_container(self.client.inner(), docker_file, docker_image).await
    }
}

impl TaskBackend for DockerBackend {
    async fn run(
        &self,
        task: &TaskRequest<'_>,
        token: CancellationToken,
    ) -> Result<NonEmpty<ExitStatus>, TaskRunError> {
        let task = self.create_docker_task(task).await?;
        self.backend.run(task, token)?.await
    }
}
