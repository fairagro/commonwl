use crate::{
    V1_2_0,
    backend::mount::remove_materialized_inputs,
    command, cwl_version,
    environment::{
        env::handle_environment,
        runtime::{Runtime, build_runtime},
        workdir::{WorkDirMount, handle_inplace_update, stage_work_dir},
    },
    expression::{EvaluationContext, do_eval},
    input::{collect_inputs, flatten_inputs, get_stdin},
    io::file::collect_secondary_files_for_inputs,
    output::{
        OutputCollectionContext, collect_command_outputs, collect_expression_outputs,
        collect_workflow_outputs,
    },
    request::{
        ExecutionRequest, InputObject, create_execution_request_from_document,
        create_execution_request_with_inputs,
    },
    scatter,
    schema::format_validation::get_format_validator,
    tree::build_execution_tree,
    workflow::{collect_raw_inputs, eval_inputs},
};
use anyhow::Context;
use async_trait::async_trait;
use crankshaft::engine::service::runner::backend::TaskRunError;
use cwl_core::IntegerOrExpression;
use cwl_core::{
    BoolOrExpression, docstring,
    documents::{CWLDocument, ScatterMethod, StringOrDocument, WorkflowStep},
    files::FileOrDirectory,
    inputs::DefaultValue,
    requirements::{
        DockerRequirement, EnvVarRequirement, InitialWorkDirRequirement,
        InlineJavascriptRequirement, InplaceUpdateRequirement, LoadListingRequirement,
        MultipleInputFeatureRequirement, NetworkAccess, ResourceRequirement,
        ScatterFeatureRequirement, StringOrInclude, ToolTimeLimit,
    },
};
use cwl_engine_storage::{RemoteTempDir, Storage, StorageBackend, StoragePath};
use futures_util::{
    FutureExt,
    future::{BoxFuture, join_all},
};
use indexmap::IndexMap;
use nonempty::NonEmpty;
use std::time::Duration;
use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
    process::ExitStatus,
    sync::Arc,
};
use tempfile::tempdir;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tracing::{Instrument, debug, error, info};
use uuid::Uuid;

pub mod docker;
pub mod local;
pub mod mount;
#[cfg(feature = "tes")]
pub mod tes;

#[async_trait]
pub trait TaskBackend: Send + Sync + 'static {
    async fn run(
        &self,
        request: &TaskExecutionRequest<'_>,
        token: CancellationToken,
    ) -> anyhow::Result<TaskExecutionResult>;

    fn task_scoped(&self) -> Arc<dyn TaskBackend>;
    fn storage(&self) -> Arc<StorageBackend>;

    /// provides the path where the inputs are stored in the container
    fn container_input_dir(&self) -> String;

    /// provides the path where the workdir (iwdr) is stored in the container
    fn container_work_dir(&self) -> String;

    /// provides the path where the tmpdir is located in the container
    fn container_tmp_dir(&self) -> String;

    /// where files are stored by the backend. e.g. /tmp for local backends or <s3://my-bucket> for remote
    fn data_store(&self) -> &StoragePath;
}

#[derive(Debug, Clone, Default)]
pub struct ExecutionResult {
    pub exit_status: NonEmpty<ExitStatus>,
    pub stdout: String,
    pub stderr: String,
    pub outputs: HashMap<String, DefaultValue>,
}

#[derive(Debug)]
pub struct TaskExecutionRequest<'a> {
    pub id: &'a str,
    pub description: Option<&'a str>,

    pub command: &'a [&'a str],
    pub inputs: &'a [FileOrDirectory],
    pub mounts: &'a [WorkDirMount],

    pub env: &'a IndexMap<String, String>,
    pub runtime: &'a Runtime,
    pub eval_context: &'a EvaluationContext<'a>,

    pub docker: Option<&'a DockerRequirement>,
    pub timelimit: Option<&'a ToolTimeLimit>,
    pub network: bool,
    pub use_container: bool,

    pub stdin_file: Option<&'a String>,
    pub stdout_file: Option<&'a String>,
    pub stderr_file: Option<&'a String>,

    pub outdir: &'a StoragePath,
    pub tmpdir: &'a StoragePath,

    pub specificationdir: &'a Path,

    // The container side mounted workdir
    pub execution_path: &'a str,
}

