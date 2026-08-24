use std::sync::{Mutex, OnceLock};
use std::time::Instant;

// Serializes the whole save-clipboard -> probe/sniff -> write -> paste ->
// restore-clipboard critical section across every call site (main pipeline,
// retry, settings "try it" preview) so two injections can never interleave
// their clipboard operations — one's "restore" putting back the *other's*
// dictated text instead of the user's real original clipboard, or clobbering
// a paste that's still settling. `tokio::sync::Mutex`, not `std::sync::Mutex`,
// since the guard is held across `.await` points.
static INJECTION_LOCK: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();
pub(super) fn injection_lock() -> &'static tokio::sync::Mutex<()> {
    INJECTION_LOCK.get_or_init(|| tokio::sync::Mutex::new(()))
}

use crate::core::context_probe::{ContextProbeSource, InjectionContextProbe, SelectionState};
use crate::core::text_context;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "windows")]
mod windows;

// Maximum bytes stored for backspace-tracking. Covers any practical editing sequence
// while keeping the per-injection allocation bounded.
const HISTORY_TAIL: usize = 512;

// Retry gap between successive OpenClipboard attempts when the clipboard is held
// by another process.
#[cfg(target_os = "windows")]
const CLIPBOARD_OPEN_RETRY_MS: u64 = 50;
#[cfg(target_os = "windows")]
const CLIPBOARD_RESTORE_RETRY_MS: u64 = 60;
#[cfg(target_os = "windows")]
const CLIPBOARD_RESTORE_ATTEMPTS: usize = 3;

// Settle time after SetForegroundWindow before writing to the clipboard; some
// compositor/DWM frame cycles are needed before the HWND is fully active.
#[cfg(target_os = "windows")]
const REFOCUS_SETTLE_MS: u64 = 150;

// Settle time after a successful clipboard write before beginning the Ctrl+V
// sequence - ensures the data is visible to the target app's clipboard reader.
#[cfg(target_os = "windows")]
const CLIPBOARD_WRITE_SETTLE_MS: u64 = 50;

// Retry gap between successive clipboard write attempts.
#[cfg(target_os = "windows")]
const CLIPBOARD_WRITE_RETRY_MS: u64 = 50;

// Gap between releasing modifier keys (Alt/Ctrl) and sending Ctrl+V.
// Without this, some apps (browsers, IDEs) process V without Ctrl because
// the alt-up and V-down land in the same message-pump cycle.
#[cfg(target_os = "windows")]
const MODIFIER_GAP_MS: u64 = 30;

// Grace window for a Win key that still reads down right before a paste —
// releasing Ctrl to stop dictation and releasing Win a beat later is normal
// human timing, not the stuck-key OS bug this guards against. Polled at
// WIN_KEY_GRACE_POLL_MS intervals up to WIN_KEY_GRACE_POLL_ATTEMPTS times
// (150ms total) before escalating to forced recovery.
#[cfg(target_os = "windows")]
const WIN_KEY_GRACE_POLL_MS: u64 = 30;
#[cfg(target_os = "windows")]
const WIN_KEY_GRACE_POLL_ATTEMPTS: u32 = 5;

// Settle time after releasing Win specifically, before the first
// is_win_key_down() check. Win participates in OS-wide/shell hotkey
// dispatch (unlike a plain app-level Ctrl/Alt release), which can need more
// time to settle than MODIFIER_GAP_MS's 30ms — that constant was tuned for
// beating an app's own message pump, not a system-wide hotkey matcher.
#[cfg(target_os = "windows")]
const WIN_KEY_RELEASE_SETTLE_MS: u64 = 80;

