use crate::{
    context::Runtime,
    expression::{self, EvaluationContext},
    input::validation::validate_command_input,
    pathmapper::PathMapper,
};
use cwl_core::{
    IntegerOrExpression, OneOrMany,
    documents::{Argument, CommandLineTool},
    files::FileOrDirectory,
    inputs::{
        CommandInputParameterType, CommandInputSchema, CommandInputType, CommandLineBinding,
        DefaultValue,
    },
    requirements::{InlineJavascriptRequirement, ShellCommandRequirement},
    types::CWLType,
    value_as_string,
};
use std::collections::HashMap;

#[derive(Debug, Clone)]
struct BoundBinding {
    sort_key: Vec<SortKey>,
    binding: CommandLineBinding,
    value: DefaultValue,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum SortKey {
    Int(i32),
    Str(String),
}

pub fn build_command(
    tool: &CommandLineTool,
    inputs: &HashMap<String, DefaultValue>,
    runtime: &Runtime,
    path_mapper: Option<&PathMapper>,
) -> anyhow::Result<Vec<String>> {
    let mut args: Vec<String> = vec![];

    let ijsr = tool.get_requirement_or_hint::<InlineJavascriptRequirement>();

    if let Some(base_command) = &tool.base_command {
        let cmd = match &base_command {
            OneOrMany::One(cmd) => cmd,
            OneOrMany::Many(vec) => {
                if vec.is_empty() {
                    &String::new()
                } else {
                    &vec[0]
                }
            }
        };

        if !cmd.is_empty() {
            args.push(cmd.clone());
            if let OneOrMany::Many(vec) = &base_command {
                args.extend_from_slice(&vec[1..]);
            }
        }
    }

    let mut bindings: Vec<BoundBinding> = vec![];

    //handle arguments...
    if let Some(arguments) = &tool.arguments {
        for (i, arg) in arguments.iter().enumerate() {
            let mut sort_key = vec![];
            match arg {
                Argument::String(str) => {
                    let binding = CommandLineBinding {
                        value_from: Some(str.clone()),
                        ..Default::default()
                    };
                    sort_key.push(SortKey::Int(0));
                    sort_key.push(SortKey::Int(i32::try_from(i)?));
                    bindings.push(BoundBinding {
                        sort_key,
                        binding,
                        value: DefaultValue::Any(serde_yaml::Value::Null),
                    });
                }
                Argument::Binding(binding) => {
                    let position = binding.position.clone().map(|p| match p {
                        IntegerOrExpression::Int(i) => i,
                        IntegerOrExpression::Long(l) => i32::try_from(l).unwrap_or_default(),
                        IntegerOrExpression::Expression(_s) => todo!(), //evaluate expression
                    });
                    sort_key.push(SortKey::Int(position.unwrap_or_default()));
                    sort_key.push(SortKey::Int(i32::try_from(i)?));
                    bindings.push(BoundBinding {
                        sort_key,
                        binding: binding.clone(),
                        value: DefaultValue::Any(serde_yaml::Value::Null),
                    });
                }
            }
        }
    }

    for input in &tool.inputs {
        //check input id is present (should always be the case!)
        let Some(input_id) = &input.id else {
            anyhow::bail!("No input id");
        };
        //check for value given
        let value = inputs
            .get(input_id)
            .unwrap_or(&DefaultValue::Any(serde_yaml::Value::Null));

        if matches!(value, DefaultValue::Any(serde_yaml::Value::Null))
            && !input.r#type.is_null_allowed()
        {
            //We have null value and type is not nullable!
            anyhow::bail!("No input for `{}` given!", input.id.as_ref().unwrap());
        }

        //do not flags if false
        if matches!(value, DefaultValue::Any(serde_yaml::Value::Bool(false))) {
            continue;
        }

        collect_input_bindings(
            &input.r#type,
            &input.input_binding,
            value,
            input_id,
            &[],
            &mut bindings,
        )?;
    }

    bindings.sort_by(|a, b| a.sort_key.cmp(&b.sort_key));

    if tool.has_requirement_or_hint::<ShellCommandRequirement>() {
        let mut cmd = args.clone();
        for bound in bindings {
            let mut arg = if is_argument(&bound) {
                use_value_from(&bound.binding, inputs, runtime, ijsr, path_mapper)?
            } else {
                generate_arg(&bound.binding, bound.value, runtime, ijsr, path_mapper)?
            };

            if bound.binding.shell_quote.unwrap_or(true) {
                arg = apply_shell_quote(arg);
            }
            cmd.extend(arg);
        }
        //correct paths before joining them
        if let Some(path_mapper) = path_mapper {
            cmd = path_mapper.correct_execution_paths(cmd);
        }

        let cmdline = cmd.join(" ");
        args = get_shell_command();
        args.push(cmdline);
    } else {
        for bound in bindings {
            let arg = if is_argument(&bound) {
                use_value_from(&bound.binding, inputs, runtime, ijsr, path_mapper)?
            } else {
                generate_arg(&bound.binding, bound.value, runtime, ijsr, path_mapper)?
            };
            args.extend(arg);
        }
    }

    //correct paths
    if let Some(path_mapper) = path_mapper {
        args = path_mapper.correct_execution_paths(args);
    }

    //remove empty args
    args.retain(|s| !s.is_empty());

    Ok(args)
}

fn collect_input_bindings(
    schema: &CommandInputParameterType,
    binding: &Option<CommandLineBinding>,
    value: &DefaultValue,
    name: &str,
    base_sort_key: &[SortKey],
    bindings: &mut Vec<BoundBinding>,
) -> anyhow::Result<()> {
    let matched_schema =
        if let CommandInputParameterType::CommandInputType(OneOrMany::Many(types)) = schema {
            types
                .iter()
                .find(|&t| validate_command_input(&t.clone().into(), value))
                .map(|ty| CommandInputParameterType::from(ty.clone()))
        } else {
            None
        };
    let schema = if let Some(schema) = &matched_schema {
        schema
    } else {
        schema
    };
    if let CommandInputParameterType::CommandInputType(OneOrMany::One(
        CommandInputType::CommandInputSchema(schema),
    )) = schema
    {
        match schema.as_ref() {
            CommandInputSchema::Enum(_) => {}
            CommandInputSchema::Record(record) => {
                //add the root record binding with a value of null
                if let Some(rec_binding) = &record.input_binding {
                    let rec_binding = rec_binding.clone();
                    let mut sort_key = base_sort_key.to_owned();
                    if let Some(root_binding) = &binding {
                        sort_key.push(SortKey::Int(
                            get_binding_position(root_binding).unwrap_or_default(),
                        ));
                        sort_key.push(SortKey::Str(name.to_owned()));
                    }
                    sort_key.push(SortKey::Int(
                        get_binding_position(&rec_binding).unwrap_or_default(),
                    ));

                    let value = DefaultValue::Any(serde_yaml::Value::Null);

                    bindings.push(BoundBinding {
                        sort_key,
                        binding: rec_binding,
                        value,
                    });
                }

                if let Some(fields) = &record.fields {
                    let DefaultValue::Any(serde_yaml::Value::Mapping(map)) = value else {
                        panic!("previous validation of `{name}` did not work")
                    };
                    for field in fields {
                        if let Some(fi_binding) = &field.input_binding {
                            let fi_binding = fi_binding.clone();
                            let mut sort_key = base_sort_key.to_owned();
                            if let Some(root_binding) = &binding {
                                sort_key.push(SortKey::Int(
                                    get_binding_position(root_binding).unwrap_or_default(),
                                ));
                                sort_key.push(SortKey::Str(name.to_owned()));
                            }
                            sort_key.push(SortKey::Int(
                                get_binding_position(&fi_binding).unwrap_or_default(),
                            ));

                            let is_optional = match &field.r#type {
                                OneOrMany::One(t) => t.is_null_allowed(),
                                OneOrMany::Many(items) => items.iter().any(|i| i.is_null_allowed()),
                            };
                            if let Some(value) = map.get(field.name.clone()) {
                                let value = serde_yaml::from_value(value.clone())?;
                                let schema = field.r#type.clone();

                                collect_input_bindings(
                                    &CommandInputParameterType::CommandInputType(schema),
                                    &Some(fi_binding),
                                    &value,
                                    &field.name,
                                    &sort_key,
                                    bindings,
                                )?
                            } else if !is_optional {
                                //not found but not optional
                                anyhow::bail!(
                                    "input did not provide input for struct field {}",
                                    field.name.clone()
                                );
                            }
                        }
                    }
                }
            }
            CommandInputSchema::Array(array) => {
                //at this point we can assume, that input has the correct format
                let DefaultValue::Any(serde_yaml::Value::Sequence(vec)) = value else {
                    panic!("previous validation of `{name}` did not work")
                };

                //do not add binding for empty vec
                if vec.is_empty() {
                    return Ok(());
                }

                let should_recurse = binding
                    .clone()
                    .map(|b| b.item_separator.is_none() && b.value_from.is_none())
                    .unwrap_or(true);
                if should_recurse {
                    for (ix, item) in vec.iter().enumerate() {
                        //reassign to DefaultValue
                        let item = serde_yaml::from_value(item.clone())?;
                        let mut sort_key = base_sort_key.to_owned();
                        //add root key
                        if let Some(binding) = &binding {
                            sort_key.push(SortKey::Int(
                                get_binding_position(binding).unwrap_or_default(),
                            ));
                            sort_key.push(SortKey::Str(name.to_owned()));
                        }
                        sort_key.push(SortKey::Int(ix as i32));

                        let schema = array.items.clone();

                        let binding = array.input_binding.clone().or_else(|| binding.clone());
                        if let Some(binding) = binding {
                            collect_input_bindings(
                                &CommandInputParameterType::CommandInputType(schema),
                                &Some(binding),
                                &item,
                                name,
                                &sort_key,
                                bindings,
                            )?;
                        }
                    }
                }
            }
        }
    }

