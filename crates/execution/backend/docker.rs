use crankshaft::{
    config::backend::docker::Config,
    engine::{
        Task,
        service::{
            name::{GeneratorIterator, UniqueAlphanumeric},
            runner::{Backend, backend::{TaskRunError, docker}},
        },
    },
};
use nonempty::NonEmpty;
use std::{process::ExitStatus, sync::{Arc, Mutex}};
use tokio_util::sync::CancellationToken;

use crate::backend::TaskBackend;

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
    async fn run(self, task: Task, token: CancellationToken) -> Result<NonEmpty<ExitStatus>, TaskRunError> {
        self.backend.run(task, token)?.await 
    }
}
