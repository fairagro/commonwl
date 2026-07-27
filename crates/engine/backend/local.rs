use crate::{
    backend::{
        TaskBackend, TaskExecutionRequest, TaskExecutionResult,
        local::command::CommandBackend,
        mount::{MountStrategy, mount_input, mount_workdir_item},
    },
    docker::{ContainerBuildOptions, ContainerEngine, build_container_command},
    expression::{do_eval, do_eval_to_string},
};
use anyhow::{Context, ensure};
use async_trait::async_trait;
use crankshaft::engine::{
    Task,
    service::runner::{Backend, backend::TaskRunError},
    task::{
        Execution, Input, Output, Resources,
        input::{self, Contents},
        output::{self},
    },
};
use cwl_core::{IntegerOrExpression, files::FileOrDirectory, requirements::StringOrInclude};
use cwl_engine_storage::{StorageBackend, StoragePath};
use nonempty::nonempty;
use std::{env, fs, sync::Arc, time::Duration};
use tokio_util::sync::CancellationToken;
use tracing::{debug, error};
use url::Url;
use uuid::Uuid;

pub mod command;

#[derive(Debug, Clone)]
pub struct LocalBackend {
    uuid: String,
    container_engine: ContainerEngine,
    storage: Arc<StorageBackend>,
    data_store: StoragePath,
    //wrapper to crankshaft backend
    backend: Arc<CommandBackend>,
}

impl LocalBackend {
    #[must_use]
    pub fn new(
        container_engine: ContainerEngine,
        storage: Arc<StorageBackend>,
        data_store: StoragePath,
    ) -> Self {
        let backend = Arc::new(CommandBackend::new(storage.clone()));

        Self {
            uuid: Uuid::new_v4().to_string()[..8].to_string(),
            container_engine,
            storage,
            data_store,
            backend,
        }
    }
}

