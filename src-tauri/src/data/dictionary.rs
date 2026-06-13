use crate::data::db;
use crate::system::text::{has_distinctive_features, tokenize_lower_alnum};

const MAX_PROMPT_ENTRIES: usize = 48;
const MAX_PROMPT_CHARS: usize = 5_000;
const FALLBACK_RECENT_ENTRIES: usize = 16;
const MIN_MATCHED_PROMPT_ENTRIES: usize = 8;

pub fn build_relevant_dictionary_prompt_from(
    entries: &[db::DictionaryEntry],
    raw_text: &str,
) -> String {
    if entries.is_empty() {
        return String::new();
    }

    let raw_lower = raw_text.to_lowercase();
    let raw_tokens = tokenize_lower_alnum(raw_text);
    let raw_match_tokens: Vec<&str> = raw_tokens
        .iter()
        .map(String::as_str)
        .filter(|token| token.chars().count() >= 4)
        .collect();
    let mut selected: Vec<&db::DictionaryEntry> = entries
        .iter()
        .filter(|entry| entry_matches_raw(entry, &raw_lower, &raw_match_tokens))
        .collect();

    if selected.len() < MIN_MATCHED_PROMPT_ENTRIES {
        for entry in entries.iter().take(FALLBACK_RECENT_ENTRIES) {
            if selected.iter().any(|picked| picked.id == entry.id) {
                continue;
            }
            selected.push(entry);
            if selected.len() >= MIN_MATCHED_PROMPT_ENTRIES {
                break;
            }
        }
    }

    build_dictionary_prompt_limited(selected.into_iter())
}

fn entry_matches_raw(entry: &db::DictionaryEntry, raw_lower: &str, raw_tokens: &[&str]) -> bool {
    contains_nonempty(raw_lower, &entry.term.to_lowercase())
        || entry
            .mistake
            .as_ref()
            .is_some_and(|mistake| contains_nonempty(raw_lower, &mistake.to_lowercase()))
        || fuzzy_token_match(entry, raw_tokens)
}

fn contains_nonempty(haystack: &str, needle: &str) -> bool {
    !needle.trim().is_empty() && haystack.contains(needle)
}

fn fuzzy_token_match(entry: &db::DictionaryEntry, raw_tokens: &[&str]) -> bool {
    matches_source_tokens(&entry.term, raw_tokens)
        || entry
            .mistake
            .as_ref()
            .is_some_and(|mistake| matches_source_tokens(mistake, raw_tokens))
}

fn matches_source_tokens(source: &str, raw_tokens: &[&str]) -> bool {
    for token in source
        .split(|ch: char| !ch.is_alphanumeric())
        .filter(|t| !t.is_empty())
    {
        let candidate = token.to_lowercase();
        if candidate.chars().count() < 4 {
            continue;
        }
        for raw in raw_tokens {
            if candidate == *raw || candidate.starts_with(raw) || raw.starts_with(&candidate) {
                return true;
            }
            if edit_distance_leq_one(&candidate, raw) {
                return true;
            }
        }
    }
    false
}

