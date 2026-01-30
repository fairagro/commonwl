use crate::{
    expression::{EvaluationContext, do_eval},
    pathmapper::PathMapper,
};
use cwl_core::{
    BoolOrExpression, OneOrMany,
    documents::CWLDocument,
    files::{File, FileOrDirectory},
    inputs::DefaultValue,
    types::SecondaryFileSchema,
};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

pub fn handle_secondary_file_schema(
    path: impl AsRef<Path>,
    item: &SecondaryFileSchema,
    context: &EvaluationContext,
) -> anyhow::Result<Option<PathBuf>> {
    let pattern_value = if let Ok(pattern_value) = do_eval(&item.pattern, context) {
        pattern_value
    } else {
        serde_yaml::Value::String(item.pattern.clone())
    };
    let pattern = match pattern_value {
        serde_yaml::Value::String(s) => s,
        _ => return Ok(None),
    };

    let mut secondary_path_str = path.as_ref().as_os_str().to_owned();
    secondary_path_str.push(&pattern);

    //check required and existent
    let is_required = if let Some(BoolOrExpression::Expression(req_exp)) = &item.required {
        do_eval(req_exp, context)?.as_bool().unwrap_or(false)
    } else {
        matches!(&item.required, Some(BoolOrExpression::Bool(true)))
    };
    let secondary_path = Path::new(&secondary_path_str);
    if !secondary_path.exists() && !context.workdir.unwrap().join(secondary_path).exists() {
        if is_required {
            anyhow::bail!("required secondary file not found {pattern}");
        }
        return Ok(None);
    }

    Ok(Some(PathBuf::from(secondary_path_str)))
}

pub fn collect_secondary_files_for_inputs(
    doc: &CWLDocument,
    values: &mut HashMap<String, DefaultValue>,
    context: &EvaluationContext,
    path_mapper: &mut PathMapper,
) -> anyhow::Result<()> {
    //we can now handle the secondary files which are dependend on inputs...
    for input in &doc.get_inputs() {
        if let Some(secondary_files) = &input.secondary_files {
            let input_id = input.id.as_ref().unwrap();
            let value = values.get_mut(input_id).unwrap();
            handle_value(value, secondary_files, context, path_mapper)?;

            fn handle_value(
                value: &mut DefaultValue,
                secondary_files: &OneOrMany<SecondaryFileSchema>,
                context: &EvaluationContext,
                path_mapper: &mut PathMapper,
            ) -> anyhow::Result<()> {
                //we need to check all types that may contain files...
                match value {
                    DefaultValue::FileOrDirectory(FileOrDirectory::File(file)) => {
                        file.dry_validation();
                        if let Some(path) = &file.path {
                            let mut secondaries = vec![];
                            for item in secondary_files.as_many() {
                                if let Some(result) =
                                    handle_secondary_file_schema(path, &item, context)?
                                {
                                    let file =
                                        File::builder().path(result.to_string_lossy()).build();
                                    secondaries.push(FileOrDirectory::File(file));
                                    path_mapper.add(result)?;
                                }
                            }
                            file.secondary_files = Some(secondaries);
                        }
                    }
                    DefaultValue::Any(serde_yaml::Value::Sequence(arr)) => {
                        for item in arr {
                            if let Ok(mut dv) = serde_yaml::from_value::<DefaultValue>(item.clone())
                            {
                                handle_value(&mut dv, secondary_files, context, path_mapper)?;
                                *item = serde_yaml::to_value(&dv)?;
                            }
                        }
                    }
                    DefaultValue::Any(serde_yaml::Value::Mapping(map)) => {
                        for item in map.values_mut() {
                            if let Ok(mut dv) = serde_yaml::from_value::<DefaultValue>(item.clone())
                            {
                                handle_value(&mut dv, secondary_files, context, path_mapper)?;
                                *item = serde_yaml::to_value(&dv)?;
                            }
                        }
                    }
                    _ => {}
                }
                Ok(())
            }
        }
    }
    Ok(())
}