#[allow(clippy::too_many_lines)]
#[async_trait]
impl TaskBackend for LocalBackend {
    async fn run(
        &self,
        request: &TaskExecutionRequest<'_>,
        token: CancellationToken,
    ) -> anyhow::Result<TaskExecutionResult> {
        let stdout_file = if let Some(s) = request.stdout_file {
            &format!("{}/{s}", self.container_tmp_dir())
        } else {
            &format!("{}/stdout", self.container_tmp_dir())
        };

        let stderr_file = if let Some(s) = request.stderr_file {
            &format!("{}/{s}", self.container_tmp_dir())
        } else {
            &format!("{}/stderr", self.container_tmp_dir())
        };

        let mut inputs = request.inputs.to_vec();

        //this is a local only backend so we assume outdir is local
        ensure!(request.outdir.is_local());
        let outdir = request.outdir.as_local_path()?;
        ensure!(request.tmpdir.is_local());
        let tmpdir = request.tmpdir.as_local_path()?;

        //lock in file literals
        for file in &mut inputs
            .iter_mut()
            .filter_map(|f| {
                if let FileOrDirectory::File(f) = f {
                    Some(f)
                } else {
                    None
                }
            })
            .filter(|f| f.location.is_none() && f.contents.is_some())
        {
            let file_uuid = &Uuid::new_v4().to_string()[0..8];
            let location = tmpdir.join(file_uuid);
            debug!("Writing File literal to {location:?}");
            fs::write(&location, file.contents.as_ref().unwrap()).with_context(|| {
                format!("Could not lock in File Literal at {}", location.display())
            })?;
            file.location = Some(
                Url::from_file_path(&location)
                    .map_err(|()| {
                        anyhow::anyhow!("Could not create URL from {}", location.display())
                    })?
                    .to_string(),
            );
        }

        let mut args = request
            .command
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>();

        let (extras, mounts): (Vec<_>, Vec<_>) = request.mounts.iter().cloned().partition(|m| {
            !m.target.to_file_path().unwrap().starts_with(&outdir) && request.use_container
        });

        //handle docker requirement
        if let Some(dr) = &request.docker {
            let dr = (*dr).clone();
            let image_id = dr.docker_pull.or(dr.docker_image_id).unwrap();

            let df = if let Some(df) = dr.docker_file {
                Some(match df {
                    StringOrInclude::Include(include) => include.include.clone(),
                    StringOrInclude::String(s) => {
                        let tmp = request.tmpdir.join("Dockerfile")?;
                        self.storage
                            .upload_bytes(s.as_bytes(), &tmp.as_url()?)
                            .await?;
                        tmp.as_local_path()?.to_string_lossy().into_owned()
                    }
                })
            } else {
                None
            };

            let options = ContainerBuildOptions::builder()
                .docker_image_id(image_id)
                .network(request.network)
                .engine(self.container_engine)
                .env(request.env.clone())
                .outdir(self.container_work_dir())
                .tmpdir(request.runtime.tmpdir.to_string_lossy())
                .workdir(request.execution_path)
                .maybe_docker_file(df)
                .mounts(extras)
                .build();
            args = build_container_command(args, &inputs, options, request.specificationdir)?;
        }

        //build crankshaft task object
        #[allow(clippy::cast_precision_loss)]
        let mut task = Task::builder()
            .name(request.id)
            .maybe_description(request.description)
            .executions(nonempty![
                Execution::builder()
                    .work_dir(self.container_work_dir())
                    .env(request.env.clone())
                    .program(&args[0])
                    .args(&args[1..])
                    .image("unsupported")
                    .stdout(stdout_file)
                    .stderr(stderr_file)
                    .maybe_stdin(request.stdin_file)
                    .build()
            ])
            .resources(
                Resources::builder()
                    .cpu(request.runtime.cores as f64)
                    .disk(request.runtime.outdir_size as f64)
                    .ram(request.runtime.ram as f64)
                    .build(),
            )
            .build();

        //add file inputs to task
        for input in &inputs {
            mount_input(&mut task, input)?;
        }

        for mount in mounts {
            let inputs = mount_workdir_item(
                mount.clone(),
                &request.outdir.as_url()?,
                request.execution_path,
                request.use_container,
                self.storage(),
                MountStrategy::Local,
            )
            .await?;
            for input in inputs {
                task.add_input(input);
            }
        }

        //add outdir mount
        task.add_input(
            Input::builder()
                .name("outdir")
                .contents(Contents::Path(outdir.clone()))
                .path(self.container_work_dir())
                .ty(input::Type::Directory)
                .read_only(false)
                .build(),
        );

        //add tmpdir input
        task.add_input(
            Input::builder()
                .name("tmpdir")
                .contents(Contents::Path(tmpdir.clone()))
                .path(self.container_tmp_dir())
                .ty(input::Type::Directory)
                .read_only(false)
                .build(),
        );

        //handle stderr output
        let stderr_out_file = if let Some(stderr) = request.stderr_file {
            let stderr = do_eval_to_string(stderr, request.eval_context);
            outdir.join(stderr)
        } else {
            tmpdir.join(format!("stderr_{}", &Uuid::new_v4().to_string()[..8]))
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
        let stdout_out_file = if let Some(stdout) = request.stdout_file {
            let stdout = do_eval_to_string(stdout, request.eval_context);
            outdir.join(stdout)
        } else {
            tmpdir.join(format!("stdout_{}", &Uuid::new_v4().to_string()[..8]))
        };
        task.add_output(
            Output::builder()
                .name("stdout")
                .path(stdout_file)
                .url(Url::from_file_path(&stdout_out_file).unwrap())
                .ty(output::Type::File)
                .build(),
        );

        //the backend needs to copy back workdir
        task.add_output(
            Output::builder()
                .name("workdir")
                .path(self.container_work_dir())
                .url(Url::from_file_path(outdir).unwrap())
                .ty(output::Type::Directory)
                .build(),
        );

        let timelimit = request
            .timelimit
            .as_ref()
            .and_then(|ttl| match &ttl.timelimit {
                IntegerOrExpression::Int(i) => Some(i64::from(*i)),
                IntegerOrExpression::Long(i) => Some(*i),
                IntegerOrExpression::Expression(e) => {
                    let value = do_eval(e, request.eval_context).ok()?;
                    value.as_i64()
                }
            });
        let exit_status = if let Some(timeout) = timelimit
            && timeout != 0
        {
            let limit = Duration::from_secs(timeout.try_into()?); //this is intended to throw!
            let token_clone = token.clone();
            tokio::select! {
                result = self.backend.run(task, token)? => result?,
                () = tokio::time::sleep(limit) => {
                    token_clone.cancel();
                    error!("Timelimit reached: {timeout}");
                    return Err(TaskRunError::Canceled.into());
                }
            }
        } else {
            //no time constraint
            self.backend.run(task, token)?.await?
        };
        Ok(TaskExecutionResult {
            exit_status,
            stdout_file: StoragePath::Local(stdout_out_file),
            stderr_file: StoragePath::Local(stderr_out_file),
        })
    }

    fn task_scoped(&self) -> Arc<dyn TaskBackend> {
        Arc::new(LocalBackend::new(
            self.container_engine,
            self.storage(),
            self.data_store().clone(),
        )) as Arc<dyn TaskBackend>
    }

    fn storage(&self) -> Arc<StorageBackend> {
        self.storage.clone()
    }

    fn container_input_dir(&self) -> String {
        env::temp_dir()
            .join(&self.uuid)
            .join("inputs")
            .to_string_lossy()
            .into_owned()
    }
    fn container_work_dir(&self) -> String {
        env::temp_dir()
            .join(&self.uuid)
            .join("work")
            .to_string_lossy()
            .into_owned()
    }
    fn container_tmp_dir(&self) -> String {
        env::temp_dir()
            .join(&self.uuid)
            .join("tmp")
            .to_string_lossy()
            .into_owned()
    }

    fn data_store(&self) -> &StoragePath {
        &self.data_store
    }
}

impl Drop for LocalBackend {
    fn drop(&mut self) {
        if fs::exists(self.container_input_dir()).unwrap_or_default() {
            fs::remove_dir_all(self.container_input_dir())
                .expect("Could not remove backend's input directory");
        }

        if fs::exists(self.container_tmp_dir()).unwrap_or_default() {
            fs::remove_dir_all(self.container_tmp_dir())
                .expect("Could not remove backend's tmp directory");
        }

        if fs::exists(self.container_work_dir()).unwrap_or_default() {
            fs::remove_dir_all(self.container_work_dir())
                .expect("Could not remove backend's working directory");
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        ContainerEngine, InputObject, LocalBackend, create_execution_request_from_document,
        create_execution_request_with_inputs, execute, execute_commandline_tool,
    };
    use cwl_core::{
        OneOrMany,
        documents::{CWLDocument, CommandLineTool},
        files::{Dirent, File, FileOrDirectory},
        inputs::DefaultValue,
        outputs::{
            CommandOutputArraySchema, CommandOutputBinding, CommandOutputParameter,
            CommandOutputSchema, CommandOutputType,
        },
        requirements::{
            Include, InitialWorkDirRequirement, ListingItems, StringOrInclude, ToolRequirements,
            WorkDirItems,
        },
        types::CWLType,
    };
    use cwl_engine_storage::{StorageBackend, StoragePath};
    use std::{env, path::Path, sync::Arc};
    use tempfile::tempdir;
    use tokio_util::sync::CancellationToken;

    #[tokio::test]
    async fn test_url_in_inputs() {
        let base_dir =
            dunce::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata"))
                .unwrap();
        let specification_path = base_dir.join("cat-tool.cwl");

        let mut inputs = InputObject::default();
        inputs.inputs.insert(
            "file1".to_string(),
            DefaultValue::FileOrDirectory(FileOrDirectory::File(
                File::builder()
                    .location("https://raw.githubusercontent.com/JensKrumsieck/PorphyStruct/refs/heads/master/README.md")
                    .build(),
            )),
        );

        let storage = Arc::new(StorageBackend::new());
        let data_store = StoragePath::from_local(Path::new(&env::temp_dir()));
        let backend = Arc::new(LocalBackend::new(
            ContainerEngine::Docker,
            storage,
            data_store,
        ));

        let tmpdir = tempdir().unwrap();
        let request = create_execution_request_with_inputs(
            specification_path,
            inputs,
            Some(tmpdir.path()),
            None,
        )
        .unwrap();
        let cancellation_token = CancellationToken::new();
        let result = execute_commandline_tool(backend, &request, cancellation_token).await;
        dbg!(&result);
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_local_backend_simple() {
        let base_dir =
            dunce::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/"))
                .unwrap();
        let specification_path = dunce::canonicalize(base_dir.join("echo.cwl")).unwrap();

        let storage = Arc::new(StorageBackend::new());
        let data_store = StoragePath::from_local(&env::temp_dir());
        let backend = Arc::new(LocalBackend::new(
            ContainerEngine::Docker,
            storage,
            data_store,
        ));
        let tmpdir = tempdir().unwrap();
        let request = crate::create_execution_request_with_inputs(
            specification_path,
            InputObject::default(),
            Some(tmpdir.path()),
            None,
        )
        .unwrap();

        let cancellation_token = CancellationToken::new();
        let result = crate::execute(backend, &request, cancellation_token).await;
        dbg!(&result);
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_local_backend_raw() {
        let base_dir =
            dunce::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/debug"))
                .unwrap();
        let specification_path = dunce::canonicalize(base_dir.join("debug.cwl")).unwrap();

        let storage = Arc::new(StorageBackend::new());
        let data_store = StoragePath::from_local(&env::temp_dir());
        let backend = Arc::new(LocalBackend::new(
            ContainerEngine::Docker,
            storage,
            data_store,
        ));
        let tmpdir = tempdir().unwrap();
        let request = crate::create_execution_request_with_inputs(
            specification_path,
            InputObject::default(),
            Some(tmpdir.path()),
            None,
        )
        .unwrap();

        let cancellation_token = CancellationToken::new();
        let result = crate::execute(backend, &request, cancellation_token).await;
        dbg!(&result);
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_local_backend_subfolder() {
        let base_dir =
            dunce::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/"))
                .unwrap();
        let specification_path = dunce::canonicalize(base_dir.join("subfolder/echo.cwl")).unwrap();

        let storage = Arc::new(StorageBackend::new());
        let data_store = StoragePath::from_local(&env::temp_dir());
        let backend = Arc::new(LocalBackend::new(
            ContainerEngine::Docker,
            storage,
            data_store,
        ));
        let tmpdir = tempdir().unwrap();
        let request = crate::create_execution_request_with_inputs(
            specification_path,
            InputObject::default(),
            Some(tmpdir.path()),
            None,
        )
        .unwrap();

        let cancellation_token = CancellationToken::new();
        let result = crate::execute(backend, &request, cancellation_token).await;
        dbg!(&result);
        assert!(result.is_ok());
    }

    #[tokio::test]
    #[cfg(not(target_os = "macos"))]
    async fn test_local_backend_docker_staged_script() {
        let base_dir = dunce::canonicalize(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/language_workflow/"),
        )
        .unwrap();

        let specification_path =
            dunce::canonicalize(base_dir.join("workflows/calculation/calculation.cwl")).unwrap();

        let storage = Arc::new(StorageBackend::new());
        let data_store = StoragePath::from_local(&env::temp_dir());
        let backend = Arc::new(LocalBackend::new(
            ContainerEngine::Docker,
            storage,
            data_store,
        ));

        let tmpdir = tempdir().unwrap();

        let mut inputs = InputObject::default();
        inputs.inputs.insert(
            "speakers".to_string(),
            DefaultValue::FileOrDirectory(FileOrDirectory::File(
                File::builder()
                    .location(
                        dunce::canonicalize(base_dir.join("data/speakers_revised.csv"))
                            .unwrap()
                            .to_string_lossy()
                            .into_owned(),
                    )
                    .build(),
            )),
        );
        inputs.inputs.insert(
            "population".to_string(),
            DefaultValue::FileOrDirectory(FileOrDirectory::File(
                File::builder()
                    .location(
                        dunce::canonicalize(base_dir.join("data/population.csv"))
                            .unwrap()
                            .to_string_lossy()
                            .into_owned(),
                    )
                    .build(),
            )),
        );

        let request = crate::create_execution_request_with_inputs(
            specification_path,
            inputs,
            Some(tmpdir.path()),
            None,
        )
        .unwrap();

        let cancellation_token = CancellationToken::new();
        let result = crate::execute(backend, &request, cancellation_token).await;
        dbg!(&result);
        assert!(result.is_ok());

        assert!(tmpdir.path().join("results.csv").exists());
    }

    #[tokio::test]
    #[cfg(not(target_os = "macos"))]
    async fn test_local_backend_docker_iwdr_include() {
        let base_dir =
            dunce::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata"))
                .unwrap();

        let specification_path = base_dir.join("load.cwl");

        let storage = Arc::new(StorageBackend::new());
        let data_store = StoragePath::from_local(&env::temp_dir());
        let backend = Arc::new(LocalBackend::new(
            ContainerEngine::Docker,
            storage,
            data_store,
        ));

        let tmpdir = tempdir().unwrap();
        let request = crate::create_execution_request_with_inputs(
            specification_path,
            InputObject::default(),
            Some(tmpdir.path()),
            None,
        )
        .unwrap();

        let cancellation_token = CancellationToken::new();
        let result = crate::execute_commandline_tool(backend, &request, cancellation_token).await;
        dbg!(&result);
        assert!(result.is_ok());
    }

    #[tokio::test]
    #[cfg(not(target_os = "macos"))] //ignore on macos because of CI issues with docker
    async fn test_local_backend_language_workflow() {
        let base_dir = dunce::canonicalize(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/language_workflow/"),
        )
        .unwrap();
        let specification_path =
            dunce::canonicalize(base_dir.join("workflows/main/main.cwl")).unwrap();
        let inputs_path = base_dir.join("inputs.yml");

        let storage = Arc::new(StorageBackend::new());
        let data_store = StoragePath::from_local(&env::temp_dir());
        let backend = Arc::new(LocalBackend::new(
            ContainerEngine::Docker,
            storage,
            data_store,
        ));
        let tmpdir = tempdir().unwrap();
        let request =
            crate::create_execution_request(specification_path, inputs_path, Some(tmpdir.path()))
                .unwrap();
        let cancellation_token = CancellationToken::new();
        let result = crate::execute(backend, &request, cancellation_token).await;
        dbg!(&result);
        assert!(result.is_ok());

        //check if output file exists
        let out_file = tmpdir.path().join("results.svg");
        assert!(out_file.exists());
    }

    #[tokio::test]
    async fn test_local_backend_inline() {
        let storage = Arc::new(StorageBackend::new());
        let backend = Arc::new(LocalBackend::new(
            ContainerEngine::Docker,
            storage,
            StoragePath::from_local(&env::temp_dir()),
        ));

        let tool = CommandLineTool::builder()
            .cwl_version("v1.2")
            .base_command(&["python3", "workflows/temp/temp.py"])
            .requirements(vec![ToolRequirements::InitialWorkDirRequirement(
                InitialWorkDirRequirement::builder()
                    .listing(WorkDirItems::ListingItems(vec![ListingItems::Dirent(
                        Dirent::builder()
                            .entryname("workflows/temp/temp.py")
                            .entry(StringOrInclude::Include(
                                Include::builder().include("workflows/temp/temp.py").build(),
                            ))
                            .build(),
                    )]))
                    .build(),
            )])
            .outputs(vec![
                CommandOutputParameter::builder()
                    .id("catch_all")
                    .output_binding(
                        CommandOutputBinding::builder()
                            .glob(OneOrMany::One("*".to_string()))
                            .build(),
                    )
                    .r#type(CommandOutputType::CommandOutputSchema(Box::new(
                        CommandOutputSchema::Array(
                            CommandOutputArraySchema::builder()
                                .items(OneOrMany::Many(vec![
                                    CommandOutputType::CWLType(CWLType::Null),
                                    CommandOutputType::CWLType(CWLType::File),
                                    CommandOutputType::CWLType(CWLType::Directory),
                                ]))
                                .build(),
                        ),
                    )))
                    .build(),
            ])
            .build();

        let base_dir =
            dunce::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/debug"))
                .unwrap();

        let job = create_execution_request_from_document(
            CWLDocument::CommandLineTool(tool),
            InputObject::default(),
            base_dir,
            Some(tempdir().unwrap().path()), //pumps halt irgendwoa hin
            None,
        )
        .unwrap();

        let cancellation_token = CancellationToken::new();
        let result = execute(backend, &job, cancellation_token).await;

        dbg!(&result);
        assert!(result.is_ok());
    }
}
