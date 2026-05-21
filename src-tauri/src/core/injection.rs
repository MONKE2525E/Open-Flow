use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

// How long a previous injection stays relevant for spacing and capitalisation decisions.
// The keyboard hook resets this early whenever the user types, so the timeout is mainly
// a safety net for inactivity (e.g. the user idle for a minute then dictates again).
const INJECTION_STALE: Duration = Duration::from_secs(60);

static LAST_INJECTION: OnceLock<Mutex<Option<(usize, String, Instant)>>> = OnceLock::new();

fn last_injection() -> &'static Mutex<Option<(usize, String, Instant)>> {
    LAST_INJECTION.get_or_init(|| Mutex::new(None))
}

/// Full reset — called on Enter, character keys, arrows, etc. The cursor context
/// is unknown after such input, so the next injection starts fresh.
pub fn reset_injection_history() {
    if let Ok(mut guard) = last_injection().lock() {
        *guard = None;
    }
}

/// Called on Backspace. Pops the last character off the tracked text so the
/// next injection still knows what's immediately before the cursor after the
/// deletion. Clears the record entirely once the tracked text empties.
pub fn backspace_injection_history() {
    if let Ok(mut guard) = last_injection().lock() {
        let empty = if let Some((_, ref mut text, ref mut time)) = *guard {
            text.pop();
            *time = Instant::now();
            text.is_empty()
        } else {
            false
        };
        if empty {
            *guard = None;
        }
    }
}

#[cfg(target_os = "windows")]
unsafe fn read_clipboard_unicode() -> Option<Vec<u16>> {
    use windows::Win32::System::DataExchange::{
        CloseClipboard, GetClipboardData, IsClipboardFormatAvailable, OpenClipboard,
    };
    use windows::Win32::System::Memory::{GlobalLock, GlobalUnlock};

    const CF_UNICODETEXT: u32 = 13;

    if IsClipboardFormatAvailable(CF_UNICODETEXT).is_ok() && OpenClipboard(None).is_ok() {
        let saved = GetClipboardData(CF_UNICODETEXT).ok().and_then(|h| {
            let ptr = GlobalLock(windows::Win32::Foundation::HGLOBAL(h.0)) as *const u16;
            if ptr.is_null() {
                return None;
            }
            let mut len = 0usize;
            while *ptr.add(len) != 0 {
                len += 1;
            }
            let v = std::slice::from_raw_parts(ptr, len + 1).to_vec();
            let _ = GlobalUnlock(windows::Win32::Foundation::HGLOBAL(h.0));
            Some(v)
        });
        CloseClipboard().ok();
        saved
    } else {
        None
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
) -> anyhow::Result<String> {
    #[cfg(target_os = "windows")]
    {
        use windows::Win32::Foundation::HWND;
        use windows::Win32::UI::Input::KeyboardAndMouse::{
            SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS,
            KEYEVENTF_KEYUP, VK_CONTROL, VK_LMENU, VK_V,
        };
        use windows::Win32::UI::WindowsAndMessaging::SetForegroundWindow;

        unsafe {
            // Save existing clipboard
            let saved: Option<Vec<u16>> = read_clipboard_unicode();

            // Restore focus to the window the user was dictating into.
            // The user may have switched windows during the transcription/cleanup
            // pipeline; without this the Ctrl+V paste lands in the wrong app.
            // WH_KEYBOARD_LL hooks give the process implicit foreground lock
            // permission, so SetForegroundWindow succeeds from here.
            if target_hwnd != 0 {
                let _ = SetForegroundWindow(HWND(target_hwnd as *mut core::ffi::c_void));
                tokio::time::sleep(tokio::time::Duration::from_millis(150)).await;
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

            // Look up what we last injected into this window. The keyboard hook
            // resets this whenever the user edits text, so by the time we reach
            // here the history reflects the actual state of the cursor context.
            let peeked: Option<char> = if contextual_caps || auto_spacing {
                let guard = last_injection()
                    .lock()
                    .map_err(|_| anyhow::anyhow!("injection history mutex poisoned"))?;
                match *guard {
                    Some((hwnd, ref text, ref instant))
                        if hwnd == target_hwnd && instant.elapsed() < INJECTION_STALE =>
                    {
                        text.chars().next_back()
                    }
                    _ => None,
                }
                // guard dropped here — Mutex not held across any await
            } else {
                None
            };

            let sentence_enders: &[char] = &['.', '!', '?', '\n', '\r'];
            let mut adjusted = if contextual_caps {
                let should_lower = peeked
                    .map(|c| !sentence_enders.contains(&c))
                    .unwrap_or(false);
                if should_lower {
                    let mut chars = text.chars();
                    match chars.next() {
                        Some(first) => first.to_lowercase().collect::<String>() + chars.as_str(),
                        None => text.to_owned(),
                    }
                } else {
                    text.to_owned()
                }
            } else {
                text.to_owned()
            };

            if auto_spacing {
                if let Some(c) = peeked {
                    if !c.is_whitespace() && !adjusted.starts_with(char::is_whitespace) {
                        adjusted = format!(" {adjusted}");
                    }
                }
            }

            let text_to_inject = adjusted.as_str();

            // Write injection text — retry up to 3 times if another process holds the clipboard.
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
                    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
                }
            }
            if !clipboard_written {
                return Err(anyhow::anyhow!(
                    "OpenClipboard failed after 3 attempts — clipboard held by another process"
                ));
            }

            tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

            // Step 1 — clear any dangling Alt the target app may have from the
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

            // Step 2 — let the app process the modifier-state change before we
            // inject Ctrl+V.  Without this pause, some apps (browsers, IDEs) end
            // up processing V without Ctrl because the Alt-up and V-down land in
            // the same message-pump cycle.
            tokio::time::sleep(tokio::time::Duration::from_millis(30)).await;

            // Step 3 — clean Ctrl+V with no dangling modifiers.
            let paste = [
                ki(VK_CONTROL, 0),
                ki(VK_V, 0),
                ki(VK_V, KEYEVENTF_KEYUP.0),
                ki(VK_CONTROL, KEYEVENTF_KEYUP.0),
            ];
            SendInput(&paste, std::mem::size_of::<INPUT>() as i32);
            tokio::time::sleep(tokio::time::Duration::from_millis(80)).await;

            // Restore clipboard
            if let Some(saved_wide) = saved {
                let _ = write_clipboard_unicode(&saved_wide);
            }

            // Record the full injected text so the keyboard hook can pop characters
            // off it on Backspace, keeping the context accurate after editing.
            if target_hwnd != 0 && !adjusted.is_empty() {
                if let Ok(mut guard) = last_injection().lock() {
                    *guard = Some((target_hwnd, adjusted.clone(), Instant::now()));
                }
            }

            Ok(adjusted)
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        log::warn!("inject_text: not on Windows — skipping target_hwnd={target_hwnd}");

        Ok(text.to_string())
    }
}
