use crate::expression::EvaluationContext;
use cwl_core::types::SecondaryFileSchema;
use std::path::{Path, PathBuf};

pub fn handle_secondary_file_schema(
    path: impl AsRef<Path>,
    item: &SecondaryFileSchema,
    _context: &EvaluationContext,
) -> PathBuf {
    let mut secondary_path_str = path.as_ref().as_os_str().to_owned();
    secondary_path_str.push(&item.pattern);
    PathBuf::from(secondary_path_str)
}
