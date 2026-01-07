use cwl_core::{
    IntegerOrExpression, OneOrMany,
    documents::{Argument, CommandLineTool},
    inputs::{CommandLineBinding, DefaultValue},
    requirements::ShellCommandRequirement,
    value_as_string,
};
use serde_yaml::Value;
use std::{borrow::Cow, collections::HashMap, vec};

#[derive(Debug, Clone)]
struct BoundBinding {
    sort_key: Vec<SortKey>,
    binding: CommandLineBinding,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum SortKey {
    Int(i32),
    Str(String),
}

pub(super) fn build_command(
    tool: &CommandLineTool,
    inputs: &HashMap<String, serde_yaml::Value>,
) -> anyhow::Result<Vec<String>> {
    let mut args: Vec<String> = vec![];

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
                    bindings.push(BoundBinding { sort_key, binding });
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
                    });
                }
            }
        }
    }

    //handle inputs
    let mut values = HashMap::new();
    for input in &tool.inputs {
        if let Some(binding) = &input.input_binding {
            let position = binding.position.clone().map(|p| match p {
                IntegerOrExpression::Int(i) => i,
                IntegerOrExpression::Long(l) => i32::try_from(l).unwrap_or_default(),
                IntegerOrExpression::Expression(_s) => todo!(), //evaluate expression
            });
            let binding = binding.clone();
            let sort_key = vec![
                SortKey::Int(position.unwrap_or_default()),
                SortKey::Str(input.id.clone().unwrap_or_default()),
            ];

            //TODO: Handle Value from
            let value = inputs.get(&input.id.clone().unwrap_or_default());
            // we got an actual input value
            let binding_value = if let Some(value) = value
                && !value.is_null()
            {
                serde_yaml::from_value::<DefaultValue>(value.clone())?
            } else if let Some(default) = &input.default {
                default.clone()
            } else {
                DefaultValue::Any(Value::Null)
            };
            values.insert(input.id.clone().unwrap_or_default(), binding_value.clone());

            bindings.push(BoundBinding { sort_key, binding });
        }
    }

    //do sort
    bindings.sort_by(|a, b| a.sort_key.cmp(&b.sort_key));

    //add bindings
    if tool.has_requirement_or_hint::<ShellCommandRequirement>() {
        let mut cmd = vec![];
        for binding in bindings {
            if let SortKey::Str(input_id) = &binding.sort_key[1] {
                let mut arg = generate_arg(&binding.binding, values[input_id].clone())?;
                if binding.binding.shell_quote.unwrap_or(true) {
                    arg = apply_shell_quote(arg);
                }
                cmd.extend(arg);
            } else {
                //we have an "Argument" here
                let mut arg = use_value_from(&binding.binding);
                if binding.binding.shell_quote.unwrap_or(true) {
                    arg = apply_shell_quote(arg);
                }
                cmd.extend(arg);
            }
        }
        let cmdline = cmd.join(" ");
        args.extend(get_shell_command());
        args.push(cmdline);
    } else {
        for binding in bindings {
            if let SortKey::Str(input_id) = &binding.sort_key[1] {
                let arg = generate_arg(&binding.binding, values[input_id].clone())?;
                args.extend(arg);
            } else {
                //we have an "Argument" here
                args.extend(use_value_from(&binding.binding));
            }
        }
    }
    //remove empty args
    args.retain(|s| !s.is_empty());

    //append stdin i guess?
    if let Some(stdin) = &tool.stdin {
        args.push(stdin.clone());
    }

    Ok(args)
}

fn generate_arg(binding: &CommandLineBinding, input: DefaultValue) -> anyhow::Result<Vec<String>> {
    let sep = binding.separate.unwrap_or(true);

    if binding.prefix.is_none() && !sep {
        anyhow::bail!("If 'separate' is false, 'prefix' must be set.");
    }

    let mut argl = vec![];

    match input {
        DefaultValue::Any(value) => match value {
            Value::Sequence(arr) => {
                if let Some(separator) = &binding.item_separator {
                    argl = vec![DefaultValue::Any(Value::String(
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
            _ => argl = vec![DefaultValue::Any(value)],
        },
        DefaultValue::FileOrDirectory(fd) => argl = vec![DefaultValue::FileOrDirectory(fd)],
    }

    Ok(argl
        .into_iter()
        .flat_map(|j| {
            if let DefaultValue::Any(Value::Null) = j {
                vec![]
            } else {
                let s = j.to_string();
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

fn use_value_from(binding: &CommandLineBinding) -> Vec<String> {
    if let Some(p) = &binding.prefix
        && let Some(v) = &binding.value_from
    {
        vec![p.clone(), v.clone()]
    } else if let Some(v) = &binding.value_from {
        vec![v.clone()]
    } else {
        vec![]
    }
}

fn apply_shell_quote(arg: Vec<String>) -> Vec<String> {
    arg.iter()
        .map(|a| shlex::try_quote(a).unwrap_or(Cow::Borrowed(a)).to_string())
        .collect()
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

#[cfg(test)]
mod tests {
    use super::*;

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
        let tool = &serde_yaml::from_str(yaml).unwrap();

        let inputs = r#"{
    "file1": {
        "class": "File",
        "path": "hello.txt"
    }
}"#;

        let input_values = serde_yaml::from_str(inputs).unwrap();
        let cmd = build_command(tool, &input_values).unwrap();
        let cmdline = cmd.join(" ");
        assert_eq!(cmdline, "cat hello.txt");
    }

    #[test]
    fn test_build_command_stdin() {
        let yaml = r"
class: CommandLineTool
cwlVersion: v1.2
inputs: []
outputs: []
baseCommand: [cat]
stdin: hello.txt";
        let tool = &serde_yaml::from_str(yaml).unwrap();

        let cmd = build_command(tool, &HashMap::new()).unwrap();
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
        let tool = &serde_yaml::from_str(yaml).unwrap();

        let input_values = serde_yaml::from_str(inputs).unwrap();
        let cmd = build_command(tool, &input_values).unwrap();

        let shell_cmd = get_shell_command();

        assert_eq!(
            cmd,
            vec![
                &shell_cmd[0],
                &shell_cmd[1],
                "cd '$(inputs.indir.path)' && find . | sort"
            ]
        );
    }
}
