use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

// How long a previous injection stays relevant for spacing and capitalisation decisions.
// The keyboard hook resets this early whenever the user types, so the timeout is mainly
// a safety net for inactivity (e.g. the user idle for a minute then dictates again).
const INJECTION_STALE: Duration = Duration::from_secs(60);

// Maximum bytes stored for backspace-tracking. Covers any practical editing sequence
// while keeping the per-injection allocation bounded.
const HISTORY_TAIL: usize = 512;

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

    // GDI object formats — GetClipboardData returns an opaque GDI handle for these,
    // not an HGLOBAL, so GlobalSize/GlobalLock are undefined on them.
    const CF_BITMAP: u32 = 2;
    const CF_METAFILEPICT: u32 = 3;
    const CF_PALETTE: u32 = 9;
    const CF_ENHMETAFILE: u32 = 14;

    // Per-format cap: skip anything larger than 32 MB to stay within the 200 MB
    // RAM budget. Typical screenshots are 2–8 MB as CF_DIB; 32 MB is generous.
    const MAX_FORMAT_BYTES: usize = 32 * 1024 * 1024;

    let mut entries = Vec::new();

    let opened = (0..3).any(|i| {
        if i > 0 { std::thread::sleep(std::time::Duration::from_millis(50)); }
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
        if matches!(fmt, CF_BITMAP | CF_METAFILEPICT | CF_PALETTE | CF_ENHMETAFILE) {
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

    let opened = (0..3).any(|i| {
        if i > 0 { std::thread::sleep(std::time::Duration::from_millis(50)); }
        OpenClipboard(None).is_ok()
    });
    if !opened {
        return;
    }

    EmptyClipboard().ok();

    for (fmt, data) in &saved.entries {
        if let Ok(hg) = GlobalAlloc(GMEM_MOVEABLE, data.len()) {
            let ptr = GlobalLock(hg) as *mut u8;
            if ptr.is_null() {
                let _ = GlobalFree(Some(hg));
                continue;
            }
            std::ptr::copy_nonoverlapping(data.as_ptr(), ptr, data.len());
            let _ = GlobalUnlock(hg);
            if SetClipboardData(*fmt, Some(HANDLE(hg.0))).is_err() {
                let _ = GlobalFree(Some(hg));
            }
        }
    }

    CloseClipboard().ok();
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

            // Retrieve up to 3 characters immediately before the cursor from the
            // injection history tail. History is cleared whenever the user types,
            // moves the cursor, or switches windows — so when it's absent the context
            // is genuinely unknown and we default to capitalising (safe for new fields).
            let context: String = if contextual_caps || auto_spacing {
                match last_injection().lock() {
                    Ok(guard) => (*guard)
                        .as_ref()
                        .filter(|(hwnd, _, instant)| {
                            *hwnd == target_hwnd && instant.elapsed() < INJECTION_STALE
                        })
                        .map(|(_, text, _)| {
                            let chars: Vec<char> = text.chars().collect();
                            let n = chars.len();
                            chars[n.saturating_sub(3)..].iter().collect()
                        })
                        .unwrap_or_default(),
                    Err(_) => {
                        log::error!("injection history mutex poisoned");
                        String::new()
                    }
                }
            } else {
                String::new()
            };

            // Strip trailing whitespace from context to find the last meaningful char.
            // "Hello. " → trimmed = "Hello." → last = '.' → capitalize (new sentence).
            // "Hello"   → trimmed = "Hello"  → last = 'o' → lowercase (mid-sentence).
            // ""        → no prior context              → capitalize (new/empty field).
            let sentence_enders: &[char] = &['.', '!', '?', '\n', '\r'];
            let trimmed_ctx = context.trim_end_matches(|c: char| c.is_whitespace());
            let should_capitalize = trimmed_ctx.is_empty()
                || trimmed_ctx
                    .chars()
                    .next_back()
                    .map(|c| sentence_enders.contains(&c))
                    .unwrap_or(true);

            let mut adjusted = if contextual_caps {
                if should_capitalize {
                    text.to_owned()
                } else {
                    let mut chars = text.chars();
                    match chars.next() {
                        Some(first) => first.to_lowercase().collect::<String>() + chars.as_str(),
                        None => text.to_owned(),
                    }
                }
            } else {
                text.to_owned()
            };

            // Add a space only when the cursor sits immediately after a non-whitespace
            // character. Empty context (new field) or trailing whitespace → no space.
            if auto_spacing {
                if let Some(c) = context.chars().next_back() {
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

            // Restore all previously saved clipboard formats.
            restore_clipboard_all(&saved);

            // Record a tail of the injected text so the keyboard hook can pop
            // characters off it on Backspace, keeping the context accurate after
            // editing. Only the last HISTORY_TAIL bytes are kept to bound memory use.
            if target_hwnd != 0 && !adjusted.is_empty() {
                if let Ok(mut guard) = last_injection().lock() {
                    let tail = if adjusted.len() > HISTORY_TAIL {
                        let mut start = adjusted.len() - HISTORY_TAIL;
                        while !adjusted.is_char_boundary(start) {
                            start += 1;
                        }
                        adjusted[start..].to_owned()
                    } else {
                        adjusted.clone()
                    };
                    *guard = Some((target_hwnd, tail, Instant::now()));
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
