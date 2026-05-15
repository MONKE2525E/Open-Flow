use crate::data::db::{self, Db};

/// If the entire transcription is just a snippet trigger (ignoring trailing punctuation
/// added by the transcription model), return the expansion directly.
///
/// The transcription model always appends a period — "roblox" becomes "roblox." —
/// which `expand_snippets` would leave as an orphaned "." after replacing the trigger.
/// This function strips that punctuation before matching and returns the raw expansion
/// text, so no period bleeds through. Also increments the snippet's use_count.
///
pub fn try_pure_snippet_expand_from(
    text: &str,
    snippets: &[db::Snippet],
    db: &Db,
) -> Option<String> {
    let normalized = text
        .trim()
        .trim_end_matches(['.', ',', '?', '!'])
        .to_lowercase();
    let matched = snippets
        .iter()
        .find(|s| s.trigger.to_lowercase() == normalized)?;
    let _ = db::increment_snippet_use(db, matched.id);
    Some(matched.expansion.clone())
}

pub fn collect_snippet_instructions_from(text: &str, snippets: &[db::Snippet]) -> String {
    let text_lower = text.to_lowercase();
    let mut active_instructions: Vec<String> = Vec::new();

    for snippet in snippets.iter() {
        let needle = snippet.trigger.to_lowercase();
        if text_lower.contains(&needle) && !snippet.instructions.is_empty() {
            active_instructions.push(snippet.instructions.clone());
        }
    }

    if active_instructions.is_empty() {
        String::new()
    } else {
        active_instructions.join("\n")
    }
}

pub fn expand_snippets_from(text: &str, snippets: &mut [db::Snippet], db: &Db) -> String {
    // Longest triggers first — prevents short prefix matches shadowing longer ones.
    snippets.sort_by(|a, b| b.trigger.len().cmp(&a.trigger.len()));

    let mut result = text.to_string();

    for snippet in snippets.iter() {
        let needle = snippet.trigger.to_lowercase();
        let haystack = result.to_lowercase();

        // Find all non-overlapping occurrences (right-to-left to keep indices valid).
        let mut positions: Vec<usize> = Vec::new();
        let mut search_from = 0;
        while let Some(pos) = haystack[search_from..].find(&needle) {
            let abs = search_from + pos;
            positions.push(abs);
            search_from = abs + needle.len();
        }

        if positions.is_empty() {
            continue;
        }

        // Replace right-to-left so earlier indices stay valid.
        for pos in positions.into_iter().rev() {
            result.replace_range(pos..pos + needle.len(), &snippet.expansion);
        }

        let _ = db::increment_snippet_use(db, snippet.id);
    }

    result
}

/// Apply mechanical final-output constraints from matched snippet instructions.
/// These are intentionally narrow: they only cover hard formatting rules that
/// should be guaranteed even if the cleanup model misses the prompt override.
pub fn apply_cleanup_instruction_overrides(text: &str, instructions: &str) -> String {
    if instructions.trim().is_empty() {
        return text.to_string();
    }

    let mut result = text.to_string();
    let normalized = instructions.to_lowercase();

    if wants_all_caps(&normalized) {
        result = result.to_uppercase();
    }

    if wants_no_final_period(&normalized) {
        result = strip_final_periods(&result);
    }

    if wants_final_exclamation(&normalized) {
        result = ensure_final_exclamation(&result);
    }

    // Always collapse consecutive trailing periods — prevents "expansion." + LLM "." = ".."
    if !wants_final_exclamation(&normalized) && !wants_no_final_period(&normalized) {
        result = dedup_trailing_periods(&result);
    }

    result
}

fn wants_all_caps(instructions: &str) -> bool {
    (instructions.contains("all capital") || instructions.contains("all caps"))
        && !instructions.contains("do not use all capital")
        && !instructions.contains("don't use all capital")
        && !instructions.contains("never use all capital")
}

fn wants_no_final_period(instructions: &str) -> bool {
    let has_no_period = instructions.contains("no period")
        || instructions.contains("no periods")
        || instructions.contains("never add a period")
        || instructions.contains("do not add a period")
        || instructions.contains("don't add a period")
        || instructions.contains("don't end with a period")
        || instructions.contains("do not end with a period")
        || instructions.contains("never end with a period")
        || instructions.contains("don't use periods")
        || instructions.contains("do not use periods")
        || instructions.contains("never use periods")
        || instructions.contains("don't add periods")
        || instructions.contains("do not add periods")
        || instructions.contains("without a period")
        || instructions.contains("without periods")
        || instructions.contains("no trailing period")
        || instructions.contains("no final period")
        || instructions.contains("omit the period")
        || instructions.contains("remove the period")
        || instructions.contains("no punctuation at the end");
    has_no_period && !instructions.contains("always add a period")
}

fn wants_final_exclamation(instructions: &str) -> bool {
    (instructions.contains("always end with an exclamation")
        || instructions.contains("end with an exclamation")
        || instructions.contains("end with !")
        || instructions.contains("ends with !"))
        && !instructions.contains("do not end with an exclamation")
        && !instructions.contains("don't end with an exclamation")
        && !instructions.contains("never end with an exclamation")
}

fn strip_final_periods(text: &str) -> String {
    let trimmed_len = text.trim_end().len();
    let trailing_ws = &text[trimmed_len..];
    let mut body = text[..trimmed_len].to_string();

    while body.ends_with('.') {
        body.pop();
    }

    format!("{body}{trailing_ws}")
}

/// Remove duplicate consecutive trailing periods (e.g. ".." → ".").
/// Called unconditionally so a snippet expansion that already has a period
/// doesn't gain a second one from the LLM.
fn dedup_trailing_periods(text: &str) -> String {
    let trimmed_len = text.trim_end().len();
    let trailing_ws = &text[trimmed_len..];
    let mut body = text[..trimmed_len].to_string();

    // Pop extras, keep exactly one if multiple periods trail.
    let mut period_count = 0usize;
    while body.ends_with('.') {
        body.pop();
        period_count += 1;
    }
    if period_count > 0 {
        body.push('.');
    }

    format!("{body}{trailing_ws}")
}

fn ensure_final_exclamation(text: &str) -> String {
    let trimmed_len = text.trim_end().len();
    let trailing_ws = &text[trimmed_len..];
    let mut body = text[..trimmed_len].to_string();

    while matches!(body.chars().last(), Some('.') | Some('!') | Some('?')) {
        body.pop();
    }

    body.push('!');
    body.push_str(trailing_ws);
    body
}

#[cfg(test)]
mod tests {
    use super::apply_cleanup_instruction_overrides;

    #[test]
    fn uppercase_override_applies_to_entire_output() {
        let output =
            apply_cleanup_instruction_overrides("Hello from Open Flow.", "use all capital letters");

        assert_eq!(output, "HELLO FROM OPEN FLOW.");
    }

    #[test]
    fn no_period_override_removes_final_period() {
        let output = apply_cleanup_instruction_overrides("Please ship this.", "never add a period");

        assert_eq!(output, "Please ship this");
    }

    #[test]
    fn exclamation_override_forces_final_exclamation_mark() {
        let output = apply_cleanup_instruction_overrides(
            "Please ship this.",
            "always end with an exclamation mark",
        );

        assert_eq!(output, "Please ship this!");
    }

    #[test]
    fn compatible_overrides_apply_in_order() {
        let output = apply_cleanup_instruction_overrides(
            "Please ship this.",
            "use all capital letters\nnever add a period\nalways end with an exclamation mark",
        );

        assert_eq!(output, "PLEASE SHIP THIS!");
    }
}
