use crate::data::db::{self, Db};

/// Builds a system-prompt block from the user's personal dictionary.
/// This is injected into every cleanup LLM call so the AI knows which
/// specialised terms the user uses and how to spell them correctly.
pub fn build_dictionary_prompt(db: &Db) -> String {
    let entries = match db::query_dictionary(db) {
        Ok(e) => e,
        Err(e) => {
            log::error!("build_dictionary_prompt: {e}");
            return String::new();
        }
    };
    if entries.is_empty() {
        return String::new();
    }

    let lines: Vec<String> = entries
        .iter()
        .map(|e| {
            if let Some(mistake) = &e.mistake {
                format!(
                    "- \"{}\" — the transcription often writes \"{}\" instead; \
                 always output \"{}\" when you see something that sounds like it.",
                    e.term, mistake, e.term
                )
            } else {
                format!(
                    "- \"{}\" — a real term this user uses; recognise and preserve it \
                 exactly, even if it looks unusual.",
                    e.term
                )
            }
        })
        .collect();

    format!(
        "USER VOCABULARY — The following are real words and terms this user \
        commonly says. They may be proper nouns, brand names, jargon, or niche \
        words that a generic transcription model is unlikely to know. When any \
        of these appear in the transcription (possibly misspelled or misheard), \
        correct them to the exact spelling shown and keep them in the output:\n{}",
        lines.join("\n")
    )
}

/// Post-LLM substitution pass. Directly replaces known `mistake` patterns with
/// the correct `term`. Belt-and-suspenders after the prompt-based correction.
pub fn apply_substitutions(text: &str, db: &Db) -> String {
    let entries = match db::query_dictionary(db) {
        Ok(e) => e,
        Err(e) => {
            log::error!("apply_substitutions: {e}");
            return text.to_string();
        }
    };

    let replaceable: Vec<(&str, &str)> = entries
        .iter()
        .filter_map(|e| e.mistake.as_deref().map(|m| (m, e.term.as_str())))
        .collect();

    if replaceable.is_empty() {
        return text.to_string();
    }

    let mut result = text.to_string();
    for (mistake, term) in &replaceable {
        let needle = mistake.to_lowercase();
        // Guard: skip empty patterns — find("") always returns Some(0), causing an
        // infinite loop in the position-collection loop below.
        if needle.is_empty() {
            continue;
        }
        let haystack = result.to_lowercase();
        let mut positions = Vec::new();
        let mut from = 0usize;
        while let Some(p) = haystack[from..].find(&needle) {
            let abs = from + p;
            positions.push(abs);
            from = abs + needle.len();
        }
        // Use needle.len() (the length of what was actually found in the lowercase
        // haystack) rather than mistake.len(). For any character whose lowercase
        // form has a different UTF-8 byte length (e.g. 'İ' → 'i' + combining dot),
        // using mistake.len() would produce an out-of-bounds range and panic.
        // expand_snippets in snippets.rs already uses this correct pattern.
        for pos in positions.into_iter().rev() {
            result.replace_range(pos..pos + needle.len(), term);
        }
    }
    result
}
