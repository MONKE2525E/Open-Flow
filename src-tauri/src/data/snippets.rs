use crate::data::db::{self, Db};

/// Replace all snippet triggers in `text` with their expansions.
/// Matching is case-insensitive. Longer triggers are evaluated first so a
/// short trigger can't shadow a longer one that shares its prefix.
/// Each matched snippet's `use_count` is incremented in the database.
pub fn expand_snippets(text: &str, db: &Db) -> String {
    let mut snippets = match db::query_snippets(db) {
        Ok(s) => s,
        Err(e) => { log::error!("expand_snippets: {e}"); return text.to_string(); }
    };

    // Longest triggers first — prevents short prefix matches shadowing longer ones.
    snippets.sort_by(|a, b| b.trigger.len().cmp(&a.trigger.len()));

    let mut result = text.to_string();

    for snippet in &snippets {
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

        if positions.is_empty() { continue; }

        // Replace right-to-left so earlier indices stay valid.
        for pos in positions.into_iter().rev() {
            result.replace_range(pos..pos + needle.len(), &snippet.expansion);
        }

        let _ = db::increment_snippet_use(db, snippet.id);
    }

    result
}
