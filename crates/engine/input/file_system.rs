use crate::{
    checksum, expression::EvaluationContext, pathmapper::PathMapper,
    secondary_files::collect_secondary_files_for_inputs,
};
use cwl_core::{
    FileMetaData, FilePathMetaData, Integer,
    documents::CWLDocument,
    files::{File, FileOrDirectory},
    get_file_metadata, get_path_metadata,
    inputs::{DefaultValue, OperationInputParameter},
};
use dircpy::copy_dir;
use std::{collections::HashMap, fs, path::Path};

pub fn create_flattened_inputs(
    inputs: &mut HashMap<String, DefaultValue>,
    doc: &CWLDocument,
    eval_context: &EvaluationContext,
    path_mapper: &mut PathMapper,
    working_dir: &Path,
    tmp_dir: &Path,
) -> anyhow::Result<Vec<FileOrDirectory>> {
    collect_secondary_files_for_inputs(doc, inputs, eval_context, path_mapper)?;

    //handle synthethic directories
    let mut flattened_inputs = flatten_inputs(inputs.values());
    handle_synthetic_directories(&mut flattened_inputs, path_mapper, working_dir, tmp_dir)?;
    Ok(flattened_inputs)
}

//flattens inputs of any type to a list of file or directory
fn flatten_inputs<'a, I: Iterator<Item = &'a DefaultValue>>(inputs: I) -> Vec<FileOrDirectory> {
    let mut flattened = vec![];
    for input in inputs {
        flatten_inputs_impl(input, &mut flattened);
    }
    flattened
}

fn flatten_inputs_impl(dv: &DefaultValue, flattened: &mut Vec<FileOrDirectory>) {
    match dv {
        DefaultValue::FileOrDirectory(fod) => {
            flattened.push(fod.clone());
            if let FileOrDirectory::File(f) = fod
                && let Some(secondary_files) = &f.secondary_files
            {
                flattened.extend(secondary_files.clone());
            }
        }
        DefaultValue::Any(v) => match v {
            serde_yaml::Value::Sequence(values) => {
                for v in values {
                    if let Ok(dv) = serde_yaml::from_value(v.clone()) {
                        flatten_inputs_impl(&dv, flattened);
                    }
                }
            }
            serde_yaml::Value::Mapping(mapping) => {
                for v in mapping.values() {
                    if let Ok(dv) = serde_yaml::from_value(v.clone()) {
                        flatten_inputs_impl(&dv, flattened);
                    }
                }
            }
            _ => {}
        },
    }
}

pub fn fill_input_metadata(
    inputs: &HashMap<String, DefaultValue>,
    doc: &CWLDocument,
    path_mapper: &PathMapper,
) -> anyhow::Result<HashMap<String, DefaultValue>> {
    let mut map = HashMap::new();
    let providers = doc.get_inputs();

    for (key, value) in inputs {
        let input = providers
            .iter()
            .find(|i| i.id == Some(key.to_string()))
            .unwrap();
        let value = create_metadata_for_input(value, input, path_mapper)?;
        map.insert(key.clone(), value);
    }

    Ok(map)
}

fn create_metadata_for_input(
    value: &DefaultValue,
    input: &OperationInputParameter,
    path_mapper: &PathMapper,
) -> anyhow::Result<DefaultValue> {
    match value {
        DefaultValue::FileOrDirectory(FileOrDirectory::File(f)) if f.path.is_some() => {
            let path = f.path.clone().unwrap();

            let path = Path::new(&path);
            let guest_path = path_mapper.get_guest(path).unwrap();
            let host_path = path_mapper.get_host(guest_path).unwrap();
            let FilePathMetaData {
                basename,
                nameroot,
                nameext,
                dirname,
            } = get_path_metadata(host_path);
            let FileMetaData { size, checksum } = get_file_metadata(host_path)?;

            Ok(DefaultValue::FileOrDirectory(FileOrDirectory::File(
                File::builder()
                    .path(host_path.to_string_lossy())
                    .maybe_basename(basename)
                    .maybe_nameroot(nameroot)
                    .maybe_nameext(nameext)
                    .maybe_dirname(dirname)
                    .maybe_checksum(checksum)
                    .size(Integer::Long(size as i64))
                    .maybe_format(f.format.clone())
                    .build(),
            )))
        }
        DefaultValue::FileOrDirectory(FileOrDirectory::Directory(d)) if d.path.is_some() => {
            let path = d.path.clone().unwrap();
            let path = Path::new(&path);
            let guest_path = path_mapper.get_guest(path).unwrap();
            let host_path = path_mapper.get_host(guest_path).unwrap();
            let mut d = d.clone();
            d.path = Some(host_path.to_string_lossy().to_string());
            if let Some(load_listing) = input.load_listing {
                d.load_listing(load_listing)?;
            }

            if let Some(listing) = &mut d.listing {
                for item in listing {
                    item.dry_validation();
                    if let DefaultValue::FileOrDirectory(fod) = create_metadata_for_input(
                        &DefaultValue::FileOrDirectory(item.clone()),
                        input,
                        path_mapper,
                    )? {
                        *item = fod;
                    }
                }
            }

            Ok(DefaultValue::FileOrDirectory(FileOrDirectory::Directory(d)))
        }
        DefaultValue::FileOrDirectory(FileOrDirectory::Directory(d)) if d.listing.is_some() => {
            let mut d = d.clone();
            if let Some(listing) = &mut d.listing {
                for item in listing {
                    item.dry_validation();
                    if let DefaultValue::FileOrDirectory(fod) = create_metadata_for_input(
                        &DefaultValue::FileOrDirectory(item.clone()),
                        input,
                        path_mapper,
                    )? {
                        *item = fod;
                    }
                }
            }
            Ok(DefaultValue::FileOrDirectory(FileOrDirectory::Directory(d)))
        }
        DefaultValue::FileOrDirectory(FileOrDirectory::File(file)) if file.contents.is_some() => {
            let contents = file.contents.clone().unwrap();
            let mut f = file.clone();
            f.checksum = Some(checksum(&contents));
            Ok(DefaultValue::FileOrDirectory(FileOrDirectory::File(f)))
        }
        DefaultValue::Any(serde_yaml::Value::Sequence(vec)) => {
            let mut items = vec![];
            for item in vec {
                let dv = serde_yaml::from_value(item.clone())?;
                items.push(create_metadata_for_input(&dv, input, path_mapper)?);
            }
            let value = serde_yaml::to_value(&items)?;
            Ok(DefaultValue::Any(value))
        }
        //TODO: records
        default => Ok(default.clone()),
    }
}

