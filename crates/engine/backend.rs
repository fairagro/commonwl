use crate::{
    command,
    environment::{
        env::handle_environment,
        runtime::{Runtime, build_runtime},
        workdir::{WorkDirMount, stage_work_dir},
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
    workflow::build_step_input_object,
};
use anyhow::Context;
use cwl_core::{
    docstring,
    documents::{CWLDocument, ScatterMethod, StringOrDocument, WorkflowStep},
    files::FileOrDirectory,
    inputs::DefaultValue,
    requirements::{
        DockerRequirement, EnvVarRequirement, InitialWorkDirRequirement,
        InlineJavascriptRequirement, LoadListingRequirement, MultipleInputFeatureRequirement,
        ResourceRequirement, ScatterFeatureRequirement, ToolTimeLimit,
    },
};
use futures_util::{
    FutureExt,
    future::{BoxFuture, join_all},
};
use indexmap::IndexMap;
use nonempty::NonEmpty;
use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
    process::ExitStatus,
};
use tempfile::tempdir;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tracing::info;

pub mod docker;
pub mod mount;

pub trait TaskBackend {
    const INPUT_DIR: &str;
    const WORK_DIR: &str;
    const TMP_DIR: &str;

    fn run<'a>(
        &self,
        request: &'a TaskExecutionRequest<'a>,
        token: CancellationToken,
    ) -> impl Future<Output = anyhow::Result<TaskExecutionResult>> + Send;
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
    pub use_container: bool,

    pub stdin_file: Option<&'a String>,
    pub stdout_file: Option<&'a String>,
    pub stderr_file: Option<&'a String>,

    pub outdir: &'a Path,
    pub tmpdir: &'a Path,
    pub working_dir: &'a Path,
    pub staged_dir: &'a str,
}

#[derive(Debug, Clone)]

pub struct TaskExecutionResult {
    pub exit_status: NonEmpty<ExitStatus>,
    pub stdout_file: PathBuf,
    pub stderr_file: PathBuf,
}

pub fn execute<T: TaskBackend + Clone + Send + 'static>(
    backend: T,
    request: &ExecutionRequest,
    token: CancellationToken,
) -> BoxFuture<'_, anyhow::Result<ExecutionResult>> {
    match &request.specification {
        CWLDocument::Workflow(_) => execute_workflow(backend, request, token).boxed(),
        CWLDocument::CommandLineTool(_) | CWLDocument::ExpressionTool(_) => {
            execute_commandline_tool(backend, request, token).boxed()
        }
        _ => panic!("Unsupported document type for execution"),
    }
}

