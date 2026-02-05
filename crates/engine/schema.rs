use crate::requirements::ProcessRequirements;
use cwl_core::{
    OneOrMany,
    documents::CWLDocument,
    inputs::{
        CommandInputParameter, CommandInputParameterType, CommandInputSchema, CommandInputType,
        InputSchema, InputType, WorkflowInputParameter,
    },
    outputs::{
        CommandOutputParameter, CommandOutputParameterType, CommandOutputType,
        ExpressionToolOutputParameter, OutputType, WorkflowOutputParameter,
    },
};
use std::collections::HashMap;

pub fn replace_schema_definitions(
    doc: &mut CWLDocument,
    requirements: &[ProcessRequirements],
) -> anyhow::Result<()> {
    let schema_defs = if let Some(sdr) = requirements
        .iter()
        .map(|i| {
            if let ProcessRequirements::SchemaDefRequirement(r) = i {
                Some(r)
            } else {
                None
            }
        })
        .next()
        .flatten()
    {
        Some(get_schema_definitions(&sdr.types)?)
    } else {
        None
    };

    //inputs are changed recursively... do we need that for outputs too??
    if let Some(defs) = schema_defs {
        match doc {
            CWLDocument::CommandLineTool(clt) => {
                add_schema_defs_to_command_inputs(&mut clt.inputs, &defs)?;
                add_schema_defs_to_command_outputs(&mut clt.outputs, &defs)?;
            }
            CWLDocument::ExpressionTool(et) => {
                add_schema_defs_to_inputs(&mut et.inputs, &defs)?;
                add_schema_defs_to_expression_outputs(&mut et.outputs, &defs)?;
            }
            CWLDocument::Workflow(wf) => {
                add_schema_defs_to_inputs(&mut wf.inputs, &defs)?;
                add_schema_defs_to_outputs(&mut wf.outputs, &defs)?;
            }
            _ => {}
        }
    }

    Ok(())
}

fn add_schema_defs_to_command_inputs(
    inputs: &mut Vec<CommandInputParameter>,
    defs: &HashMap<String, serde_yaml::Value>,
) -> anyhow::Result<()> {
    for input in inputs {
        match &mut input.r#type {
            CommandInputParameterType::CommandInputType(OneOrMany::One(ty)) => {
                add_schema_defs_to_command_inputs_impl(ty, defs)?
            }
            CommandInputParameterType::CommandInputType(OneOrMany::Many(tys)) => {
                for ty in tys {
                    add_schema_defs_to_command_inputs_impl(ty, defs)?
                }
            }
            _ => {}
        }
    }
    Ok(())
}

fn add_schema_defs_to_command_inputs_impl(
    r#type: &mut CommandInputType,
    defs: &HashMap<String, serde_yaml::Value>,
) -> anyhow::Result<()> {
    match r#type {
        CommandInputType::CommandInputSchema(schema) => match &mut **schema {
            CommandInputSchema::Record(rec) => {
                if let Some(fields) = &mut rec.fields {
                    for field in fields {
                        match &mut field.r#type {
                            OneOrMany::One(item) => {
                                add_schema_defs_to_command_inputs_impl(item, defs)?;
                            }
                            OneOrMany::Many(items) => {
                                for item in items {
                                    add_schema_defs_to_command_inputs_impl(item, defs)?;
                                }
                            }
                        }
                    }
                }
            }
            CommandInputSchema::Array(arr) => match &mut arr.items {
                OneOrMany::One(item) => {
                    add_schema_defs_to_command_inputs_impl(item, defs)?;
                }
                OneOrMany::Many(items) => {
                    for item in items {
                        add_schema_defs_to_command_inputs_impl(item, defs)?;
                    }
                }
            },
            _ => {}
        },
        CommandInputType::String(s) => {
            if let Some(def) = defs.get(&format!("#{s}")) {
                let new_type: CommandInputType = serde_yaml::from_value(def.clone())?;
                *r#type = new_type;
            } else if s.starts_with("#")
                && let Some(def) = defs.get(s)
            {
                let new_type: CommandInputType = serde_yaml::from_value(def.clone())?;
                *r#type = new_type;
            } else if s.contains('#')
                && let Some(ar) = s.split_once('#')
                && let Some(def) = defs.get(&format!("#{}", ar.1))
            {
                let new_type: CommandInputType = serde_yaml::from_value(def.clone())?;
                *r#type = new_type;
            }
        }
        _ => {}
    }

    Ok(())
}

fn add_schema_defs_to_inputs(
    inputs: &mut Vec<WorkflowInputParameter>,
    defs: &HashMap<String, serde_yaml::Value>,
) -> anyhow::Result<()> {
    for input in inputs {
        match &mut input.r#type {
            OneOrMany::One(ty) => add_schema_defs_to_inputs_impl(ty, defs)?,
            OneOrMany::Many(tys) => {
                for ty in tys {
                    add_schema_defs_to_inputs_impl(ty, defs)?
                }
            }
        }
    }
    Ok(())
}

