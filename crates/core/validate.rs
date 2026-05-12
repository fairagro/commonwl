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
