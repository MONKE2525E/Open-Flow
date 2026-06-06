use std::collections::HashSet;
use std::sync::{Mutex, OnceLock};
use tauri::{AppHandle, Emitter};
use tauri_plugin_store::StoreExt;

#[cfg(windows)]
use crate::core::context_probe::{
    describe_selection_state, resolve_context_from_tail, stable_metadata_hash, ContextProbeSource,
    InjectionContextProbe, SelectionState,
};
#[cfg(not(windows))]
use crate::core::context_probe::{ContextProbeSource, InjectionContextProbe};
#[cfg(windows)]
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering};

use crate::core::correction_diff::{detect_span_corrections, CandidateCorrection};
use crate::core::text_context::is_invisible_prefix_char as is_invisible_probe_char;
#[cfg(any(windows, test))]
use crate::core::text_context::SentenceContext;
use crate::data::{db, store};
use crate::DbHandle;

const MONITOR_WINDOW_SECS: u64 = 60;
const POLL_INTERVAL_SECS: u64 = 2;
const BASELINE_CAPTURE_DELAY_MS: u64 = 250;
const BASELINE_RETRY_DELAY_MS: u64 = 500;
const EVENT_MONITOR_POLL_MS: u64 = 250;
const REJECTION_WINDOW_SECS: u64 = 8;
const REJECTION_POLL_MS: u64 = 500;
const CACHE_REJECTION_WINDOW_SECS: u64 = 10;
const PENDING_RETENTION_DAYS: i64 = 2;
const PROMOTION_THRESHOLD: i64 = 2;
const STABLE_TEXT_OBSERVATIONS_REQUIRED: usize = 2;
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

#[cfg(windows)]
struct EventModeHookGuard;

#[cfg(windows)]
impl EventModeHookGuard {
    fn new() -> Self {
        ACTIVE_EVENT_MODE_MONITORS.fetch_add(1, Ordering::SeqCst);
        Self
    }
}

