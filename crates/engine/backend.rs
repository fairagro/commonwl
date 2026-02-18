use crate::{
    command,
    context::{Runtime, build_runtime},
    environment::{build_environment, handle_environment},
    expression::{EvaluationContext, do_eval},
    format::get_format_validator,
    input::{
        InputObject, collect_inputs, file_system::create_flattened_inputs, get_stdin,
        load_input_file_from_file,
    },
    io::file::collect_secondary_files_for_inputs,
    output::{OutputCollectionContext, collect_command_outputs},
    requirements::{ProcessHints, ProcessRequirements, collect_hints, collect_requirements},
    schema::replace_schema_definitions,
    workdir::{WorkDirMount, stage_work_dir},
};
use cwl_core::{
    docstring,
    documents::CWLDocument,
    files::FileOrDirectory,
    inputs::DefaultValue,
    load_cwl_file,
    requirements::{
        DockerRequirement, EnvVarRequirement, InitialWorkDirRequirement,
        InlineJavascriptRequirement, LoadListingRequirement, ResourceRequirement, ToolTimeLimit,
    },
};
use indexmap::IndexMap;
use nonempty::NonEmpty;
use std::{
    collections::HashMap,
    env, fs,
    path::{Path, PathBuf},
    process::ExitStatus,
};
use tempfile::tempdir;
use tokio_util::sync::CancellationToken;
use tracing::info;

pub mod docker;

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
#[derive(Debug, Clone)]

pub struct ExecutionResult {
    pub exit_status: NonEmpty<ExitStatus>,
    pub stdout: String,
    pub stderr: String,
    pub outputs: HashMap<String, DefaultValue>,
}

#[derive(Debug, Clone)]
pub struct ExecutionRequest {
    pub specification: CWLDocument,
    pub inputs: HashMap<String, serde_yaml::Value>,
    pub working_dir: PathBuf,
    pub out_dir: PathBuf,
    pub requirements: Vec<ProcessRequirements>,
    pub hints: Vec<ProcessHints>,
    pub environment: IndexMap<String, String>,
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

pub async fn execute<T: TaskBackend>(
    backend: T,
    request: &ExecutionRequest,
    token: CancellationToken,
) -> anyhow::Result<ExecutionResult> {
    //create validator
    let fv = get_format_validator(&request.specification, &request.working_dir)?;

    let CWLDocument::CommandLineTool(tool) = &request.specification else {
        panic!("Currently only CommandLineTool is supported in Docker backend");
    };

    //get neccessary requirements
    let ijsr = tool.get_requirement_or_hint::<InlineJavascriptRequirement>();
    let dr = tool.get_requirement_or_hint::<DockerRequirement>();
    let rr = tool.get_requirement_or_hint::<ResourceRequirement>();
    let iwdr = tool.get_requirement_or_hint::<InitialWorkDirRequirement>();
    let evr = tool.get_requirement_or_hint::<EnvVarRequirement>();
    let ttl = tool.get_requirement_or_hint::<ToolTimeLimit>();
    let llr = tool.get_requirement_or_hint::<LoadListingRequirement>();

    let stage_dir = Path::new(T::INPUT_DIR);

    let mut staged_inputs = collect_inputs(
        &request.specification,
        &request.inputs,
        &request.working_dir,
        stage_dir,
        llr,
        Some(&fv),
    )?;

    let outdir = tempdir()?;
    let tmpdir = tempdir()?;

    let eval_context = &mut EvaluationContext {
        workdir: Some(&request.working_dir),
        ijsr,
        inputs: Some(&staged_inputs.clone()),
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
        &mut staged_inputs,
        eval_context,
        &request.working_dir,
    )?;

    let eval_context = &mut EvaluationContext {
        inputs: Some(&staged_inputs.clone()),
        workdir: Some(&request.working_dir),
        ijsr,
        runtime: Some(&runtime),
        ..Default::default()
    };

    //needs to be constructed after we created the eval context
    let flattened_inputs = create_flattened_inputs(&staged_inputs)?;

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
            &mut staged_inputs,
        )?
    } else {
        vec![]
    };

    let eval_context = &mut EvaluationContext {
        inputs: Some(&staged_inputs),
        workdir: Some(&request.working_dir),
        ijsr,
        runtime: Some(&runtime),
        ..Default::default()
    };

    //collect command string and correct args for staged paths
    let mut args = command::build_command(tool, &staged_inputs, &runtime)?;

    //correct and add the stdin value
    let mut stdin = get_stdin(tool, &staged_inputs);
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
    let json = serde_json::to_string_pretty(&outputs)?;
    println!("{json}");

    //evaluate exitstatus based on tool's expected exit codes

    Ok(ExecutionResult {
        exit_status: result.exit_status,
        stdout,
        stderr,
        outputs,
    })
}

/// Load an execution context from a CWL specification file and an inputs file.
pub fn load_execution_context(
    specification_path: impl AsRef<Path> + std::fmt::Debug,
    inputs_path: impl AsRef<Path> + std::fmt::Debug,
    outputs_path: Option<&Path>,
) -> anyhow::Result<ExecutionRequest> {
    let working_dir = env::current_dir()?;
    let base_path = specification_path.as_ref().parent().unwrap_or(&working_dir);

    let inputs = load_input_file_from_file(inputs_path, base_path)?;
    load_execution_context_with_inputs(specification_path, inputs, outputs_path)
}

/// Load an execution context from a CWL specification file and an already built inputs object (if inputs come as arguments, for example).
pub fn load_execution_context_with_inputs(
    specification_path: impl AsRef<Path> + std::fmt::Debug,
    inputs: InputObject,
    outputs_path: Option<&Path>,
) -> anyhow::Result<ExecutionRequest> {
    let doc = load_cwl_file(&specification_path, true)?;

    let working_dir = env::current_dir()?;
    let base_path = specification_path.as_ref().parent().unwrap_or(&working_dir);

    load_execution_context_from_document(doc, inputs, base_path, outputs_path)
}

/// Load an execution context from a CWL Document and an inputs object (if coming from workflow step for example).
pub fn load_execution_context_from_document(
    mut specification: CWLDocument,
    inputs: InputObject,
    base_path: impl AsRef<Path>,
    outputs_path: Option<&Path>,
) -> anyhow::Result<ExecutionRequest> {
    let environment = build_environment(&inputs);
    let requirements = collect_requirements(&specification, &inputs);
    let hints = collect_hints(&specification, &inputs);

    replace_schema_definitions(&mut specification, &requirements)?;

    let ctx = ExecutionRequest {
        requirements,
        hints,
        specification,
        inputs: inputs.inputs,
        working_dir: base_path.as_ref().to_path_buf(),
        out_dir: outputs_path.unwrap_or(base_path.as_ref()).to_path_buf(),
        environment,
    };

    Ok(ctx)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_execution_context() {
        let spec_path =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/cwl/tests/cat-tool.cwl");
        let inputs_path =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/cwl/tests/cat-job.json");

        let ctx = load_execution_context(&spec_path, inputs_path, None);
        assert!(ctx.is_ok());

        let ctx = ctx.unwrap();
        assert_eq!(ctx.inputs.len(), 1);
        assert_eq!(ctx.working_dir, spec_path.parent().unwrap());
    }
}
