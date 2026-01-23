use clap::{Arg, ArgMatches, Command, builder::ValueParser};
use commonwl::engine::backend::{TaskBackend, docker::DockerBackend, load_execution_context};
use crankshaft::config::backend::docker::Config;
use std::{env, path::{Path, PathBuf}, process::exit};
use tokio_util::sync::CancellationToken;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let matches = cli();
    let cwl_file = matches.get_one::<String>("spec").unwrap();
    let input_job = matches.get_one::<String>("job").unwrap();

    let base_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    let spec_path = base_dir.join(cwl_file);
    let job_path = base_dir.join(input_job);

    let outdir = matches.get_one::<PathBuf>("outdir").unwrap();

    let config = Config::default();
    let backend = DockerBackend::new(config).await?;
    let request = load_execution_context(spec_path, job_path, Some(outdir))?;
    let cancellation_token = CancellationToken::new();
    let exit_status = backend.run(&request, cancellation_token).await?;

    exit(exit_status.first().code().unwrap())
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
        .arg(Arg::new("job").help("Input file").required(true).index(2))
        .get_matches()
}