#[derive(Debug, Clone)]

pub struct TaskExecutionResult {
    pub exit_status: NonEmpty<ExitStatus>,
    pub stdout_file: StoragePath,
    pub stderr_file: StoragePath,
}

/// Runs `task` on `backend`, enforcing `timelimit`
pub(crate) async fn run_with_timelimit(
    backend: &dyn crankshaft::engine::service::runner::Backend,
    task: crankshaft::engine::Task,
    token: CancellationToken,
    timelimit: Option<&ToolTimeLimit>,
    eval_context: &EvaluationContext<'_>,
) -> anyhow::Result<NonEmpty<ExitStatus>> {
    let timeout = timelimit.and_then(|ttl| match &ttl.timelimit {
        IntegerOrExpression::Int(i) => Some(i64::from(*i)),
        IntegerOrExpression::Long(i) => Some(*i),
        IntegerOrExpression::Expression(e) => {
            let value = do_eval(e, eval_context).ok()?;
            value.as_i64()
        }
    });

    if let Some(timeout) = timeout
        && timeout != 0
    {
        let limit = Duration::from_secs(timeout.try_into()?); //this is intended to throw!
        let token_clone = token.clone();
        tokio::select! {
            result = backend.run(task, token)? => Ok(result?),
            () = tokio::time::sleep(limit) => {
                token_clone.cancel();
                error!("Timelimit reached: {timeout}");
                Err(TaskRunError::Canceled.into())
            }
        }
    } else {
        //no time constraint
        Ok(backend.run(task, token)?.await?)
    }
}

pub(crate) struct ResolvedDockerRequirement {
    pub image_id: String,
    pub dockerfile: Option<StringOrInclude>,
}

/// Shared by the Local/Docker/TES backends
pub(crate) fn resolve_docker_requirement(
    dr: Option<&DockerRequirement>,
    default_image: &str,
) -> ResolvedDockerRequirement {
    let default = || ResolvedDockerRequirement {
        image_id: default_image.to_string(),
        dockerfile: None,
    };
    let Some(dr) = dr else { return default() };

    if let Some(df) = &dr.docker_file {
        let image_id = dr
            .docker_image_id
            .clone()
            .unwrap_or_else(|| format!("cwl-build-{}", &Uuid::new_v4().to_string()[..8]));
        return ResolvedDockerRequirement {
            image_id,
            dockerfile: Some(df.clone()),
        };
    }
    if let Some(pull) = &dr.docker_pull {
        return ResolvedDockerRequirement {
            image_id: pull.clone(),
            dockerfile: None,
        };
    }
    if let Some(id) = &dr.docker_image_id {
        return ResolvedDockerRequirement {
            image_id: id.clone(),
            dockerfile: None,
        };
    }
    default()
}

///Executes a `CWLDocument`
/// # Panics
///  Panics if `Operation` is used as this anstract type is not meant to be executed
pub fn execute(
    backend: Arc<dyn TaskBackend>,
    request: &ExecutionRequest,
    token: CancellationToken,
) -> BoxFuture<'_, anyhow::Result<ExecutionResult>> {
    match &request.specification {
        CWLDocument::Workflow(_) => execute_workflow(backend, request, token).boxed(),
        CWLDocument::CommandLineTool(_) | CWLDocument::ExpressionTool(_) => {
            execute_commandline_tool(backend, request, token).boxed()
        }
        CWLDocument::Operation(_) => panic!("Unsupported document type for execution"),
    }
}