// Settle time after the paste (Ctrl+V) before restoring saved clipboard
// formats - gives the target app time to read the clipboard before we
// overwrite it with the original contents. `SendInput` only queues the
// keystroke into the system input queue; it does not block until the target
// window's message loop has actually processed it. Under CPU/GPU contention
// (e.g. a local STT/LLM model still winding down on the same machine) that
// processing can lag, and heavier editors (rich-text frameworks doing
// paste-parsing/transaction work, not a plain textbox) take longer to read
// the clipboard than a simple control does.
//
// Losing this race is not a cosmetic glitch: the target reads the clipboard
// AFTER we've already restored it, so it pastes whatever the user's
// clipboard held *before* dictation - silently, with no error - which can be
// arbitrarily large/irrelevant content overwriting their actual selection
// (observed in practice: a "ginormous" prior clipboard item pasted in place
// of the transcription). The cost of waiting longer than necessary is a
// fraction of a second of perceived latency; the cost of not waiting long
// enough is silent data corruption in whatever the user was working on. That
// asymmetry justifies a wide margin over a "usually enough" one - 80ms had
// none, 150ms still weakly matched a real report of this exact failure, so
// this doubles the margin again rather than inching it up. Lowered from 300
// to 250 per explicit user request to shorten perceived latency between
// back-to-back dictations; still well above the 150ms that produced a real
// corruption report, so the safety margin is kept, just narrower.
#[cfg(target_os = "windows")]
const PASTE_SETTLE_MS: u64 = 250;

#[cfg(test)]
const SENTENCE_ENDERS: &[char] = &['.', '!', '?', '\n', '\r'];
#[derive(Clone, Debug)]
enum CursorContextState {
    Unknown {
        _instant: Instant,
    },
    Known {
        hwnd: usize,
        tail: String,
        instant: Instant,
    },
}

// Post-paste verification text helpers. Pure functions (no UIA/OS access) so
// they're unit-testable on every platform; the Windows injection path drives
// the actual reads and owns the retry timing. They exist because Chromium/
// ProseMirror-style editors (ChatGPT, Claude, Slack, ...) commit a paste to
// their own state before the accessibility tree catches up — a caret-local
// read taken too early still sees the pre-paste text, which used to flag a
// *successful* paste as failed (the false positives reported in Chrome).

// Capped suffix for the cheap tail check — enough to catch "nothing landed" or
// "wrong window" without being thrown off by trailing whitespace/newline
// normalization differences between what we sent and what UIA reports back.
#[cfg_attr(not(windows), allow(dead_code))]
const PASTE_VERIFY_SUFFIX_CHARS: usize = 20;

// A `contains` cross-check on the control's full text only counts when the
// fragment is long enough to be distinctive; short dictations rely on the
// precise ends-with check instead.
#[cfg_attr(not(windows), allow(dead_code))]
const PASTE_VERIFY_CONTAINS_MIN_CHARS: usize = 12;

#[cfg_attr(not(windows), allow(dead_code))]
fn injected_suffix(injected: &str) -> String {
    let mut chars: Vec<char> = injected
        .trim_end()
        .chars()
        .rev()
        .take(PASTE_VERIFY_SUFFIX_CHARS)
        .collect();
    chars.reverse();
    chars.into_iter().collect()
}

// Cheap, lenient tail check for post-paste verification: true if the
// control's freshly-read caret-local tail ends with (roughly) the end of what
// we just injected.
#[cfg_attr(not(windows), allow(dead_code))]
fn paste_tail_matches(injected: &str, probe_tail: &str) -> bool {
    if injected.trim_end().is_empty() {
        return true;
    }
    probe_tail.trim_end().ends_with(&injected_suffix(injected))
}

