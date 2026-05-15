use crate::data::db;

const MAX_PROMPT_ENTRIES: usize = 48;
const MAX_PROMPT_CHARS: usize = 5_000;
const FALLBACK_RECENT_ENTRIES: usize = 16;

pub fn build_relevant_dictionary_prompt_from(
    entries: &[db::DictionaryEntry],
    raw_text: &str,
) -> String {
    if entries.is_empty() {
        return String::new();
    }

    let raw_lower = raw_text.to_lowercase();
    let mut selected: Vec<&db::DictionaryEntry> = entries
        .iter()
        .filter(|entry| entry_matches_raw(entry, &raw_lower))
        .collect();

    if selected.is_empty() {
        selected.extend(entries.iter().take(FALLBACK_RECENT_ENTRIES));
    }

    build_dictionary_prompt_limited(selected.into_iter())
}

fn entry_matches_raw(entry: &db::DictionaryEntry, raw_lower: &str) -> bool {
    contains_nonempty(raw_lower, &entry.term.to_lowercase())
        || entry
            .mistake
            .as_ref()
            .is_some_and(|mistake| contains_nonempty(raw_lower, &mistake.to_lowercase()))
}

fn contains_nonempty(haystack: &str, needle: &str) -> bool {
    !needle.trim().is_empty() && haystack.contains(needle)
}

fn build_dictionary_prompt_limited<'a>(
    entries: impl Iterator<Item = &'a db::DictionaryEntry>,
) -> String {
    let mut lines = Vec::new();
    let mut chars = 0usize;

    for entry in entries.take(MAX_PROMPT_ENTRIES) {
        let line = format_dictionary_entry(entry);
        chars += line.len();
        if chars > MAX_PROMPT_CHARS && !lines.is_empty() {
            break;
        }
        lines.push(line);
    }

    if lines.is_empty() {
        return String::new();
    }

    format!(
        "USER VOCABULARY - These are real words and terms this user says. \
        Correct likely misspellings to the exact spelling shown:\n{}",
        lines.join("\n")
    )
}

fn format_dictionary_entry(entry: &db::DictionaryEntry) -> String {
    if let Some(mistake) = &entry.mistake {
        format!(
            "- \"{}\" - transcription often writes \"{}\" instead; output \"{}\".",
            entry.term, mistake, entry.term
        )
    } else {
        format!(
            "- \"{}\" - real user term; preserve this exact spelling.",
            entry.term
        )
    }
}

pub fn apply_substitutions_from(text: &str, entries: &[db::DictionaryEntry]) -> String {
    let replaceable: Vec<(&str, &str)> = entries
        .iter()
        .filter_map(|e| e.mistake.as_deref().map(|m| (m, e.term.as_str())))
        .collect();

    if replaceable.is_empty() {
        return text.to_string();
    }

    let mut result = text.to_string();
    let mut haystack = result.to_lowercase();
    for (mistake, term) in &replaceable {
        let needle = mistake.to_lowercase();
        if needle.is_empty() {
            continue;
        }
        let mut positions = Vec::new();
        let mut from = 0usize;
        while let Some(p) = haystack[from..].find(&needle) {
            let abs = from + p;
            positions.push(abs);
            from = abs + needle.len();
        }
        if positions.is_empty() {
            continue;
        }
        // Use the lowercase needle length because result indices were found in
        // the lowercase haystack. Using the original mistake length can panic
        // when lowercase expansion changes UTF-8 byte length.
        for pos in positions.into_iter().rev() {
            result.replace_range(pos..pos + needle.len(), term);
        }
        haystack = result.to_lowercase();
    }
    result
}

#[cfg(test)]
mod tests {
    use super::build_relevant_dictionary_prompt_from;
    use crate::data::db::DictionaryEntry;

    fn entry(id: i64, term: &str, mistake: Option<&str>) -> DictionaryEntry {
        DictionaryEntry {
            id,
            term: term.to_string(),
            mistake: mistake.map(str::to_string),
            auto_learned: false,
            correction_count: 0,
            created_at: "now".to_string(),
        }
    }

    #[test]
    fn relevant_prompt_prefers_matching_entries() {
        let entries = vec![
            entry(1, "Open Flow", Some("open floor")),
            entry(2, "UnrelatedTerm", None),
        ];

        let prompt = build_relevant_dictionary_prompt_from(
            &entries,
            "Please mention open floor in this sentence.",
        );

        assert!(prompt.contains("Open Flow"));
        assert!(!prompt.contains("UnrelatedTerm"));
    }

    #[test]
    fn relevant_prompt_keeps_small_recent_fallback() {
        let entries = vec![
            entry(1, "RecentTerm", None),
            entry(2, "AnotherTerm", Some("another turn")),
        ];

        let prompt = build_relevant_dictionary_prompt_from(&entries, "No obvious match here.");

        assert!(prompt.contains("RecentTerm"));
        assert!(prompt.contains("AnotherTerm"));
    }
}
