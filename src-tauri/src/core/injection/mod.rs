use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use crate::core::context_probe::{ContextProbeSource, InjectionContextProbe, SelectionState};
use crate::core::text_context;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "windows")]
mod windows;

// Maximum bytes stored for backspace-tracking. Covers any practical editing sequence
// while keeping the per-injection allocation bounded.
const HISTORY_TAIL: usize = 512;

// How long a previous injection stays relevant for spacing and capitalization decisions.
// The keyboard hook resets this early whenever the user types, so the timeout is mainly
// a safety net for inactivity (e.g. the user idle for a minute then dictates again).
const INJECTION_STALE: Duration = Duration::from_secs(60);

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

// Settle time after the paste (Ctrl+V) before restoring saved clipboard
// formats - gives the target app time to read the clipboard before we
// overwrite it with the original contents.
#[cfg(target_os = "windows")]
const PASTE_SETTLE_MS: u64 = 80;

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
    fn uppercase_first_word_handles_prefix_symbols() {
        assert_eq!(uppercase_first_word("hello world"), "Hello world");
        assert_eq!(uppercase_first_word("\"hello world"), "\"Hello world");
        assert_eq!(uppercase_first_word("(hello world"), "(Hello world");
    }

    #[test]
    fn lowercase_first_word_blocked_for_i_acronyms_and_camelcase() {
        // Common grammatical words are lowercased in continuation context.
        assert_eq!(
            lowercase_first_word_if_safe("The fix is ready"),
            ("the fix is ready".into(), true)
        );
        assert_eq!(
            lowercase_first_word_if_safe("Let me explain"),
            ("let me explain".into(), true)
        );
        assert_eq!(
            lowercase_first_word_if_safe("Make sure to do this"),
            ("make sure to do this".into(), true)
        );
        // Proper nouns must NOT be lowercased (not in the safe list).
        assert_eq!(
            lowercase_first_word_if_safe("London is a city"),
            ("London is a city".into(), false)
        );
        assert_eq!(
            lowercase_first_word_if_safe("Monday meeting"),
            ("Monday meeting".into(), false)
        );
        // "I", CamelCase, and acronyms are always blocked.
        assert_eq!(
            lowercase_first_word_if_safe("OpenAI ships model updates"),
            ("OpenAI ships model updates".into(), false)
        );
        assert_eq!(
            lowercase_first_word_if_safe("I am here"),
            ("I am here".into(), false)
        );
        assert_eq!(
            lowercase_first_word_if_safe("HTTP server"),
            ("HTTP server".into(), false)
        );
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
}
#[derive(Clone, Copy, Debug)]
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
// Common grammatical function words that are never proper nouns.
// Lowercasing is only applied to words in this list so that Title-Case proper
// nouns (London, Monday, Google) are preserved in continuation context.
const SAFE_TO_LOWERCASE: &[&str] = &[
    "the",
    "a",
    "an",
    "this",
    "that",
    "these",
    "those",
    "it",
    "he",
    "she",
    "we",
    "they",
    "you",
    "is",
    "was",
    "are",
    "were",
    "be",
    "been",
    "being",
    "have",
    "has",
    "had",
    "do",
    "does",
    "did",
    "will",
    "would",
    "could",
    "should",
    "may",
    "might",
    "must",
    "can",
    "and",
    "or",
    "but",
    "if",
    "so",
    "then",
    "because",
    "though",
    "although",
    "my",
    "your",
    "his",
    "her",
    "our",
    "their",
    "its",
    "let",
    "just",
    "not",
    "also",
    "even",
    "now",
    "here",
    "there",
    "make",
    "get",
    "go",
    "see",
    "think",
    "say",
    "tell",
    "look",
    "seem",
    "all",
    "some",
    "any",
    "no",
    "more",
    "most",
    "very",
    "well",
    "still",
    "when",
    "where",
    "how",
    "what",
    "which",
    "please",
    "yes",
    "no",
    "ok",
    "okay",
    "actually",
    "basically",
    "honestly",
    "literally",
    "really",
    "totally",
    "with",
    "into",
    "onto",
    "upon",
    "about",
];