// Full-text cross-check for the caret-range-lag case: the caret-range read
// can stay stale/anchored (or scoped to a stale block) even after the
// accessibility tree reflects the paste in the document/value text. Verifies
// the injected text actually reached the field by looking at the whole thing.
#[cfg_attr(not(windows), allow(dead_code))]
fn full_text_confirms_paste(injected: &str, full_text: &str) -> bool {
    let injected_trim = injected.trim_end();
    if injected_trim.is_empty() {
        return true;
    }
    let suffix = injected_suffix(injected);
    if full_text.trim_end().ends_with(&suffix) {
        return true;
    }
    // A paste into the middle of existing text leaves our tail mid-field, not
    // at the very end — only trust that when the fragment is distinctive.
    suffix.chars().count() >= PASTE_VERIFY_CONTAINS_MIN_CHARS && full_text.contains(&suffix)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_context_detects_sentence_boundary() {
        assert!(matches!(
            classify_context("Hello. "),
            ContextKind::SentenceBoundary
        ));
        assert!(matches!(
            classify_context("Hello?\n"),
            ContextKind::SentenceBoundary
        ));
        assert!(matches!(
            classify_context(""),
            ContextKind::SentenceBoundary
        ));
    }

    #[test]
    fn classify_context_detects_continuation() {
        assert!(matches!(
            classify_context("hello, "),
            ContextKind::Continuation
        ));
        assert!(matches!(
            classify_context("path/to/"),
            ContextKind::Continuation
        ));
        assert!(matches!(
            classify_context("label:"),
            ContextKind::Continuation
        ));
    }

    #[test]
    fn contextual_caps_disabled_preserves_caps_lock_uppercased_text() {
        // Regression: caps-lock uppercasing must be the final casing decision.
        // With contextual_caps off (as finalize.rs forces when caps lock is on),
        // a mid-sentence continuation must not lowercase the first letter of an
        // already-all-caps word ("THE" -> "tHE").
        let probe = InjectionContextProbe {
            context: crate::core::text_context::SentenceContext::MidSentence,
            source: ContextProbeSource::CaretLocal,
            context_tail: "hello ".to_string(),
            context_head: String::new(),
            left_reliable: true,
            right_reliable: true,
            selection_state: SelectionState::CollapsedCaret,
            control_identity_hash: "test".to_string(),
            control_type: "test".to_string(),
            target_id: 1,
        };
        let (adjusted, _, case_decision) = apply_probe_adjustments(
            "THE REPORT IS READY",
            false,
            false,
            "casual",
            "en",
            false,
            &probe,
        );
        assert_eq!(adjusted, "THE REPORT IS READY");
        assert!(matches!(
            case_decision,
            CaseDecision::ContextualCapsDisabled
        ));
    }

    #[test]
    fn confirmed_unfinished_text_lowercases_an_ordinary_leading_capital() {
        let probe = InjectionContextProbe {
            context: crate::core::text_context::SentenceContext::MidSentence,
            source: ContextProbeSource::CaretLocal,
            context_tail: "unfinished".to_string(),
            context_head: String::new(),
            left_reliable: true,
            right_reliable: true,
            selection_state: SelectionState::CollapsedCaret,
            control_identity_hash: "test".to_string(),
            control_type: "test".to_string(),
            target_id: 1,
        };
        let (adjusted, context, case_decision) =
            apply_probe_adjustments("Hello again", true, true, "casual", "en", false, &probe);
        assert_eq!(adjusted, " hello again");
        assert!(matches!(context, ContextKind::Continuation));
        assert!(matches!(
            case_decision,
            CaseDecision::ContinuationLowercased
        ));
    }

    #[test]
    fn confirmed_comma_continuation_lowercases_even_before_another_titlecase_word() {
        let probe = InjectionContextProbe {
            context: crate::core::text_context::SentenceContext::MidSentence,
            source: ContextProbeSource::CaretLocal,
            context_tail: "Yabba Dabba Dooba,".to_string(),
            context_head: String::new(),
            left_reliable: true,
            right_reliable: true,
            selection_state: SelectionState::CollapsedCaret,
            control_identity_hash: "test".to_string(),
            control_type: "test".to_string(),
            target_id: 1,
        };
        let (adjusted, context, case_decision) = apply_probe_adjustments(
            "Dabba Doo.",
            true,
            true,
            "casual",
            "en",
            false,
            &probe,
        );
        assert_eq!(adjusted, " dabba Doo.");
        assert!(matches!(context, ContextKind::Continuation));
        assert!(matches!(
            case_decision,
            CaseDecision::ContinuationLowercased
        ));
    }

    #[test]
    fn unconfirmed_left_edge_never_capitalizes_or_adds_leading_space() {
        let probe = InjectionContextProbe {
            context: crate::core::text_context::SentenceContext::NewSentence,
            source: ContextProbeSource::CaretLocal,
            context_tail: String::new(),
            context_head: "existing".to_string(),
            left_reliable: false,
            right_reliable: true,
            selection_state: SelectionState::CollapsedCaret,
            control_identity_hash: "test".to_string(),
            control_type: "test".to_string(),
            target_id: 1,
        };
        let (adjusted, context, case_decision) =
            apply_probe_adjustments("hello", true, true, "casual", "en", false, &probe);
        assert_eq!(adjusted, "hello ");
        assert!(matches!(context, ContextKind::Unknown));
        assert!(matches!(
            case_decision,
            CaseDecision::ConservativeDegradePreserved
        ));
    }

    #[test]
    fn history_tracks_manual_typing_and_backspace_by_window() {
        reset_injection_history();
        append_or_reset_injection_history(11, 'h');
        append_or_reset_injection_history(11, 'i');

        let state = last_injection().lock().expect("lock");
        match &*state {
            CursorContextState::Known { hwnd, tail, .. } => {
                assert_eq!(*hwnd, 11);
                assert_eq!(tail, "hi");
            }
            CursorContextState::Unknown { .. } => panic!("expected known state"),
        }
        drop(state);

        backspace_injection_history(22);
        let state = last_injection().lock().expect("lock");
        assert!(matches!(&*state, CursorContextState::Unknown { .. }));
    }

    #[test]
    fn paste_tail_matches_matches_our_suffix() {
        assert!(paste_tail_matches("hello world", "some prefix hello world"));
        assert!(paste_tail_matches("hello world", "hello world"));
        // trailing whitespace / newline normalization is tolerated
        assert!(paste_tail_matches("hello world ", "prefix hello world\n"));
        // long injected text only needs its final fragment to match
        let long = "the quick brown fox jumps over the lazy dog near the river";
        assert!(paste_tail_matches(
            long,
            "stale pre-paste text the quick brown fox jumps over the lazy dog near the river"
        ));
        // empty injected text trivially passes
        assert!(paste_tail_matches("", "anything at all"));
        // a genuinely different tail fails
        assert!(!paste_tail_matches("hello world", "some other text"));
    }

    #[test]
    fn full_text_confirms_paste_checks_whole_document() {
        // appended at the end of the field
        assert!(full_text_confirms_paste(
            "dictated text here",
            "existing prompt dictated text here"
        ));
        // pasted mid-document: our suffix sits mid-field, not at the end
        assert!(full_text_confirms_paste(
            "dictated text here",
            "start dictated text here rest of document"
        ));
        // short dictation still verified by the precise ends-with check
        assert!(full_text_confirms_paste("short", "abc short"));
        // ...but a short fragment is never trusted as a mid-field contains
        assert!(!full_text_confirms_paste("short", "abc shortx"));
        // empty injected text trivially passes
        assert!(full_text_confirms_paste("", "anything at all"));
        // unrelated text fails
        assert!(!full_text_confirms_paste(
            "didn't paste",
            "totally unrelated text"
        ));
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ContextKind {
    Unknown,
    SentenceBoundary,
    Continuation,
}
impl ContextKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Unknown => "unknown",
            Self::SentenceBoundary => "sentence_boundary",
            Self::Continuation => "continuation",
        }
    }
}