pub async fn execute_workflow<T: TaskBackend + Clone + Send + 'static>(
    backend: T,
    request: &ExecutionRequest,
    token: CancellationToken,
) -> anyhow::Result<ExecutionResult> {
    //create validator
    let fv = get_format_validator(&request.specification, &request.working_dir)?;

    let CWLDocument::Workflow(wf) = &request.specification else {
        panic!("Not a Workflow");
    };

    let llr = request.get_requirement_or_hint::<LoadListingRequirement>();
    let sfr = request.get_requirement_or_hint::<ScatterFeatureRequirement>();
    let ijsr = request.get_requirement_or_hint::<InlineJavascriptRequirement>();
    let mir = wf.get_requirement_or_hint::<MultipleInputFeatureRequirement>();

    let inputs = collect_inputs(
        &request.specification,
        &request.inputs,
        &request.working_dir,
        &request.working_dir, //?
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

    let waves = build_execution_tree(wf)?;
    let mut completed_outputs: HashMap<String, DefaultValue> = HashMap::new();

    //insert inputs into completed outputs
    for (k, v) in &inputs {
        completed_outputs.insert(k.clone(), v.clone());
    }

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
            let backend_clone = backend.clone();
            let token_clone = token.clone();
            let inputs = build_step_input_object(step, &completed_outputs, mir, eval_context)?;
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
                let dims: Vec<usize> = scatter_inputs.iter().map(|v| v.len()).collect();
                let jobs = scatter::gather_jobs(&scatter_inputs, &scatter_keys, method)?;

                //no jobs -> we add empty output arrays
                if jobs.is_empty() {
                    for output in &step.out {
                        let key = format!("{}/{}", step_id_clone, output.id());
                        let empty_result = scatter::empty(method, &dims);
                        completed_outputs.insert(key, DefaultValue::Any(empty_result));
                    }
                    continue;
                }

                for job in jobs {
                    let mut sub_inputs = inputs.clone();
                    for (k, v) in job {
                        sub_inputs.inputs.insert(k, v);
                    }

                    handles.push(execute_step(
                        step,
                        backend_clone.clone(),
                        &working_dir_clone,
                        Some(&step_outdir),
                        sub_inputs,
                        token_clone.clone(),
                        request,
                    )?);
                }
                scatter_meta.insert(step_id_clone.clone(), (*method, dims));
            } else {
                handles.push(execute_step(
                    step,
                    backend_clone,
                    &working_dir_clone,
                    Some(&step_outdir),
                    inputs,
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
                let key = format!("{}/{}", step_id, output_name);
                if scattered_step_ids.contains(&step_id) && sfr.is_some() {
                    scatter_accum.entry(key).or_default().push(value);
                } else if sfr.is_some() {
                    //if no value was produces we add an empty vec
                    scatter_accum
                        .entry(key)
                        .or_default()
                        .push(DefaultValue::Any(serde_yaml::Value::Sequence(vec![])));
                } else {
                    completed_outputs.insert(key, value);
                }
            }
        }

        if sfr.is_some() {
            // For scattered steps, we need to aggregate outputs into arrays
            for (key, values) in scatter_accum {
                let step_id = key.split('/').next().unwrap().to_string();
                let aggregated = if let Some((method, dims)) = scatter_meta.get(&step_id) {
                    match method {
                        ScatterMethod::NestedCrossproduct => {
                            let raw: Vec<serde_yaml::Value> = values
                                .into_iter()
                                .map(serde_yaml::to_value)
                                .collect::<Result<_, _>>()?;
                            scatter::nest_results(raw, dims)
                        }
                        // flat and dotproduct both produce a flat array
                        _ => serde_yaml::to_value(values)?,
                    }
                } else {
                    serde_yaml::to_value(values)?
                };

                completed_outputs.insert(key, DefaultValue::Any(aggregated));
            }
        }
    }
    let cc = OutputCollectionContext {
        source_dir: collection_dir.path(),
        tmp_dir: Path::new(""), //not used
        dest_dir: &request.out_dir,
        workdir: Path::new(""), //not used
        eval_context,
        validator: &fv,
    };
    let outputs = collect_workflow_outputs(&wf.outputs, completed_outputs, &cc)?;

    Ok(ExecutionResult {
        exit_status: NonEmpty::new(ExitStatus::default()),
        stdout: String::new(),
        stderr: String::new(),
        outputs,
    })
}

fn execute_step<T: TaskBackend + Clone + Send + 'static>(
    step: &WorkflowStep,
    backend: T,
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
            let handle: tokio::task::JoinHandle<anyhow::Result<(String, ExecutionResult)>> =
                tokio::spawn(async move {
                    let result = execute(backend, &request, token).await?;
                    Ok((step_id_clone, result))
                });
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
            let handle: tokio::task::JoinHandle<anyhow::Result<(String, ExecutionResult)>> =
                tokio::spawn(async move {
                    let result = execute(backend, &request, token).await?;
                    Ok((step_id_clone, result))
                });
            Ok(handle)
        }
    }
}