fn is_safe_lowercase_candidate(word: &str) -> bool {
    if word.is_empty() || word == "I" {
        return false;
    }
    // Only lowercase words from the explicit safe list; Title-Case proper nouns
    // (London, Monday, Google) must never be lowercased.
    SAFE_TO_LOWERCASE.contains(&word.to_lowercase().as_str())
}
fn find_first_alpha_span(text: &str) -> Option<(usize, usize)> {
    let mut start = None;
    let mut end = 0usize;
    for (idx, ch) in text.char_indices() {
        if start.is_none() {
            if ch.is_alphabetic() {
                start = Some(idx);
                end = idx + ch.len_utf8();
            }
            continue;
        }
        if ch.is_alphabetic() || ch == '\'' || ch == '-' {
            end = idx + ch.len_utf8();
            continue;
        }
        break;
    }
    start.map(|s| (s, end))
}
#[cfg(test)]
fn uppercase_first_word(text: &str) -> String {
    let Some((start, _)) = find_first_alpha_span(text) else {
        return text.to_owned();
    };
    let mut chars = text[start..].chars();
    let Some(first) = chars.next() else {
        return text.to_owned();
    };
    if !first.is_lowercase() {
        return text.to_owned();
    }
    let first_len = first.len_utf8();
    let mut out = String::with_capacity(text.len());
    out.push_str(&text[..start]);
    out.push_str(&first.to_uppercase().collect::<String>());
    out.push_str(&text[start + first_len..]);
    out
}
fn lowercase_first_word_if_safe(text: &str) -> (String, bool) {
    let Some((start, end)) = find_first_alpha_span(text) else {
        return (text.to_owned(), false);
    };
    let word = &text[start..end];
    if !is_safe_lowercase_candidate(word) {
        return (text.to_owned(), false);
    }
    let mut chars = text[start..].chars();
    let Some(first) = chars.next() else {
        return (text.to_owned(), false);
    };
    let first_len = first.len_utf8();
    let mut out = String::with_capacity(text.len());
    out.push_str(&text[..start]);
    out.push_str(&first.to_lowercase().collect::<String>());
    out.push_str(&text[start + first_len..]);
    (out, true)
}
fn unavailable_injection_probe() -> InjectionContextProbe {
    InjectionContextProbe::unavailable(ContextProbeSource::Unavailable, "unavailable")
}

fn fallback_probe_from_history(target_hwnd: usize) -> Option<InjectionContextProbe> {
    if target_hwnd == 0 || crate::core::window_context::get_foreground_hwnd() != target_hwnd {
        return None;
    }

    match last_injection().lock() {
        Ok(guard) => match &*guard {
            CursorContextState::Known {
                hwnd,
                tail,
                instant,
            } if *hwnd == target_hwnd && instant.elapsed() < INJECTION_STALE => {
                Some(InjectionContextProbe {
                    context: crate::core::text_context::classify_context_tail(tail),
                    source: ContextProbeSource::HistoryFallback,
                    context_tail: tail.clone(),
                    control_type: "history_tail".to_string(),
                    selection_state: SelectionState::Unknown,
                    control_identity_hash: "history_tail".to_string(),
                })
            }
            CursorContextState::Unknown { _instant } => {
                let _ = _instant.elapsed();
                None
            }
            _ => None,
        },
        Err(_) => {
            log::error!("injection history mutex poisoned");
            None
        }
    }
}

fn apply_probe_adjustments(
    text: &str,
    contextual_caps: bool,
    auto_spacing: bool,
    profile: &str,
    probe: &InjectionContextProbe,
) -> (String, ContextKind, CaseDecision) {
    let prefix_class = text_context::classify_leading_prefix(text);
    let context_kind = context_kind_from_sentence_context(probe.context);
    let mut adjusted = text.to_owned();

    let case_decision = if !contextual_caps {
        CaseDecision::ContextualCapsDisabled
    } else if !probe.source.supports_contextual_casing() {
        CaseDecision::ConservativeDegradePreserved
    } else if probe.context == crate::core::text_context::SentenceContext::NewSentence
        && profile == "very_casual"
    {
        CaseDecision::SentenceBoundaryPreservedVeryCasual
    } else {
        match prefix_class {
            text_context::InjectionPrefixClass::PlainWordStart => match context_kind {
                ContextKind::Unknown => CaseDecision::UnknownContextPreserved,
                ContextKind::SentenceBoundary => {
                    adjusted =
                        text_context::format_injection_text(text, probe.context, prefix_class);
                    CaseDecision::SentenceBoundaryCapitalized
                }
                ContextKind::Continuation => {
                    let (lowered, did_lower) = lowercase_first_word_if_safe(text);
                    if did_lower {
                        adjusted = lowered;
                        CaseDecision::ContinuationLowercased
                    } else {
                        CaseDecision::ContinuationPreserved
                    }
                }
            },
            text_context::InjectionPrefixClass::HardSentenceTerminator => {
                adjusted = text_context::format_injection_text(text, probe.context, prefix_class);
                CaseDecision::SentenceBoundaryCapitalized
            }
            text_context::InjectionPrefixClass::SoftPunctuationPrefix
            | text_context::InjectionPrefixClass::InvisibleOrAmbiguousPrefix => {
                match context_kind {
                    ContextKind::Unknown => CaseDecision::UnknownContextPreserved,
                    ContextKind::SentenceBoundary => {
                        adjusted = text_context::apply_contextual_casing(text, probe.context);
                        CaseDecision::SentenceBoundaryCapitalized
                    }
                    ContextKind::Continuation => CaseDecision::ContinuationPreserved,
                }
            }
        }
    };

    if auto_spacing
        && text_context::should_add_leading_injection_space(
            text,
            probe.context,
            prefix_class,
            probe.source.supports_auto_spacing(),
            &probe.context_tail,
        )
    {
        adjusted = format!(" {adjusted}");
    }

    (adjusted, context_kind, case_decision)
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

#[allow(unused_variables)]
pub async fn inject_text(
    text: &str,
    target_hwnd: usize,
    contextual_caps: bool,
    auto_spacing: bool,
    profile: &str,
    clipboard_sniff_enabled: bool,
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
            clipboard_sniff_enabled,
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
            clipboard_sniff_enabled,
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