///Executes a CWL `Worfklow` object
/// # Panics
/// Panics if document is not a `Workflow`
/// # Errors
/// Errors in multiple occasions. Tells you basically something is wrong with the CWL
#[allow(clippy::too_many_lines)]
pub async fn execute_workflow(
    backend: Arc<dyn TaskBackend>,
    request: &ExecutionRequest,
    token: CancellationToken,
) -> anyhow::Result<ExecutionResult> {
    //create validator
    let fv = get_format_validator(&request.specification, &request.working_dir)?;
    let cwl_version = cwl_version(&request.specification)?;

    let CWLDocument::Workflow(wf) = &request.specification else {
        panic!("Not a Workflow");
    };

    let llr = request.get_requirement_or_hint::<LoadListingRequirement>();
    let ijsr = request.get_requirement_or_hint::<InlineJavascriptRequirement>();
    let sfr = request.get_requirement_or_hint::<ScatterFeatureRequirement>();
    //we don't want to inherit those requirements from parent here!
    let mir = wf.get_requirement_or_hint::<MultipleInputFeatureRequirement>();

    let mut inputs = collect_inputs(
        &request.specification,
        &request.inputs,
        &request.working_dir,
        Path::new(""),
        llr,
        Some(&fv),
    )?;

    let collection_dir = tempdir()?;

    let eval_context = &mut EvaluationContext {
        workdir: Some(&request.working_dir),
        ijsr,
        inputs: Some(&inputs.clone()),
        ..Default::default()
    };

    //collect secondary files using evalcontrext and reassign inputs
    collect_secondary_files_for_inputs(
        &request.specification,
        &mut inputs,
        eval_context,
        &request.working_dir,
    )?;

    let eval_context = &mut EvaluationContext {
        workdir: Some(&request.working_dir),
        ijsr,
        inputs: Some(&inputs.clone()),
        ..Default::default()
    };

    let waves = build_execution_tree(wf)?;
    let mut completed_outputs: HashMap<String, DefaultValue> = HashMap::new();

    //insert inputs into completed outputs
    for (k, v) in &inputs {
        completed_outputs.insert(k.clone(), v.clone());
    }

    let has_scatter_steps = wf.steps.iter().any(|i| i.scatter.is_some());

    let mut scatter_meta: HashMap<String, (ScatterMethod, Vec<usize>)> = HashMap::new();
    for wave in waves {
        let mut handles = Vec::new();

        let scattered_step_ids: HashSet<String> = wave
            .iter()
            .filter(|step| step.scatter.is_some() && sfr.is_some())
            .map(|step| step.id.clone().unwrap())
            .collect::<HashSet<_>>();

        // Accumulate scatter outputs: step_id/output_name -> Vec<Value>
        let mut scatter_accum: HashMap<String, Vec<DefaultValue>> = HashMap::new();

        for step in wave {
            let step_id_clone = step.id.clone().unwrap();
            let token_clone = token.clone();
            let inputs = collect_raw_inputs(step, &completed_outputs, mir)?;
            let working_dir_clone = request.working_dir.clone();
            let step_outdir = collection_dir.path().join(&step_id_clone);
            info!("Starting execution of step {}", step_id_clone);

            //decide if we need to scatter this step
            if let Some(scatter) = &step.scatter
                && sfr.is_some()
            {
                info!("{step_id_clone} is a scattered Job");
                let scatter_keys = scatter.as_many();
                let method = step
                    .scatter_method
                    .as_ref()
                    .unwrap_or(&ScatterMethod::Dotproduct);

                let scatter_inputs = scatter::gather_inputs(&scatter_keys, &inputs)?;
                let dims: Vec<usize> = scatter_inputs.iter().map(Vec::len).collect();
                let jobs = scatter::gather_jobs(&scatter_inputs, &scatter_keys, *method)?;

                //no jobs -> we add empty output arrays
                if jobs.is_empty() {
                    for output in &step.out {
                        let key = format!("{}/{}", step_id_clone, output.id());
                        let empty_result = scatter::empty(*method, &dims);
                        completed_outputs.insert(key, DefaultValue::Any(empty_result));
                    }
                    continue;
                }

                for (job_index, job) in jobs.into_iter().enumerate() {
                    let mut sub_inputs = inputs.clone();
                    for (k, v) in job {
                        //convert to default value (inner values we extracted before as vec!)
                        sub_inputs.inputs.insert(k, serde_json::from_value(v)?);
                    }

                    let step_inputs = eval_inputs(step, sub_inputs, eval_context)?;
                    let eval_context = eval_context.clone().with_inputs(&step_inputs.inputs);
                    if let Some(when) = &step.when {
                        if cwl_version < V1_2_0 {
                            anyhow::bail!(
                                "Conditional execution with when is not supported for CWL version {cwl_version}",
                            );
                        }
                        let serde_json::Value::Bool(result) = do_eval(when, &eval_context)? else {
                            anyhow::bail!("Condition {when} did not evaluate to boolean");
                        };
                        if !result {
                            let null_outputs = step
                                .out
                                .iter()
                                .map(|o| {
                                    (o.id().clone(), DefaultValue::Any(serde_json::Value::Null))
                                })
                                .collect::<HashMap<_, _>>();
                            //spawn the "Null-Job"
                            let step_id = step_id_clone.clone();
                            debug!(
                                "Scatter step skipped because when evaluated to false in {step_id_clone}"
                            );
                            let span = tracing::Span::current();
                            handles.push(tokio::spawn(
                                async move {
                                    Ok((
                                        step_id,
                                        ExecutionResult {
                                            outputs: null_outputs,
                                            ..Default::default()
                                        },
                                    ))
                                }
                                .instrument(span),
                            ));
                            continue;
                        }
                    }

                    let backend_clone = backend.task_scoped();
                    let job_outdir = collection_dir
                        .path()
                        .join(format!("{step_id_clone}_{job_index}"));

                    handles.push(execute_step(
                        step,
                        backend_clone.clone(),
                        &working_dir_clone,
                        Some(&job_outdir),
                        step_inputs,
                        token_clone.clone(),
                        request,
                    )?);
                }
                scatter_meta.insert(step_id_clone.clone(), (*method, dims));
            } else {
                let step_inputs = eval_inputs(step, inputs, eval_context)?;
                let eval_context = eval_context.clone().with_inputs(&step_inputs.inputs);
                if let Some(when) = &step.when {
                    if cwl_version < V1_2_0 {
                        anyhow::bail!(
                            "Conditional execution with when is not supported for CWL version {cwl_version}",
                        );
                    }
                    let serde_json::Value::Bool(result) = do_eval(when, &eval_context)? else {
                        anyhow::bail!("Condition {when} did not evaluate to boolean");
                    };
                    if !result {
                        debug!("Step skipped because when evaluated to false in {step_id_clone}",);
                        let null_outputs = step
                            .out
                            .iter()
                            .map(|o| (o.id().clone(), DefaultValue::Any(serde_json::Value::Null)))
                            .collect::<HashMap<_, _>>();
                        //spawn the "Null-Job"
                        let step_id = step_id_clone.clone();
                        debug!("Step skipped because when evaluated to false in {step_id_clone}");

                        let span = tracing::Span::current();
                        handles.push(tokio::spawn(
                            async move {
                                Ok((
                                    step_id,
                                    ExecutionResult {
                                        outputs: null_outputs,
                                        ..Default::default()
                                    },
                                ))
                            }
                            .instrument(span),
                        ));
                        continue;
                    }
                }

                let backend_clone = backend.task_scoped();

                handles.push(execute_step(
                    step,
                    backend_clone,
                    &working_dir_clone,
                    Some(&step_outdir),
                    step_inputs,
                    token_clone,
                    request,
                )?);
            }
        }

        let wave_results = join_all(handles).await;

        for join_result in wave_results {
            let (step_id, exec_result) = join_result
                .context("Step task panicked")?
                .context("Step execution failed")?;
            for (output_name, value) in exec_result.outputs {
                let key = format!("{step_id}/{output_name}");
                if scattered_step_ids.contains(&step_id) && sfr.is_some() {
                    scatter_accum.entry(key).or_default().push(value);
                } else {
                    completed_outputs.insert(key, value);
                }
            }
        }

        if sfr.is_some() && has_scatter_steps {
            // For scattered steps, we need to aggregate outputs into arrays
            for (key, values) in scatter_accum {
                let step_id = key.split('/').next().unwrap().to_string();
                let aggregated = if let Some((method, dims)) = scatter_meta.get(&step_id) {
                    match method {
                        ScatterMethod::NestedCrossproduct => {
                            let raw: Vec<serde_json::Value> = values
                                .into_iter()
                                .map(serde_json::to_value)
                                .collect::<Result<_, _>>()?;
                            scatter::nest_results(&raw, dims)
                        }
                        // flat and dotproduct both produce a flat array
                        _ => serde_json::to_value(values)?,
                    }
                } else {
                    serde_json::to_value(values)?
                };

                completed_outputs.insert(key, DefaultValue::Any(aggregated));
            }
        }
    }
    let cc = OutputCollectionContext {
        source_dir: &StoragePath::Local(collection_dir.path().to_path_buf()), //TODO
        dest_dir: &request.out_dir,
        workdir: Path::new(""), //not used
        eval_context,
        validator: &fv,
    };
    let outputs =
        collect_workflow_outputs(&wf.outputs, &completed_outputs, &cc, mir, backend.storage())
            .await?;

    Ok(ExecutionResult {
        exit_status: NonEmpty::new(ExitStatus::default()),
        stdout: String::new(),
        stderr: String::new(),
        outputs,
    })
}