fn edit_distance_leq_one(a: &str, b: &str) -> bool {
    if a == b {
        return true;
    }

    let a_len = a.chars().count();
    let b_len = b.chars().count();
    if a_len.abs_diff(b_len) > 1 {
        return false;
    }

    if a_len == b_len {
        let mut mismatches = 0usize;
        for (ca, cb) in a.chars().zip(b.chars()) {
            if ca != cb {
                mismatches += 1;
                if mismatches > 1 {
                    return false;
                }
            }
        }
        return true;
    }

    let (longer, shorter) = if a_len > b_len { (a, b) } else { (b, a) };
    let mut long_it = longer.chars().peekable();
    let mut short_it = shorter.chars().peekable();
    let mut used_skip = false;

    loop {
        match (long_it.peek().copied(), short_it.peek().copied()) {
            (None, None) => return true,
            (Some(_), None) => return !used_skip,
            (None, Some(_)) => return false,
            (Some(lc), Some(sc)) if lc == sc => {
                long_it.next();
                short_it.next();
            }
            (Some(_), Some(_)) => {
                if used_skip {
                    return false;
                }
                used_skip = true;
                long_it.next();
            }
        }
    }
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

pub fn apply_substitutions_from(text: &str, entries: &[db::DictionaryEntry]) -> (String, Vec<i64>) {
    // Auto-learned mistakes that look like plain common words (no distinctive
    // features) are left to the cleanup LLM's contextual judgment instead of
    // a blunt mechanical replace, so a mis-learned pair like "rock" -> "Groq"
    // can't clobber every legitimate use of "rock".
    let mut replaceable: Vec<(i64, &str, &str)> = entries
        .iter()
        .filter_map(|e| {
            let mistake = e.mistake.as_deref()?;
            if e.auto_learned && !has_distinctive_features(mistake) {
                return None;
            }
            Some((e.id, mistake, e.term.as_str()))
        })
        .collect();
    replaceable.sort_by_key(|(_, mistake, _)| std::cmp::Reverse(mistake.len()));

    if replaceable.is_empty() {
        return (text.to_string(), Vec::new());
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
    let mut applied_ids: Vec<i64> = Vec::new();
    for (id, mistake, term) in &replaceable {
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
            for (start, matched) in result.match_indices(*mistake) {
                let end = start + matched.len();
                if is_boundary(&result, start, end) {
                    positions.push(start);
                }
            }
        }
        if positions.is_empty() {
            continue;
        }
        applied_ids.push(*id);
        for pos in positions.into_iter().rev() {
            result.replace_range(pos..pos + mistake.len(), term);
        }
    }
    (result, applied_ids)
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

    fn auto_learned_entry(id: i64, term: &str, mistake: &str) -> DictionaryEntry {
        DictionaryEntry {
            auto_learned: true,
            confidence_tier: "medium".to_string(),
            ..entry(id, term, Some(mistake))
        }
    }

    #[test]
    fn relevant_prompt_prefers_matching_entries() {
        let entries = vec![
            entry(1, "Verenu", Some("open floor")),
            entry(2, "UnrelatedTerm", None),
        ];

        let prompt = build_relevant_dictionary_prompt_from(
            &entries,
            "Please mention open floor in this sentence.",
        );

        assert!(prompt.contains("Verenu"));
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
    fn relevant_prompt_includes_close_spelling_match() {
        let entries = vec![
            entry(1, "unifi", Some("unified")),
            entry(2, "Verenu", Some("open floor")),
        ];
        let prompt =
            build_relevant_dictionary_prompt_from(&entries, "home assistant and unify setup");
        assert!(prompt.contains("unifi"));
    }

    #[test]
    fn substitutions_respect_word_boundaries() {
        let entries = vec![entry(1, "Kubernetes", Some("kube"))];
        let (out, applied) = apply_substitutions_from("kube kubelet", &entries);
        assert_eq!(out, "Kubernetes kubelet");
        assert_eq!(applied, vec![1]);
    }

    #[test]
    fn substitutions_support_non_ascii_terms() {
        let entries = vec![entry(1, "cliche", Some("cliché"))];
        let (out, applied) = apply_substitutions_from("Use cliché in this line.", &entries);
        assert_eq!(out, "Use cliche in this line.");
        assert_eq!(applied, vec![1]);
    }

    #[test]
    fn auto_learned_common_word_mistake_is_not_mechanically_substituted() {
        let entries = vec![auto_learned_entry(1, "Groq", "rock")];
        let (out, applied) = apply_substitutions_from("I love rock music", &entries);
        assert_eq!(out, "I love rock music");
        assert!(applied.is_empty());
    }

    #[test]
    fn auto_learned_distinctive_mistake_is_still_substituted() {
        let entries = vec![auto_learned_entry(1, "vscode", "vsc0de")];
        let (out, applied) = apply_substitutions_from("please open vsc0de now", &entries);
        assert_eq!(out, "please open vscode now");
        assert_eq!(applied, vec![1]);
    }

    #[test]
    fn manual_common_word_mistake_is_still_mechanically_substituted() {
        let entries = vec![entry(1, "Groq", Some("rock"))];
        let (out, applied) = apply_substitutions_from("I love rock music", &entries);
        assert_eq!(out, "I love Groq music");
        assert_eq!(applied, vec![1]);
    }
}
