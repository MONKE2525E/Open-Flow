use std::collections::HashSet;
use std::sync::{Mutex, OnceLock};
use tauri::{AppHandle, Emitter};

use crate::data::db;
use crate::DbHandle;

const MONITOR_WINDOW_SECS: u64 = 60;
const POLL_INTERVAL_SECS: u64 = 2;
const BASELINE_CAPTURE_DELAY_MS: u64 = 250;
const BASELINE_RETRY_DELAY_MS: u64 = 500;
const REJECTION_WINDOW_SECS: u64 = 8;
const REJECTION_POLL_MS: u64 = 500;
const CACHE_REJECTION_WINDOW_SECS: u64 = 10;
const PENDING_RETENTION_DAYS: i64 = 2;
const PROMOTION_THRESHOLD: i64 = 2;
const STABLE_TEXT_OBSERVATIONS_REQUIRED: usize = 2;
const MIN_CANDIDATE_NORM_LEN: usize = 2;
const MAX_SPAN_GROWTH_WORDS: usize = 5;
const MAX_REPLACEMENTS_PER_SPAN: usize = 2;
const MAX_CHANGED_OPS_PER_SPAN: usize = 4;
const MIN_CANDIDATE_CONFIDENCE: f64 = 0.45;
const PAIR_COOLDOWN_MINUTES: i64 = 0;

static ACTIVE_MONITORS: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();

fn active_monitors() -> &'static Mutex<HashSet<String>> {
    ACTIVE_MONITORS.get_or_init(|| Mutex::new(HashSet::new()))
}

struct MonitorKeyGuard {
    key: String,
}

impl MonitorKeyGuard {
    fn new(key: String) -> Self {
        Self { key }
    }
}

