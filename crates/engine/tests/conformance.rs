use crankshaft::config::backend::docker::Config;
use cwl_core::{Integer, files::FileOrDirectory, inputs::DefaultValue};
use cwl_engine::{
    ContainerEngine, DockerBackend, EngineStatus, ExecutionResult, InputObject, LocalBackend,
    TaskBackend, create_execution_request_with_inputs, evaluate_exitcodes, execute,
    load_input_file_from_file,
};
use cwl_engine_storage::{StorageBackend, StoragePath};
use serde::Deserialize;
use std::{
    fs,
    path::{Path, PathBuf},
    sync::Arc,
};
use tempfile::tempdir;
use tokio_util::sync::CancellationToken;

#[tokio::test]
async fn test_conformance_docker_clt() {
    //implementation limit
    let limit = 178;

    //there is a test the docker backend can not fulfill yet (113)
    let limit_after = limit - 113;
    let limit_before = limit - limit_after - 1;

    //load and select tests
    let tests = load_conformance_tests().unwrap();
    let selected_tests = tests
        .iter()
        .filter(|t| t.tags.contains(&"command_line_tool".to_string()))
        .collect::<Vec<_>>();
    let before = &selected_tests[..limit_before];
    let after = &selected_tests[limit_before + 1..limit_before + 1 + limit_after];

    //create docker backend
    let config = Config::default();
    let storage = Arc::new(StorageBackend::new());
    let data_store = StoragePath::from_local(Path::new("/tmp"));
    let backend = Arc::new(
        DockerBackend::new(config, storage, data_store)
            .await
            .unwrap(),
    );

    execute_conformance_test(backend, before.iter().copied().chain(after.iter().copied())).await;
}

#[tokio::test]
async fn test_conformance_local_clt() {
    //load and select tests
    let tests = load_conformance_tests().unwrap();
    let selected_tests = tests
        .iter()
        .filter(|t| t.tags.contains(&"command_line_tool".to_string()))
        .collect::<Vec<_>>();

    //create local backend
    let storage = Arc::new(StorageBackend::new());
    let data_store = StoragePath::from_local(Path::new("/tmp"));
    let backend = Arc::new(LocalBackend::new(
        ContainerEngine::Docker,
        storage,
        data_store,
    ));

    execute_conformance_test(backend, selected_tests.into_iter().take(178)).await;
}

#[tokio::test]
async fn test_conformance_docker_et() {
    //load and select tests
    let tests = load_conformance_tests().unwrap();
    let selected_tests = tests
        .iter()
        .filter(|t| t.tags.contains(&"expression_tool".to_string()));

    //create docker backend
    let config = Config::default();
    let storage = Arc::new(StorageBackend::new());
    let data_store = StoragePath::from_local(Path::new("/tmp"));
    let backend = Arc::new(
        DockerBackend::new(config, storage, data_store)
            .await
            .unwrap(),
    );

    //all expression tools pass! :)
    execute_conformance_test(backend, selected_tests).await;
}

#[tokio::test]
async fn test_conformance_docker_wf() {
    //load and select tests
    let tests = load_conformance_tests().unwrap();
    let selected_tests = tests
        .iter()
        .filter(|t| t.tags.contains(&"workflow".to_string()));

    //create docker backend
    let config = Config::default();
    let storage = Arc::new(StorageBackend::new());
    let data_store = StoragePath::from_local(Path::new("/tmp"));
    let backend = Arc::new(
        DockerBackend::new(config, storage, data_store)
            .await
            .unwrap(),
    );

    //all workflow pass! :)
    execute_conformance_test(backend, selected_tests).await;
}

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

fn load_conformance_tests() -> anyhow::Result<Vec<ConformanceTest>> {
    let base_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/cwl");
    let instruction_file = base_dir.join("conformance_tests.yaml");

    load_test_file(&instruction_file, &base_dir)
}

fn load_test_file(file: &Path, root: &Path) -> anyhow::Result<Vec<ConformanceTest>> {
    let contents = fs::read_to_string(file)?;
    let parent = file.parent().unwrap();

    let raw: serde_yaml::Value = serde_yaml::from_str(&contents)?;
    let resolved = resolve_imports(raw, parent, root)?;
    let resolved = rewrite_paths(resolved, parent, root);

    let items = match resolved {
        serde_yaml::Value::Sequence(seq) => flatten_sequences(seq),
        other => vec![other],
    };

    items
        .into_iter()
        .map(|v| serde_yaml::from_value::<ConformanceTest>(v).map_err(Into::into))
        .collect()
}

