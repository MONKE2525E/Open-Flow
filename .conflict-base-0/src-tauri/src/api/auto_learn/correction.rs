//! Correction-detection algorithm extracted from auto_learn.rs: edit distance,
//! word tokenization, candidate scoring, anchor location, and span
//! diff/alignment (plus the thin baseline-capture wrappers around the parent's
//! focused-text reader). No DB access or COM/hook lifecycle lives here — the
//! data structs, tuning constants, and StableTextGate stay in the parent so
//! these functions can borrow them as a child module. Heavily covered by the
//! auto_learn test module; this was a behavior-preserving move.

use super::{
    read_focused_text, AlignOp, CandidateCorrection, CorrectionMetrics, TextAnchor, WordToken,
    MAX_CHANGED_OPS_PER_SPAN, MAX_REPLACEMENTS_PER_SPAN, MAX_SPAN_GROWTH_WORDS,
    MIN_CANDIDATE_NORM_LEN,
};
use crate::system::text::has_distinctive_features;

pub(super) fn edit_distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let (m, n) = (a.len(), b.len());
    let mut row: Vec<usize> = (0..=n).collect();
    for i in 1..=m {
        let mut prev = row[0];
        row[0] = i;
        for j in 1..=n {
            let old = row[j];
            row[j] = if a[i - 1] == b[j - 1] {
                prev
            } else {
                1 + prev.min(row[j]).min(row[j - 1])
            };
            prev = old;
        }
    }
    row[n]
}

pub(super) fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '\'' | '-' | '_')
}

pub(super) fn normalize_word(word: &str) -> String {
    word.chars()
        .filter(|c| is_word_char(*c))
        .flat_map(char::to_lowercase)
        .collect()
}

pub(super) fn tokenize_words(text: &str) -> Vec<WordToken> {
    let mut tokens = Vec::new();
    let mut start = None;

    for (idx, ch) in text.char_indices() {
        if is_word_char(ch) {
            if start.is_none() {
                start = Some(idx);
            }
        } else if let Some(s) = start.take() {
            let raw = &text[s..idx];
            let norm = normalize_word(raw);
            if !norm.is_empty() {
                tokens.push(WordToken {
                    raw: raw.to_string(),
                    norm,
                });
            }
        }
    }

    if let Some(s) = start {
        let raw = &text[s..];
        let norm = normalize_word(raw);
        if !norm.is_empty() {
            tokens.push(WordToken {
                raw: raw.to_string(),
                norm,
            });
        }
    }

    tokens
}

pub(super) fn is_common_word(word: &str) -> bool {
    matches!(
        word,
        "a" | "about"
            | "after"
            | "all"
            | "also"
            | "am"
            | "an"
            | "and"
            | "are"
            | "as"
            | "ask"
            | "at"
            | "be"
            | "but"
            | "by"
            | "can"
            | "do"
            | "for"
            | "from"
            | "get"
            | "go"
            | "had"
            | "has"
            | "have"
            | "he"
            | "her"
            | "him"
            | "his"
            | "i"
            | "if"
            | "in"
            | "is"
            | "it"
            | "its"
            | "just"
            | "me"
            | "my"
            | "no"
            | "not"
            | "of"
            | "on"
            | "or"
            | "our"
            | "out"
            | "so"
            | "ship"
            | "shop"
            | "that"
            | "the"
            | "them"
            | "then"
            | "there"
            | "their"
            | "they"
            | "this"
            | "to"
            | "up"
            | "us"
            | "was"
            | "we"
            | "what"
            | "when"
            | "with"
            | "you"
            | "your"
    )
}

pub(super) fn compute_correction_metrics(
    original: &WordToken,
    corrected: &WordToken,
) -> CorrectionMetrics {
    let a_len = original.norm.chars().count();
    let b_len = corrected.norm.chars().count();
    let max_len = a_len.max(b_len);
    let distance = edit_distance(&original.norm, &corrected.norm);
    CorrectionMetrics {
        a_len,
        b_len,
        max_len,
        distance,
    }
}

pub(super) fn is_candidate_correction(
    original: &WordToken,
    corrected: &WordToken,
    metrics: CorrectionMetrics,
) -> bool {
    if original.norm.is_empty() || corrected.norm.is_empty() {
        return false;
    }
    if original.norm == corrected.norm {
        return false;
    }
    if metrics.a_len < MIN_CANDIDATE_NORM_LEN || metrics.b_len < MIN_CANDIDATE_NORM_LEN {
        return false;
    }

    let original_distinct = has_distinctive_features(&original.raw);
    let corrected_distinct = has_distinctive_features(&corrected.raw);
    if metrics.max_len <= 3 && !original_distinct && !corrected_distinct {
        return false;
    }

    if is_common_word(&original.norm)
        && is_common_word(&corrected.norm)
        && !original_distinct
        && !corrected_distinct
    {
        return false;
    }

    if is_plain_suffix_completion(&original.norm, &corrected.norm)
        && !original_distinct
        && !corrected_distinct
    {
        return false;
    }

    metrics.distance <= 2_usize.max(metrics.max_len / 2)
        || ((original_distinct || corrected_distinct)
            && metrics.max_len >= 4
            && metrics.distance <= 3)
}