    //add root binding
    if let Some(binding) = &binding
        && !matches!(
            schema,
            CommandInputParameterType::CommandInputType(OneOrMany::One(CommandInputType::CWLType(
                CWLType::Null
            )))
        )
    {
        let binding = binding.clone();
        let mut sort_key = base_sort_key.to_owned();
        sort_key.push(SortKey::Int(
            get_binding_position(&binding).unwrap_or_default(),
        ));
        sort_key.push(SortKey::Str(name.to_owned()));

        bindings.push(BoundBinding {
            sort_key,
            binding,
            value: value.clone(),
        });
    }

    Ok(())
}

pub(crate) fn generate_arg(
    binding: &CommandLineBinding,
    mut value: DefaultValue,
    runtime: &Runtime,
    ijsr: Option<&InlineJavascriptRequirement>,
    path_mapper: Option<&PathMapper>,
) -> anyhow::Result<Vec<String>> {
    let sep = binding.separate.unwrap_or(true);

    if binding.prefix.is_none() && !sep {
        anyhow::bail!("if `separate` is false a prefix is mandatory.")
    }

    if let Some(value_from) = &binding.value_from {
        let json_value = serde_json::to_value(value)?;
        let result = if let Ok(result) = expression::do_eval(
            value_from,
            &EvaluationContext {
                context: Some(&json_value),
                runtime: Some(runtime),
                ijsr,
                workdir: path_mapper.map(|m| m.base_dir()),
                ..Default::default()
            },
        ) {
            result
        } else {
            //default to value_from unevaluated
            serde_yaml::Value::String(value_from.to_string())
        };
        value = serde_yaml::from_value(result)?;
    }

    let mut argl = vec![];

    match value {
        DefaultValue::Any(value) => match value {
            serde_yaml::Value::Sequence(arr) => {
                if let Some(separator) = &binding.item_separator {
                    argl = vec![DefaultValue::Any(serde_yaml::Value::String(
                        arr.iter()
                            .map(|i| value_as_string(i).unwrap_or_default())
                            .collect::<Vec<_>>()
                            .join(separator),
                    ))];
                } else if binding.value_from.is_some() {
                    let mut val = arr
                        .iter()
                        .map(|i| value_as_string(i).unwrap_or_default())
                        .collect::<Vec<_>>();
                    if let Some(prefix) = &binding.prefix {
                        val.insert(0, prefix.clone());
                    }
                    return Ok(val);
                } else if let Some(prefix) = &binding.prefix {
                    return Ok(vec![prefix.clone()]);
                } else {
                    return Ok(vec![]);
                }
            }
            serde_yaml::Value::Mapping(_) => {
                if let Some(prefix) = &binding.prefix {
                    return Ok(vec![prefix.clone()]);
                } else {
                    return Ok(vec![]);
                }
            }
            _ => argl = vec![DefaultValue::Any(value.clone())],
        },
        DefaultValue::FileOrDirectory(fd) => {
            let mut fd = fd.clone();
            //get mapped path
            if let Some(path_mapper) = path_mapper
                && let Some(path) = fd.path()
            {
                fd.set_path(
                    path_mapper
                        .get_guest(path)
                        .map(|p| p.to_string_lossy().into_owned()),
                );
            }

            argl = vec![DefaultValue::FileOrDirectory(fd)]
        }
    }

    Ok(argl
        .into_iter()
        .flat_map(|j| {
            if let DefaultValue::Any(serde_yaml::Value::Null) = j {
                vec![]
            } else {
                let s = to_str(&j);
                if sep {
                    if let Some(p) = &binding.prefix {
                        vec![p.clone(), s]
                    } else {
                        vec![s]
                    }
                } else if let Some(p) = &binding.prefix {
                    vec![format!("{p}{s}")]
                } else {
                    vec![s]
                }
            }
        })
        .collect::<Vec<String>>())
}