fn execute_step(
    step: &WorkflowStep,
    backend: Arc<dyn TaskBackend>,
    working_dir: &Path,
    outdir: Option<&Path>,
    inputs: InputObject,
    token: CancellationToken,
    request: &ExecutionRequest,
) -> anyhow::Result<JoinHandle<anyhow::Result<(String, ExecutionResult)>>> {
    let step_id_clone = step.id.clone().unwrap();
    match &step.run {
        StringOrDocument::String(s) => {
            let specification_path = working_dir.join(s);

            let request = create_execution_request_with_inputs(
                specification_path,
                inputs,
                outdir,
                Some(request),
            )?;

            let span = tracing::Span::current();
            let handle: tokio::task::JoinHandle<anyhow::Result<(String, ExecutionResult)>> =
                tokio::spawn(
                    async move {
                        let result = execute(backend, &request, token).await?;
                        Ok((step_id_clone, result))
                    }
                    .instrument(span),
                );
            Ok(handle)
        }
        StringOrDocument::Document(cwldocument) => {
            let request = create_execution_request_from_document(
                *cwldocument.clone(),
                inputs,
                working_dir,
                outdir,
                Some(request),
            )?;

            let span = tracing::Span::current();
            let handle: tokio::task::JoinHandle<anyhow::Result<(String, ExecutionResult)>> =
                tokio::spawn(
                    async move {
                        let result = execute(backend, &request, token).await?;
                        Ok((step_id_clone, result))
                    }
                    .instrument(span),
                );
            Ok(handle)
        }
    }
}

