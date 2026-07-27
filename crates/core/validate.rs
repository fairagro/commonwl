use cwl_salad::Identifiable;
use regex::Regex;
use std::sync::LazyLock;
use validator::ValidationError;

pub static CWL_VERSION: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^v(\d+)\.(\d+)(?:\.(\d+)(?:-([0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*))?(?:\+([0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*))?)?$").unwrap()
});

/// Validate that the expression starts with $() or ${}
/// # Errors
/// Returns `ValidationError` if the expression does not start with $() or ${}
pub fn validate_expression(expr: &str) -> Result<(), ValidationError> {
    if !expr.trim_start().starts_with("$(") && !expr.trim_start().starts_with("${") {
        return Err(ValidationError::new("invalid_expression"));
    }
    Ok(())
}

/// Validate that no two items in a CWL `inputs`/`outputs`/`steps` list share the same `id`.
/// Per the CWL spec, identifiers must be unique within their scope.
/// # Errors
/// Returns `ValidationError` if two items have the same, non-empty `id`.
pub fn validate_unique_ids<T: Identifiable>(items: &[T]) -> Result<(), ValidationError> {
    let mut seen = std::collections::HashSet::new();
    for item in items {
        if let Some(id) = item.get_id()
            && !seen.insert(id)
        {
            return Err(ValidationError::new("duplicate_id"));
        }
    }
    Ok(())
}

/// Validate that a `requirements` list does not specify the same requirement class twice.
/// Per the CWL spec, it is an error to specify a value for a requirement class more than once.
/// # Errors
/// Returns `ValidationError` if two entries are of the same requirement variant.
pub fn validate_unique_requirements<T>(items: &[T]) -> Result<(), ValidationError> {
    for i in 0..items.len() {
        if items[i + 1..]
            .iter()
            .any(|other| std::mem::discriminant(&items[i]) == std::mem::discriminant(other))
        {
            return Err(ValidationError::new("duplicate_requirement"));
        }
    }
    Ok(())
}
