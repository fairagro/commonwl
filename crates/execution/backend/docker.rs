use crate::inputs::collect_inputs;
use crate::{
    backend::{TaskBackend, TaskRequest},
    command,
};
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
use cwl_core::{
    documents::CWLDocument,
    files::FileOrDirectory,
    inputs::{DefaultValue, InputDataProvider},
};
use nonempty::NonEmpty;
use nonempty::nonempty;
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

fn create_docker_task(req: &TaskRequest) -> anyhow::Result<Task> {
    let CWLDocument::CommandLineTool(tool) = req.definition else {
        panic!("damn");
    };

    let args = command::build_command(tool, req.inputs)?;
    let mut task = Task::builder()
        .maybe_name(tool.label.clone())
        .executions(nonempty![
            Execution::builder()
                .work_dir(CONTAINER_WORKDIR)
                .program(&args[0])
                .args(&args[1..])
                .image("alpine") //todo get real imnage
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

pub struct DockerBackend {
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

        Ok(Self { backend })
    }
}

impl TaskBackend for DockerBackend {
    async fn run(
        self,
        task: &TaskRequest<'_>,
        token: CancellationToken,
    ) -> Result<NonEmpty<ExitStatus>, TaskRunError> {
        let task = create_docker_task(task).unwrap();
        self.backend.run(task, token)?.await
    }
}