/// Executes a CWL `CommandLineTool`
/// # Errors
/// Can return error in multiple occasions.
#[allow(clippy::too_many_lines)]
pub async fn execute_commandline_tool(
    backend: Arc<dyn TaskBackend>,
    request: &ExecutionRequest,
    token: CancellationToken,
) -> anyhow::Result<ExecutionResult> {
    //create validator
    let fv = get_format_validator(&request.specification, &request.working_dir)?;
    let cwl_version = cwl_version(&request.specification)?;

    //get neccessary requirements
    let ijsr = request.get_requirement_or_hint::<InlineJavascriptRequirement>();
    let dr = request.get_requirement_or_hint::<DockerRequirement>();
    let rr = request.get_requirement_or_hint::<ResourceRequirement>();
    let iwdr = request.get_requirement_or_hint::<InitialWorkDirRequirement>();
    let evr = request.get_requirement_or_hint::<EnvVarRequirement>();
    let ttl = request.get_requirement_or_hint::<ToolTimeLimit>();
    let llr = request.get_requirement_or_hint::<LoadListingRequirement>();
    let iur = request.get_requirement_or_hint::<InplaceUpdateRequirement>();
    let na = request.get_requirement_or_hint::<NetworkAccess>();

    let outdir = RemoteTempDir::new_under(backend.data_store(), backend.storage()).await?;
    let tmpdir = RemoteTempDir::new_under(backend.data_store(), backend.storage()).await?;

    let default_input_dir = backend.container_input_dir();
    let stage_dir = match &request.specification {
        CWLDocument::CommandLineTool(_) => Path::new(&default_input_dir),
        CWLDocument::ExpressionTool(_) => Path::new("."), //outdir.path()
        _ => unreachable!(),
    };

    let mut inputs = collect_inputs(
        &request.specification,
        &request.inputs,
        &request.working_dir,
        stage_dir,
        llr,
        Some(&fv),
    )?;
    let eval_context = &mut EvaluationContext {
        workdir: Some(&request.working_dir),
        ijsr,
        inputs: Some(&inputs.clone()),
        ..Default::default()
    };

    //handle docker output dir
    let workdir = if let Some(dr) = dr
        && let Some(dr_outdir) = &dr.docker_output_directory
    {
        dr_outdir
    } else {
        &backend.container_work_dir()
    };

    //create runtime struct
    let mut runtime = build_runtime(rr, eval_context, &cwl_version)?;
    runtime.outdir = PathBuf::from(workdir);
    runtime.tmpdir = PathBuf::from(backend.container_tmp_dir());

    eval_context.runtime = Some(&runtime);

    //collect secondary files using evalcontrext and reassign inputs
    collect_secondary_files_for_inputs(
        &request.specification,
        &mut inputs,
        eval_context,
        &request.working_dir,
    )?;

    let eval_context = &mut EvaluationContext {
        inputs: Some(&inputs.clone()),
        workdir: Some(&request.working_dir),
        ijsr,
        runtime: Some(&runtime),
        ..Default::default()
    };

    //evalute environment expressions
    let mut environment = handle_environment(request.environment.clone(), evr, eval_context)?;
    environment.insert("HOME".to_string(), runtime.outdir.to_string_lossy().into());
    environment.insert(
        "TMPDIR".to_string(),
        runtime.tmpdir.to_string_lossy().into(),
    );

    // handle iwdr copy/link to outdir
    let mounts = if let Some(iwdr) = iwdr {
        stage_work_dir(
            iwdr,
            &request.working_dir,
            outdir.storage_path(),
            eval_context,
            workdir,
            &mut inputs,
        )?
    } else {
        vec![]
    };

    //needs to be constructed after we created the eval context
    let mut flattened_inputs = flatten_inputs(&inputs);
    flattened_inputs = remove_materialized_inputs(flattened_inputs, &mounts, workdir);

    let eval_context = &mut EvaluationContext {
        inputs: Some(&inputs),
        workdir: Some(&request.working_dir),
        ijsr,
        runtime: Some(&runtime),
        ..Default::default()
    };
    //execute commandline tool
    if let CWLDocument::CommandLineTool(tool) = &request.specification {
        //collect command string and correct args for staged paths
        let mut args = command::build_command(tool, &inputs, &runtime)?;

        //correct and add the stdin value
        let mut stdin = get_stdin(tool, &inputs);
        if let Some(stdin) = &mut stdin {
            //evaluate expression
            *stdin = if let Ok(value) = do_eval(stdin, eval_context) {
                match value {
                    serde_json::Value::String(s) => s,
                    other => serde_saphyr::to_string(&other)?.trim().to_owned(),
                }
            } else {
                stdin.clone()
            };

            args.push(stdin.clone());
        }

        info!("Executing: {}", args.join(" "));

        let network_access = if let Some(na) = na {
            match &na.network_access {
                BoolOrExpression::Bool(b) => *b,
                BoolOrExpression::Expression(ex) => {
                    let val = do_eval(ex, eval_context)?;
                    if let serde_json::Value::Bool(b) = val {
                        b
                    } else {
                        false
                    }
                }
            }
        } else {
            false
        };

        let doc = tool.doc.as_ref().map(|d| docstring(d.clone()));
        let id = tool.id.clone().unwrap_or("Unnamed".to_owned())
            + "_"
            + &Uuid::new_v4().to_string()[..8];
        let result = backend
            .run(
                &TaskExecutionRequest {
                    id: &id,
                    description: doc.as_deref(),

                    command: args
                        .iter()
                        .map(String::as_str)
                        .collect::<Vec<&str>>()
                        .as_slice(),
                    inputs: &flattened_inputs,
                    mounts: &mounts,

                    env: &environment,
                    runtime: &runtime,
                    eval_context,

                    docker: dr,
                    timelimit: ttl,
                    network: network_access,
                    use_container: tool.has_requirement::<DockerRequirement>(), //hints no sufficient

                    stdin_file: stdin.as_ref(),
                    stdout_file: tool.stdout.as_ref(),
                    stderr_file: tool.stderr.as_ref(),

                    outdir: outdir.storage_path(),
                    tmpdir: tmpdir.storage_path(),
                    specificationdir: &request.working_dir,
                    execution_path: workdir,
                },
                token,
            )
            .await?;

        let first_code = result.exit_status.first().code().unwrap_or(1);

        //update runtime
        let mut runtime = runtime.clone();
        runtime.exit_code = Some(first_code);
        runtime.outdir = outdir
            .storage_path()
            .as_url()?
            .to_file_path()
            .map_err(|()| anyhow::anyhow!("Could not create file path from URL for outdir"))?; //is this smart?

        let eval_context = eval_context.clone().with_runtime(&runtime);

        //evaluate stderr/stdout
        let stdout = backend
            .storage()
            .read_file(&result.stdout_file.as_url()?)
            .await
            .with_context(|| format!("Could not find stdout file {:?}", result.stdout_file))?;
        if !stdout.is_empty() {
            eprintln!("{stdout}");
        }
        let stderr = backend
            .storage()
            .read_file(&result.stderr_file.as_url()?)
            .await
            .with_context(|| format!("Could not find stderr file {:?}", result.stderr_file))?;
        if !stderr.is_empty() {
            eprintln!("{stderr}");
        }

        // need to collect outputs
        if !&request.out_dir.exists() {
            fs::create_dir_all(&request.out_dir).with_context(|| {
                format!("Could not create dir {}", request.out_dir.to_string_lossy())
            })?;
        }
        let outputs = collect_command_outputs(
            &tool.outputs,
            &result.stdout_file,
            &result.stderr_file,
            &OutputCollectionContext {
                source_dir: outdir.storage_path(),
                dest_dir: &request.out_dir,
                workdir: Path::new(workdir),
                eval_context: &eval_context,
                validator: &fv,
            },
            backend.storage(),
        )
        .await?;

        if iur.is_some_and(|i| i.inplace_update) {
            handle_inplace_update(mounts, backend.storage()).await?;
        }

        Ok(ExecutionResult {
            exit_status: result.exit_status,
            stdout,
            stderr,
            outputs,
        })
    } else if let CWLDocument::ExpressionTool(tool) = &request.specification {
        let expression = &tool.expression;

        info!("Executing: {expression}");

        //definitivly use js engine
        if eval_context.ijsr.is_none() {
            eval_context.ijsr = Some(&InlineJavascriptRequirement {
                expression_lib: None,
            });
        }

        let result = do_eval(expression, eval_context)?;
        let outputs = collect_expression_outputs(
            &tool.outputs,
            &result,
            &OutputCollectionContext {
                source_dir: outdir.storage_path(),
                dest_dir: &request.out_dir,
                workdir: Path::new(workdir),
                eval_context,
                validator: &fv,
            },
            backend.storage(),
        )
        .await?;

        Ok(ExecutionResult {
            exit_status: NonEmpty::new(ExitStatus::default()),
            stdout: String::new(),
            stderr: String::new(),
            outputs,
        })
    } else {
        anyhow::bail!("Unsupported document type for execution")
    }
}