fn resolve_imports(
    value: serde_yaml::Value,
    parent: &Path,
    root: &Path,
) -> anyhow::Result<serde_yaml::Value> {
    match value {
        serde_yaml::Value::Mapping(map) => {
            if let Some(import_val) = map.get("$import") {
                let import_path = import_val
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("$import value is not a string"))?;
                let full_path = parent.join(import_path);
                let contents = fs::read_to_string(&full_path)?;
                let imported: serde_yaml::Value =
                    if full_path.extension().is_some_and(|e| e == "json") {
                        serde_json::from_str(&contents)?
                    } else {
                        serde_yaml::from_str(&contents)?
                    };
                let import_parent = full_path.parent().unwrap();
                // Resolve imports within the imported file relative to its own location
                let resolved = resolve_imports(imported, import_parent, root)?;
                // If it's a list of tests, rewrite their tool/job paths now
                return Ok(rewrite_paths(resolved, import_parent, root));
            }

            let resolved = map
                .into_iter()
                .map(|(k, v)| resolve_imports(v, parent, root).map(|r| (k, r)))
                .collect::<anyhow::Result<serde_yaml::Mapping>>()?;
            Ok(serde_yaml::Value::Mapping(resolved))
        }

        serde_yaml::Value::Sequence(seq) => {
            let resolved = seq
                .into_iter()
                .map(|v| resolve_imports(v, parent, root))
                .collect::<anyhow::Result<Vec<_>>>()?;
            Ok(serde_yaml::Value::Sequence(resolved))
        }

        other => Ok(other),
    }
}

fn rewrite_paths(value: serde_yaml::Value, dir: &Path, root: &Path) -> serde_yaml::Value {
    let relative_dir = dir.strip_prefix(root).unwrap_or(Path::new(""));
    if relative_dir.as_os_str().is_empty() {
        return value;
    }

    match value {
        serde_yaml::Value::Sequence(seq) => serde_yaml::Value::Sequence(
            seq.into_iter()
                .map(|v| rewrite_paths(v, dir, root))
                .collect(),
        ),
        serde_yaml::Value::Mapping(mut map) => {
            for key in ["tool", "job"] {
                let k = serde_yaml::Value::String(key.to_string());
                if let Some(serde_yaml::Value::String(path_str)) = map.get(&k) {
                    let new_path = relative_dir.join(path_str).to_string_lossy().into_owned();
                    map.insert(k, serde_yaml::Value::String(new_path));
                }
            }
            serde_yaml::Value::Mapping(map)
        }
        other => other,
    }
}

fn flatten_sequences(seq: Vec<serde_yaml::Value>) -> Vec<serde_yaml::Value> {
    let mut out = vec![];
    for item in seq {
        match item {
            serde_yaml::Value::Sequence(inner) => out.extend(flatten_sequences(inner)),
            other => out.push(other),
        }
    }
    out
}

async fn execute_conformance_test<T: TaskBackend + Clone + Send + 'static>(
    backend: Arc<T>,
    tests: impl Iterator<Item = &ConformanceTest>,
) {
    for test in tests {
        let base_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../testdata/cwl")
            .canonicalize()
            .unwrap();
        let specification_path = base_dir.join(&test.tool);
        let base = specification_path.parent().unwrap();
        let inputs = if let Some(job) = &test.job {
            load_input_file_from_file(base_dir.join(job), base).unwrap()
        } else {
            InputObject::default()
        };
        let outdir = tempdir().unwrap();
        let request = create_execution_request_with_inputs(
            specification_path,
            inputs,
            Some(outdir.path()),
            None,
        )
        .unwrap();

        let cancellation_token = CancellationToken::new();

        eprintln!("Running Test {}", test.id);
        let backend = backend.task_scoped();
        let result = execute(backend, &request, cancellation_token).await;
        if test.should_fail {
            if result.is_ok() {
                dbg!(&result);
            }
            let status = if let Ok(result) = &result {
                evaluate_exitcodes(&result.exit_status, &request.specification)
            } else {
                EngineStatus::Undefined(666)
            };
            assert!(result.is_err() || matches!(status, EngineStatus::Failure(_)));
        } else {
            if result.is_err() {
                dbg!(&result);
            }
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
            assert!(
                result.outputs.contains_key(&key),
                "Could not find key {}, outputs: {:#?}",
                key,
                result.outputs
            );
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
            if exp_seq.len() != act_seq.len() {
                return false;
            }
            let mut claimed = vec![false; act_seq.len()];
            exp_seq.iter().all(|exp| {
                if let Some(i) = act_seq
                    .iter()
                    .enumerate()
                    .position(|(i, act)| !claimed[i] && compare_yaml_or_fod(exp, act))
                {
                    claimed[i] = true;
                    true
                } else {
                    false
                }
            })
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
                        eprintln!(
                            "Could not validate basename for {file:?}, {actual_basename}!={expected_basename}"
                        );
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
