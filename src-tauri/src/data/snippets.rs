use crate::data::db::{self, Db};
use std::cmp::Reverse;
use std::collections::HashMap;

#[derive(Clone)]
struct Match {
    start: usize,
    end: usize,
    snippet_idx: usize,
}

fn lowercase_with_source_map(input: &str) -> (String, Vec<usize>) {
    let mut lowered = String::with_capacity(input.len());
    let mut source_map = Vec::with_capacity(input.len());

    for (src_idx, ch) in input.char_indices() {
        for lower_ch in ch.to_lowercase() {
            let mut buf = [0_u8; 4];
            let encoded = lower_ch.encode_utf8(&mut buf);
            lowered.push_str(encoded);
            for _ in 0..encoded.len() {
                source_map.push(src_idx);
            }
        }
    }

    (lowered, source_map)
}

fn end_of_char_at(text: &str, start: usize) -> usize {
    text[start..]
        .chars()
        .next()
        .map(|ch| start + ch.len_utf8())
        .unwrap_or(text.len())
}

fn is_word_char(ch: char) -> bool {
    ch.is_alphanumeric() || ch == '_' || ch == '\'' || ch == '\u{2019}' || ch == '-'
}

/// Strip all non-alphanumeric characters and collapse whitespace for fuzzy trigger matching.
fn strip_punctuation_for_matching(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut last_was_space = true;
    for c in s.chars() {
        if c.is_alphanumeric() {
            result.push(c);
            last_was_space = false;
        } else if !last_was_space {
            result.push(' ');
            last_was_space = true;
        }
    }
    if last_was_space && !result.is_empty() {
        result.pop();
    }
    result
}

/// Split a trigger field into individual trigger phrases.
/// A trigger field may contain multiple phrases separated by commas,
/// e.g. "Gemini Goal, Gemini Gold". Empty segments are filtered out.
fn parse_triggers(trigger: &str) -> impl Iterator<Item = &str> {
    trigger
        .split(',')
        .map(|t| t.trim())
        .filter(|t| !t.is_empty())
}

fn collect_trigger_matches(text: &str, snippets: &[db::Snippet]) -> Vec<Match> {
    struct TriggerTarget {
        needle: String,
        snippet_idx: usize,
    }

    // Flatten all aliases into individual targets sorted by needle length descending.
    // This prevents a shorter alias from one snippet shadowing a longer trigger from another
    // snippet, which snippet-level sorting by max-length cannot guarantee.
    let mut targets: Vec<TriggerTarget> = Vec::new();
    for (snippet_idx, snippet) in snippets.iter().enumerate() {
        for t in parse_triggers(&snippet.trigger) {
            let needle = t.to_lowercase();
            if !needle.is_empty() {
                targets.push(TriggerTarget {
                    needle,
                    snippet_idx,
                });
            }
        }
    }
    targets.sort_by_key(|t| Reverse(t.needle.len()));

    let (haystack, source_map) = lowercase_with_source_map(text);
    let mut all_matches: Vec<Match> = Vec::new();

    for target in &targets {
        let needle = &target.needle;
        let snippet_idx = target.snippet_idx;

        let mut search_from = 0;
        while let Some(pos) = haystack[search_from..].find(needle.as_str()) {
            let abs = search_from + pos;
            let before_ok = abs == 0
                || !haystack[..abs]
                    .chars()
                    .next_back()
                    .map(is_word_char)
                    .unwrap_or(false);
            let after_ok = abs + needle.len() >= haystack.len()
                || !haystack[abs + needle.len()..]
                    .chars()
                    .next()
                    .map(is_word_char)
                    .unwrap_or(false);
            if before_ok && after_ok {
                let start = source_map[abs];
                let end_lower = abs + needle.len();
                let mut end = if end_lower >= haystack.len() {
                    text.len()
                } else {
                    source_map[end_lower]
                };
                if end <= start {
                    let last_src = source_map[end_lower.saturating_sub(1)];
                    end = end_of_char_at(text, last_src);
                }
                all_matches.push(Match {
                    start,
                    end,
                    snippet_idx,
                });
            }
            search_from = abs + needle.len();
        }
    }

    all_matches.sort_by(|a, b| {
        a.start
            .cmp(&b.start)
            .then_with(|| a.snippet_idx.cmp(&b.snippet_idx))
            .then_with(|| b.end.cmp(&a.end))
    });

    let mut selected: Vec<Match> = Vec::new();
    let mut last_end = 0usize;
    for m in all_matches {
        if m.start < last_end {
            continue;
        }
        last_end = m.end;
        selected.push(m);
    }

    selected
}

pub fn count_words_without_snippet_triggers(text: &str, snippets: &[db::Snippet]) -> i64 {
    let matches = collect_trigger_matches(text, snippets);
    let mut count = 0_i64;

    for (start, word) in text.split_whitespace().scan(0usize, |search_from, word| {
        let relative = text[*search_from..].find(word)?;
        let start = *search_from + relative;
        *search_from = start + word.len();
        Some((start, word))
    }) {
        let end = start + word.len();
        let overlaps_snippet = matches.iter().any(|m| start < m.end && end > m.start);
        if !overlaps_snippet && word.chars().any(char::is_alphanumeric) {
            count += 1;
        }
    }

    count
}