fn use_value_from(
    binding: &CommandLineBinding,
    inputs: &HashMap<String, DefaultValue>,
    runtime: &Runtime,
    ijsr: Option<&InlineJavascriptRequirement>,
    path_mapper: Option<&PathMapper>,
) -> anyhow::Result<Vec<String>> {
    //evaluate first
    let mut value = DefaultValue::Any(serde_yaml::Value::String(
        binding.value_from.clone().unwrap_or_default(),
    ));

    //try to evaluate expression
    if let Ok(result) = expression::do_eval(
        value.as_str().unwrap(), //works per definition
        &EvaluationContext {
            inputs: Some(inputs),
            runtime: Some(runtime),
            ijsr,
            workdir: path_mapper.map(|m| m.base_dir()),
            ..Default::default()
        },
    ) {
        let dv: DefaultValue = serde_yaml::from_value(result)?;
        //do not add boolenas if false
        if matches!(dv, DefaultValue::Any(serde_yaml::Value::Bool(false))) {
            return Ok(vec![]);
        }
        value = dv
    }

    let values = match value {
        DefaultValue::Any(serde_yaml::Value::Sequence(vec)) => vec
            .iter()
            .map(|i| to_str(&DefaultValue::Any(i.clone())))
            .collect::<Vec<_>>(),
        default => vec![to_str(&default)],
    };

    if let Some(p) = &binding.prefix
        && binding.value_from.is_some()
    {
        Ok([vec![p.clone()], values].concat())
    } else if binding.value_from.is_some() {
        Ok(values)
    } else {
        Ok(vec![])
    }
}

