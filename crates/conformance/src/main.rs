use clap::{Arg, ArgMatches, Command, builder::ValueParser};
use commonwl::engine::{
    ContainerEngine, DockerBackend, EngineStatus, InputObject, LocalBackend, TaskBackend,
    TesBackend, create_execution_request, create_execution_request_with_inputs, evaluate_exitcodes,
    execute,
};
use core::panic;
use crankshaft::config::backend::{docker::Config, tes};
use cwl_engine_storage::StorageBackend;
use std::{
    env,
    path::{Path, PathBuf},
    process::exit,
    sync::Arc,
};
use tokio_util::sync::CancellationToken;
use tracing::Level;
use url::Url;

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
    let debug = matches.get_one::<bool>("debug").unwrap_or(&false);

    if !quiet && !debug {
        tracing_subscriber::fmt().with_max_level(Level::INFO).init();
    }
    if *debug {
        tracing_subscriber::fmt()
            .with_max_level(Level::DEBUG)
            .init();
    }
    let backend_select = env::var("BACKEND").unwrap_or("docker".to_string());
    let backend: Arc<dyn TaskBackend> = if backend_select == "docker" {
        let config = Config::default();
        Arc::new(DockerBackend::new(config).await?)
    } else if backend_select == "local" {
        Arc::new(LocalBackend::new(ContainerEngine::Docker))
    } else if backend_select == "tes" {
        let config = tes::Config::builder()
            .url(Url::parse("http://localhost:8000")?)
            .interval(10)
            .build();
        Arc::new(TesBackend::new(config).await?)
    } else {
        panic!()
    };

    let request = if let Some(job_path) = job_path {
        create_execution_request(spec_path, job_path, Some(outdir))?
    } else {
        create_execution_request_with_inputs(spec_path, InputObject::default(), Some(outdir), None)?
    };

    let cancellation_token = CancellationToken::new();
    let storage = Arc::new(StorageBackend::new());
    let result = execute(backend, storage, &request, cancellation_token).await?;
    let exit_status = result.exit_status;

    let evaluated_code = evaluate_exitcodes(&exit_status, &request.specification);

    if let EngineStatus::Success(_) = evaluated_code {
        let json = serde_json::to_string_pretty(&result.outputs)?;
        println!("{json}");
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
        .arg(
            Arg::new("debug")
                .short('d')
                .long("debug")
                .action(clap::ArgAction::SetTrue),
        )
        .arg(Arg::new("spec").help("CWL file").required(true).index(1))
        .arg(Arg::new("job").help("Input file").required(false).index(2))
        .get_matches()
}
