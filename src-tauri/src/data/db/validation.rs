//! Input normalization and validation helpers shared across the db submodules.

use anyhow::Result;

pub const SNIPPET_TRIGGER_CHAR_LIMIT: usize = 300;
pub const DICTIONARY_ENTRY_CHAR_LIMIT: usize = 120;

pub fn require_nonempty_trimmed(field: &str, value: &str) -> Result<String> {
    let normalized = value.trim();
    if normalized.is_empty() {
        return Err(anyhow::anyhow!("{field} cannot be empty"));
    }
    Ok(normalized.to_string())
}

pub fn normalize_optional_trimmed(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

pub fn normalize_multiline(value: &str) -> String {
    value
        .replace("\r\n", "\n")
        .replace('\r', "\n")
        .trim()
        .to_string()
}

pub fn validate_char_limit(field: &str, value: &str, limit: usize) -> Result<()> {
    if value.chars().count() > limit {
        return Err(anyhow::anyhow!(
            "{field} must be {limit} characters or fewer"
        ));
    }
    Ok(())
}

pub fn require_row_changed(changed: usize, item: &str, id: i64) -> Result<()> {
    if changed == 0 {
        return Err(anyhow::anyhow!("{item} {id} was not found"));
    }
    Ok(())
}
