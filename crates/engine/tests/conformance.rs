use crankshaft::config::backend::docker::Config;
use cwl_core::{Integer, files::FileOrDirectory, inputs::DefaultValue};
use cwl_engine::{
    backend::{
        ExecutionResult, TaskBackend, docker::DockerBackend, load_execution_context_with_inputs,
    },
    input::{InputObject, load_input_file_from_file},
};
use serde::Deserialize;
use std::{
    fs,
    path::{Path, PathBuf},
};
use tempfile::tempdir;
use tokio_util::sync::CancellationToken;

#[derive(Deserialize, Debug)]
struct ConformanceTest {
    job: Option<PathBuf>,
    tool: PathBuf,
    output: Option<serde_yaml::Value>,
    id: String,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    should_fail: bool,
}

#[derive(Deserialize, Debug)]
#[serde(untagged)]
enum ConformanceTestItem {
    Test(ConformanceTest),
    Import {
        #[serde(rename = "$import")]
        import: String,
    },
}

fn load_conformance_tests() -> anyhow::Result<Vec<ConformanceTest>> {
    let base_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/cwl");
    let instruction_file = base_dir.join("conformance_tests.yaml");

    load_test_file(&instruction_file)
}

fn load_test_file(file: &Path) -> anyhow::Result<Vec<ConformanceTest>> {
    let contents = fs::read_to_string(file)?;
    let parsed: Vec<ConformanceTestItem> = serde_yaml::from_str(&contents)?;
    let mut result = vec![];
    let parent = file.parent().unwrap();
    for item in parsed {
        match item {
            ConformanceTestItem::Test(test) => result.push(test),
            ConformanceTestItem::Import { import } => {
                result.extend(load_test_file(&parent.join(import))?)
            }
        }
    }

    Ok(result)
}

#[tokio::test]
async fn test_command_line_tools_docker_backend() {
    //implementation limit
    let limit = 36;
    let tests = load_conformance_tests().unwrap();
    let selected_tests = tests
        .iter()
        .filter(|t| t.tags.contains(&"command_line_tool".to_string()))
        .collect::<Vec<_>>();

    for test in selected_tests.iter().take(limit) {
        let base_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../testdata/cwl")
            .canonicalize()
            .unwrap();
        let specification_path = base_dir.join(&test.tool);
        let inputs = if let Some(job) = &test.job {
            load_input_file_from_file(base_dir.join(job), base_dir.join("tests")).unwrap()
        } else {
            InputObject::default()
        };
        let outdir = tempdir().unwrap();
        let request =
            load_execution_context_with_inputs(specification_path, inputs, Some(outdir.path()))
                .unwrap();

        let config = Config::default();
        let backend = DockerBackend::new(config).await.unwrap();

        let cancellation_token = CancellationToken::new();

        eprintln!("Running Test {}", test.id);
        let result = backend.run(&request, cancellation_token).await;
        if test.should_fail {
            assert!(result.is_err());
        } else {
            assert!(result.is_ok());
            let result = result.unwrap();
            evaluate_result(&test.output.clone().unwrap(), result);
        }
    }
}

fn evaluate_result(output: &serde_yaml::Value, result: ExecutionResult) {
    if let serde_yaml::Value::Mapping(output) = output {
        for (key, value) in output {
            let key = key.as_str().unwrap().to_string();
            assert!(result.outputs.contains_key(&key));
            evaluate_item(value, result.outputs.get(&key).unwrap());
        }
    } else {
        panic!()
    }
}

fn evaluate_item(value: &serde_yaml::Value, result: &DefaultValue) {
    assert!(
        match result {
            DefaultValue::FileOrDirectory(fod) => compare_file_or_directory(value, fod),
            DefaultValue::Any(actual) => compare_yaml_values(value, actual),
        },
        "could not validate {result:?} with {value:?}"
    )
}

fn compare_yaml_values(expected: &serde_yaml::Value, actual: &serde_yaml::Value) -> bool {
    match (expected, actual) {
        (serde_yaml::Value::String(s), _) if s == "Any" => true,

        (serde_yaml::Value::Mapping(exp_map), serde_yaml::Value::Mapping(act_map)) => {
            exp_map.iter().all(|(key, exp_val)| {
                act_map
                    .get(key)
                    .map(|act_val| compare_yaml_or_fod(exp_val, act_val))
                    .unwrap_or(false)
            })
        }

        (serde_yaml::Value::Sequence(exp_seq), serde_yaml::Value::Sequence(act_seq)) => {
            exp_seq.len() == act_seq.len()
                && exp_seq
                    .iter()
                    .zip(act_seq.iter())
                    .all(|(exp, act)| compare_yaml_or_fod(exp, act))
        }

        (serde_yaml::Value::String(exp), serde_yaml::Value::String(act)) => exp == act,
        (serde_yaml::Value::Number(exp), serde_yaml::Value::Number(act)) => exp == act,
        (serde_yaml::Value::Bool(exp), serde_yaml::Value::Bool(act)) => exp == act,
        (serde_yaml::Value::Null, serde_yaml::Value::Null) => true,

        _ => false,
    }
}

