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
    let mut replaceable: Vec<(&str, &str)> = entries
        .iter()
        .filter_map(|e| e.mistake.as_deref().map(|m| (m, e.term.as_str())))
        .collect();
    replaceable.sort_by(|(a, _), (b, _)| b.len().cmp(&a.len()));

    if replaceable.is_empty() {
        return text.to_string();
    }

    fn is_word_char(ch: char) -> bool {
        ch.is_alphanumeric() || matches!(ch, '\'' | '-' | '_')
    }

    fn is_boundary(haystack: &str, start: usize, end: usize) -> bool {
        let left_ok = haystack[..start]
            .chars()
            .next_back()
            .is_none_or(|ch| !is_word_char(ch));
        let right_ok = haystack[end..]
            .chars()
            .next()
            .is_none_or(|ch| !is_word_char(ch));
        left_ok && right_ok
    }

    let mut result = text.to_string();
    for (mistake, term) in &replaceable {
        if mistake.is_empty() {
            continue;
        }
        let mut positions = Vec::new();
        if mistake.is_ascii() {
            let mut i = 0usize;
            while i + mistake.len() <= result.len() {
                if !result.is_char_boundary(i) {
                    i += 1;
                    continue;
                }
                let end = i + mistake.len();
                if !result.is_char_boundary(end) {
                    i += 1;
                    continue;
                }
                if result[i..end].eq_ignore_ascii_case(mistake) && is_boundary(&result, i, end) {
                    positions.push(i);
                }
                i += 1;
            }
        } else {
            for (start, matched) in result.match_indices(mistake) {
                let end = start + matched.len();
                if is_boundary(&result, start, end) {
                    positions.push(start);
                }
            }
        }
        if positions.is_empty() {
            continue;
        }
        for pos in positions.into_iter().rev() {
            result.replace_range(pos..pos + mistake.len(), term);
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::{apply_substitutions_from, build_relevant_dictionary_prompt_from};
    use crate::data::db::DictionaryEntry;

    fn entry(id: i64, term: &str, mistake: Option<&str>) -> DictionaryEntry {
        DictionaryEntry {
            id,
            term: term.to_string(),
            mistake: mistake.map(str::to_string),
            auto_learned: false,
            correction_count: 0,
            confidence_tier: "manual".to_string(),
            last_seen_at: None,
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

    #[test]
    fn substitutions_respect_word_boundaries() {
        let entries = vec![entry(1, "Kubernetes", Some("kube"))];
        let out = apply_substitutions_from("kube kubelet", &entries);
        assert_eq!(out, "Kubernetes kubelet");
    }

    #[test]
    fn substitutions_support_non_ascii_terms() {
        let entries = vec![entry(1, "cliche", Some("cliché"))];
        let out = apply_substitutions_from("Use cliché in this line.", &entries);
        assert_eq!(out, "Use cliche in this line.");
    }
}