fn add_schema_defs_to_inputs_impl(
    r#type: &mut InputType,
    defs: &HashMap<String, serde_yaml::Value>,
) -> anyhow::Result<()> {
    match r#type {
        InputType::InputSchema(schema) => match &mut **schema {
            InputSchema::Record(rec) => {
                if let Some(fields) = &mut rec.fields {
                    for field in fields {
                        match &mut field.r#type {
                            OneOrMany::One(item) => {
                                add_schema_defs_to_inputs_impl(item, defs)?;
                            }
                            OneOrMany::Many(items) => {
                                for item in items {
                                    add_schema_defs_to_inputs_impl(item, defs)?;
                                }
                            }
                        }
                    }
                }
            }
            InputSchema::Array(arr) => match &mut arr.items {
                OneOrMany::One(item) => {
                    add_schema_defs_to_inputs_impl(item, defs)?;
                }
                OneOrMany::Many(items) => {
                    for item in items {
                        add_schema_defs_to_inputs_impl(item, defs)?;
                    }
                }
            },
            _ => {}
        },
        InputType::String(s) => {
            if let Some(def) = defs.get(&format!("#{s}")) {
                let new_type: InputType = serde_yaml::from_value(def.clone())?;
                *r#type = new_type;
            } else if s.starts_with("#")
                && let Some(def) = defs.get(s)
            {
                let new_type: InputType = serde_yaml::from_value(def.clone())?;
                *r#type = new_type;
            }
        }
        _ => {}
    }

    Ok(())
}

fn add_schema_defs_to_command_outputs(
    inputs: &mut Vec<CommandOutputParameter>,
    defs: &HashMap<String, serde_yaml::Value>,
) -> anyhow::Result<()> {
    for input in inputs {
        if let CommandOutputParameterType::CommandOutputType(OneOrMany::One(
            CommandOutputType::String(s),
        )) = &mut input.r#type
            && let Some(def) = defs.get(&format!("#{s}"))
        {
            let new_type: CommandOutputParameterType = serde_yaml::from_value(def.clone())?;
            input.r#type = new_type;
        }
    }
    Ok(())
}

fn add_schema_defs_to_expression_outputs(
    inputs: &mut Vec<ExpressionToolOutputParameter>,
    defs: &HashMap<String, serde_yaml::Value>,
) -> anyhow::Result<()> {
    for input in inputs {
        if let OneOrMany::One(OutputType::String(s)) = &mut input.r#type
            && let Some(def) = defs.get(&format!("#{s}"))
        {
            let new_type: OneOrMany<OutputType> = serde_yaml::from_value(def.clone())?;
            input.r#type = new_type;
        }
    }
    Ok(())
}

fn add_schema_defs_to_outputs(
    inputs: &mut Vec<WorkflowOutputParameter>,
    defs: &HashMap<String, serde_yaml::Value>,
) -> anyhow::Result<()> {
    for input in inputs {
        if let CommandOutputParameterType::CommandOutputType(OneOrMany::One(
            CommandOutputType::String(s),
        )) = &mut input.r#type
            && let Some(def) = defs.get(&format!("#{s}"))
        {
            let new_type: CommandOutputParameterType = serde_yaml::from_value(def.clone())?;
            input.r#type = new_type;
        }
    }
    Ok(())
}

pub fn get_schema_definitions(
    value: &serde_yaml::Value,
) -> anyhow::Result<HashMap<String, serde_yaml::Value>> {
    let mut defs = extract_schema_definitions(value)?;

    if !defs.is_empty() {
        //replace recursive definitions
        let snapshot = &defs.clone();
        for item in defs.values_mut() {
            replace_schema_references(item, snapshot)?;
        }
    }
    Ok(defs)
}

fn extract_schema_definitions(
    value: &serde_yaml::Value,
) -> anyhow::Result<HashMap<String, serde_yaml::Value>> {
    let mut schemas = HashMap::new();

    if let serde_yaml::Value::Sequence(types) = value {
        for type_def in types {
            if let serde_yaml::Value::Mapping(type_map) = type_def
                && let Some(serde_yaml::Value::String(name)) =
                    type_map.get(serde_yaml::Value::String("name".to_string()))
            {
                schemas.insert(format!("#{}", name), type_def.clone());
            }
        }
    }

    Ok(schemas)
}

fn replace_schema_references(
    value: &mut serde_yaml::Value,
    schemas: &HashMap<String, serde_yaml::Value>,
) -> anyhow::Result<()> {
    match value {
        serde_yaml::Value::String(s) => {
            if s.starts_with('#') && schemas.contains_key(s) {
                *value = schemas[s].clone();
            } else if s.contains('#')
                && let Some(ar) = s.split_once('#')
            {
                //by doing this we accept that each id can be given only once!
                let s = format!("#{}", ar.1);
                if schemas.contains_key(&s) {
                    *value = schemas[&s].clone();
                }
            }
        }
        serde_yaml::Value::Sequence(arr) => {
            for item in arr {
                replace_schema_references(item, schemas)?;
            }
        }
        serde_yaml::Value::Mapping(map) => {
            for v in map.values_mut() {
                replace_schema_references(v, schemas)?;
            }
        }
        _ => {}
    }
    Ok(())
}