fn compare_yaml_or_fod(expected: &serde_yaml::Value, actual: &serde_yaml::Value) -> bool {
    if let Ok(default_value) = serde_yaml::from_value::<DefaultValue>(actual.clone()) {
        match default_value {
            DefaultValue::FileOrDirectory(fod) => compare_file_or_directory(expected, &fod),
            DefaultValue::Any(yaml_val) => compare_yaml_values(expected, &yaml_val),
        }
    } else {
        compare_yaml_values(expected, actual)
    }
}

fn compare_file_or_directory(expected: &serde_yaml::Value, actual: &FileOrDirectory) -> bool {
    match actual {
        FileOrDirectory::File(file) => {
            if let Some(serde_yaml::Value::String(class)) = expected.get("class")
                && class != "File"
            {
                eprintln!("Could not validate class for {file:?}");
                return false;
            }

            if let Some(serde_yaml::Value::String(expected_checksum)) = expected.get("checksum") {
                if let Some(actual_checksum) = &file.checksum {
                    if expected_checksum != actual_checksum {
                        eprintln!("Could not validate checksum for {file:?}");
                        return false;
                    }
                } else {
                    //require checksum but not given
                    eprintln!("Could not validate checksum for {file:?}, not given");
                    return false;
                }
            }

            if let Some(serde_yaml::Value::Number(expected_size)) = expected.get("size") {
                if let Some(Integer::Long(actual_size)) = &file.size {
                    if expected_size.as_i64().unwrap() != *actual_size {
                        eprintln!("Could not validate size for {file:?}");
                        return false;
                    }
                } else if let Some(Integer::Int(actual_size)) = &file.size {
                    if expected_size.as_i64().unwrap() as i32 != *actual_size {
                        eprintln!("Could not validate size for {file:?}");
                        return false;
                    }
                } else {
                    //require size but not given
                    eprintln!("Could not validate size for {file:?}, not given");
                    return false;
                }
            }

            if let Some(serde_yaml::Value::String(expected_basename)) = expected.get("basename") {
                if let Some(actual_basename) = &file.basename {
                    if expected_basename != "Any" && expected_basename != actual_basename {
                        eprintln!("Could not validate basename for {file:?}");
                        return false;
                    }
                } else {
                    //require basename  but not given
                    eprintln!("Could not validate basename for {file:?}, not given");
                    return false;
                }
            }

            if let Some(serde_yaml::Value::String(expected_format)) = expected.get("format") {
                if let Some(actual_format) = &file.format {
                    if expected_format != "Any" && expected_format != actual_format {
                        eprintln!("Could not validate format for {file:?}");
                        return false;
                    }
                } else {
                    //require format but not given
                    eprintln!("Could not validate format for {file:?}, not given");
                    return false;
                }
            }

            true
        }
        FileOrDirectory::Directory(directory) => {
            if let Some(serde_yaml::Value::String(class)) = expected.get("class")
                && class != "Directory"
            {
                eprintln!("Could not validate class for {directory:?}");
                return false;
            }

            if let Some(serde_yaml::Value::String(expected_basename)) = expected.get("basename") {
                if let Some(actual_basename) = &directory.basename {
                    if expected_basename != "Any" && expected_basename != actual_basename {
                        eprintln!("Could not validate basename for {directory:?}");
                        return false;
                    }
                } else {
                    //require basename but not given
                    eprintln!("Could not validate basename for {directory:?}");
                    return false;
                }
            }

            if let Some(exp_listing) = expected.get("listing") {
                if let Some(actual_listing) = &directory.listing {
                    return compare_yaml_values(
                        exp_listing,
                        &serde_yaml::to_value(actual_listing).unwrap(),
                    );
                } else {
                    //require listing but not given
                    eprintln!("Could not validate listing for {directory:?}");
                    return false;
                }
            }

            true
        }
    }
}
