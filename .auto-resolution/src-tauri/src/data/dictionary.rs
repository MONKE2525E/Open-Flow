use crate::data::db;
use crate::system::text::{has_distinctive_features, tokenize_lower_alnum};

const MAX_PROMPT_ENTRIES: usize = 24;
const MAX_PROMPT_CHARS: usize = 3_000;
const MAX_FALLBACK_ENTRIES: usize = 8;
const SHORT_TRANSCRIPT_TOKENS: usize = 4;

#[cfg(test)]
pub fn build_relevant_dictionary_prompt_from(
    entries: &[db::DictionaryEntry],
    raw_text: &str,
) -> String {
    build_relevant_dictionary_prompt_from_sources(entries, raw_text, None, None)
}

/// Build vocabulary evidence from the candidate transcripts and the small
/// app/context hint. Matching entries are ranked instead of dumping the
/// context dictionary into every request. The primary candidate has the
/// highest weight; an alternate or context hit can surface a useful term but
/// never becomes a replacement rule.
pub fn build_relevant_dictionary_prompt_from_sources(
    entries: &[db::DictionaryEntry],
    primary_text: &str,
    alternate_text: Option<&str>,
    context_text: Option<&str>,
) -> String {
    if entries.is_empty() {
        return String::new();
    }

    let primary_tokens = tokenize_lower_alnum(primary_text);
    let short_or_ambiguous = primary_tokens.len() <= SHORT_TRANSCRIPT_TOKENS;
    let sources = [
        Some((primary_text, 100u16)),
        alternate_text.map(|text| (text, 82u16)),
        context_text.map(|text| (text, 52u16)),
    ]
    .into_iter()
    .enumerate()
    .filter_map(|(index, source)| {
        source.map(|(text, weight)| {
            (
                index,
                text,
                weight,
                text.to_lowercase(),
                tokenize_lower_alnum(text),
            )
        })
    })
    .collect::<Vec<_>>();

    let mut ranked: Vec<(u16, usize, &db::DictionaryEntry)> = entries
        .iter()
        .enumerate()
        .filter_map(|(index, entry)| {
            let mut score = 0u16;
            let mut primary_match = false;
            let mut alternate_match = false;
            for (original_index, _text, weight, lower, tokens) in &sources {
                let Some(match_score) = entry_match_score(entry, lower, tokens) else {
                    continue;
                };
                score = score.max(weight.saturating_mul(match_score) / 100);
                if *original_index == 0 {
                    primary_match = true;
                } else if *original_index == 1 {
                    alternate_match = true;
                }
            }
            if primary_match && alternate_match {
                score = score.saturating_add(18);
            }
            (score > 0).then_some((score, index, entry))
        })
        .collect();
    ranked.sort_by(|(score_a, index_a, _), (score_b, index_b, _)| {
        score_b.cmp(score_a).then_with(|| index_a.cmp(index_b))
    });

    let mut selected: Vec<&db::DictionaryEntry> = ranked
        .iter()
        .take(MAX_PROMPT_ENTRIES)
        .map(|(_, _, entry)| *entry)
        .collect();

    // A one- or two-word dictation often contains exactly the term the STT
    // missed, so having a tiny set of high-signal candidates is safer than
    // returning no vocabulary at all. Only distinctive/proper/technical
    // entries qualify; ordinary words are never sprayed into a short prompt.
    if selected.is_empty() && short_or_ambiguous {
        selected = entries
            .iter()
            .filter(|entry| entry_has_fallback_signal(entry))
            .take(MAX_FALLBACK_ENTRIES)
            .collect();
    }

    build_dictionary_prompt_limited(selected.into_iter())
}

/// Split a `mistake` field into individual mistranscription variants. A
/// dictionary entry may list several alternate mishearings of the same term
/// separated by commas (e.g. "Varinu, Verena, Virinu"), mirroring how
/// `snippets::parse_triggers` lets one snippet respond to several phrases —
/// same reasoning applies here: one real term can get mangled by a
/// transcription model in more than one way, and users need to be able to
/// list all of them against a single correct spelling.
fn parse_dictionary_mistakes(mistake: &str) -> impl Iterator<Item = &str> {
    mistake
        .split(',')
        .map(|m| m.trim())
        .filter(|m| !m.is_empty())
}

