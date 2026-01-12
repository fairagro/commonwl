use crankshaft::engine::service::runner::backend::TaskRunError;
use cwl_core::documents::CWLDocument;
use nonempty::NonEmpty;
use std::{collections::HashMap, process::ExitStatus};
use tokio_util::sync::CancellationToken;

pub mod docker;

pub struct TaskRequest<'a> {
    pub definition: &'a CWLDocument,
    pub inputs: &'a HashMap<String, serde_yaml::Value>,
}

pub trait TaskBackend {
    fn run(
        self,
        task: &TaskRequest<'_>,
        token: CancellationToken,
    ) -> impl Future<Output = Result<NonEmpty<ExitStatus>, TaskRunError>> + Send;
}
