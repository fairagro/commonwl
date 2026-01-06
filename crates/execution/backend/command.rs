use std::collections::HashMap;

use cwl_core::{
    IntegerOrExpression, OneOrMany,
    documents::{Argument, CommandLineTool},
    inputs::CommandLineBinding, requirements::ToolRequirements,
};

#[derive(Debug, Clone)]
struct BoundBinding {
    sort_key: Vec<SortKey>,
    command: CommandLineBinding,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum SortKey {
    Int(i32),
    Str(String),
}

pub(super) fn build_command(
    tool: &CommandLineTool,
    _inputs: HashMap<String, serde_yaml::Value>,
) -> anyhow::Result<Vec<String>> {
    let Some(base_command) = &tool.base_command else {
        return Ok(vec![]); //do we return empty or do we throw?    
    };
    let mut args: Vec<String> = vec![];

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
                        command: binding,
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
                        command: binding.clone(),
                    });
                }
            }
        }
    }

    //handle inputs
    //TODO!

    
    //do sort
    bindings.sort_by(|a, b| a.sort_key.cmp(&b.sort_key));

    //add bindings
    for binding in bindings.iter().map(|b| &b.command) {
        if let Some(prefix) = &binding.prefix {
            args.push(prefix.to_string());
        }
        if let Some(value) = &binding.value_from {
            //TODO: proper handling instead of unwrapping
            if tool.requirements.clone().unwrap().iter().any(|req| matches!(req, ToolRequirements::ShellCommandRequirement(_))) {
                if let Some(shellquote) = binding.shell_quote {
                    if shellquote {
                        args.push(format!("\"{value}\""));
                    } else {
                        args.push(value.to_string());
                    }
                } else {
                    args.push(value.to_string());
                }
            } else {
                args.push(value.to_string());
            }
        }
    }

    //remove empty args
    args.retain(|s| !s.is_empty());

    Ok(args)
}