fn context_kind_from_sentence_context(
    context: crate::core::text_context::SentenceContext,
) -> ContextKind {
    match context {
        crate::core::text_context::SentenceContext::NewSentence => ContextKind::SentenceBoundary,
        crate::core::text_context::SentenceContext::MidSentence => ContextKind::Continuation,
        crate::core::text_context::SentenceContext::Unknown => ContextKind::Unknown,
    }
}
#[derive(Clone, Copy, Debug)]
enum CaseDecision {
    ContextualCapsDisabled,
    ConservativeDegradePreserved,
    UnknownContextPreserved,
    SentenceBoundaryCapitalized,
    SentenceBoundaryPreservedVeryCasual,
    ContinuationLowercased,
    ContinuationPreserved,
}
impl CaseDecision {
    fn as_str(self) -> &'static str {
        match self {
            Self::ContextualCapsDisabled => "contextual_caps_disabled",
            Self::ConservativeDegradePreserved => "conservative_degrade_preserved",
            Self::UnknownContextPreserved => "unknown_context_preserved",
            Self::SentenceBoundaryCapitalized => "sentence_boundary_capitalized",
            Self::SentenceBoundaryPreservedVeryCasual => "sentence_boundary_preserved_very_casual",
            Self::ContinuationLowercased => "continuation_lowercased",
            Self::ContinuationPreserved => "continuation_preserved",
        }
    }
}
pub struct InjectionOutcome {
    pub text: String,
    pub context_state: &'static str,
    pub case_decision: &'static str,
    pub probe_source: &'static str,
    pub selection_state: &'static str,
}
static LAST_INJECTION: OnceLock<Mutex<CursorContextState>> = OnceLock::new();
fn unknown_context() -> CursorContextState {
    CursorContextState::Unknown {
        _instant: Instant::now(),
    }
}
fn last_injection() -> &'static Mutex<CursorContextState> {
    LAST_INJECTION.get_or_init(|| Mutex::new(unknown_context()))
}
fn trim_tail_to_limit(text: &mut String) {
    if text.len() > HISTORY_TAIL {
        let excess = text.len() - HISTORY_TAIL;
        let mut trim_at = excess;
        while !text.is_char_boundary(trim_at) {
            trim_at += 1;
        }
        *text = text[trim_at..].to_owned();
    }
}
#[cfg(test)]
fn trimmed_context_for_decision(text: &str) -> &str {
    text.trim_end_matches(|c: char| c.is_whitespace() && !SENTENCE_ENDERS.contains(&c))
}
#[cfg(test)]
fn classify_context(tail: &str) -> ContextKind {
    let trimmed = trimmed_context_for_decision(tail);
    if trimmed
        .chars()
        .next_back()
        .map(|c| SENTENCE_ENDERS.contains(&c))
        .unwrap_or(true)
    {
        ContextKind::SentenceBoundary
    } else {
        ContextKind::Continuation
    }
}
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn unavailable_injection_probe() -> InjectionContextProbe {
    InjectionContextProbe::unavailable(ContextProbeSource::Unavailable, "unavailable")
}