#[cfg(windows)]
impl Drop for EventModeHookGuard {
    fn drop(&mut self) {
        loop {
            let current = ACTIVE_EVENT_MODE_MONITORS.load(Ordering::SeqCst);
            if current == 0 {
                return;
            }
            if ACTIVE_EVENT_MODE_MONITORS
                .compare_exchange(current, current - 1, Ordering::SeqCst, Ordering::SeqCst)
                .is_ok()
            {
                if current == 1 {
                    request_value_change_hook_shutdown();
                }
                return;
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TextAnchor {
    start: usize,
    end: usize,
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

fn evidence_session_id(seed: &str, app_context: &str) -> String {
    let (seed_hash, app_hash) = pair_hash(seed, app_context);
    let now_nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or_default();
    format!("{seed_hash}:{app_hash}:{now_nanos}")
}

fn monitor_key(injected_text: &str, app_context: &str) -> String {
    let (lhs, rhs) = pair_hash(injected_text, app_context);
    format!("{rhs}:{lhs}")
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

/// Source A: diff raw vs clean LLM output and feed divergence as evidence.
/// Called from the pipeline immediately after cleanup, before injection.
/// Works cross-platform (no accessibility required).
pub fn record_cleanup_divergence(
    raw: &str,
    clean: &str,
    app_context: &str,
    db: DbHandle,
    app: tauri::AppHandle,
    window_days: i64,
) {
    use crate::core::correction_diff::is_plausible_transcription_confusion;

    if raw.trim().is_empty() || clean.trim().is_empty() || raw == clean {
        return;
    }

    let candidates = detect_span_corrections(raw, clean);
    if candidates.is_empty() {
        return;
    }

    let session_id = evidence_session_id(raw, app_context);
    for candidate in candidates {
        // Gate: must be a plausible transcription confusion, not a grammar fix.
        if !is_plausible_transcription_confusion(&candidate.mistake, &candidate.correction) {
            log::debug!(
                "auto-learn: divergence candidate rejected by phonetic gate: {:?} → {:?}",
                candidate.mistake,
                candidate.correction
            );
            continue;
        }
        record_evidence_and_promote(
            &db,
            &app,
            &candidate.mistake,
            &candidate.correction,
            "cleanup_divergence",
            SOURCE_WEIGHT_CLEANUP_DIVERGENCE,
            candidate.confidence,
            &session_id,
            app_context,
            window_days,
        );
    }
}

// Thresholds for the evidence scoring engine.
const SCORE_AUTO: f64 = 1.6;
const SCORE_SUGGEST: f64 = 0.7;
const EVIDENCE_HALF_LIFE_DAYS: f64 = 1.5;
// Source weights: post-edit (user typed it themselves) is stronger than cleanup divergence.
pub const SOURCE_WEIGHT_POST_EDIT: f64 = 1.0;
pub const SOURCE_WEIGHT_CLEANUP_DIVERGENCE: f64 = 0.6;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EvidenceRecordOutcome {
    Ignored,
    Recorded,
    Suggested,
    Promoted,
    PromotionSkipped,
}

/// Insert one evidence observation and run the scoring engine.
/// Returns true if a new dictionary entry was auto-promoted.
/// source: "post_edit" | "cleanup_divergence"
/// window_days: from PipelineConfig.auto_learn_window_days
pub fn record_evidence_and_promote(
    db: &DbHandle,
    app: &tauri::AppHandle,
    wrong: &str,
    correct: &str,
    source: &str,
    weight: f64,
    confidence: f64,
    session_id: &str,
    app_context: &str,
    window_days: i64,
) -> bool {
    match record_evidence_and_maybe_promote(
        db,
        wrong,
        correct,
        source,
        weight,
        confidence,
        session_id,
        app_context,
        window_days,
    ) {
        EvidenceRecordOutcome::Promoted => {
            app.emit("open-flow:dictionary-updated", ()).ok();
            true
        }
        EvidenceRecordOutcome::Suggested => {
            app.emit("open-flow:suggestion-added", ()).ok();
            false
        }
        _ => false,
    }
}

fn record_evidence_and_maybe_promote(
    db: &DbHandle,
    wrong: &str,
    correct: &str,
    source: &str,
    weight: f64,
    confidence: f64,
    session_id: &str,
    app_context: &str,
    window_days: i64,
) -> EvidenceRecordOutcome {
    // Skip pairs in the never-learn list.
    if db::is_never_learn_pair(db, wrong, correct).unwrap_or(false) {
        return EvidenceRecordOutcome::Ignored;
    }

    if confidence < MIN_CANDIDATE_CONFIDENCE {
        return EvidenceRecordOutcome::Ignored;
    }

    if let Err(e) = db::insert_correction_evidence(
        db,
        wrong,
        correct,
        source,
        weight,
        confidence,
        session_id,
        app_context,
    ) {
        log::warn!("auto-learn: evidence insert failed: {e}");
        return EvidenceRecordOutcome::Ignored;
    }

    let ev = match db::compute_evidence_score(
        db,
        wrong,
        correct,
        window_days,
        EVIDENCE_HALF_LIFE_DAYS,
    ) {
        Ok(ev) => ev,
        Err(e) => {
            log::warn!("auto-learn: score computation failed: {e}");
            return EvidenceRecordOutcome::Recorded;
        }
    };

    let corroborated = ev.distinct_sessions >= 2 || ev.distinct_sources >= 2;
    if !corroborated {
        return EvidenceRecordOutcome::Recorded;
    }

    if ev.score >= SCORE_AUTO {
        // Auto-add.
        let tier = if ev.score >= SCORE_AUTO * 1.5 {
            "high"
        } else {
            "medium"
        };
        match db::insert_dictionary_entry_auto_learned(db, correct, Some(wrong), tier) {
            Ok(true) => {
                let _ = db::delete_dictionary_suggestion(db, wrong, correct);
                let _ = db::log_auto_learn_event(
                    db,
                    "promotion",
                    "promoted",
                    app_context,
                    "",
                    "",
                    ev.score,
                );
                log::info!(
                    "auto-learn: auto-promoted {wrong:?} → {correct:?} (score={:.2})",
                    ev.score
                );
                return EvidenceRecordOutcome::Promoted;
            }
            Ok(false) => {
                log::debug!("auto-learn: auto-promote skipped (manual entry or mismatch)");
                return EvidenceRecordOutcome::PromotionSkipped;
            }
            Err(e) => {
                log::warn!("auto-learn: auto-promote failed: {e}");
                return EvidenceRecordOutcome::PromotionSkipped;
            }
        }
    } else if ev.score >= SCORE_SUGGEST {
        let summary = format!(
            "sessions={} sources={} score={:.2}",
            ev.distinct_sessions, ev.distinct_sources, ev.score
        );
        if let Err(e) = db::upsert_dictionary_suggestion(db, wrong, correct, ev.score, &summary) {
            log::warn!("auto-learn: suggestion upsert failed: {e}");
            return EvidenceRecordOutcome::Recorded;
        } else {
            log::debug!(
                "auto-learn: suggestion upserted {wrong:?} → {correct:?} (score={:.2})",
                ev.score
            );
            return EvidenceRecordOutcome::Suggested;
        }
    }

    EvidenceRecordOutcome::Recorded
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
    static FOCUSED_TEXT_STATE: std::cell::RefCell<FocusedTextState> = const { std::cell::RefCell::new(FocusedTextState::new()) };
}

#[cfg(windows)]
struct FocusedTextReader {
    automation: windows::Win32::UI::Accessibility::IUIAutomation,
}

#[cfg_attr(not(windows), allow(dead_code))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FocusedTextProbe {
    Text(String),
    NonTextFocus,
    Unavailable,
}

#[cfg(windows)]
fn control_type_label(control_type: i32) -> String {
    use windows::Win32::UI::Accessibility::{
        UIA_CustomControlTypeId, UIA_DocumentControlTypeId, UIA_EditControlTypeId,
        UIA_PaneControlTypeId, UIA_TextControlTypeId, UIA_WindowControlTypeId,
    };

    match control_type {
        value if value == UIA_EditControlTypeId.0 => "edit".to_string(),
        value if value == UIA_DocumentControlTypeId.0 => "document".to_string(),
        value if value == UIA_TextControlTypeId.0 => "text".to_string(),
        value if value == UIA_PaneControlTypeId.0 => "pane".to_string(),
        value if value == UIA_WindowControlTypeId.0 => "window".to_string(),
        value if value == UIA_CustomControlTypeId.0 => "custom".to_string(),
        other => format!("control_type_{other}"),
    }
}

#[cfg_attr(not(windows), allow(dead_code))]
fn is_effectively_empty_text(text: &str) -> bool {
    text.chars().all(is_invisible_probe_char)
}

#[cfg(test)]
fn classify_caret_char(ch: char) -> Option<SentenceContext> {
    if matches!(ch, '.' | '!' | '?' | '\n' | '\r') {
        return Some(SentenceContext::NewSentence);
    }
    if is_invisible_probe_char(ch) {
        return None;
    }
    if ch.is_alphanumeric()
        || matches!(
            ch,
            ',' | ';' | ':' | '-' | '–' | '—' | '/' | '\\' | ')' | ']' | '}' | '>'
        )
    {
        return Some(SentenceContext::MidSentence);
    }
    None
}

#[cfg(windows)]
unsafe fn read_previous_context_text(
    range: &windows::Win32::UI::Accessibility::IUIAutomationTextRange,
) -> Option<String> {
    use windows::Win32::UI::Accessibility::{TextPatternRangeEndpoint_Start, TextUnit_Character};

    let caret = range.Clone().ok()?;
    let moved = caret
        .MoveEndpointByUnit(TextPatternRangeEndpoint_Start, TextUnit_Character, -64)
        .ok()?;
    if moved == 0 {
        return None;
    }

    let text = caret.GetText(-1).ok()?.to_string();
    if text.is_empty() {
        None
    } else {
        Some(text)
    }
}

#[cfg(windows)]
unsafe fn range_is_collapsed(
    range: &windows::Win32::UI::Accessibility::IUIAutomationTextRange,
) -> bool {
    use windows::Win32::UI::Accessibility::{
        TextPatternRangeEndpoint_End, TextPatternRangeEndpoint_Start,
    };

    matches!(
        range.CompareEndpoints(
            TextPatternRangeEndpoint_Start,
            range,
            TextPatternRangeEndpoint_End
        ),
        Ok(0)
    )
}

#[cfg(test)]
fn resolve_injection_context(field_empty: bool, caret_context: Option<char>) -> SentenceContext {
    if field_empty {
        SentenceContext::NewSentence
    } else if let Some(ch) = caret_context {
        classify_caret_char(ch).unwrap_or(SentenceContext::Unknown)
    } else {
        SentenceContext::Unknown
    }
}

#[cfg(windows)]
struct FocusedTextState {
    // Reader drops before COM guard because fields drop in declaration order.
    reader: Option<Option<FocusedTextReader>>,
    com: Option<ComGuard>,
}

#[cfg(windows)]
impl FocusedTextState {
    const fn new() -> Self {
        Self {
            reader: None,
            com: None,
        }
    }
}

#[cfg(windows)]
impl FocusedTextReader {
    fn new() -> Option<Self> {
        use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_INPROC_SERVER};
        use windows::Win32::UI::Accessibility::{CUIAutomation, IUIAutomation};

        unsafe {
            let automation: IUIAutomation =
                CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER).ok()?;
            Some(Self { automation })
        }
    }

    fn read_probe(&self) -> FocusedTextProbe {
        use windows::Win32::UI::Accessibility::{
            IUIAutomationTextPattern, IUIAutomationValuePattern, UIA_TextPatternId,
            UIA_ValuePatternId,
        };

        unsafe {
            let element = match self.automation.GetFocusedElement() {
                Ok(element) => element,
                Err(_) => return FocusedTextProbe::Unavailable,
            };
            let mut saw_text_pattern = false;
            let mut accessible_empty: Option<String> = None;

            if let Ok(pattern) =
                element.GetCurrentPatternAs::<IUIAutomationValuePattern>(UIA_ValuePatternId)
            {
                saw_text_pattern = true;
                if let Ok(val) = pattern.CurrentValue() {
                    let s = val.to_string();
                    if !is_effectively_empty_text(&s) {
                        return FocusedTextProbe::Text(s);
                    }
                    accessible_empty = Some(s);
                }
            }

            if let Ok(pattern) =
                element.GetCurrentPatternAs::<IUIAutomationTextPattern>(UIA_TextPatternId)
            {
                saw_text_pattern = true;
                if let Ok(doc_range) = pattern.DocumentRange() {
                    if let Ok(val) = doc_range.GetText(-1) {
                        let s = val.to_string();
                        if !is_effectively_empty_text(&s) {
                            return FocusedTextProbe::Text(s);
                        }
                        accessible_empty = Some(s);
                    }
                }
            }

            if let Some(s) = accessible_empty {
                return FocusedTextProbe::Text(s);
            }

            if saw_text_pattern {
                FocusedTextProbe::Text(String::new())
            } else {
                FocusedTextProbe::NonTextFocus
            }
        }
    }

    fn read_injection_context_probe(&self) -> InjectionContextProbe {
        use windows::Win32::UI::Accessibility::{
            IUIAutomationTextPattern, IUIAutomationTextPattern2, IUIAutomationValuePattern,
            UIA_TextPattern2Id, UIA_TextPatternId, UIA_ValuePatternId,
        };

        unsafe {
            let element = match self.automation.GetFocusedElement() {
                Ok(element) => element,
                Err(_) => {
                    return InjectionContextProbe::unavailable(
                        ContextProbeSource::Unavailable,
                        "unknown",
                    );
                }
            };

            let control_type = element
                .CurrentControlType()
                .map(|value| control_type_label(value.0))
                .unwrap_or_else(|_| "unknown".to_string());
            let value_pattern = element
                .GetCurrentPatternAs::<IUIAutomationValuePattern>(UIA_ValuePatternId)
                .ok();
            let text_pattern = element
                .GetCurrentPatternAs::<IUIAutomationTextPattern>(UIA_TextPatternId)
                .ok();
            let text_pattern2 = element
                .GetCurrentPatternAs::<IUIAutomationTextPattern2>(UIA_TextPattern2Id)
                .ok();
            let read_only = value_pattern
                .as_ref()
                .and_then(|pattern| pattern.CurrentIsReadOnly().ok())
                .map(|value| value.as_bool());
            let value_is_empty = value_pattern
                .as_ref()
                .and_then(|pattern| pattern.CurrentValue().ok())
                .map(|value| is_effectively_empty_text(&value.to_string()));
            let automation_id = element
                .CurrentAutomationId()
                .map(|value| value.to_string())
                .unwrap_or_default();
            let class_name = element
                .CurrentClassName()
                .map(|value| value.to_string())
                .unwrap_or_default();
            let native_hwnd = element
                .CurrentNativeWindowHandle()
                .map(|value| format!("{:p}", value.0))
                .unwrap_or_default();
            let control_identity_hash = stable_metadata_hash(&[
                control_type.as_str(),
                automation_id.as_str(),
                class_name.as_str(),
                native_hwnd.as_str(),
            ]);

            if read_only == Some(true) {
                return InjectionContextProbe {
                    context: SentenceContext::Unknown,
                    source: ContextProbeSource::UnsupportedControl,
                    context_tail: String::new(),
                    control_type,
                    selection_state: SelectionState::Unknown,
                    control_identity_hash,
                };
            }

            if value_is_empty == Some(true) {
                return InjectionContextProbe {
                    context: SentenceContext::NewSentence,
                    source: ContextProbeSource::EmptyField,
                    context_tail: String::new(),
                    control_type,
                    selection_state: SelectionState::Unknown,
                    control_identity_hash,
                };
            }

            let mut range_seen = false;
            let mut range_collapsed = false;
            let mut caret_context: Option<String> = None;
            let mut source = ContextProbeSource::UnsupportedControl;

            if let Some(pattern) = &text_pattern2 {
                let mut is_active = windows::core::BOOL::default();
                if let Ok(range) = pattern.GetCaretRange(&mut is_active) {
                    if is_active.as_bool() {
                        range_seen = true;
                        range_collapsed = true;
                        caret_context = read_previous_context_text(&range);
                        if caret_context.is_some() {
                            source = ContextProbeSource::CaretLocal;
                        } else {
                            source = ContextProbeSource::EmptyField;
                        }
                    }
                }
            }

            if caret_context.is_none() && !range_seen {
                if let Some(pattern) = &text_pattern {
                    if let Ok(selection) = pattern.GetSelection() {
                        if let Ok(len) = selection.Length() {
                            if len == 1 {
                                if let Ok(range) = selection.GetElement(0) {
                                    range_seen = true;
                                    range_collapsed = range_is_collapsed(&range);
                                    if range_collapsed {
                                        caret_context = read_previous_context_text(&range);
                                        source = if caret_context.is_some() {
                                            ContextProbeSource::CaretLocal
                                        } else {
                                            ContextProbeSource::EmptyField
                                        };
                                    } else {
                                        source = ContextProbeSource::AmbiguousSelection;
                                    }
                                }
                            } else if len > 1 {
                                range_seen = true;
                                source = ContextProbeSource::AmbiguousSelection;
                            }
                        }
                    }
                }
            }

            let field_empty = caret_context.is_none() && source == ContextProbeSource::EmptyField;
            let context = resolve_context_from_tail(field_empty, caret_context.as_deref());
            if matches!(context, SentenceContext::Unknown) && caret_context.is_some() {
                source = ContextProbeSource::AmbiguousSelection;
            }
            let selection_state = describe_selection_state(range_seen, range_collapsed);

            InjectionContextProbe {
                context,
                source,
                context_tail: caret_context.unwrap_or_default(),
                control_type,
                selection_state,
                control_identity_hash,
            }
        }
    }
}

#[cfg_attr(not(windows), allow(dead_code))]
#[cfg(windows)]
pub fn read_focused_text_probe() -> FocusedTextProbe {
    FOCUSED_TEXT_STATE.with(|cell| {
        let mut guard = cell.borrow_mut();
        if guard.com.is_none() {
            guard.com = Some(ComGuard::init());
        }
        let reader = guard.reader.get_or_insert_with(FocusedTextReader::new);
        match reader.as_ref() {
            Some(reader) => reader.read_probe(),
            None => FocusedTextProbe::Unavailable,
        }
    })
}

#[cfg(windows)]
pub fn read_focused_text() -> Option<String> {
    match read_focused_text_probe() {
        FocusedTextProbe::Text(text) => Some(text),
        FocusedTextProbe::NonTextFocus | FocusedTextProbe::Unavailable => None,
    }
}

#[cfg_attr(not(windows), allow(dead_code))]
#[cfg(windows)]
pub fn read_injection_context_probe() -> InjectionContextProbe {
    FOCUSED_TEXT_STATE.with(|cell| {
        let mut guard = cell.borrow_mut();
        if guard.com.is_none() {
            guard.com = Some(ComGuard::init());
        }
        let reader = guard.reader.get_or_insert_with(FocusedTextReader::new);
        match reader.as_ref() {
            Some(reader) => reader.read_injection_context_probe(),
            None => {
                InjectionContextProbe::unavailable(ContextProbeSource::Unavailable, "unavailable")
            }
        }
    })
}

#[cfg(windows)]
static VALUE_CHANGE_HOOK_READY: AtomicBool = AtomicBool::new(false);
#[cfg(windows)]
static VALUE_CHANGE_HOOK_SPAWNED: AtomicBool = AtomicBool::new(false);
#[cfg(windows)]
static VALUE_CHANGE_SEQ: AtomicU64 = AtomicU64::new(0);
#[cfg(windows)]
static VALUE_CHANGE_HOOK_THREAD_ID: AtomicU32 = AtomicU32::new(0);
#[cfg(windows)]
static ACTIVE_EVENT_MODE_MONITORS: AtomicUsize = AtomicUsize::new(0);
#[cfg(windows)]
static VALUE_CHANGE_HOOK_STOP_REQUESTED: AtomicBool = AtomicBool::new(false);
#[cfg(windows)]
const WM_APP_AUTO_LEARN_STOP: u32 = windows::Win32::UI::WindowsAndMessaging::WM_APP + 17;

#[cfg(windows)]
fn request_value_change_hook_shutdown() {
    use windows::Win32::Foundation::{LPARAM, WPARAM};
    use windows::Win32::UI::WindowsAndMessaging::PostThreadMessageW;

    if !VALUE_CHANGE_HOOK_SPAWNED.load(Ordering::SeqCst) {
        return;
    }
    VALUE_CHANGE_HOOK_STOP_REQUESTED.store(true, Ordering::SeqCst);

    let thread_id = VALUE_CHANGE_HOOK_THREAD_ID.load(Ordering::SeqCst);
    if thread_id == 0 {
        return;
    }

    unsafe {
        if PostThreadMessageW(thread_id, WM_APP_AUTO_LEARN_STOP, WPARAM(0), LPARAM(0)).is_err() {
            log::debug!(
                "auto-learn: failed to post stop message to value-change hook thread id={thread_id}"
            );
        }
    }
}

#[cfg(windows)]
unsafe extern "system" fn value_change_event_proc(
    _hook: windows::Win32::UI::Accessibility::HWINEVENTHOOK,
    event: u32,
    hwnd: windows::Win32::Foundation::HWND,
    _id_object: i32,
    _id_child: i32,
    _id_event_thread: u32,
    _event_time: u32,
) {
    use windows::Win32::UI::WindowsAndMessaging::{
        GetForegroundWindow, IsChild, EVENT_OBJECT_VALUECHANGE,
    };

    if event != EVENT_OBJECT_VALUECHANGE {
        return;
    }
    if hwnd.0.is_null() {
        return;
    }

    let foreground = GetForegroundWindow();
    if foreground.0.is_null() {
        return;
    }

    if hwnd == foreground || IsChild(foreground, hwnd).as_bool() {
        VALUE_CHANGE_SEQ.fetch_add(1, Ordering::Relaxed);
    }
}

#[cfg(windows)]
fn ensure_value_change_hook() -> bool {
    if VALUE_CHANGE_HOOK_READY.load(Ordering::Relaxed)
        && !VALUE_CHANGE_HOOK_STOP_REQUESTED.load(Ordering::SeqCst)
    {
        return true;
    }
    if VALUE_CHANGE_HOOK_SPAWNED
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return VALUE_CHANGE_HOOK_READY.load(Ordering::Relaxed)
            && !VALUE_CHANGE_HOOK_STOP_REQUESTED.load(Ordering::SeqCst);
    }

    let (spawned, should_reset_flags) = {
        let (ready_tx, ready_rx) = std::sync::mpsc::channel();

        let spawn_result = std::thread::Builder::new()
            .name("auto_learn_value_change_hook".to_string())
            .spawn(move || unsafe {
                use windows::Win32::System::Threading::GetCurrentThreadId;
                use windows::Win32::UI::Accessibility::{SetWinEventHook, UnhookWinEvent};
                use windows::Win32::UI::WindowsAndMessaging::{
                    DispatchMessageW, GetMessageW, PeekMessageW, TranslateMessage,
                    EVENT_OBJECT_VALUECHANGE, MSG, PM_NOREMOVE, WINEVENT_OUTOFCONTEXT,
                };

                let thread_id = GetCurrentThreadId();
                VALUE_CHANGE_HOOK_THREAD_ID.store(thread_id, Ordering::SeqCst);
                let mut queue_msg = MSG::default();
                let _ = PeekMessageW(&mut queue_msg, None, 0, 0, PM_NOREMOVE);

                let hook = SetWinEventHook(
                    EVENT_OBJECT_VALUECHANGE,
                    EVENT_OBJECT_VALUECHANGE,
                    None,
                    Some(value_change_event_proc),
                    0,
                    0,
                    WINEVENT_OUTOFCONTEXT,
                );

                let ready = !hook.is_invalid();
                let _ = ready_tx.send(ready);
                if !ready {
                    VALUE_CHANGE_HOOK_THREAD_ID.store(0, Ordering::SeqCst);
                    VALUE_CHANGE_HOOK_SPAWNED.store(false, Ordering::Relaxed);
                    VALUE_CHANGE_HOOK_STOP_REQUESTED.store(false, Ordering::SeqCst);
                    return;
                }
                VALUE_CHANGE_HOOK_READY.store(true, Ordering::Relaxed);
                VALUE_CHANGE_HOOK_STOP_REQUESTED.store(false, Ordering::SeqCst);
                if ACTIVE_EVENT_MODE_MONITORS.load(Ordering::SeqCst) == 0 {
                    let _ = UnhookWinEvent(hook);
                    VALUE_CHANGE_HOOK_READY.store(false, Ordering::Relaxed);
                    VALUE_CHANGE_HOOK_SPAWNED.store(false, Ordering::Relaxed);
                    VALUE_CHANGE_HOOK_THREAD_ID.store(0, Ordering::SeqCst);
                    VALUE_CHANGE_HOOK_STOP_REQUESTED.store(false, Ordering::SeqCst);
                    return;
                }

                let mut msg = MSG::default();
                loop {
                    let status = GetMessageW(&mut msg, None, 0, 0).0;
                    if status == -1 {
                        log::error!("GetMessageW failed in auto-learn hook thread");
                        break;
                    }
                    if status == 0 {
                        break;
                    }
                    if msg.message == WM_APP_AUTO_LEARN_STOP {
                        if ACTIVE_EVENT_MODE_MONITORS.load(Ordering::SeqCst) == 0 {
                            break;
                        }
                        VALUE_CHANGE_HOOK_STOP_REQUESTED.store(false, Ordering::SeqCst);
                        continue;
                    }
                    let _ = TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }

                let _ = UnhookWinEvent(hook);
                VALUE_CHANGE_HOOK_READY.store(false, Ordering::Relaxed);
                VALUE_CHANGE_HOOK_SPAWNED.store(false, Ordering::Relaxed);
                VALUE_CHANGE_HOOK_THREAD_ID.store(0, Ordering::SeqCst);
                VALUE_CHANGE_HOOK_STOP_REQUESTED.store(false, Ordering::SeqCst);
            });

        match spawn_result {
            Ok(_) => match ready_rx.recv_timeout(std::time::Duration::from_secs(2)) {
                Ok(ready) => (ready, false),
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => (false, false),
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => (false, true),
            },
            Err(_) => (false, true),
        }
    };

    if should_reset_flags {
        VALUE_CHANGE_HOOK_SPAWNED.store(false, Ordering::Relaxed);
        VALUE_CHANGE_HOOK_STOP_REQUESTED.store(false, Ordering::SeqCst);
    }
    spawned
}

#[cfg_attr(not(windows), allow(dead_code))]
#[cfg(not(windows))]
pub fn read_focused_text_probe() -> FocusedTextProbe {
    FocusedTextProbe::Unavailable
}

#[cfg(target_os = "macos")]
pub fn read_focused_text() -> Option<String> {
    crate::core::context_probe_macos::read_focused_text_sync()
}

#[cfg(not(any(windows, target_os = "macos")))]
pub fn read_focused_text() -> Option<String> {
    None
}

#[cfg_attr(not(windows), allow(dead_code))]
#[cfg(not(windows))]
pub fn read_injection_context_probe() -> InjectionContextProbe {
    InjectionContextProbe::unavailable(ContextProbeSource::Unavailable, "unavailable")
}

fn auto_learn_event_mode_enabled(app: &AppHandle) -> bool {
    app.store("settings.json")
        .ok()
        .and_then(|store| {
            store
                .get(store::AUTO_LEARN_EVENT_MODE)
                .and_then(|v| v.as_bool())
        })
        .unwrap_or(false)
}

fn event_mode_poll_sleep_duration(hook_ready: bool) -> std::time::Duration {
    if hook_ready {
        std::time::Duration::from_millis(EVENT_MONITOR_POLL_MS)
    } else {
        std::time::Duration::from_secs(POLL_INTERVAL_SECS)
    }
}

pub fn start_monitor(
    injected_text: String,
    app_context: String,
    db: DbHandle,
    app: AppHandle,
    window_days: i64,
) {
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

    let event_mode = auto_learn_event_mode_enabled(&app);

    std::thread::spawn(move || {
        let _monitor_guard = MonitorKeyGuard::new(key);
        #[cfg(windows)]
        let _event_mode_hook_guard = if event_mode {
            Some(EventModeHookGuard::new())
        } else {
            None
        };

        if let Err(e) = db::prune_pending_corrections(&db, PENDING_RETENTION_DAYS) {
            log::warn!("auto-learn prune failed: {e}");
        }
        let _ = db::log_auto_learn_event(
            &db,
            "monitor",
            if event_mode {
                "event_mode"
            } else {
                "poll_mode"
            },
            &app_context,
            "",
            "",
            0.0,
        );

        std::thread::sleep(std::time::Duration::from_millis(BASELINE_CAPTURE_DELAY_MS));
        let mut baseline_text = capture_baseline_text(&injected_text);

        if baseline_text.is_none() {
            std::thread::sleep(std::time::Duration::from_millis(BASELINE_RETRY_DELAY_MS));
            baseline_text = capture_baseline_text(&injected_text);
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
            std::time::Instant::now() + std::time::Duration::from_secs(MONITOR_WINDOW_SECS);
        let mut recorded_this_session: HashSet<(String, String)> = HashSet::new();
        // Stable within this monitor, unique across repeated identical dictations.
        let session_id = evidence_session_id(&injected_text, &app_context);
        #[cfg(windows)]
        let mut last_event_seq = VALUE_CHANGE_SEQ.load(Ordering::Relaxed);

        loop {
            if std::time::Instant::now() >= deadline {
                break;
            }

            if event_mode {
                #[cfg(windows)]
                {
                    if ensure_value_change_hook() {
                        let timeout_at = std::time::Instant::now()
                            + std::time::Duration::from_secs(POLL_INTERVAL_SECS);
                        let mut saw_event = false;
                        while std::time::Instant::now() < timeout_at {
                            let seq = VALUE_CHANGE_SEQ.load(Ordering::Relaxed);
                            if seq != last_event_seq {
                                last_event_seq = seq;
                                saw_event = true;
                                break;
                            }
                            std::thread::sleep(event_mode_poll_sleep_duration(true));
                        }
                        if !saw_event {
                            continue;
                        }
                    } else {
                        std::thread::sleep(event_mode_poll_sleep_duration(false));
                    }
                }
                #[cfg(not(windows))]
                std::thread::sleep(event_mode_poll_sleep_duration(false));
            } else {
                std::thread::sleep(std::time::Duration::from_secs(POLL_INTERVAL_SECS));
            }

            let Some(current_text) = read_focused_text() else {
                continue;
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
                let key = (candidate.mistake.clone(), candidate.correction.clone());
                if recorded_this_session.contains(&key) {
                    continue;
                }
                recorded_this_session.insert(key);
                record_evidence_and_promote(
                    &db,
                    &app,
                    &candidate.mistake,
                    &candidate.correction,
                    "post_edit",
                    SOURCE_WEIGHT_POST_EDIT,
                    candidate.confidence,
                    &session_id,
                    &app_context,
                    window_days,
                );
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
                app.emit("open-flow:dictionary-entry-rejected", ids.len())
                    .ok();
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
        let baseline_char_count = baseline_text.chars().count();
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
                        && current.chars().count() < baseline_char_count
                }
            };

            if rejected {
                // Guard against false positives from window switches: only fire
                // if the original injection window is still in the foreground.
                let still_focused =
                    tokio::task::spawn_blocking(move || is_target_window_focused(target_hwnd))
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
        RejectionTarget::DictEntries {
            ids: applied_entry_ids,
        },
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
    use crate::core::text_context::SentenceContext;
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
    fn evidence_path_creates_suggestion_after_corroborated_score() {
        let db = db::open(":memory:").expect("test db");

        assert_eq!(
            record_evidence_and_maybe_promote(
                &db,
                "Koobernetes",
                "Kubernetes",
                "post_edit",
                SOURCE_WEIGHT_POST_EDIT,
                0.5,
                "session-1",
                "test-app",
                3,
            ),
            EvidenceRecordOutcome::Recorded
        );
        assert_eq!(
            record_evidence_and_maybe_promote(
                &db,
                "Koobernetes",
                "Kubernetes",
                "post_edit",
                SOURCE_WEIGHT_POST_EDIT,
                0.5,
                "session-2",
                "test-app",
                3,
            ),
            EvidenceRecordOutcome::Suggested
        );

        let suggestions = db::query_dictionary_suggestions(&db).expect("suggestions");
        assert_eq!(suggestions.len(), 1);
        assert_eq!(suggestions[0].wrong_word, "Koobernetes");
        assert_eq!(suggestions[0].correct_word, "Kubernetes");
    }

    #[test]
    fn evidence_path_promotes_after_corroborated_auto_score() {
        let db = db::open(":memory:").expect("test db");

        assert_eq!(
            record_evidence_and_maybe_promote(
                &db,
                "Koobernetes",
                "Kubernetes",
                "post_edit",
                SOURCE_WEIGHT_POST_EDIT,
                0.9,
                "session-1",
                "test-app",
                3,
            ),
            EvidenceRecordOutcome::Recorded
        );
        assert_eq!(
            record_evidence_and_maybe_promote(
                &db,
                "Koobernetes",
                "Kubernetes",
                "post_edit",
                SOURCE_WEIGHT_POST_EDIT,
                0.9,
                "session-2",
                "test-app",
                3,
            ),
            EvidenceRecordOutcome::Promoted
        );

        let entries = db::query_dictionary(&db).expect("dictionary");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].term, "Kubernetes");
        assert_eq!(entries[0].mistake.as_deref(), Some("Koobernetes"));
        assert!(entries[0].auto_learned);
    }

    #[test]
    fn evidence_path_corroborates_two_sources_in_one_session() {
        let db = db::open(":memory:").expect("test db");

        assert_eq!(
            record_evidence_and_maybe_promote(
                &db,
                "Koobernetes",
                "Kubernetes",
                "cleanup_divergence",
                SOURCE_WEIGHT_CLEANUP_DIVERGENCE,
                0.8,
                "session-1",
                "test-app",
                3,
            ),
            EvidenceRecordOutcome::Recorded
        );
        assert_eq!(
            record_evidence_and_maybe_promote(
                &db,
                "Koobernetes",
                "Kubernetes",
                "post_edit",
                SOURCE_WEIGHT_POST_EDIT,
                0.8,
                "session-1",
                "test-app",
                3,
            ),
            EvidenceRecordOutcome::Suggested
        );
    }

    #[test]
    fn evidence_path_respects_never_learn_pairs() {
        let db = db::open(":memory:").expect("test db");
        db::insert_never_learn_pair(&db, "Koobernetes", "Kubernetes").expect("never learn");

        assert_eq!(
            record_evidence_and_maybe_promote(
                &db,
                "Koobernetes",
                "Kubernetes",
                "post_edit",
                SOURCE_WEIGHT_POST_EDIT,
                0.9,
                "session-1",
                "test-app",
                3,
            ),
            EvidenceRecordOutcome::Ignored
        );

        let score =
            db::compute_evidence_score(&db, "Koobernetes", "Kubernetes", 3, 1.5).expect("score");
        assert_eq!(score.score, 0.0);
    }

    #[test]
    fn evidence_path_keeps_manual_conflict_as_suggestion_problem() {
        let db = db::open(":memory:").expect("test db");
        db::insert_dictionary_entry(&db, "Kubernetes", Some("manual typo")).expect("manual");

        assert_eq!(
            record_evidence_and_maybe_promote(
                &db,
                "Koobernetes",
                "Kubernetes",
                "post_edit",
                SOURCE_WEIGHT_POST_EDIT,
                0.9,
                "session-1",
                "test-app",
                3,
            ),
            EvidenceRecordOutcome::Recorded
        );
        assert_eq!(
            record_evidence_and_maybe_promote(
                &db,
                "Koobernetes",
                "Kubernetes",
                "post_edit",
                SOURCE_WEIGHT_POST_EDIT,
                0.9,
                "session-2",
                "test-app",
                3,
            ),
            EvidenceRecordOutcome::PromotionSkipped
        );

        let entries = db::query_dictionary(&db).expect("dictionary");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].mistake.as_deref(), Some("manual typo"));
        assert!(!entries[0].auto_learned);
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
    fn blank_field_probe_capitalizes() {
        let context = resolve_injection_context(true, None);
        assert_eq!(context, SentenceContext::NewSentence);
        assert!(context.should_capitalize());
    }

    #[test]
    fn unknown_probe_capitalizes() {
        let context = resolve_injection_context(false, None);
        assert_eq!(context, SentenceContext::Unknown);
        assert!(context.should_capitalize());
    }

    #[test]
    fn sentence_ending_characters_capitalize() {
        assert_eq!(
            resolve_injection_context(false, Some('.')),
            SentenceContext::NewSentence
        );
        assert_eq!(
            resolve_injection_context(false, Some('!')),
            SentenceContext::NewSentence
        );
        assert_eq!(
            resolve_injection_context(false, Some('?')),
            SentenceContext::NewSentence
        );
    }

    #[test]
    fn mid_sentence_probe_lowercases() {
        let context = resolve_injection_context(false, Some('a'));
        assert_eq!(context, SentenceContext::MidSentence);
        assert!(!context.should_capitalize());
    }

    #[test]
    fn blank_field_overrides_stale_context() {
        assert_eq!(
            resolve_injection_context(true, Some('a')),
            SentenceContext::NewSentence
        );
    }

    #[test]
    fn invisible_probe_characters_do_not_force_lowercase() {
        assert_eq!(classify_caret_char('\u{200b}'), None);
        assert_eq!(classify_caret_char('\u{feff}'), None);
        assert_eq!(classify_caret_char('"'), None);
        assert_eq!(classify_caret_char('('), None);
        assert_eq!(
            resolve_injection_context(false, Some('\u{200b}')),
            SentenceContext::Unknown
        );
        assert_eq!(
            resolve_injection_context(false, Some('"')),
            SentenceContext::Unknown
        );
    }

    #[test]
    fn event_mode_hook_unavailable_uses_poll_interval_sleep() {
        assert_eq!(
            event_mode_poll_sleep_duration(false),
            std::time::Duration::from_secs(POLL_INTERVAL_SECS)
        );
    }

    #[test]
    fn event_mode_hook_ready_uses_fast_poll_sleep() {
        assert_eq!(
            event_mode_poll_sleep_duration(true),
            std::time::Duration::from_millis(EVENT_MONITOR_POLL_MS)
        );
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