/// Creates the synthetic directory and adds it to the pathmapper
fn handle_synthetic_directories(
    flattened_inputs: &mut Vec<FileOrDirectory>,
    path_mapper: &mut PathMapper,
    work_dir: &Path,
    tmpdir: &Path,
) -> anyhow::Result<()> {
    for mut input in flattened_inputs {
        input.dry_validation();
        let mut path = input.path().cloned();

        if path.is_none()
            && let FileOrDirectory::Directory(dir) = &mut input
            && let Some(listing) = &mut dir.listing
            && let Some(basename) = &dir.basename
        {
            //create from listing
            let host_path = tmpdir.join(basename);
            fs::create_dir(&host_path)?;

            let base_path = Path::new(basename);

            //fix path
            let host_path_str = host_path.to_string_lossy().into_owned();
            path = Some(host_path_str);
            dir.path = path;

            for item in listing {
                item.dry_validation();

                if let Some(c_path) = item.path() {
                    let c_host_path = host_path.join(c_path);
                    let staged_path = path_mapper.predict_staged_path(base_path.join(c_path));

                    path_mapper.add_tripel(&c_host_path, staged_path, c_path)?;

                    let source_path = work_dir.join(c_path);
                    //copy into tmpdir
                    match item {
                        FileOrDirectory::File(_) => {
                            fs::copy(&source_path, &c_host_path)?;
                        }
                        FileOrDirectory::Directory(_) => copy_dir(&source_path, &c_host_path)?,
                    }
                } else if let FileOrDirectory::File(file) = item
                    && let Some(c_contents) = &file.contents
                {
                    //write file literal if part of dir
                    let filename = if let Some(basename) = &file.basename {
                        basename
                    } else {
                        &checksum(c_contents)
                    };

                    file.path = Some(base_path.join(filename).to_string_lossy().into_owned());

                    let c_host_path = host_path.join(filename);
                    let staged_path = path_mapper.predict_staged_path(base_path.join(filename));
                    path_mapper.add_tripel(&c_host_path, &staged_path, filename)?;
                    fs::write(c_host_path, c_contents)?;
                }
            }

            let staged_path = path_mapper.predict_staged_path(basename);
            path_mapper.add_tripel(&host_path, staged_path, basename)?;
        }
    }

    Ok(())
}

///adds the synthetic dirs we now created in flatten_inputs to the staged_inputs item
///We could not do this before because we needed the evaluation context in place to do_eval
pub fn add_synthethic_paths(
    mut staged_inputs: HashMap<String, DefaultValue>,
    path_mapper: &PathMapper,
) -> HashMap<String, DefaultValue> {
    for item in staged_inputs.values_mut() {
        lock_item(item, path_mapper);
    }
    staged_inputs
}

pub fn lock_item(item: &mut DefaultValue, path_mapper: &PathMapper) {
    match item {
        DefaultValue::FileOrDirectory(fod) => lock_fod(fod, path_mapper),
        DefaultValue::Any(serde_yaml::Value::Sequence(vec)) => {
            for item in vec {
                if let Ok(mut dv) = serde_yaml::from_value(item.clone()) {
                    lock_item(&mut dv, path_mapper);
                    if let Ok(updated) = serde_yaml::to_value(dv) {
                        *item = updated
                    }
                }
            }
        }
        DefaultValue::Any(serde_yaml::Value::Mapping(map)) => {
            for item in map.values_mut() {
                if let Ok(mut dv) = serde_yaml::from_value(item.clone()) {
                    lock_item(&mut dv, path_mapper);
                    if let Ok(updated) = serde_yaml::to_value(dv) {
                        *item = updated
                    }
                }
            }
        }
        _ => {}
    }
}

fn lock_fod(fod: &mut FileOrDirectory, path_mapper: &PathMapper) {
    if fod.path().is_none()
        && let Some(basename) = fod.basename()
        && let Some(guest) = path_mapper.get_guest(basename)
    {
        fod.set_path(
            path_mapper
                .get_host(guest)
                .map(|p| p.to_string_lossy().to_string()),
        );

        if let FileOrDirectory::Directory(dir) = fod
            && let Some(listing) = &mut dir.listing
        {
            for item in listing {
                lock_fod(item, path_mapper);
            }
        }
    }
}