fn apply_probe_adjustments(
    text: &str,
    contextual_caps: bool,
    auto_spacing: bool,
    profile: &str,
    language: &str,
    protected_initial_case: bool,
    probe: &InjectionContextProbe,
) -> (String, ContextKind, CaseDecision) {
    let formatting_enabled = contextual_caps || auto_spacing;
    let source_reliable = probe.source == ContextProbeSource::EmptyField
        || (probe.source == ContextProbeSource::CaretLocal
            && !matches!(probe.selection_state, SelectionState::Unknown));
    let left_reliable = formatting_enabled && source_reliable && probe.left_reliable;
    let right_reliable = formatting_enabled && source_reliable && probe.right_reliable;
    let context_kind = if left_reliable {
        context_kind_from_sentence_context(probe.context)
    } else {
        ContextKind::Unknown
    };
    let decision = text_context::decide_insertion(
        text,
        text_context::CaretTextContext {
            left: &probe.context_tail,
            right: &probe.context_head,
            left_reliable,
            right_reliable,
            language,
            casing_enabled: contextual_caps,
            preserve_sentence_case: profile == "very_casual",
            protected_initial_case,
        },
    );
    let adjusted = decision.text;
    let case_decision = if !contextual_caps && (formatting_enabled || left_reliable) {
        if !left_reliable {
            CaseDecision::ConservativeDegradePreserved
        } else {
            CaseDecision::ContextualCapsDisabled
        }
    } else if !contextual_caps {
        CaseDecision::ContextualCapsDisabled
    } else if !left_reliable {
        CaseDecision::ConservativeDegradePreserved
    } else {
        match decision.case_action {
            text_context::CaseAction::CapitalizeFirstWord => {
                CaseDecision::SentenceBoundaryCapitalized
            }
            text_context::CaseAction::LowercaseFirstWord => CaseDecision::ContinuationLowercased,
            text_context::CaseAction::Preserve
                if context_kind == ContextKind::SentenceBoundary && profile == "very_casual" =>
            {
                CaseDecision::SentenceBoundaryPreservedVeryCasual
            }
            text_context::CaseAction::Preserve if context_kind == ContextKind::Unknown => {
                CaseDecision::UnknownContextPreserved
            }
            text_context::CaseAction::Preserve => CaseDecision::ContinuationPreserved,
        }
    };

    // Redacted diagnostics for capitalization decisions (issue follow-up: CLI /
    // terminal inputs being read as mid-sentence). Logs the control identity and
    // the *class* of the last char before the caret — never the tail content.
    log::debug!(
        "injection: smart-format control_type={} probe_source={} context={} tail_len={} head_len={} left_reliable={} right_reliable={} tail_signal={} head_signal={} case_decision={} leading_space={} trailing_space={} reason={}",
        probe.control_type,
        probe.source.as_str(),
        probe.context.as_str(),
        probe.context_tail.chars().count(),
        probe.context_head.chars().count(),
        left_reliable,
        right_reliable,
        context_tail_signal(&probe.context_tail),
        context_head_signal(&probe.context_head),
        case_decision.as_str(),
        decision.leading_space,
        decision.trailing_space,
        decision.reason,
    );

    (adjusted, context_kind, case_decision)
}

