use super::*;

struct SavedClipboard {
    entries: Vec<(u32, Vec<u8>)>,
}

/// Guarantees the saved clipboard is restored exactly once, even on an early
/// return/error/panic — not just on the normal success path. Armed with the
/// saved snapshot; `restore_now()` disarms it (via `Option::take`) so the
/// `Drop` fallback is a no-op when the normal path already ran, never a
/// second (redundant) restore.
struct ClipboardRestoreGuard {
    saved: Option<SavedClipboard>,
}
impl ClipboardRestoreGuard {
    fn new(saved: SavedClipboard) -> Self {
        Self { saved: Some(saved) }
    }
    fn restore_now(&mut self) {
        if let Some(saved) = self.saved.take() {
            unsafe {
                restore_clipboard_all(&saved);
            }
        }
    }
}
impl Drop for ClipboardRestoreGuard {
    fn drop(&mut self) {
        // Fallback only - a no-op if restore_now() already ran. Safe to call
        // from Drop: restore_clipboard_all is fully synchronous, no .await.
        if let Some(saved) = self.saved.take() {
            unsafe {
                restore_clipboard_all(&saved);
            }
        }
    }
}

unsafe fn save_clipboard_all() -> SavedClipboard {
    use ::windows::Win32::Foundation::HGLOBAL;
    use ::windows::Win32::System::DataExchange::{
        CloseClipboard, EnumClipboardFormats, GetClipboardData, OpenClipboard,
    };
    use ::windows::Win32::System::Memory::{GlobalLock, GlobalSize, GlobalUnlock};

    const CF_BITMAP: u32 = 2;
    const CF_METAFILEPICT: u32 = 3;
    const CF_PALETTE: u32 = 9;
    const CF_ENHMETAFILE: u32 = 14;
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

unsafe fn restore_clipboard_all(saved: &SavedClipboard) {
    use ::windows::Win32::Foundation::{GlobalFree, HANDLE};
    use ::windows::Win32::System::DataExchange::{
        CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData,
    };
    use ::windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};

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

unsafe fn write_clipboard_unicode(data: &[u16]) -> anyhow::Result<()> {
    use ::windows::Win32::Foundation::{GlobalFree, HANDLE};
    use ::windows::Win32::System::DataExchange::{
        CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData,
    };
    use ::windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};

    const CF_UNICODETEXT: u32 = 13;

    if OpenClipboard(None).is_ok() {
        EmptyClipboard().ok();
        let hg = match GlobalAlloc(GMEM_MOVEABLE, data.len() * 2) {
            Ok(hg) => hg,
            Err(e) => {
                // Allocation failed after the clipboard was opened: release it
                // before erroring, otherwise the system clipboard stays locked
                // for every other process until we exit.
                CloseClipboard().ok();
                return Err(anyhow::anyhow!("GlobalAlloc failed: {e}"));
            }
        };
        let ptr = GlobalLock(hg) as *mut u16;
        if ptr.is_null() {
            // GlobalLock failed: free the block we own and release the
            // clipboard rather than dereferencing null / leaking hg.
            let _ = GlobalFree(Some(hg));
            CloseClipboard().ok();
            return Err(anyhow::anyhow!("GlobalLock failed"));
        }
        std::ptr::copy_nonoverlapping(data.as_ptr(), ptr, data.len());
        let _ = GlobalUnlock(hg);
        if let Err(e) = SetClipboardData(CF_UNICODETEXT, Some(HANDLE(hg.0))) {
            // The system only takes ownership of hg on success, so on failure we
            // still own it: free it and release the clipboard before erroring.
            let _ = GlobalFree(Some(hg));
            CloseClipboard().ok();
            return Err(anyhow::anyhow!("SetClipboardData failed: {e}"));
        }
        CloseClipboard().ok();
        Ok(())
    } else {
        Err(anyhow::anyhow!("OpenClipboard failed"))
    }
}

// Reads CF_UNICODETEXT from the clipboard. Returns `Some("")` when the format
// is present but empty, and `None` only when the clipboard can't be read. Async
// so the OpenClipboard retry backoff yields to the runtime instead of blocking
// the executor thread with a synchronous sleep.
async fn read_clipboard_text() -> Option<String> {
    use ::windows::Win32::Foundation::HGLOBAL;
    use ::windows::Win32::System::DataExchange::{CloseClipboard, GetClipboardData, OpenClipboard};
    use ::windows::Win32::System::Memory::{GlobalLock, GlobalSize, GlobalUnlock};

    const CF_UNICODETEXT: u32 = 13;

    let mut opened = false;
    for i in 0..3 {
        if i > 0 {
            tokio::time::sleep(tokio::time::Duration::from_millis(CLIPBOARD_OPEN_RETRY_MS)).await;
        }
        if unsafe { OpenClipboard(None) }.is_ok() {
            opened = true;
            break;
        }
    }
    if !opened {
        return None;
    }

    let mut text: Option<String> = None;
    unsafe {
        if let Ok(h) = GetClipboardData(CF_UNICODETEXT) {
            let hg = HGLOBAL(h.0);
            let size = GlobalSize(hg);
            if size == 0 {
                text = Some(String::new());
            } else {
                let ptr = GlobalLock(hg) as *const u16;
                if !ptr.is_null() {
                    let units = size / 2;
                    let slice = std::slice::from_raw_parts(ptr, units);
                    let end = slice.iter().position(|&c| c == 0).unwrap_or(units);
                    text = Some(String::from_utf16_lossy(&slice[..end]));
                    let _ = GlobalUnlock(hg);
                }
            }
        }
        CloseClipboard().ok();
    }
    text
}