fn entry_match_score(
    entry: &db::DictionaryEntry,
    source_lower: &str,
    source_tokens: &[String],
) -> Option<u16> {
    let source_tokens: Vec<&str> = source_tokens
        .iter()
        .map(String::as_str)
        .filter(|token| token.chars().count() >= 4)
        .collect();
    let sources = std::iter::once(entry.term.as_str()).chain(
        entry
            .mistake
            .as_deref()
            .into_iter()
            .flat_map(parse_dictionary_mistakes),
    );
    let mut best = 0u16;
    for candidate in sources {
        if contains_term(source_lower, candidate) {
            best = best.max(100);
        } else if matches_source_tokens(candidate, &source_tokens) {
            best = best.max(70);
        }
    }
    (best > 0).then_some(best)
}

fn contains_term(haystack: &str, needle: &str) -> bool {
    let needle = needle.trim().to_lowercase();
    if needle.is_empty() {
        return false;
    }
    if needle.chars().all(char::is_alphanumeric) {
        return tokenize_lower_alnum(haystack)
            .iter()
            .any(|token| token == &needle);
    }
    haystack.contains(&needle)
}

fn entry_has_fallback_signal(entry: &db::DictionaryEntry) -> bool {
    has_distinctive_features(&entry.term)
        || looks_like_proper_or_brand_name(&entry.term)
        || entry.mistake.as_deref().is_some_and(|mistake| {
            parse_dictionary_mistakes(mistake).any(|variant| {
                has_distinctive_features(variant) || looks_like_proper_or_brand_name(variant)
            })
        })
}

fn looks_like_proper_or_brand_name(value: &str) -> bool {
    let mut chars = value.chars();
    chars.next().is_some_and(|first| first.is_uppercase())
        && chars.any(|character| character.is_lowercase())
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
    let header = "Vocabulary evidence (not a replacement list):";
    let mut rendered = header.to_string();

    for entry in entries.take(MAX_PROMPT_ENTRIES) {
        let line = format_dictionary_entry(entry);
        let candidate = format!("{rendered}\n{line}");
        if candidate.chars().count() > MAX_PROMPT_CHARS {
            break;
        }
        rendered = candidate;
    }

    if rendered == header {
        return String::new();
    }

    rendered
}

fn format_dictionary_entry(entry: &db::DictionaryEntry) -> String {
    match &entry.mistake {
        Some(mistake) => {
            let variants: Vec<&str> = parse_dictionary_mistakes(mistake).collect();
            if variants.is_empty() {
                return format!("- known term: \"{}\"", entry.term);
            }
            let quoted = variants
                .iter()
                .map(|v| format!("\"{v}\""))
                .collect::<Vec<_>>()
                .join(", ");
            format!(
                "- preferred: \"{}\"; possible STT variants: {}",
                entry.term, quoted
            )
        }
        None => format!("- known term: \"{}\"", entry.term),
    }
}