fn context_head_signal(head: &str) -> &'static str {
    match head.chars().find(|c| !c.is_whitespace()) {
        None => {
            if head.contains('\n') || head.contains('\r') {
                "newline_only"
            } else {
                "empty_or_ws"
            }
        }
        Some(ch) if ch.is_alphanumeric() => "alnum",
        Some(')' | ']' | '}' | '.' | ',' | ';' | ':' | '!' | '?') => {
            "punct"
        }
        Some(_) => "other",
    }
}

/// Redacted classification of the last meaningful character before the caret,
/// for diagnosing contextual-capitalization decisions. Returns only a category
/// label, never the tail content.
fn context_tail_signal(tail: &str) -> &'static str {
    match tail.chars().rev().find(|c| !c.is_whitespace()) {
        None => {
            if tail.contains('\n') || tail.contains('\r') {
                "newline_only"
            } else {
                "empty_or_ws"
            }
        }
        Some('.') | Some('!') | Some('?') => "sentence_end",
        Some(ch) if ch.is_alphanumeric() => "alnum",
        Some(',') | Some(';') | Some(':') | Some('-') | Some('–') | Some('—') | Some('/')
        | Some('\\') => "soft_punct",
        Some(_) => "other",
    }
}

#[cfg_attr(not(any(test, target_os = "windows")), allow(dead_code))]
pub fn reset_injection_history() {
    if let Ok(mut guard) = last_injection().lock() {
        *guard = unknown_context();
    }
}
#[cfg_attr(not(any(test, target_os = "windows")), allow(dead_code))]
pub fn backspace_injection_history(hwnd: usize) {
    if let Ok(mut guard) = last_injection().lock() {
        match &mut *guard {
            CursorContextState::Known {
                hwnd: stored_hwnd,
                tail,
                instant,
            } if *stored_hwnd == hwnd => {
                tail.pop();
                *instant = Instant::now();
                if tail.is_empty() {
                    *guard = unknown_context();
                }
            }
            _ => {
                *guard = unknown_context();
            }
        }
    }
}
#[cfg_attr(not(any(test, target_os = "windows")), allow(dead_code))]
pub fn append_or_reset_injection_history(hwnd: usize, ch: char) {
    if let Ok(mut guard) = last_injection().lock() {
        match &mut *guard {
            CursorContextState::Known {
                hwnd: stored_hwnd,
                tail,
                instant,
            } if *stored_hwnd == hwnd => {
                tail.push(ch);
                trim_tail_to_limit(tail);
                *instant = Instant::now();
            }
            _ => {
                let mut tail = ch.to_string();
                trim_tail_to_limit(&mut tail);
                *guard = CursorContextState::Known {
                    hwnd,
                    tail,
                    instant: Instant::now(),
                };
            }
        }
    }
}
/// Write `text` to the OS clipboard without injecting. Used as a fallback
/// when Verenu itself holds foreground focus and a normal paste would
/// land in our own WebView.
pub async fn copy_to_clipboard(text: &str) -> anyhow::Result<()> {
    // Doesn't restore anything itself, but writing while another call is mid
    // save/restore could still corrupt that call's "original" snapshot or get
    // immediately clobbered — share the same critical section.
    let _guard = injection_lock().lock().await;
    #[cfg(windows)]
    {
        return windows::copy_to_clipboard(text).await;
    }
    #[cfg(target_os = "macos")]
    {
        return macos::copy_to_clipboard(text).await;
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        let _ = text;
        anyhow::bail!("copy_to_clipboard: unsupported platform")
    }
}