pub(super) async fn copy_to_clipboard(text: &str) -> anyhow::Result<()> {
    let wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
    for attempt in 0..3u32 {
        if unsafe { write_clipboard_unicode(&wide) }.is_ok() {
            return Ok(());
        }
        if attempt < 2 {
            tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
        }
    }
    anyhow::bail!("copy_to_clipboard: clipboard held after 3 attempts")
}

// Chromium/Electron populate the clipboard asynchronously after Ctrl+C, so the
// sniff polls a few times before concluding the field is empty.
const SNIFF_READ_ATTEMPTS: usize = 4;
const SNIFF_READ_INTERVAL_MS: u64 = 25;

// Verifies what is actually selectable immediately before the caret. UIA can
// report phantom text before the cursor in a visually empty box (Chromium /
// Electron controls), which makes contextual-caps wrongly lowercase. Selecting
// one char back reflects what the user actually sees. Mirrors
// `macos_clipboard_sniff_context`. Relies on the caller (`inject_text`) having
// already saved the clipboard before the probe step; the caller's paste and
// final `restore_clipboard_all(&saved)` put the user's clipboard back.
async fn windows_clipboard_sniff_context(target_hwnd: usize) -> Option<InjectionContextProbe> {
    use ::windows::Win32::UI::Input::KeyboardAndMouse::{
        SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS, KEYEVENTF_KEYUP,
        VK_C, VK_CONTROL, VK_LEFT, VK_RIGHT, VK_SHIFT,
    };

    // Don't send keystrokes to the wrong window if focus moved mid-pipeline.
    if crate::core::window_context::get_foreground_hwnd() != target_hwnd {
        return None;
    }

    unsafe {
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

        // Seed an empty sentinel so a failed copy (empty box) reads as empty
        // rather than as a stale clipboard value. Bail if it can't be set so we
        // never misread leftover clipboard text as content before the caret.
        if write_clipboard_unicode(&[0u16]).is_err() {
            return None;
        }

        let one = std::mem::size_of::<INPUT>() as i32;

        // Select one character back. Press Shift, tap Left, release Shift as
        // separate sends with gaps: an atomic Shift+Left batch is seen as a plain
        // Left in some controls (Chromium/Electron), which moves the caret
        // instead of selecting — that both fooled the sniff and corrupted the
        // paste ("HellHi.o").
        SendInput(&[ki(VK_SHIFT, 0)], one);
        tokio::time::sleep(tokio::time::Duration::from_millis(MODIFIER_GAP_MS)).await;
        SendInput(&[ki(VK_LEFT, 0), ki(VK_LEFT, KEYEVENTF_KEYUP.0)], one);
        tokio::time::sleep(tokio::time::Duration::from_millis(MODIFIER_GAP_MS)).await;
        SendInput(&[ki(VK_SHIFT, KEYEVENTF_KEYUP.0)], one);
        tokio::time::sleep(tokio::time::Duration::from_millis(MODIFIER_GAP_MS)).await;

        // Ctrl+C: copy the selection, if any.
        let copy = [
            ki(VK_CONTROL, 0),
            ki(VK_C, 0),
            ki(VK_C, KEYEVENTF_KEYUP.0),
            ki(VK_CONTROL, KEYEVENTF_KEYUP.0),
        ];
        SendInput(&copy, one);

        // Poll the clipboard — Chromium/Electron populate it asynchronously, so a
        // single immediate read can miss the copied character.
        let mut sniffed = String::new();
        for _ in 0..SNIFF_READ_ATTEMPTS {
            tokio::time::sleep(tokio::time::Duration::from_millis(SNIFF_READ_INTERVAL_MS)).await;
            if let Some(s) = read_clipboard_text().await {
                if !s.is_empty() {
                    sniffed = s;
                    break;
                }
            }
        }

        // ALWAYS collapse back to the original caret. Right moves to the right
        // edge of a selection, or undoes a Left that merely moved the caret —
        // either way the caret returns to where it started. Skipping this on a
        // failed read is what corrupted the paste, so it runs unconditionally.
        SendInput(&[ki(VK_RIGHT, 0), ki(VK_RIGHT, KEYEVENTF_KEYUP.0)], one);
        tokio::time::sleep(tokio::time::Duration::from_millis(MODIFIER_GAP_MS)).await;

        let context = if sniffed.is_empty() {
            crate::core::text_context::SentenceContext::NewSentence
        } else {
            crate::core::text_context::classify_context_tail(&sniffed)
        };

        Some(InjectionContextProbe {
            context,
            source: ContextProbeSource::ClipboardSniff,
            context_tail: sniffed,
            control_type: "clipboard_sniff".to_string(),
            selection_state: SelectionState::Unknown,
            control_identity_hash: "clipboard_sniff".to_string(),
        })
    }
}

