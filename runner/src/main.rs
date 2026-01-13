use std::{collections::HashMap, path::PathBuf};

use clap::{arg, command, value_parser};
use commonwl::{
    execution::{backend::docker::DockerBackend, run_command},
    load_cwl_file,
};
use crankshaft::config::backend::docker::Config;
use tokio::fs;
use tokio_util::sync::CancellationToken;
use tracing::level_filters::LevelFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    //logging
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(LevelFilter::INFO)
        .finish();
    tracing::subscriber::set_global_default(subscriber)?;

    //setup cli
    let matches = command!("CWL Runner")
        .arg(arg!(<CWL_FILE> "CWL File").value_parser(value_parser!(PathBuf)))
        .arg(
            arg!(<YAML_FILE> "Input YAML")
                .required(false)
                .value_parser(value_parser!(PathBuf)),
        )
        .get_matches();

    let Some(cwl_file) = matches.get_one::<PathBuf>("CWL_FILE") else {
        anyhow::bail!("CWL File not given")
    };
    let maybe_input_file = matches.get_one::<PathBuf>("YAML_FILE");

    let cwl_specification = load_cwl_file(cwl_file, true)?;
    let input_data = if let Some(input_file) = maybe_input_file {
        let contents = fs::read_to_string(input_file).await?;
        serde_yaml::from_str::<HashMap<String, serde_yaml::Value>>(&contents)?
    } else {
        HashMap::new()
    };

    let token = CancellationToken::new();
    let backend = DockerBackend::new(Config::default()).await?;
    run_command(&cwl_specification, input_data, backend, token).await?;

    Ok(())
}