pub async fn execute_commandline_tool<T: TaskBackend + Clone + Send + 'static>(
    backend: T,
    request: &ExecutionRequest,
    token: CancellationToken,
) -> anyhow::Result<ExecutionResult> {
    //create validator
    let fv = get_format_validator(&request.specification, &request.working_dir)?;

    //get neccessary requirements
    let ijsr = request.get_requirement_or_hint::<InlineJavascriptRequirement>();
    let dr = request.get_requirement_or_hint::<DockerRequirement>();
    let rr = request.get_requirement_or_hint::<ResourceRequirement>();
    let iwdr = request.get_requirement_or_hint::<InitialWorkDirRequirement>();
    let evr = request.get_requirement_or_hint::<EnvVarRequirement>();
    let ttl = request.get_requirement_or_hint::<ToolTimeLimit>();
    let llr = request.get_requirement_or_hint::<LoadListingRequirement>();

    let outdir = tempdir()?;
    let tmpdir = tempdir()?;

    let stage_dir = match &request.specification {
        CWLDocument::CommandLineTool(_) => Path::new(T::INPUT_DIR),
        CWLDocument::ExpressionTool(_) => outdir.path(),
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
        T::WORK_DIR
    };

    //create runtime struct
    let mut runtime = build_runtime(rr, eval_context);
    runtime.outdir = PathBuf::from(workdir);
    runtime.tmpdir = PathBuf::from(T::TMP_DIR);

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

    //needs to be constructed after we created the eval context
    let flattened_inputs = flatten_inputs(&inputs)?;

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
            outdir.path(),
            eval_context,
            workdir,
            &mut inputs,
        )?
    } else {
        vec![]
    };

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
                serde_yaml::to_string(&value)?.trim().to_owned()
            } else {
                stdin.to_string()
            };

            args.push(stdin.to_string());
        }

        info!("Executing: {}", args.join(" "));

        let doc = tool.doc.as_ref().map(|d| docstring(d.clone()));
        let result = backend
            .run(
                &TaskExecutionRequest {
                    id: &tool.id.clone().unwrap_or("Unnamed".to_owned()),
                    description: doc.as_deref(),

                    command: args
                        .iter()
                        .map(|s| s.as_str())
                        .collect::<Vec<&str>>()
                        .as_slice(),
                    inputs: &flattened_inputs,
                    mounts: &mounts,

                    env: &environment,
                    runtime: &runtime,
                    eval_context,

                    docker: dr,
                    timelimit: ttl,
                    use_container: tool.has_requirement::<DockerRequirement>(), //hints no sufficient

                    stdin_file: stdin.as_ref(),
                    stdout_file: tool.stdout.as_ref(),
                    stderr_file: tool.stderr.as_ref(),

                    outdir: outdir.path(),
                    tmpdir: tmpdir.path(),
                    working_dir: &request.working_dir,
                    staged_dir: workdir,
                },
                token,
            )
            .await?;

        let first_code = result.exit_status.first().code().unwrap_or(1);

        //update runtime
        let mut runtime = runtime.clone();
        runtime.exit_code = Some(first_code);
        runtime.outdir = outdir.path().to_path_buf();

        let eval_context = eval_context.clone().with_runtime(&runtime);

        //evaluate stderr/stdout
        let stdout = fs::read_to_string(&result.stdout_file)?;
        if !stdout.is_empty() {
            eprintln!("{stdout}");
        }
        let stderr = fs::read_to_string(&result.stderr_file)?;
        if !stderr.is_empty() {
            eprintln!("{stderr}");
        }

        // need to collect outputs
        if !&request.out_dir.exists() {
            fs::create_dir_all(&request.out_dir)?;
        }

        let outputs = collect_command_outputs(
            &tool.outputs,
            &result.stdout_file,
            &result.stderr_file,
            &OutputCollectionContext {
                source_dir: outdir.path(),
                dest_dir: &request.out_dir,
                tmp_dir: tmpdir.path(),
                workdir: Path::new(workdir),
                eval_context: &eval_context,
                validator: &fv,
            },
        )?;
        //evaluate exitstatus based on tool's expected exit codes

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
                source_dir: outdir.path(),
                dest_dir: &request.out_dir,
                tmp_dir: tmpdir.path(),
                workdir: Path::new(workdir),
                eval_context,
                validator: &fv,
            },
        )?;

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

pub fn evaluate_exitcodes(exit_codes: NonEmpty<ExitStatus>, doc: &CWLDocument) -> EngineStatus {
    //currently we only look at first code
    let actual_code = exit_codes.first();
    let code = actual_code.code().unwrap();
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
