use super::*;

#[cfg(test)]
pub(super) fn should_use_cleanup_cache(raw: &str) -> bool {
    let (tokens, _) = number_parser::tokenize_cache_key_parts(raw);
    should_use_cleanup_cache_tokens(&tokens)
}

pub(super) fn should_use_cleanup_cache_tokens(tokens: &[String]) -> bool {
    let mut numeric_count = 0usize;
    let mut has_math_operator = false;

    for t in tokens {
        if t.chars().any(|c| c.is_ascii_digit()) || is_number_word_token(t) {
            numeric_count += 1;
            continue;
        }
        if matches!(
            t.as_str(),
            "plus" | "minus" | "times" | "multiplied" | "multiply" | "divided" | "over" | "x"
        ) {
            has_math_operator = true;
        }
    }

    !(has_math_operator && numeric_count >= 2)
}

pub(super) fn parse_sqlite_utc(s: &str) -> Option<DateTime<Utc>> {
    let naive = NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S").ok()?;
    Some(DateTime::from_naive_utc_and_offset(naive, Utc))
}

pub(super) fn sqlite_utc_plus(days: i64) -> String {
    (Utc::now() + Duration::days(days))
        .format("%Y-%m-%d %H:%M:%S")
        .to_string()
}

pub(super) fn next_cache_expiry(
    hit_count: i64,
    created_at: &str,
    existing_expires_at: &str,
    now: DateTime<Utc>,
) -> String {
    let base = now + Duration::days(7);
    let created = parse_sqlite_utc(created_at).unwrap_or(now);
    let age = now.signed_duration_since(created);

    let next = if hit_count >= 5 && age <= Duration::days(60) {
        if age <= Duration::days(30) {
            now + Duration::days(30)
        } else {
            now + Duration::days(365)
        }
    } else if hit_count >= 2 && age <= Duration::days(14) {
        if age <= Duration::days(7) {
            now + Duration::days(7)
        } else {
            now + Duration::days(30)
        }
    } else {
        let existing = parse_sqlite_utc(existing_expires_at).unwrap_or(base);
        if existing > base {
            existing
        } else {
            base
        }
    };

    next.format("%Y-%m-%d %H:%M:%S").to_string()
}
pub(super) fn should_run_cleanup_llm(
    cleanup_enabled: bool,
    has_cleanup_key: bool,
    no_pure_expansion: bool,
    cleanup_intensity: &str,
    _profile: &str,
    needs_transcript_fusion: bool,
) -> bool {
    cleanup_enabled
        && has_cleanup_key
        && no_pure_expansion
        && (cleanup_intensity != "none" || needs_transcript_fusion)
}

pub(super) fn style_scoped_cleanup_cache_key(
    base_key: &str,
    profile: &str,
    cleanup_intensity: &str,
) -> String {
    if base_key.is_empty() {
        return String::new();
    }
    format!("{base_key}|profile:{profile}|intensity:{cleanup_intensity}")
}

pub(super) fn snippet_instructions_fingerprint(instructions: &str) -> u64 {
    // djb2 hash — deterministic across runs, no external dep
    let mut h: u64 = 5381;
    for b in instructions.bytes() {
        h = h.wrapping_shl(5).wrapping_add(h).wrapping_add(b as u64);
    }
    h
}