/// Snapshot current plain text before the pipeline writes its injection payload.
/// Clipboard phrase insertion is deliberately Windows-only for now.
pub async fn read_current_clipboard_text() -> Option<String> {
    #[cfg(windows)]
    {
        windows::read_clipboard_text().await
    }
    #[cfg(not(windows))]
    {
        None
    }
}

#[allow(unused_variables)]
pub async fn inject_text(
    text: &str,
    target_hwnd: usize,
    contextual_caps: bool,
    auto_spacing: bool,
    profile: &str,
    language: &str,
    protected_initial_case: bool,
) -> anyhow::Result<InjectionOutcome> {
    #[cfg(any(test, debug_assertions))]
    if crate::testing::is_enabled() {
        crate::testing::record_injection(crate::testing::InjectionRecord {
            text: text.to_string(),
            target_hwnd,
            contextual_caps,
            auto_spacing,
            profile: profile.to_string(),
        });
        return Ok(InjectionOutcome {
            text: text.to_string(),
            context_state: "test_mode",
            case_decision: "test_mode_passthrough",
            probe_source: "test_harness",
            selection_state: "unknown",
        });
    }

    #[cfg(target_os = "windows")]
    {
        return windows::inject_text(
            text,
            target_hwnd,
            contextual_caps,
            auto_spacing,
            profile,
            language,
            protected_initial_case,
        )
        .await;
    }

    #[cfg(target_os = "macos")]
    {
        return macos::inject_text(
            text,
            target_hwnd,
            contextual_caps,
            auto_spacing,
            profile,
            language,
            protected_initial_case,
        )
        .await;
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        log::warn!("inject_text: not on Windows - skipping target_hwnd={target_hwnd}");

        Ok(InjectionOutcome {
            text: text.to_string(),
            context_state: "unknown",
            case_decision: "contextual_caps_disabled",
            probe_source: "unavailable",
            selection_state: "unknown",
        })
    }
}