pub(crate) fn to_str(val: &DefaultValue) -> String {
    match val {
        DefaultValue::FileOrDirectory(fd) => match fd.path() {
            Some(path) => path.to_string(),
            None => {
                if let FileOrDirectory::File(file) = fd
                    && let Some(checksum) = &file.checksum
                {
                    checksum.to_string()
                } else {
                    "\"No path given\"".to_string()
                }
            }
        },
        DefaultValue::Any(value) => match value {
            serde_yaml::Value::String(s) => s.to_string(),
            serde_yaml::Value::Number(n) => n.to_string(),
            _ => String::new(),
        },
    }
}

fn is_argument(bound: &BoundBinding) -> bool {
    matches!(bound.value, DefaultValue::Any(serde_yaml::Value::Null))
}

fn get_binding_position(binding: &CommandLineBinding) -> Option<i32> {
    binding.position.clone().map(|p| match p {
        IntegerOrExpression::Int(i) => i,
        IntegerOrExpression::Long(l) => i32::try_from(l).unwrap_or_default(),
        IntegerOrExpression::Expression(_s) => todo!(), //evaluate expression
    })
}

fn get_shell_command() -> Vec<String> {
    let shell = if cfg!(target_os = "windows") {
        "cmd"
    } else {
        "sh"
    };
    let param = if cfg!(target_os = "windows") {
        "/C"
    } else {
        "-c"
    };
    vec![shell.to_string(), param.to_string()]
}

fn apply_shell_quote(arg: Vec<String>) -> Vec<String> {
    arg.iter().map(|a| format!("'{a}'")).collect()
}

#[cfg(test)]
mod tests {
    use crate::input::collect_inputs;

    use super::*;
    use cwl_core::{
        documents::CWLDocument,
        inputs::{CommandInputArraySchema, CommandInputParameter},
        load_cwl_file,
        types::CWLType,
    };
    use std::path::PathBuf;