pub enum EngineStatus {
    Success(i32),
    Failure(i32),
    Undefined(i32),
}
#[must_use]
pub fn evaluate_exitcodes(exit_codes: &NonEmpty<ExitStatus>, doc: &CWLDocument) -> EngineStatus {
    //currently we only look at first code
    let actual_code = exit_codes.first();
    let code = actual_code.code().unwrap_or(1);
    if let CWLDocument::CommandLineTool(tool) = doc {
        let success_codes = tool.success_codes.clone().unwrap_or(vec![0]);
        let failure_codes = tool.permanent_fail_codes.clone().unwrap_or(vec![1]);
        if success_codes.contains(&code) {
            EngineStatus::Success(code)
        } else if failure_codes.contains(&code) {
            EngineStatus::Failure(code)
        } else {
            EngineStatus::Undefined(code)
        }
    } else if code != 0 {
        EngineStatus::Failure(code)
    } else {
        EngineStatus::Success(code)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_docker_requirement_none_uses_default() {
        let resolved = resolve_docker_requirement(None, "default:tag");
        assert_eq!(resolved.image_id, "default:tag");
        assert!(resolved.dockerfile.is_none());
    }

    #[test]
    fn test_resolve_docker_requirement_empty_uses_default() {
        let dr = DockerRequirement::default();
        let resolved = resolve_docker_requirement(Some(&dr), "default:tag");
        assert_eq!(resolved.image_id, "default:tag");
        assert!(resolved.dockerfile.is_none());
    }

    #[test]
    fn test_resolve_docker_requirement_pull() {
        let dr = DockerRequirement::builder().docker_pull("my/image:1.0").build();
        let resolved = resolve_docker_requirement(Some(&dr), "default:tag");
        assert_eq!(resolved.image_id, "my/image:1.0");
        assert!(resolved.dockerfile.is_none());
    }

    #[test]
    fn test_resolve_docker_requirement_image_id_only() {
        let dr = DockerRequirement::builder().docker_image_id("my-built-image").build();
        let resolved = resolve_docker_requirement(Some(&dr), "default:tag");
        assert_eq!(resolved.image_id, "my-built-image");
        assert!(resolved.dockerfile.is_none());
    }

    #[test]
    fn test_resolve_docker_requirement_file_with_image_id() {
        let dr = DockerRequirement::builder()
            .docker_file(StringOrInclude::String("FROM ubuntu".to_string()))
            .docker_image_id("tagged-build")
            .build();
        let resolved = resolve_docker_requirement(Some(&dr), "default:tag");
        assert_eq!(resolved.image_id, "tagged-build");
        assert!(resolved.dockerfile.is_some());
    }

    #[test]
    fn test_resolve_docker_requirement_file_only_generates_tag() {
        let dr = DockerRequirement::builder()
            .docker_file(StringOrInclude::String("FROM ubuntu".to_string()))
            .build();
        let resolved = resolve_docker_requirement(Some(&dr), "default:tag");
        assert!(resolved.image_id.starts_with("cwl-build-"));
        assert!(resolved.dockerfile.is_some());
    }
}