/// If the entire transcription is just a snippet trigger (ignoring punctuation
/// added by the transcription model), return the expansion directly.
///
/// The transcription model always appends a period — "roblox" becomes "roblox." —
/// which `expand_snippets` would leave as an orphaned "." after replacing the trigger.
/// This function strips all punctuation before matching and returns the raw expansion
/// text, so no period bleeds through. Also increments the snippet's use_count.
///
pub fn try_pure_snippet_expand_from(
    text: &str,
    snippets: &[db::Snippet],
    db: &Db,
) -> Option<String> {
    let normalized = strip_punctuation_for_matching(&text.to_lowercase());
    let matched = snippets.iter().find(|s| {
        parse_triggers(&s.trigger)
            .any(|t| strip_punctuation_for_matching(&t.to_lowercase()) == normalized)
    })?;
    let _ = db::increment_snippet_use(db, matched.id);
    Some(matched.expansion.clone())
}

pub fn collect_snippet_instructions_from(text: &str, snippets: &[db::Snippet]) -> String {
    let text_lower = text.to_lowercase();
    let text_stripped = strip_punctuation_for_matching(&text_lower);
    let mut active_instructions: Vec<String> = Vec::new();

    for snippet in snippets.iter() {
        let found = parse_triggers(&snippet.trigger).any(|t| {
            let needle = t.to_lowercase();
            let needle_stripped = strip_punctuation_for_matching(&needle);
            text_lower.contains(&needle)
                || (!needle_stripped.is_empty() && text_stripped.contains(&needle_stripped))
        });
        if found && !snippet.instructions.is_empty() {
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
    let mut result = text.to_string();
    let selected = collect_trigger_matches(&result, snippets);

    for m in selected.iter().rev() {
        result.replace_range(m.start..m.end, &snippets[m.snippet_idx].expansion);
    }

    let mut usage_counts: HashMap<i64, i64> = HashMap::new();
    for m in selected {
        let snippet_id = snippets[m.snippet_idx].id;
        *usage_counts.entry(snippet_id).or_insert(0) += 1;
    }
    if !usage_counts.is_empty() {
        let mut batched_counts: Vec<(i64, i64)> = usage_counts.into_iter().collect();
        batched_counts.sort_by_key(|(id, _)| *id);
        let _ = db::increment_snippet_use_counts(db, &batched_counts);
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
    use super::{
        apply_cleanup_instruction_overrides, count_words_without_snippet_triggers,
        expand_snippets_from,
    };
    use crate::data::db;

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

    #[test]
    fn snippet_does_not_match_inside_apostrophe_word() {
        let db = db::open(":memory:").expect("test db");
        let mut snippets = vec![db::Snippet {
            id: 1,
            trigger: "cant".to_string(),
            expansion: "cannot".to_string(),
            instructions: String::new(),
            use_count: 0,
            created_at: String::new(),
        }];

        let out = expand_snippets_from("I can't do that", &mut snippets, &db);
        assert_eq!(out, "I can't do that");
    }

    #[test]
    fn snippet_does_not_match_inside_hyphenated_word() {
        let db = db::open(":memory:").expect("test db");
        let mut snippets = vec![db::Snippet {
            id: 1,
            trigger: "test".to_string(),
            expansion: "exam".to_string(),
            instructions: String::new(),
            use_count: 0,
            created_at: String::new(),
        }];

        let out = expand_snippets_from("pre-test run", &mut snippets, &db);
        assert_eq!(out, "pre-test run");
    }

    #[test]
    fn word_count_ignores_standalone_snippet_triggers() {
        let snippets = vec![db::Snippet {
            id: 1,
            trigger: "email sig, signature".to_string(),
            expansion: "A long email signature with many words".to_string(),
            instructions: String::new(),
            use_count: 0,
            created_at: String::new(),
        }];

        let count = count_words_without_snippet_triggers("please add email sig thanks", &snippets);

        assert_eq!(count, 3);
    }

    #[test]
    fn word_count_ignores_pure_snippet_with_terminal_punctuation() {
        let snippets = vec![db::Snippet {
            id: 1,
            trigger: "sig".to_string(),
            expansion: "A long email signature with many words".to_string(),
            instructions: String::new(),
            use_count: 0,
            created_at: String::new(),
        }];

        let count = count_words_without_snippet_triggers("sig.", &snippets);

        assert_eq!(count, 0);
    }

    #[test]
    fn word_count_keeps_snippet_trigger_inside_hyphenated_word() {
        let snippets = vec![db::Snippet {
            id: 1,
            trigger: "test".to_string(),
            expansion: "exam".to_string(),
            instructions: String::new(),
            use_count: 0,
            created_at: String::new(),
        }];

        let count = count_words_without_snippet_triggers("pre-test run", &snippets);

        assert_eq!(count, 2);
    }
}