#[allow(unused_variables)]
pub(super) async fn inject_text(
    text: &str,
    target_hwnd: usize,
    contextual_caps: bool,
    auto_spacing: bool,
    profile: &str,
    clipboard_sniff_enabled: bool,
) -> anyhow::Result<InjectionOutcome> {
    use ::windows::Win32::Foundation::HWND;
    use ::windows::Win32::UI::Input::KeyboardAndMouse::{
        SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS, KEYEVENTF_KEYUP,
        VK_CONTROL, VK_LMENU, VK_V,
    };
    use ::windows::Win32::UI::WindowsAndMessaging::SetForegroundWindow;

    // Declared before the restore guard so it releases *after* the clipboard
    // has been restored (Rust drops in reverse declaration order).
    let _injection_guard = super::injection_lock().lock().await;

    let saved = unsafe { save_clipboard_all() };
    let mut restore_guard = ClipboardRestoreGuard::new(saved);

    if target_hwnd != 0 {
        let _ = unsafe { SetForegroundWindow(HWND(target_hwnd as *mut core::ffi::c_void)) };
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

    let mut injection_probe = if contextual_caps || auto_spacing {
        crate::core::context_probe::read_injection_context_probe().await
    } else {
        unavailable_injection_probe()
    };
    if (contextual_caps || auto_spacing) && injection_probe.source.allows_history_fallback() {
        if let Some(history_probe) = fallback_probe_from_history(target_hwnd) {
            injection_probe = history_probe;
        }
    }
    // A caret-local read that says "mid-sentence" can be wrong in Chromium /
    // Electron controls that report phantom text before the caret in an empty
    // box. Verify against what's actually selectable before lowercasing.
    if caret_local_needs_sniff_verification(
        contextual_caps,
        injection_probe.source,
        injection_probe.context,
        profile,
    ) {
        if let Some(mut sniff) = windows_clipboard_sniff_context(target_hwnd).await {
            // Spacing must follow the reliable UIA tail ("is there a visible
            // char before the caret?"), not the sniff: in Chromium/Electron
            // editors synthetic Ctrl+C is a no-op, so the sniff always reads
            // empty there. Let the sniff drive only the capitalization
            // decision, keeping UIA's tail so appends still get a space.
            sniff.context_tail = injection_probe.context_tail.clone();
            injection_probe = sniff;
        }
    }
    let (adjusted, context_kind, case_decision) = apply_probe_adjustments(
        text,
        contextual_caps,
        auto_spacing,
        profile,
        &injection_probe,
    );

    let text_to_inject = adjusted.as_str();
    let wide: Vec<u16> = text_to_inject
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    let mut clipboard_written = false;
    for attempt in 0..3u32 {
        if unsafe { write_clipboard_unicode(&wide) }.is_ok() {
            clipboard_written = true;
            break;
        }
        if attempt < 2 {
            tokio::time::sleep(tokio::time::Duration::from_millis(CLIPBOARD_WRITE_RETRY_MS)).await;
        }
    }
    if !clipboard_written {
        // Put the user's clipboard back — a sniff may have left its sentinel.
        restore_guard.restore_now();
        return Err(anyhow::anyhow!(
            "OpenClipboard failed after 3 attempts - clipboard held by another process"
        ));
    }

    tokio::time::sleep(tokio::time::Duration::from_millis(
        CLIPBOARD_WRITE_SETTLE_MS,
    ))
    .await;

    let clear = [
        ki(VK_CONTROL, 0),
        ki(VK_LMENU, KEYEVENTF_KEYUP.0),
        ki(VK_CONTROL, KEYEVENTF_KEYUP.0),
    ];
    unsafe { SendInput(&clear, std::mem::size_of::<INPUT>() as i32) };

    tokio::time::sleep(tokio::time::Duration::from_millis(MODIFIER_GAP_MS)).await;

    let paste = [
        ki(VK_CONTROL, 0),
        ki(VK_V, 0),
        ki(VK_V, KEYEVENTF_KEYUP.0),
        ki(VK_CONTROL, KEYEVENTF_KEYUP.0),
    ];
    unsafe { SendInput(&paste, std::mem::size_of::<INPUT>() as i32) };
    tokio::time::sleep(tokio::time::Duration::from_millis(PASTE_SETTLE_MS)).await;

    restore_guard.restore_now();

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
        probe_source: injection_probe.source.as_str(),
        selection_state: injection_probe.selection_state.as_str(),
    })
}
