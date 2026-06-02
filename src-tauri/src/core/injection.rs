use std::sync::{Mutex, OnceLock};
use std::time::Instant;

use crate::core::text_context;

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
    fn lowercase_first_word_only_for_safe_words() {
        assert_eq!(
            lowercase_first_word_if_safe("The fix is ready"),
            ("the fix is ready".into(), true)
        );
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
fn is_safe_lowercase_candidate(word: &str) -> bool {
    if word.is_empty() || word == "I" || word.contains(|c: char| c.is_ascii_digit()) {
        return false;
    }
    if word
        .chars()
        .any(|c| !c.is_alphabetic() && c != '\'' && c != '-')
    {
        return false;
    }
    let lower = word.to_lowercase();
    let upper = word.to_uppercase();
    if word == upper || word == lower {
        return false;
    }
    let mut chars = word.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !first.is_uppercase() {
        return false;
    }
    if chars.any(|c| c.is_uppercase()) {
        return false;
    }
    const LOWERCASE_WHITELIST: &[&str] = &[
        "a", "an", "and", "as", "at", "but", "by", "for", "from", "if", "in", "is", "it", "of",
        "on", "or", "so", "than", "that", "the", "then", "this", "to", "we", "with", "you",
    ];
    LOWERCASE_WHITELIST.contains(&lower.as_str())
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
fn injection_context(target_hwnd: usize, contextual_caps: bool, auto_spacing: bool) -> String {
    if !(contextual_caps || auto_spacing) {
        return String::new();
    }
    match last_injection().lock() {
        Ok(guard) => match &*guard {
            CursorContextState::Known {
                hwnd,
                tail,
                instant,
            } if *hwnd == target_hwnd && instant.elapsed() < INJECTION_STALE => tail.clone(),
            _ => String::new(),
        },
        Err(_) => {
            log::error!("injection history mutex poisoned");
            String::new()
        }
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

#[cfg(target_os = "windows")]
struct SavedClipboard {
    entries: Vec<(u32, Vec<u8>)>,
}

#[cfg(target_os = "windows")]
unsafe fn save_clipboard_all() -> SavedClipboard {
    use windows::Win32::Foundation::HGLOBAL;
    use windows::Win32::System::DataExchange::{
        CloseClipboard, EnumClipboardFormats, GetClipboardData, OpenClipboard,
    };
    use windows::Win32::System::Memory::{GlobalLock, GlobalSize, GlobalUnlock};

    // GDI object formats - GetClipboardData returns an opaque GDI handle for these,
    // not an HGLOBAL, so GlobalSize/GlobalLock are undefined on them.
    const CF_BITMAP: u32 = 2;
    const CF_METAFILEPICT: u32 = 3;
    const CF_PALETTE: u32 = 9;
    const CF_ENHMETAFILE: u32 = 14;

    // Per-format cap: skip anything larger than 32 MB to stay within the 200 MB
    // RAM budget. Typical screenshots are 2-8 MB as CF_DIB; 32 MB is generous.
    const MAX_FORMAT_BYTES: usize = 32 * 1024 * 1024;

    let mut entries = Vec::new();

    let opened = (0..3).any(|i| {
        if i > 0 {
            std::thread::sleep(std::time::Duration::from_millis(CLIPBOARD_OPEN_RETRY_MS));
        }
        OpenClipboard(None).is_ok()
    });
    if !opened {
        return SavedClipboard { entries };
    }

    let mut fmt = 0u32;
    loop {
        fmt = EnumClipboardFormats(fmt);
        if fmt == 0 {
            break;
        }
        if matches!(
            fmt,
            CF_BITMAP | CF_METAFILEPICT | CF_PALETTE | CF_ENHMETAFILE
        ) {
            continue;
        }
        if let Ok(h) = GetClipboardData(fmt) {
            let hg = HGLOBAL(h.0);
            let size = GlobalSize(hg);
            if size > 0 && size <= MAX_FORMAT_BYTES {
                let ptr = GlobalLock(hg) as *const u8;
                if !ptr.is_null() {
                    let data = std::slice::from_raw_parts(ptr, size).to_vec();
                    let _ = GlobalUnlock(hg);
                    entries.push((fmt, data));
                }
            }
        }
    }

    CloseClipboard().ok();
    SavedClipboard { entries }
}

#[cfg(target_os = "windows")]
unsafe fn restore_clipboard_all(saved: &SavedClipboard) {
    use windows::Win32::Foundation::{GlobalFree, HANDLE};
    use windows::Win32::System::DataExchange::{
        CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData,
    };
    use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};

    if saved.entries.is_empty() {
        return;
    }

    for attempt in 0..CLIPBOARD_RESTORE_ATTEMPTS {
        let opened = (0..3).any(|i| {
            if i > 0 {
                std::thread::sleep(std::time::Duration::from_millis(CLIPBOARD_OPEN_RETRY_MS));
            }
            OpenClipboard(None).is_ok()
        });
        if !opened {
            if attempt + 1 == CLIPBOARD_RESTORE_ATTEMPTS {
                log::warn!("clipboard restore failed: OpenClipboard unavailable");
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(CLIPBOARD_RESTORE_RETRY_MS));
            continue;
        }

        EmptyClipboard().ok();
        let mut restored = 0usize;
        for (fmt, data) in &saved.entries {
            if let Ok(hg) = GlobalAlloc(GMEM_MOVEABLE, data.len()) {
                let ptr = GlobalLock(hg) as *mut u8;
                if ptr.is_null() {
                    let _ = GlobalFree(Some(hg));
                    continue;
                }
                std::ptr::copy_nonoverlapping(data.as_ptr(), ptr, data.len());
                let _ = GlobalUnlock(hg);
                if SetClipboardData(*fmt, Some(HANDLE(hg.0))).is_ok() {
                    restored += 1;
                } else {
                    let _ = GlobalFree(Some(hg));
                }
            }
        }
        CloseClipboard().ok();

        if restored == saved.entries.len() {
            return;
        }

        log::warn!(
            "clipboard restore incomplete: restored {} of {} formats",
            restored,
            saved.entries.len()
        );
        if attempt + 1 < CLIPBOARD_RESTORE_ATTEMPTS {
            std::thread::sleep(std::time::Duration::from_millis(CLIPBOARD_RESTORE_RETRY_MS));
        }
    }
}

#[cfg(target_os = "windows")]
unsafe fn write_clipboard_unicode(data: &[u16]) -> anyhow::Result<()> {
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::System::DataExchange::{
        CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData,
    };
    use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};

    const CF_UNICODETEXT: u32 = 13;

    if OpenClipboard(None).is_ok() {
        EmptyClipboard().ok();
        let hg = GlobalAlloc(GMEM_MOVEABLE, data.len() * 2)?;
        let ptr = GlobalLock(hg) as *mut u16;
        std::ptr::copy_nonoverlapping(data.as_ptr(), ptr, data.len());
        let _ = GlobalUnlock(hg);
        SetClipboardData(CF_UNICODETEXT, Some(HANDLE(hg.0)))
            .map_err(|e| anyhow::anyhow!("SetClipboardData failed: {e}"))?;
        CloseClipboard().ok();
        Ok(())
    } else {
        Err(anyhow::anyhow!("OpenClipboard failed"))
    }
}

pub async fn inject_text(
    text: &str,
    target_hwnd: usize,
    contextual_caps: bool,
    auto_spacing: bool,
    profile: &str,
) -> anyhow::Result<InjectionOutcome> {
    #[cfg(target_os = "windows")]
    {
        use windows::Win32::Foundation::HWND;
        use windows::Win32::UI::Input::KeyboardAndMouse::{
            SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS,
            KEYEVENTF_KEYUP, VK_CONTROL, VK_LMENU, VK_V,
        };
        use windows::Win32::UI::WindowsAndMessaging::SetForegroundWindow;

        unsafe {
            // Save all clipboard formats so non-text content (images, files, etc.)
            // survives the injection and is restored afterward.
            let saved = save_clipboard_all();

            // Restore focus to the window the user was dictating into.
            // The user may have switched windows during the transcription/cleanup
            // pipeline; without this the Ctrl+V paste lands in the wrong app.
            // WH_KEYBOARD_LL hooks give the process implicit foreground lock
            // permission, so SetForegroundWindow succeeds from here.
            if target_hwnd != 0 {
                let _ = SetForegroundWindow(HWND(target_hwnd as *mut core::ffi::c_void));
                tokio::time::sleep(tokio::time::Duration::from_millis(REFOCUS_SETTLE_MS)).await;
            }

            let ki = |vk, flags: u32| INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: vk,
                        wScan: 0,
                        dwFlags: KEYBD_EVENT_FLAGS(flags),
                        time: 0,
                        dwExtraInfo: 0,
                    },
                },
            };

            // Read cursor context from the tracked tail.
            // moves the cursor, or switches windows - so when it's absent the context
            // is genuinely unknown and we default to capitalising (safe for new fields).
            let prefix_class = text_context::classify_leading_prefix(text);
            let injection_probe = if contextual_caps || auto_spacing {
                crate::api::auto_learn::read_injection_context_probe()
            } else {
                crate::api::auto_learn::InjectionContextProbe {
                    context: crate::core::text_context::SentenceContext::Unknown,
                    source: crate::api::auto_learn::InjectionProbeSource::Unavailable,
                    context_tail: String::new(),
                    control_type: "unavailable".to_string(),
                    pattern_support: "unavailable".to_string(),
                    selection_state: crate::api::auto_learn::SelectionState::Unknown,
                    control_identity_hash: "unavailable".to_string(),
                }
            };
            let context_kind = context_kind_from_sentence_context(injection_probe.context);
            let source_is_caret_local = matches!(
                injection_probe.source,
                crate::api::auto_learn::InjectionProbeSource::CaretLocal
            );

            // Apply profile-aware casing using the probe-derived context state.
            let mut adjusted = text.to_owned();
            let case_decision = if !contextual_caps {
                CaseDecision::ContextualCapsDisabled
            } else if injection_probe.context
                == crate::core::text_context::SentenceContext::NewSentence
                && profile == "very_casual"
            {
                CaseDecision::SentenceBoundaryPreservedVeryCasual
            } else {
                match prefix_class {
                    text_context::InjectionPrefixClass::PlainWordStart => match context_kind {
                        ContextKind::Unknown => {
                            adjusted = text_context::format_injection_text(
                                text,
                                injection_probe.context,
                                prefix_class,
                            );
                            if adjusted == text {
                                CaseDecision::UnknownContextPreserved
                            } else {
                                CaseDecision::SentenceBoundaryCapitalized
                            }
                        }
                        ContextKind::SentenceBoundary => {
                            adjusted = text_context::format_injection_text(
                                text,
                                injection_probe.context,
                                prefix_class,
                            );
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
                        adjusted = text_context::format_injection_text(
                            text,
                            injection_probe.context,
                            prefix_class,
                        );
                        CaseDecision::SentenceBoundaryCapitalized
                    }
                    text_context::InjectionPrefixClass::SoftPunctuationPrefix
                    | text_context::InjectionPrefixClass::InvisibleOrAmbiguousPrefix => {
                        adjusted = text_context::format_injection_text(
                            text,
                            injection_probe.context,
                            prefix_class,
                        );
                        match context_kind {
                            ContextKind::Unknown => CaseDecision::UnknownContextPreserved,
                            ContextKind::SentenceBoundary => {
                                CaseDecision::SentenceBoundaryCapitalized
                            }
                            ContextKind::Continuation => CaseDecision::ContinuationPreserved,
                        }
                    }
                }
            };

            // Add a space only when the probe sees real caret-local context.
            if auto_spacing
                && text_context::should_add_leading_injection_space(
                    text,
                    injection_probe.context,
                    prefix_class,
                    source_is_caret_local,
                    &injection_probe.context_tail,
                )
            {
                adjusted = format!(" {adjusted}");
            }

            let text_to_inject = adjusted.as_str();

            // Write injection text - retry up to 3 times if another process holds the clipboard.
            let wide: Vec<u16> = text_to_inject
                .encode_utf16()
                .chain(std::iter::once(0))
                .collect();
            let mut clipboard_written = false;
            for attempt in 0..3u32 {
                if write_clipboard_unicode(&wide).is_ok() {
                    clipboard_written = true;
                    break;
                }
                if attempt < 2 {
                    tokio::time::sleep(tokio::time::Duration::from_millis(
                        CLIPBOARD_WRITE_RETRY_MS,
                    ))
                    .await;
                }
            }
            if !clipboard_written {
                return Err(anyhow::anyhow!(
                    "OpenClipboard failed after 3 attempts - clipboard held by another process"
                ));
            }

            tokio::time::sleep(tokio::time::Duration::from_millis(
                CLIPBOARD_WRITE_SETTLE_MS,
            ))
            .await;

            // Step 1 - clear any dangling Alt the target app may have from the
            // recording gesture.  Ctrl-down is sent first so that the Alt-up is
            // NOT the message immediately following Alt-down; this prevents
            // DefWindowProc from firing SC_KEYMENU (menu-bar activation).
            // We then release Ctrl so the app fully settles before the paste.
            let clear = [
                ki(VK_CONTROL, 0),
                ki(VK_LMENU, KEYEVENTF_KEYUP.0),
                ki(VK_CONTROL, KEYEVENTF_KEYUP.0),
            ];
            SendInput(&clear, std::mem::size_of::<INPUT>() as i32);

            // Step 2 - let the app process the modifier-state change before we
            // inject Ctrl+V.  Without this pause, some apps (browsers, IDEs) end
            // up processing V without Ctrl because the Alt-up and V-down land in
            // the same message-pump cycle.
            tokio::time::sleep(tokio::time::Duration::from_millis(MODIFIER_GAP_MS)).await;

            // Step 3 - clean Ctrl+V with no dangling modifiers.
            let paste = [
                ki(VK_CONTROL, 0),
                ki(VK_V, 0),
                ki(VK_V, KEYEVENTF_KEYUP.0),
                ki(VK_CONTROL, KEYEVENTF_KEYUP.0),
            ];
            SendInput(&paste, std::mem::size_of::<INPUT>() as i32);
            tokio::time::sleep(tokio::time::Duration::from_millis(PASTE_SETTLE_MS)).await;

            // Restore all previously saved clipboard formats.
            restore_clipboard_all(&saved);

            if target_hwnd != 0 && !adjusted.is_empty() {
                if let Ok(mut guard) = last_injection().lock() {
                    let mut tail = adjusted.clone();
                    trim_tail_to_limit(&mut tail);
                    *guard = CursorContextState::Known {
                        hwnd: target_hwnd,
                        tail,
                        instant: Instant::now(),
                    };
                }
            }

            Ok(InjectionOutcome {
                text: adjusted,
                context_state: context_kind.as_str(),
                case_decision: case_decision.as_str(),
            })
        }
    }

    #[cfg(target_os = "macos")]
    {
        use core_graphics::event::{CGEvent, CGEventFlags, CGEventTapLocation, CGKeyCode};
        use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};

        const VK_ANSI_V: CGKeyCode = 9;

        if target_hwnd != 0 {
            crate::system::mac_app::activate_pid(target_hwnd as i32);
            tokio::time::sleep(Duration::from_millis(120)).await;
        }

        let context = injection_context(target_hwnd, contextual_caps, auto_spacing);
        let (mut adjusted, context_kind, case_decision) = if !contextual_caps {
            (
                text.to_owned(),
                ContextKind::Unknown,
                CaseDecision::ContextualCapsDisabled,
            )
        } else {
            match classify_context(&context) {
                ContextKind::Unknown => (
                    text.to_owned(),
                    ContextKind::Unknown,
                    CaseDecision::UnknownContextPreserved,
                ),
                ContextKind::SentenceBoundary => {
                    if profile == "very_casual" {
                        (
                            text.to_owned(),
                            ContextKind::SentenceBoundary,
                            CaseDecision::SentenceBoundaryPreservedVeryCasual,
                        )
                    } else {
                        (
                            uppercase_first_word(text),
                            ContextKind::SentenceBoundary,
                            CaseDecision::SentenceBoundaryCapitalized,
                        )
                    }
                }
                ContextKind::Continuation => {
                    let (lowered, did_lower) = lowercase_first_word_if_safe(text);
                    if did_lower {
                        (
                            lowered,
                            ContextKind::Continuation,
                            CaseDecision::ContinuationLowercased,
                        )
                    } else {
                        (
                            text.to_owned(),
                            ContextKind::Continuation,
                            CaseDecision::ContinuationPreserved,
                        )
                    }
                }
            }
        };

        if auto_spacing {
            if let Some(c) = context.chars().next_back() {
                if !c.is_whitespace() && !adjusted.starts_with(char::is_whitespace) {
                    adjusted = format!(" {adjusted}");
                }
            }
        }

        let saved = crate::system::mac_app::pasteboard_get_string();
        crate::system::mac_app::pasteboard_set_string(&adjusted);
        tokio::time::sleep(Duration::from_millis(20)).await;

        let posted = (|| -> Option<()> {
            let src = CGEventSource::new(CGEventSourceStateID::CombinedSessionState).ok()?;
            let down = CGEvent::new_keyboard_event(src.clone(), VK_ANSI_V, true).ok()?;
            // core-graphics 0.24.x exposes the Command modifier under the
            // CGEventFlagCommand name.
            down.set_flags(CGEventFlags::CGEventFlagCommand);
            down.post(CGEventTapLocation::HID);
            let up = CGEvent::new_keyboard_event(src, VK_ANSI_V, false).ok()?;
            up.set_flags(CGEventFlags::CGEventFlagCommand);
            up.post(CGEventTapLocation::HID);
            Some(())
        })();

        tokio::time::sleep(Duration::from_millis(120)).await;

        if let Some(prev) = saved {
            crate::system::mac_app::pasteboard_set_string(&prev);
        }

        if posted.is_none() {
            return Err(anyhow::anyhow!(
                "inject_text: failed to synthesise Cmd+V — grant Open Flow Accessibility permission"
            ));
        }

        if target_hwnd != 0 && !adjusted.is_empty() {
            if let Ok(mut guard) = last_injection().lock() {
                let mut tail = adjusted.clone();
                trim_tail_to_limit(&mut tail);
                *guard = CursorContextState::Known {
                    hwnd: target_hwnd,
                    tail,
                    instant: Instant::now(),
                };
            }
        }

        Ok(InjectionOutcome {
            text: adjusted,
            context_state: context_kind.as_str(),
            case_decision: case_decision.as_str(),
        })
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        log::warn!("inject_text: not on Windows - skipping target_hwnd={target_hwnd}");

        Ok(InjectionOutcome {
            text: text.to_string(),
            context_state: "unknown",
            case_decision: "contextual_caps_disabled",
        })
    }
}