pub(super) fn pair_hash(left: &str, right: &str) -> (String, String) {
    // FNV-1a 64-bit with a fixed offset/prime gives stable hashes across
    // app versions and process runs. This is telemetry bucketing, not crypto.
    fn hash_str(value: &str) -> String {
        const FNV_OFFSET_BASIS: u64 = 0xcbf29ce484222325;
        const FNV_PRIME: u64 = 0x100000001b3;
        let mut hash = FNV_OFFSET_BASIS;
        for b in value.to_lowercase().as_bytes() {
            hash ^= u64::from(*b);
            hash = hash.wrapping_mul(FNV_PRIME);
        }
        format!("{hash:016x}")
    }
    (hash_str(left), hash_str(right))
}

pub(super) fn monitor_key(injected_text: &str, app_context: &str) -> String {
    let (lhs, rhs) = pair_hash(injected_text, app_context);
    format!("{rhs}:{lhs}")
}

pub(super) fn candidate_confidence(
    original: &WordToken,
    corrected: &WordToken,
    metrics: CorrectionMetrics,
    changed_ops: usize,
    replacements_len: usize,
) -> f64 {
    let distance = metrics.distance as f64;
    let max_len = metrics.max_len.max(1) as f64;
    let ratio_score = 1.0 - (distance / max_len).min(1.0);

    let mut score = ratio_score * 0.55;
    if has_distinctive_features(&original.raw) || has_distinctive_features(&corrected.raw) {
        score += 0.25;
    }
    if is_common_word(&original.norm) && is_common_word(&corrected.norm) {
        score -= 0.2;
    }
    score -= (changed_ops.saturating_sub(1) as f64) * 0.07;
    score -= (replacements_len.saturating_sub(1) as f64) * 0.08;
    score.clamp(0.0, 1.0)
}

pub(super) fn is_plain_suffix_completion(original: &str, corrected: &str) -> bool {
    if let Some(suffix) = corrected.strip_prefix(original) {
        return is_low_signal_suffix(suffix);
    }

    if let Some(suffix) = original.strip_prefix(corrected) {
        return is_low_signal_suffix(suffix);
    }

    false
}

pub(super) fn is_low_signal_suffix(suffix: &str) -> bool {
    matches!(suffix, "s" | "d" | "e" | "g" | "ed" | "er" | "es" | "ing")
        || suffix
            .chars()
            .all(|ch| matches!(ch, 'a' | 'e' | 'i' | 'o' | 'u'))
}

pub(super) fn find_unique_anchor(haystack: &str, needle: &str) -> Option<TextAnchor> {
    if needle.trim().is_empty() {
        return None;
    }

    let mut matches = haystack.match_indices(needle);
    let (start, _) = matches.next()?;
    if matches.next().is_some() {
        return None;
    }

    Some(TextAnchor {
        start,
        end: start + needle.len(),
    })
}

pub(super) fn common_prefix_len(a: &str, b: &str) -> usize {
    let mut len = 0;
    for ((a_idx, a_ch), (_, b_ch)) in a.char_indices().zip(b.char_indices()) {
        if a_ch != b_ch {
            break;
        }
        len = a_idx + a_ch.len_utf8();
    }
    len
}

pub(super) fn common_suffix_len_after(a: &str, b: &str, prefix_len: usize) -> usize {
    let mut len = 0;
    for (a_ch, b_ch) in a[prefix_len..]
        .chars()
        .rev()
        .zip(b[prefix_len..].chars().rev())
    {
        if a_ch != b_ch {
            break;
        }
        len += a_ch.len_utf8();
    }
    len
}

pub(super) fn current_anchored_span<'a>(
    baseline: &str,
    current: &'a str,
    anchor: TextAnchor,
) -> Option<&'a str> {
    if baseline == current {
        return Some(&current[anchor.start..anchor.end]);
    }

    let prefix = common_prefix_len(baseline, current);
    let suffix = common_suffix_len_after(baseline, current, prefix);
    let base_change_start = prefix;
    let base_change_end = baseline.len().saturating_sub(suffix);
    let current_change_end = current.len().saturating_sub(suffix);

    let overlaps_anchor = base_change_start <= anchor.end && base_change_end >= anchor.start;
    if !overlaps_anchor {
        if base_change_end <= anchor.start {
            return None;
        }
        return Some(&current[anchor.start..anchor.end]);
    }

    if base_change_start < anchor.start || base_change_end > anchor.end {
        return None;
    }

    let base_change_len = base_change_end.saturating_sub(base_change_start);
    let current_change_len = current_change_end.saturating_sub(base_change_start);
    let new_end = if current_change_len >= base_change_len {
        anchor.end + (current_change_len - base_change_len)
    } else {
        anchor
            .end
            .checked_sub(base_change_len - current_change_len)?
    };

    if anchor.start > new_end || new_end > current.len() {
        return None;
    }
    if !current.is_char_boundary(anchor.start) || !current.is_char_boundary(new_end) {
        return None;
    }

    Some(&current[anchor.start..new_end])
}