impl Drop for MonitorKeyGuard {
    fn drop(&mut self) {
        if let Ok(mut active) = active_monitors().lock() {
            active.remove(&self.key);
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct WordToken {
    raw: String,
    norm: String,
}

#[derive(Debug, Clone, PartialEq)]
struct CandidateCorrection {
    mistake: String,
    correction: String,
    confidence: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TextAnchor {
    start: usize,
    end: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AlignOp {
    Equal,
    Replace,
    Insert,
    Delete,
}

#[derive(Debug, Default)]
struct StableTextGate {
    pending_text: Option<String>,
    pending_observations: usize,
    processed_text: Option<String>,
}

impl StableTextGate {
    fn observe(&mut self, text: String) -> Option<&str> {
        if self.pending_text.as_deref() == Some(text.as_str()) {
            self.pending_observations += 1;
        } else {
            self.pending_text = Some(text);
            self.pending_observations = 1;
        }

        if self.pending_observations < STABLE_TEXT_OBSERVATIONS_REQUIRED {
            return None;
        }

        let stable_text = self.pending_text.as_deref()?;
        if self.processed_text.as_deref() == Some(stable_text) {
            return None;
        }

        self.processed_text = Some(stable_text.to_string());
        self.processed_text.as_deref()
    }
}

fn edit_distance(a: &str, b: &str) -> usize {
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

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '\'' | '-' | '_')
}

fn normalize_word(word: &str) -> String {
    word.chars()
        .filter(|c| is_word_char(*c))
        .flat_map(char::to_lowercase)
        .collect()
}

fn tokenize_words(text: &str) -> Vec<WordToken> {
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

fn has_distinctive_features(token: &str) -> bool {
    if !token.is_ascii() {
        return true;
    }
    if token.len() >= 4
        && token
            .chars()
            .any(|c| matches!(c.to_ascii_lowercase(), 'q' | 'x' | 'z'))
    {
        return true;
    }
    if token
        .chars()
        .any(|c| c.is_ascii_digit() || matches!(c, '\'' | '-' | '_'))
    {
        return true;
    }

    let uppercase_count = token.chars().filter(|c| c.is_uppercase()).count();
    uppercase_count > 1 || token.chars().skip(1).any(|c| c.is_uppercase())
}

fn is_common_word(word: &str) -> bool {
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

fn is_candidate_correction(original: &WordToken, corrected: &WordToken) -> bool {
    if original.norm.is_empty() || corrected.norm.is_empty() {
        return false;
    }
    if original.norm == corrected.norm {
        return false;
    }
    let a_len = original.norm.chars().count();
    let b_len = corrected.norm.chars().count();
    if a_len < MIN_CANDIDATE_NORM_LEN || b_len < MIN_CANDIDATE_NORM_LEN {
        return false;
    }

    let original_distinct = has_distinctive_features(&original.raw);
    let corrected_distinct = has_distinctive_features(&corrected.raw);
    if a_len.max(b_len) <= 3
        && !original_distinct
        && !corrected_distinct
    {
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

    let max_len = a_len.max(b_len);
    let dist = edit_distance(&original.norm, &corrected.norm);
    dist <= 2_usize.max(max_len / 2)
        || ((original_distinct || corrected_distinct) && max_len >= 4 && dist <= 3)
}

fn pair_hash(left: &str, right: &str) -> (String, String) {
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

fn monitor_key(injected_text: &str, app_context: &str) -> String {
    let (lhs, rhs) = pair_hash(injected_text, app_context);
    format!("{rhs}:{lhs}")
}

fn candidate_confidence(
    original: &WordToken,
    corrected: &WordToken,
    changed_ops: usize,
    replacements_len: usize,
) -> f64 {
    let distance = edit_distance(&original.norm, &corrected.norm) as f64;
    let max_len = original.norm.chars().count().max(corrected.norm.chars().count()).max(1) as f64;
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

fn is_plain_suffix_completion(original: &str, corrected: &str) -> bool {
    if let Some(suffix) = corrected.strip_prefix(original) {
        return is_low_signal_suffix(suffix);
    }

    if let Some(suffix) = original.strip_prefix(corrected) {
        return is_low_signal_suffix(suffix);
    }

    false
}

fn is_low_signal_suffix(suffix: &str) -> bool {
    matches!(suffix, "s" | "d" | "e" | "g" | "ed" | "er" | "es" | "ing")
        || suffix
            .chars()
            .all(|ch| matches!(ch, 'a' | 'e' | 'i' | 'o' | 'u'))
}

fn find_unique_anchor(haystack: &str, needle: &str) -> Option<TextAnchor> {
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

fn common_prefix_len(a: &str, b: &str) -> usize {
    let mut len = 0;
    for ((a_idx, a_ch), (_, b_ch)) in a.char_indices().zip(b.char_indices()) {
        if a_ch != b_ch {
            break;
        }
        len = a_idx + a_ch.len_utf8();
    }
    len
}

fn common_suffix_len_after(a: &str, b: &str, prefix_len: usize) -> usize {
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

fn current_anchored_span<'a>(
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

fn find_last_anchor(haystack: &str, needle: &str) -> Option<TextAnchor> {
    if needle.trim().is_empty() {
        return None;
    }
    let start = haystack.rfind(needle)?;
    Some(TextAnchor {
        start,
        end: start + needle.len(),
    })
}

fn capture_baseline_text(injected_text: &str) -> Option<String> {
    let current_text = read_focused_text()?;
    if find_unique_anchor(&current_text, injected_text).is_some() {
        Some(current_text)
    } else {
        None
    }
}

fn capture_baseline_text_any(injected_text: &str) -> Option<String> {
    let current_text = read_focused_text()?;
    if current_text.contains(injected_text) {
        Some(current_text)
    } else {
        None
    }
}

fn align_word_ops(
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

fn detect_span_corrections(original_span: &str, current_span: &str) -> Vec<CandidateCorrection> {
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
            if is_candidate_correction(old, new) {
                Some(CandidateCorrection {
                    mistake: old.raw.clone(),
                    correction: new.raw.clone(),
                    confidence: candidate_confidence(old, new, changed_ops, replacements_len),
                })
            } else {
                log::debug!("auto-learn: rejected low-confidence candidate");
                None
            }
        })
        .collect()
}

fn detect_corrections_from_anchored_text(
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

#[cfg(test)]
fn diff_words(original: &str, current: &str) -> Vec<(String, String)> {
    detect_span_corrections(original, current)
        .into_iter()
        .map(|c| (c.mistake, c.correction))
        .collect()
}

fn record_candidate(
    db: &DbHandle,
    recorded_this_session: &mut HashSet<(String, String)>,
    app_context: &str,
    mistake: String,
    correction: String,
    confidence: f64,
) -> bool {
    let key = (mistake.clone(), correction.clone());
    if recorded_this_session.contains(&key) {
        let _ = db::log_auto_learn_event(
            db,
            "candidate",
            "duplicate_in_session",
            app_context,
            "",
            "",
            confidence,
        );
        return false;
    }
    recorded_this_session.insert(key);
    let (mistake_hash, correction_hash) = pair_hash(&mistake, &correction);

    if confidence < MIN_CANDIDATE_CONFIDENCE {
        let _ = db::log_auto_learn_event(
            db,
            "candidate",
            "low_confidence",
            app_context,
            &mistake_hash,
            &correction_hash,
            confidence,
        );
        return false;
    }

    if !db::upsert_auto_learn_candidate(
        db,
        &mistake,
        &correction,
        confidence,
        PAIR_COOLDOWN_MINUTES,
    )
    .unwrap_or(false)
    {
        let _ = db::log_auto_learn_event(
            db,
            "candidate",
            "cooldown_skip",
            app_context,
            &mistake_hash,
            &correction_hash,
            confidence,
        );
        return false;
    }

    if let Err(e) = db::insert_pending_correction(db, &mistake, &correction) {
        log::warn!("auto-learn pending insert failed: {e}");
        let _ = db::log_auto_learn_event(
            db,
            "candidate",
            "pending_insert_failed",
            app_context,
            &mistake_hash,
            &correction_hash,
            confidence,
        );
        return false;
    }

    let count =
        db::count_pending_corrections_recent(db, &mistake, &correction, PENDING_RETENTION_DAYS)
            .unwrap_or(0);

    if count < PROMOTION_THRESHOLD {
        let _ = db::log_auto_learn_event(
            db,
            "candidate",
            "below_threshold",
            app_context,
            &mistake_hash,
            &correction_hash,
            confidence,
        );
        return false;
    }

    let tier = if confidence >= 0.85 {
        "high"
    } else if confidence >= 0.72 {
        "medium"
    } else {
        "low"
    };

    match db::insert_dictionary_entry_auto_learned(db, &correction, Some(&mistake), tier) {
        Ok(true) => {
            let _ = db::mark_auto_learn_candidate_promoted(db, &mistake, &correction);
            let _ = db::log_auto_learn_event(
                db,
                "promotion",
                "promoted",
                app_context,
                &mistake_hash,
                &correction_hash,
                confidence,
            );
            true
        }
        Ok(false) => {
            log::debug!(
                "auto-learn: promotion skipped because dictionary entry is manual or mismatched"
            );
            let _ = db::log_auto_learn_event(
                db,
                "promotion",
                "promotion_skipped",
                app_context,
                &mistake_hash,
                &correction_hash,
                confidence,
            );
            false
        }
        Err(e) => {
            log::warn!("auto-learn dictionary promotion failed: {e}");
            let _ = db::log_auto_learn_event(
                db,
                "promotion",
                "promotion_failed",
                app_context,
                &mistake_hash,
                &correction_hash,
                confidence,
            );
            false
        }
    }
}

#[cfg(windows)]
struct ComGuard(bool);

#[cfg(windows)]
impl ComGuard {
    fn init() -> Self {
        use windows::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED};
        let hr = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
        ComGuard(hr.is_ok())
    }
}

#[cfg(windows)]
impl Drop for ComGuard {
    fn drop(&mut self) {
        if self.0 {
            use windows::Win32::System::Com::CoUninitialize;
            unsafe { CoUninitialize() };
        }
    }
}

#[cfg(windows)]
thread_local! {
    static COM_INIT: std::cell::RefCell<Option<ComGuard>> = const { std::cell::RefCell::new(None) };
}

#[cfg(windows)]
pub fn read_focused_text() -> Option<String> {
    use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_INPROC_SERVER};
    use windows::Win32::UI::Accessibility::{
        CUIAutomation, IUIAutomation, IUIAutomationTextPattern, IUIAutomationValuePattern,
        UIA_TextPatternId, UIA_ValuePatternId,
    };

    COM_INIT.with(|cell| {
        let mut guard = cell.borrow_mut();
        if guard.is_none() {
            *guard = Some(ComGuard::init());
        }
    });

    unsafe {
        let automation: IUIAutomation =
            CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER).ok()?;
        let element = automation.GetFocusedElement().ok()?;

        // Track whether any pattern was readable (even if the field is empty).
        // Only return None when UIAutomation cannot read the element at all —
        // an accessible-but-empty field must return Some("") so callers can
        // distinguish "field cleared" from "UIAutomation unavailable".
        let mut accessible_empty: Option<String> = None;

        if let Ok(pattern) =
            element.GetCurrentPatternAs::<IUIAutomationValuePattern>(UIA_ValuePatternId)
        {
            if let Ok(val) = pattern.CurrentValue() {
                let s = val.to_string();
                if !s.is_empty() {
                    return Some(s);
                }
                accessible_empty = Some(s);
            }
        }

        if let Ok(pattern) =
            element.GetCurrentPatternAs::<IUIAutomationTextPattern>(UIA_TextPatternId)
        {
            if let Ok(doc_range) = pattern.DocumentRange() {
                if let Ok(val) = doc_range.GetText(-1) {
                    let s = val.to_string();
                    if !s.is_empty() {
                        return Some(s);
                    }
                    accessible_empty = Some(s);
                }
            }
        }

        accessible_empty
    }
}

#[cfg(not(windows))]
pub fn read_focused_text() -> Option<String> {
    None
}

pub fn start_monitor(injected_text: String, app_context: String, db: DbHandle, app: AppHandle) {
    if injected_text.split_whitespace().count() < 2 {
        let _ = db::log_auto_learn_event(&db, "monitor", "too_short", &app_context, "", "", 0.0);
        return;
    }
    let key = monitor_key(&injected_text, &app_context);
    let inserted = match active_monitors().lock() {
        Ok(mut active) => active.insert(key.clone()),
        Err(_) => false,
    };
    if !inserted {
        let _ =
            db::log_auto_learn_event(&db, "monitor", "duplicate_skip", &app_context, "", "", 0.0);
        return;
    }
    let _ = db::log_auto_learn_event(&db, "monitor", "started", &app_context, "", "", 0.0);

    tauri::async_runtime::spawn(async move {
        let _monitor_guard = MonitorKeyGuard::new(key);
        if let Err(e) = db::prune_pending_corrections(&db, PENDING_RETENTION_DAYS) {
            log::warn!("auto-learn prune failed: {e}");
        }

        tokio::time::sleep(std::time::Duration::from_millis(BASELINE_CAPTURE_DELAY_MS)).await;
        let mut baseline_text = tokio::task::spawn_blocking({
            let injected_text = injected_text.clone();
            move || capture_baseline_text(&injected_text)
        })
        .await
        .ok()
        .flatten();

        if baseline_text.is_none() {
            tokio::time::sleep(std::time::Duration::from_millis(BASELINE_RETRY_DELAY_MS)).await;
            baseline_text = tokio::task::spawn_blocking({
                let injected_text = injected_text.clone();
                move || capture_baseline_text(&injected_text)
            })
            .await
            .ok()
            .flatten();
        }

        let Some(baseline_text) = baseline_text else {
            log::debug!("auto-learn: could not anchor injected text in focused control");
            let _ =
                db::log_auto_learn_event(&db, "anchor", "anchor_miss", &app_context, "", "", 0.0);
            return;
        };
        let _ = db::log_auto_learn_event(&db, "anchor", "anchor_ok", &app_context, "", "", 0.0);

        let mut stable_text_gate = StableTextGate::default();
        let deadline =
            tokio::time::Instant::now() + std::time::Duration::from_secs(MONITOR_WINDOW_SECS);
        let mut recorded_this_session: HashSet<(String, String)> = HashSet::new();

        loop {
            if tokio::time::Instant::now() >= deadline {
                break;
            }

            tokio::time::sleep(std::time::Duration::from_secs(POLL_INTERVAL_SECS)).await;

            let current_text = tokio::task::spawn_blocking(read_focused_text).await;
            let current_text = match current_text {
                Ok(Some(t)) => t,
                _ => continue,
            };

            let Some(stable_text) = stable_text_gate.observe(current_text) else {
                continue;
            };
            let _ = db::log_auto_learn_event(
                &db,
                "stable_text",
                "stable_pass",
                &app_context,
                "",
                "",
                0.0,
            );

            let diffs =
                detect_corrections_from_anchored_text(&injected_text, &baseline_text, stable_text);

            for candidate in diffs {
                if record_candidate(
                    &db,
                    &mut recorded_this_session,
                    &app_context,
                    candidate.mistake,
                    candidate.correction,
                    candidate.confidence,
                ) {
                    log::info!("auto-learn: promoted candidate pair");
                    app.emit("open-flow:dictionary-updated", ()).ok();
                }
            }
        }
        let _ = db::log_auto_learn_event(&db, "monitor", "timeout", &app_context, "", "", 0.0);
    });
}

#[derive(Debug)]
enum RejectionTarget {
    DictEntries { ids: Vec<i64> },
    CacheKey { key: String },
}

impl RejectionTarget {
    fn monitor_key_prefix(&self) -> &'static str {
        match self {
            RejectionTarget::DictEntries { .. } => "rejection",
            RejectionTarget::CacheKey { .. } => "cache_rejection",
        }
    }

    fn window_secs(&self) -> u64 {
        match self {
            RejectionTarget::DictEntries { .. } => REJECTION_WINDOW_SECS,
            RejectionTarget::CacheKey { .. } => CACHE_REJECTION_WINDOW_SECS,
        }
    }
}

fn apply_rejection(target: &RejectionTarget, db: &DbHandle, app: &AppHandle, prefix: &str) {
    match target {
        RejectionTarget::DictEntries { ids } => {
            if let Err(e) = db::delete_auto_learned_entries_by_ids(db, ids) {
                log::warn!("{prefix}: delete failed: {e}");
            } else {
                app.emit("open-flow:dictionary-entry-rejected", ids.len()).ok();
            }
        }
        RejectionTarget::CacheKey { key } => {
            if let Err(e) = db::cleanup_cache_delete_by_key(db, key) {
                log::warn!("{prefix}: delete failed: {e}");
            } else {
                app.emit("open-flow:cleanup-cache-invalidated", ()).ok();
            }
        }
    }
}

#[cfg(windows)]
fn is_target_window_focused(target_hwnd: usize) -> bool {
    use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;
    unsafe { GetForegroundWindow().0 as usize == target_hwnd }
}

#[cfg(not(windows))]
fn is_target_window_focused(_target_hwnd: usize) -> bool {
    true
}

fn run_rejection_monitor(
    injected_text: String,
    target: RejectionTarget,
    target_hwnd: usize,
    db: DbHandle,
    app: AppHandle,
) {
    let prefix = target.monitor_key_prefix();
    let (key_hash, _) = pair_hash(&injected_text, prefix);
    let key = format!("{prefix}:{key_hash}");
    let inserted = match active_monitors().lock() {
        Ok(mut active) => active.insert(key.clone()),
        Err(_) => false,
    };
    if !inserted {
        return;
    }

    tauri::async_runtime::spawn(async move {
        let _guard = MonitorKeyGuard::new(key);
        let prefix = target.monitor_key_prefix();

        tokio::time::sleep(std::time::Duration::from_millis(BASELINE_CAPTURE_DELAY_MS)).await;
        let mut baseline = tokio::task::spawn_blocking({
            let text = injected_text.clone();
            move || capture_baseline_text_any(&text)
        })
        .await
        .ok()
        .flatten();

        if baseline.is_none() {
            tokio::time::sleep(std::time::Duration::from_millis(BASELINE_RETRY_DELAY_MS)).await;
            baseline = tokio::task::spawn_blocking({
                let text = injected_text.clone();
                move || capture_baseline_text_any(&text)
            })
            .await
            .ok()
            .flatten();
        }

        let Some(baseline_text) = baseline else {
            // Text not found at capture time — either UIAutomation is unavailable,
            // or the user deleted the output before the 250ms baseline window.
            // Only fire if the original window is still focused; a window switch
            // in that 750ms window would cause a false positive otherwise.
            let should_fire = tokio::task::spawn_blocking(move || {
                read_focused_text().is_some() && is_target_window_focused(target_hwnd)
            })
            .await
            .unwrap_or(false);
            if should_fire {
                log::info!("{prefix}: text absent at baseline, firing rejection");
                apply_rejection(&target, &db, &app, prefix);
            } else {
                log::debug!("{prefix}: anchor miss or window switched, skipping");
            }
            return;
        };
        let Some(anchor) = find_last_anchor(&baseline_text, &injected_text) else {
            log::debug!("{prefix}: anchor not found");
            return;
        };

        let rejection_threshold = injected_text.chars().count() / 10;
        let deadline =
            tokio::time::Instant::now() + std::time::Duration::from_secs(target.window_secs());

        loop {
            if tokio::time::Instant::now() >= deadline {
                break;
            }

            tokio::time::sleep(std::time::Duration::from_millis(REJECTION_POLL_MS)).await;

            let current = match tokio::task::spawn_blocking(read_focused_text).await {
                Ok(Some(t)) => t,
                _ => continue,
            };

            let rejected = match current_anchored_span(&baseline_text, &current, anchor) {
                Some(span) => span.chars().count() <= rejection_threshold,
                // Anchor tracking lost (edit too complex for prefix/suffix heuristic).
                // Reject if the injected text is completely absent AND the document
                // shrank — confirming deletion rather than a stale baseline.
                None => {
                    !current.contains(injected_text.as_str())
                        && current.len() < baseline_text.len()
                }
            };

            if rejected {
                // Guard against false positives from window switches: only fire
                // if the original injection window is still in the foreground.
                let still_focused = tokio::task::spawn_blocking(move || {
                    is_target_window_focused(target_hwnd)
                })
                .await
                .unwrap_or(false);
                if still_focused {
                    log::info!("{prefix}: deletion detected, firing rejection");
                    apply_rejection(&target, &db, &app, prefix);
                    return;
                }
                log::debug!("{prefix}: rejection signal but window switched, ignoring");
            }
        }
        log::debug!("{prefix}: window expired, no rejection detected");
    });
}

pub fn start_rejection_monitor(
    injected_text: String,
    applied_entry_ids: Vec<i64>,
    target_hwnd: usize,
    db: DbHandle,
    app: AppHandle,
) {
    if applied_entry_ids.is_empty() {
        return;
    }
    run_rejection_monitor(
        injected_text,
        RejectionTarget::DictEntries { ids: applied_entry_ids },
        target_hwnd,
        db,
        app,
    );
}

pub fn start_cache_rejection_monitor(
    injected_text: String,
    cache_key: String,
    target_hwnd: usize,
    db: DbHandle,
    app: AppHandle,
) {
    run_rejection_monitor(
        injected_text,
        RejectionTarget::CacheKey { key: cache_key },
        target_hwnd,
        db,
        app,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::db;

    #[derive(Debug, serde::Deserialize)]
    struct AutoLearnCase {
        name: String,
        original: String,
        corrected: String,
        expect: Vec<[String; 2]>,
        promotes_after_two_sessions: bool,
    }

    #[test]
    fn anchored_text_ignores_surrounding_content() {
        let injected = "Ask me about Koobernetes today";
        let baseline = "Before this. Ask me about Koobernetes today After this.";
        let current = "Before this. Ask me about Kubernetes today After this.";

        let diffs = detect_corrections_from_anchored_text(injected, baseline, current);
        assert_eq!(diffs.len(), 1);
        assert_eq!(diffs[0].mistake, "Koobernetes");
        assert_eq!(diffs[0].correction, "Kubernetes");
        assert!(diffs[0].confidence > 0.0);
    }

    #[test]
    fn edits_before_injected_span_are_ignored() {
        let injected = "Ask me about Koobernetes today";
        let baseline = "Before this. Ask me about Koobernetes today After this.";
        let current = "Before this please. Ask me about Kubernetes today After this.";

        assert!(detect_corrections_from_anchored_text(injected, baseline, current).is_empty());
    }

    #[test]
    fn simple_typo_correction_is_detected() {
        assert_eq!(
            diff_words(
                "please use Koobernetes today",
                "please use Kubernetes today"
            ),
            vec![("Koobernetes".to_string(), "Kubernetes".to_string())]
        );
    }

    #[test]
    fn casing_and_punctuation_only_changes_are_rejected() {
        assert!(diff_words("Ask.", "Ask").is_empty());
        assert!(diff_words("ask", "Ask").is_empty());
    }

    #[test]
    fn inserted_words_do_not_shift_later_alignment() {
        assert_eq!(
            diff_words(
                "please use Koobernetes today",
                "please use the Kubernetes today"
            ),
            vec![("Koobernetes".to_string(), "Kubernetes".to_string())]
        );
    }

    #[test]
    fn full_sentence_rewrites_are_ignored() {
        assert!(diff_words(
            "please use Koobernetes in the deployment tomorrow",
            "rewrite this whole sentence into something else"
        )
        .is_empty());
    }

    #[test]
    fn short_common_word_swaps_are_rejected() {
        assert!(diff_words("as me later", "ask me later").is_empty());
    }

    #[test]
    fn plain_suffix_completion_is_rejected() {
        assert!(diff_words("bran rot hostin", "bran rot hosting").is_empty());
        assert!(diff_words("send the file", "sends the file").is_empty());
        assert!(diff_words("say nugga", "say nuggaaaa").is_empty());
        assert!(diff_words("scratch the cat", "scratch the cats").is_empty());
        assert!(diff_words("we should do", "we should doing").is_empty());
    }

    #[test]
    fn short_technical_brand_term_correction_is_detected() {
        assert_eq!(
            diff_words("bran rock hosting", "bran qroq hosting"),
            vec![("rock".to_string(), "qroq".to_string())]
        );
        assert_eq!(
            diff_words("bran rot hosting", "bran qroq hosting"),
            vec![("rot".to_string(), "qroq".to_string())]
        );
    }

    #[test]
    fn one_letter_candidate_is_rejected() {
        assert!(diff_words("use x hosting", "use qroq hosting").is_empty());
        assert!(diff_words("use rock hosting", "use q hosting").is_empty());
    }

    #[test]
    fn repeated_candidate_counts_once_per_session() {
        let db = db::open(":memory:").expect("test db");
        let mut recorded = HashSet::new();

        assert!(!record_candidate(
            &db,
            &mut recorded,
            "test-app",
            "Koobernetes".to_string(),
            "Kubernetes".to_string(),
            0.9,
        ));
        assert!(!record_candidate(
            &db,
            &mut recorded,
            "test-app",
            "Koobernetes".to_string(),
            "Kubernetes".to_string(),
            0.9,
        ));

        let count = db::count_pending_corrections_recent(
            &db,
            "Koobernetes",
            "Kubernetes",
            PENDING_RETENTION_DAYS,
        )
        .expect("pending count");
        assert_eq!(count, 1);
    }

    #[test]
    fn candidate_promotes_after_two_sessions() {
        let db = db::open(":memory:").expect("test db");

        for expected in [false, true] {
            let mut recorded = HashSet::new();
            assert_eq!(
                record_candidate(
                    &db,
                    &mut recorded,
                    "test-app",
                    "Koobernetes".to_string(),
                    "Kubernetes".to_string(),
                    0.9,
                ),
                expected
            );
        }

        let entries = db::query_dictionary(&db).expect("dictionary");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].term, "Kubernetes");
        assert_eq!(entries[0].mistake.as_deref(), Some("Koobernetes"));
        assert!(entries[0].auto_learned);
    }

    #[test]
    fn short_technical_term_promotes_only_after_two_sessions() {
        let db = db::open(":memory:").expect("test db");

        for expected in [false, true] {
            let mut recorded = HashSet::new();
            assert_eq!(
                record_candidate(
                    &db,
                    &mut recorded,
                    "test-app",
                    "rock".to_string(),
                    "qroq".to_string(),
                    0.9,
                ),
                expected
            );
        }

        let entries = db::query_dictionary(&db).expect("dictionary");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].term, "qroq");
        assert_eq!(entries[0].mistake.as_deref(), Some("rock"));
        assert!(entries[0].auto_learned);
    }

    #[test]
    fn stable_text_gate_waits_for_consecutive_identical_reads() {
        let mut gate = StableTextGate::default();

        assert_eq!(gate.observe("say nugga".to_string()), None);
        assert_eq!(gate.observe("say nuggaaaa".to_string()), None);
        assert_eq!(gate.observe("say nuggaaaaa".to_string()), None);
        assert_eq!(gate.observe("say nuggaaaa".to_string()), None);
        assert_eq!(
            gate.observe("say nuggaaaa".to_string()),
            Some("say nuggaaaa")
        );
        assert_eq!(gate.observe("say nuggaaaa".to_string()), None);
    }

    #[test]
    fn auto_learn_regression_matrix() {
        let raw = include_str!("../../testdata/auto_learn_cases.json");
        let cases: Vec<AutoLearnCase> = serde_json::from_str(raw).expect("valid cases");

        for case in cases {
            let actual = diff_words(&case.original, &case.corrected);
            let expected: Vec<(String, String)> = case
                .expect
                .iter()
                .map(|pair| (pair[0].clone(), pair[1].clone()))
                .collect();
            assert_eq!(actual, expected, "case {}", case.name);

            if case.promotes_after_two_sessions {
                assert!(
                    !expected.is_empty(),
                    "case {} marked promotable without expected pair",
                    case.name
                );
                let db = db::open(":memory:").expect("test db");
                let (mistake, correction) = expected[0].clone();

                for expected_promoted in [false, true] {
                    let mut recorded = HashSet::new();
                    assert_eq!(
                        record_candidate(
                            &db,
                            &mut recorded,
                            "test-app",
                            mistake.clone(),
                            correction.clone(),
                            0.9,
                        ),
                        expected_promoted,
                        "case {} promotion threshold",
                        case.name
                    );
                }
            }
        }
    }
}
