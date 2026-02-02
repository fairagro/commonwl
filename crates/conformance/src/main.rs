use clap::{Arg, ArgMatches, Command, builder::ValueParser};
use commonwl::engine::{
    backend::{
        EngineStatus, TaskBackend, docker::DockerBackend, evaluate_exitcodes,
        load_execution_context, load_execution_context_with_inputs,
    },
    input::InputObject,
};
use crankshaft::config::backend::docker::Config;
use std::{
    env,
    path::{Path, PathBuf},
    process::exit,
};
use tokio_util::sync::CancellationToken;
use tracing::Level;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let matches = cli();
    let cwl_file = matches.get_one::<String>("spec").unwrap();
    let input_job = matches.get_one::<String>("job");

    let base_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    let spec_path = base_dir.join(cwl_file);
    let job_path = input_job.map(|job| base_dir.join(job));

    let outdir = matches.get_one::<PathBuf>("outdir").unwrap();

    let quiet = matches.get_one::<bool>("quiet").unwrap_or(&false);

    if !quiet {
        tracing_subscriber::fmt().with_max_level(Level::INFO).init();
    }

    let config = Config::default();
    let backend = DockerBackend::new(config).await?;
    let request = if let Some(job_path) = job_path {
        load_execution_context(spec_path, job_path, Some(outdir))?
    } else {
        load_execution_context_with_inputs(spec_path, InputObject::default(), Some(outdir))?
    };

    let cancellation_token = CancellationToken::new();
    let result = backend.run(&request, cancellation_token).await?;
    let exit_status = result.exit_status;

    let evaluated_code = evaluate_exitcodes(exit_status, &request.specification);

    if let EngineStatus::Success(_) = evaluated_code {
        exit(0)
    } else {
        exit(1)
    }
}

fn cli() -> ArgMatches {
    Command::new("conformance_runner")
        .version("0.1")
        .arg(
            Arg::new("outdir")
                .long("outdir")
                .value_name("DIR")
                .value_parser(ValueParser::path_buf())
                .help("Output directory"),
        )
        .arg(
            Arg::new("quiet")
                .short('q')
                .long("quiet")
                .action(clap::ArgAction::SetTrue),
        )
        .arg(Arg::new("spec").help("CWL file").required(true).index(1))
        .arg(Arg::new("job").help("Input file").required(false).index(2))
        .get_matches()
}