pub(super) fn find_last_anchor(haystack: &str, needle: &str) -> Option<TextAnchor> {
    if needle.trim().is_empty() {
        return None;
    }
    let start = haystack.rfind(needle)?;
    Some(TextAnchor {
        start,
        end: start + needle.len(),
    })
}

pub(super) fn capture_baseline_text(injected_text: &str) -> Option<String> {
    let current_text = read_focused_text()?;
    if find_unique_anchor(&current_text, injected_text).is_some() {
        Some(current_text)
    } else {
        None
    }
}

pub(super) fn capture_baseline_text_any(injected_text: &str) -> Option<String> {
    let current_text = read_focused_text()?;
    if current_text.contains(injected_text) {
        Some(current_text)
    } else {
        None
    }
}

pub(super) fn align_word_ops(
    original: &[WordToken],
    current: &[WordToken],
) -> Vec<(AlignOp, Option<usize>, Option<usize>)> {
    let m = original.len();
    let n = current.len();
    let mut dp = vec![vec![0usize; n + 1]; m + 1];

    for (i, row) in dp.iter_mut().enumerate().take(m + 1) {
        row[0] = i;
    }
    for (j, cell) in dp[0].iter_mut().enumerate().take(n + 1) {
        *cell = j;
    }

    for i in 1..=m {
        for j in 1..=n {
            let replace_cost = if original[i - 1].norm == current[j - 1].norm {
                0
            } else {
                1
            };
            dp[i][j] = (dp[i - 1][j - 1] + replace_cost)
                .min(dp[i - 1][j] + 1)
                .min(dp[i][j - 1] + 1);
        }
    }

    let mut ops = Vec::new();
    let (mut i, mut j) = (m, n);
    while i > 0 || j > 0 {
        if i > 0 && j > 0 {
            let replace_cost = if original[i - 1].norm == current[j - 1].norm {
                0
            } else {
                1
            };
            if dp[i][j] == dp[i - 1][j - 1] + replace_cost {
                let op = if replace_cost == 0 {
                    AlignOp::Equal
                } else {
                    AlignOp::Replace
                };
                ops.push((op, Some(i - 1), Some(j - 1)));
                i -= 1;
                j -= 1;
                continue;
            }
        }
        if i > 0 && dp[i][j] == dp[i - 1][j] + 1 {
            ops.push((AlignOp::Delete, Some(i - 1), None));
            i -= 1;
        } else {
            ops.push((AlignOp::Insert, None, Some(j - 1)));
            j -= 1;
        }
    }

    ops.reverse();
    ops
}

pub(super) fn detect_span_corrections(
    original_span: &str,
    current_span: &str,
) -> Vec<CandidateCorrection> {
    let original = tokenize_words(original_span);
    let current = tokenize_words(current_span);

    if original.is_empty() || current.is_empty() {
        return vec![];
    }
    if current.len() > original.len() * 2 + MAX_SPAN_GROWTH_WORDS {
        return vec![];
    }

    let ops = align_word_ops(&original, &current);
    let changed_ops = ops
        .iter()
        .filter(|(op, _, _)| *op != AlignOp::Equal)
        .count();
    if changed_ops > MAX_CHANGED_OPS_PER_SPAN {
        log::debug!("auto-learn: rejected span with too many changed word operations");
        return vec![];
    }

    let replacements: Vec<_> = ops
        .iter()
        .filter_map(|(op, old_idx, new_idx)| {
            if *op == AlignOp::Replace {
                Some((old_idx.unwrap(), new_idx.unwrap()))
            } else {
                None
            }
        })
        .collect();

    if replacements.len() > MAX_REPLACEMENTS_PER_SPAN {
        log::debug!("auto-learn: rejected span with too many replacements");
        return vec![];
    }

    let replacements_len = replacements.len();
    replacements
        .into_iter()
        .filter_map(|(old_idx, new_idx)| {
            let old = &original[old_idx];
            let new = &current[new_idx];
            if old.norm.is_empty() || new.norm.is_empty() || old.norm == new.norm {
                return None;
            }
            let metrics = compute_correction_metrics(old, new);
            if is_candidate_correction(old, new, metrics) {
                Some(CandidateCorrection {
                    mistake: old.raw.clone(),
                    correction: new.raw.clone(),
                    confidence: candidate_confidence(
                        old,
                        new,
                        metrics,
                        changed_ops,
                        replacements_len,
                    ),
                })
            } else {
                log::debug!("auto-learn: rejected low-confidence candidate");
                None
            }
        })
        .collect()
}

pub(super) fn detect_corrections_from_anchored_text(
    injected_text: &str,
    baseline_full_text: &str,
    current_full_text: &str,
) -> Vec<CandidateCorrection> {
    let Some(anchor) = find_unique_anchor(baseline_full_text, injected_text) else {
        log::debug!("auto-learn: injected text was not uniquely anchored");
        return vec![];
    };

    let Some(current_span) = current_anchored_span(baseline_full_text, current_full_text, anchor)
    else {
        log::debug!("auto-learn: current edit changed text outside the injected span");
        return vec![];
    };

    detect_span_corrections(injected_text, current_span)
}