pub fn apply_substitutions_from(text: &str, entries: &[db::DictionaryEntry]) -> (String, Vec<i64>) {
    // Auto-learned mistakes that look like plain common words (no distinctive
    // features) are left to the cleanup LLM's contextual judgment instead of
    // a blunt mechanical replace, so a mis-learned pair like "rock" -> "Groq"
    // can't clobber every legitimate use of "rock". A `mistake` field may
    // list several comma-separated variants of the same real term (see
    // `parse_dictionary_mistakes`) — each variant is checked against this
    // gate and replaced independently, so one entry can carry any number of
    // known mistranscriptions for a single correct spelling.
    let mut replaceable: Vec<(i64, &str, &str)> = entries
        .iter()
        .filter_map(|e| {
            e.mistake.as_deref().map(|mistake| {
                parse_dictionary_mistakes(mistake)
                    // The LLM gets all relevant vocabulary as evidence. Only
                    // mechanically fix a variant with a distinctive technical
                    // shape; common-word pairs (for example clawed/Claude or
                    // rock/Groq) must remain context-dependent.
                    .filter(|variant| has_distinctive_features(variant))
                    .map(|variant| (e.id, variant, e.term.as_str()))
            })
        })
        .flatten()
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
    use super::{
        apply_substitutions_from, build_dictionary_prompt_limited,
        build_relevant_dictionary_prompt_from, build_relevant_dictionary_prompt_from_sources,
    };
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
    fn relevant_prompt_does_not_send_unmatched_entries() {
        let entries = vec![
            entry(1, "RecentTerm", None),
            entry(2, "AnotherTerm", Some("another turn")),
        ];

        let prompt = build_relevant_dictionary_prompt_from(
            &entries,
            "Please write this ordinary sentence without using any named product or technical term.",
        );

        assert!(prompt.is_empty());
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
        let entries = vec![entry(1, "Kubernetes", Some("kubernetez"))];
        let (out, applied) = apply_substitutions_from("kubernetez Kubernetes", &entries);
        assert_eq!(out, "Kubernetes Kubernetes");
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
    fn comma_separated_mistake_variants_all_substitute_to_the_same_term() {
        // Mirrors snippets' comma-separated trigger list: one entry can carry
        // several known mistranscriptions of the same real term.
        let entries = vec![entry(
            1,
            "Verenu",
            Some("Varinu, Verena, Virinu, Varino, Varinew, Varina"),
        )];
        let (out, applied) =
            apply_substitutions_from("I use Varinu daily but Verena crashed once", &entries);
        assert_eq!(out, "I use Varinu daily but Verena crashed once");
        assert!(applied.is_empty());
    }

    #[test]
    fn comma_separated_variants_with_surrounding_whitespace_are_trimmed() {
        let entries = vec![entry(1, "Verenu", Some(" Varinu , Verena "))];
        let (out, applied) = apply_substitutions_from("try Varinu now", &entries);
        assert_eq!(out, "try Varinu now");
        assert!(applied.is_empty());
    }

    #[test]
    fn relevant_prompt_matches_any_comma_separated_variant() {
        let entries = [entry(1, "Verenu", Some("Varinu, Verena"))];
        let prompt = build_relevant_dictionary_prompt_from(&entries, "I opened Verena today");
        assert!(prompt.contains("Verenu"));
    }

    #[test]
    fn relevant_prompt_uses_context_without_dumping_the_dictionary() {
        let entries = vec![
            entry(1, "Claude", Some("clawed")),
            entry(2, "UnrelatedTerm", None),
        ];
        let prompt = build_relevant_dictionary_prompt_from_sources(
            &entries,
            "open the editor",
            None,
            Some("Visual Studio Code — Claude"),
        );
        assert!(prompt.contains("Claude"));
        assert!(!prompt.contains("UnrelatedTerm"));
    }

    #[test]
    fn long_unmatched_dictation_does_not_trigger_a_dictionary_dump() {
        let entries = vec![
            entry(1, "Kubernetes", None),
            entry(2, "PostgreSQL", None),
            entry(3, "Verenu", None),
        ];
        let prompt = build_relevant_dictionary_prompt_from_sources(
            &entries,
            "please summarize this long sentence without any named product or technical term in it",
            None,
            None,
        );
        assert!(prompt.is_empty());
    }

    #[test]
    fn very_short_unmatched_dictation_gets_only_high_signal_fallbacks() {
        let mut entries = vec![
            entry(1, "Claude", Some("clawed")),
            entry(2, "Kubernetes", None),
            entry(3, "ordinary", None),
        ];
        entries.extend((4..20).map(|id| entry(id, &format!("Project{id}X"), None)));
        let prompt = build_relevant_dictionary_prompt_from_sources(&entries, "fix it", None, None);
        let lines = prompt.lines().filter(|line| line.starts_with("- ")).count();
        assert_eq!(lines, 8);
        assert!(prompt.contains("Claude"));
        assert!(prompt.contains("Project4X"));
        assert!(!prompt.contains("ordinary"));
    }

    #[test]
    fn fallback_and_relevance_selection_have_a_hard_size_bound() {
        let entries: Vec<DictionaryEntry> = (0..500)
            .map(|id| entry(id, &format!("TechnicalIdentifier{id}X"), None))
            .collect();
        let prompt = build_relevant_dictionary_prompt_from_sources(&entries, "one", None, None);
        assert!(prompt.chars().count() <= 3_000);
        assert_eq!(
            prompt.lines().filter(|line| line.starts_with("- ")).count(),
            8
        );
    }

    #[test]
    fn prompt_lists_every_comma_separated_variant() {
        let entries = [entry(1, "Verenu", Some("Varinu, Verena"))];
        let prompt = build_dictionary_prompt_limited(entries.iter());
        assert!(prompt.contains("\"Varinu\""));
        assert!(prompt.contains("\"Verena\""));
    }

    #[test]
    fn manual_common_word_competitor_is_not_mechanically_substituted() {
        let entries = vec![entry(1, "Groq", Some("rock"))];
        let (out, applied) = apply_substitutions_from("I love rock music", &entries);
        assert_eq!(out, "I love rock music");
        assert!(applied.is_empty());
    }
}