    #[test]
    fn test_build_command() {
        let yaml = r"
class: CommandLineTool
cwlVersion: v1.2
inputs:
  file1: 
    type: File
    inputBinding: {position: 0}
outputs:
  output_file:
    type: File
    outputBinding: {glob: output.txt}
baseCommand: cat
stdout: output.txt";
        let inputs = r#"{
    "file1": {
        "class": "File",
        "path": "hello.txt"
    }
}"#;
        let doc: CWLDocument = serde_yaml::from_str(yaml).unwrap();

        let input_values = serde_yaml::from_str(inputs).unwrap();
        let inputs = collect_inputs(&doc, &input_values).unwrap();

        let CWLDocument::CommandLineTool(tool) = doc else {
            panic!()
        };

        let cmd = build_command(&tool, &inputs, &Runtime::default(), None).unwrap();
        let cmdline = cmd.join(" ");
        assert_eq!(cmdline, "cat hello.txt");
    }

    #[test]
    fn test_build_command_args() {
        let yaml = r#"class: CommandLineTool
cwlVersion: v1.2
requirements:
  - class: ShellCommandRequirement
inputs:
  indir: Directory
outputs:
  outlist:
    type: File
    outputBinding:
      glob: output.txt
arguments: ["cd", "$(inputs.indir.path)",
  {shellQuote: false, valueFrom: "&&"},
  "find", ".",
  {shellQuote: false, valueFrom: "|"},
  "sort"]
stdout: output.txt"#;
        let inputs = r"indir:
  class: Directory
  location: testdir";
        let doc: CWLDocument = serde_yaml::from_str(yaml).unwrap();

        let input_values = serde_yaml::from_str(inputs).unwrap();
        let inputs = collect_inputs(&doc, &input_values).unwrap();

        let CWLDocument::CommandLineTool(tool) = doc else {
            panic!()
        };
        let cmd = build_command(&tool, &inputs, &Runtime::default(), None).unwrap();

        let shell_cmd = get_shell_command();

        assert_eq!(
            cmd,
            vec![
                &shell_cmd[0],
                &shell_cmd[1],
                "'cd' 'testdir' && 'find' '.' | 'sort'"
            ]
        );
    }

    #[test]
    fn test_build_command_difficult() {
        let base_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/cwl");
        let tool_path = base_dir.join("tests/bwa-mem-tool.cwl");
        let doc = load_cwl_file(tool_path, true).unwrap();

        let inputs = include_str!("../../testdata/cwl/tests/bwa-mem-job.json");
        let input_values = serde_yaml::from_str(inputs).unwrap();
        let inputs = collect_inputs(&doc, &input_values).unwrap();

        let CWLDocument::CommandLineTool(tool) = doc else {
            panic!()
        };

        let runtime = Runtime {
            cores: 69,
            ..Default::default()
        };
        let mut cmd = build_command(&tool, &inputs, &runtime, None).unwrap();
        cmd = cmd[2..].to_vec();

        assert_eq!(
            cmd,
            vec![
                "bwa",
                "mem",
                "-t",
                "69",
                "-I",
                "1,2,3,4",
                "-m",
                "3",
                "chr20.fa",
                "example_human_Illumina.pe_1.fastq",
                "example_human_Illumina.pe_2.fastq"
            ]
        );
    }

    #[test]
    fn test_build_command_difficult_2() {
        let base_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/cwl");
        let tool_path = base_dir.join("tests/binding-test.cwl");
        let doc = load_cwl_file(tool_path, true).unwrap();

        let inputs = include_str!("../../testdata/cwl/tests/bwa-mem-job.json");
        let input_values = serde_yaml::from_str(inputs).unwrap();
        let inputs = collect_inputs(&doc, &input_values).unwrap();

        let CWLDocument::CommandLineTool(tool) = doc else {
            panic!()
        };

        let mut cmd = build_command(&tool, &inputs, &Runtime::default(), None).unwrap();
        cmd = cmd[2..].to_vec();

        assert_eq!(
            cmd,
            vec![
                "bwa",
                "mem",
                "chr20.fa",
                "-XXX",
                "-YYY",
                "example_human_Illumina.pe_1.fastq",
                "-YYY",
                "example_human_Illumina.pe_2.fastq"
            ]
        );
    }

    #[test]
    fn test_build_command_with_schema_def() {
        let base_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/cwl");
        let tool_path = base_dir.join("tests/tmap-tool.cwl");
        let doc = load_cwl_file(tool_path, true).unwrap();

        let inputs = include_str!("../../testdata/cwl/tests/tmap-job.json");
        let input_values = serde_yaml::from_str(inputs).unwrap();
        let inputs = collect_inputs(&doc, &input_values).unwrap();

        let CWLDocument::CommandLineTool(tool) = doc else {
            panic!()
        };

        let mut cmd = build_command(&tool, &inputs, &Runtime::default(), None).unwrap();
        cmd = cmd[2..].to_vec();

        assert_eq!(
            cmd,
            vec![
                "tmap",
                "mapall",
                "stage1",
                "map1",
                "--min-seq-length",
                "20",
                "map2",
                "--min-seq-length",
                "20",
                "stage2",
                "map1",
                "--max-seq-length",
                "20",
                "--min-seq-length",
                "10",
                "--seed-length",
                "16",
                "map2",
                "--max-seed-hits",
                "-1",
                "--max-seq-length",
                "20",
                "--min-seq-length",
                "10"
            ]
        );
    }

    #[test]
    fn test_build_command_with_record_bindings() {
        let base_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/cwl");
        let tool_path = base_dir.join("tests/record-order.cwl");
        let doc = load_cwl_file(tool_path, true).unwrap();

        let inputs = include_str!("../../testdata/cwl/tests/record-order-job.json");
        let input_values = serde_yaml::from_str(inputs).unwrap();
        let inputs = collect_inputs(&doc, &input_values).unwrap();

        let CWLDocument::CommandLineTool(tool) = doc else {
            panic!()
        };

        let mut cmd = build_command(&tool, &inputs, &Runtime::default(), None).unwrap();
        cmd = cmd[2..].to_vec();

        assert_eq!(
            cmd,
            vec!["-a", "-b", "1", "-c", "3", "-d", "-e", "2", "-f", "4"]
        );
    }

    #[test]
    fn test_build_command_with_empty_array() {
        let base_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/cwl");
        let tool_path = base_dir.join("tests/empty-array-input.cwl");
        let doc = load_cwl_file(tool_path, true).unwrap();

        let inputs = include_str!("../../testdata/cwl/tests/empty-array-job.json");
        let input_values = serde_yaml::from_str(inputs).unwrap();
        let inputs = collect_inputs(&doc, &input_values).unwrap();

        let CWLDocument::CommandLineTool(tool) = doc else {
            panic!()
        };

        let mut cmd = build_command(&tool, &inputs, &Runtime::default(), None).unwrap();
        cmd = cmd[2..].to_vec();

        assert_eq!(cmd, Vec::<String>::new());
    }

    #[test]
    fn test_build_command_with_optional_missing() {
        let base_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/cwl");
        let tool_path = base_dir.join("tests/cat1-testcli.cwl");
        let doc = load_cwl_file(tool_path, true).unwrap();

        let inputs = include_str!("../../testdata/cwl/tests/cat-job.json");
        let input_values = serde_yaml::from_str(inputs).unwrap();
        let inputs = collect_inputs(&doc, &input_values).unwrap();

        let CWLDocument::CommandLineTool(tool) = doc else {
            panic!()
        };

        let mut cmd = build_command(&tool, &inputs, &Runtime::default(), None).unwrap();
        cmd = cmd[2..].to_vec();

        assert_eq!(cmd, vec!["cat", "hello.txt"]);
    }

    #[test]
    fn test_build_command_with_empty_binding() {
        let base_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/cwl");
        let tool_path = base_dir.join("tests/bool-empty-inputbinding.cwl");
        let doc = load_cwl_file(tool_path, true).unwrap();

        let inputs = include_str!("../../testdata/cwl/tests/bool-empty-inputbinding-job.json");
        let input_values = serde_yaml::from_str(inputs).unwrap();
        let inputs = collect_inputs(&doc, &input_values).unwrap();

        let CWLDocument::CommandLineTool(tool) = doc else {
            panic!()
        };

        let mut cmd = build_command(&tool, &inputs, &Runtime::default(), None).unwrap();
        cmd = cmd[2..].to_vec();

        assert_eq!(cmd, Vec::<String>::new());
    }

    #[test]
    fn test_generate_arg_without_prefix() {
        let b = CommandLineBinding::builder().build(); //all none
        let v = DefaultValue::Any(serde_yaml::Value::String("foo".into()));

        let res = generate_arg(&b, v, &Runtime::default(), None, None).unwrap();
        assert_eq!(res, vec!["foo"]);
    }

    #[test]
    fn test_generate_arg_with_prefix_separate() {
        let b = CommandLineBinding::builder()
            .prefix("--opt")
            .separate(true)
            .build();
        let v = DefaultValue::Any(serde_yaml::Value::String("foo".into()));

        let res = generate_arg(&b, v, &Runtime::default(), None, None).unwrap();
        assert_eq!(res, vec!["--opt", "foo"]);
    }

    #[test]
    fn test_generate_arg_with_prefix_not_separate() {
        let b = CommandLineBinding::builder()
            .prefix("--opt=")
            .separate(false)
            .build();
        let v = DefaultValue::Any(serde_yaml::Value::String("foo".into()));

        let res = generate_arg(&b, v, &Runtime::default(), None, None).unwrap();
        assert_eq!(res, vec!["--opt=foo"]);
    }

    #[test]
    fn test_generate_arg_sequence_with_separator() {
        let b = CommandLineBinding::builder()
            .prefix("--list")
            .item_separator(",")
            .build();
        let v = DefaultValue::Any(serde_yaml::Value::Sequence(vec![
            serde_yaml::Value::String("a".into()),
            serde_yaml::Value::String("b".into()),
            serde_yaml::Value::String("c".into()),
        ]));

        let res = generate_arg(&b, v, &Runtime::default(), None, None).unwrap();
        assert_eq!(res, vec!["--list", "a,b,c"]);
    }

    #[test]
    fn test_generate_arg_sequence_without_separator() {
        let b = CommandLineBinding::builder().prefix("--list").build();
        let v = DefaultValue::Any(serde_yaml::Value::Sequence(vec![
            serde_yaml::Value::String("a".into()),
            serde_yaml::Value::String("b".into()),
            serde_yaml::Value::String("c".into()),
        ]));

        let res = generate_arg(&b, v, &Runtime::default(), None, None).unwrap();
        assert_eq!(res, vec!["--list"]); //values need to be added recursively respecting the CommandLineBinding of their input schema
    }

    #[test]
    fn test_generate_arg_sequence_without_separator_recursively() {
        let i = CommandInputParameter::builder()
            .id("value")
            .r#type(CommandInputSchema::Array(
                CommandInputArraySchema::builder()
                    .items(OneOrMany::One(CWLType::String.into()))
                    .input_binding(CommandLineBinding::builder().prefix("-X").build())
                    .build(),
            ))
            .input_binding(CommandLineBinding::builder().prefix("-Y").build())
            .build();

        let schema = match &i.r#type {
            CommandInputParameterType::CommandInputType(OneOrMany::One(
                CommandInputType::CommandInputSchema(schema),
            )) => schema.as_ref(),
            _ => panic!("Expected CommandInputSchema type"),
        };

        if let CommandInputSchema::Array(array_schema) = schema {
            //array schema recurse!
            let b = i.input_binding.unwrap();
            let v = DefaultValue::Any(serde_yaml::Value::Sequence(vec![
                serde_yaml::Value::String("a".into()),
                serde_yaml::Value::String("b".into()),
                serde_yaml::Value::String("c".into()),
            ]));
            let mut res = generate_arg(&b, v.clone(), &Runtime::default(), None, None).unwrap(); //generates only -Y
            if let Some(inner_b) = &array_schema.input_binding {
                if let Some(serde_yaml::Value::Sequence(vec)) = v.try_get_value_ref() {
                    for inner_v in vec {
                        //re-serde
                        let v: DefaultValue = serde_yaml::from_value(inner_v.clone()).unwrap();
                        res.extend(
                            generate_arg(inner_b, v, &Runtime::default(), None, None).unwrap(),
                        );
                    }
                }
            } else {
                unreachable!()
            }
            assert_eq!(res, vec!["-Y", "-X", "a", "-X", "b", "-X", "c"]); //values need to be added recursively respecting the CommandLineBinding of their input schema
        } else {
            unreachable!()
        }
    }

    #[test]
    fn test_get_shell_command() {
        let cmd = get_shell_command();
        #[cfg(windows)]
        {
            assert_eq!(cmd, vec!["cmd".to_string(), "/C".to_string()])
        }
        #[cfg(unix)]
        {
            assert_eq!(cmd, vec!["sh".to_string(), "-c".to_string()])
        }
    }

    #[test]
    fn test_apply_shell_quote() {
        let args = vec!["hello world".to_string()];
        let res = apply_shell_quote(args);
        assert_eq!(res, vec!["'hello world'".to_string()])
    }
}
