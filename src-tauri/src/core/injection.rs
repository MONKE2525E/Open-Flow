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

pub async fn inject_text(text: &str, target_hwnd: usize, contextual_caps: bool) -> anyhow::Result<()> {
    #[cfg(target_os = "windows")]
    {
        use windows::Win32::Foundation::HWND;
        use windows::Win32::UI::Input::KeyboardAndMouse::{
            SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS,
            KEYEVENTF_KEYUP, VK_C, VK_CONTROL, VK_LEFT, VK_LMENU, VK_RIGHT, VK_SHIFT, VK_V,
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

            // Look-back: peek at the character immediately before the cursor.
            // If it's a non-sentence-ending character, the injection is mid-sentence
            // and the first character should be lowercase.
            let adjusted: String = if contextual_caps {
                // Shift+Left selects one character to the left.
                let sel = [
                    ki(VK_SHIFT, 0),
                    ki(VK_LEFT, 0),
                    ki(VK_LEFT, KEYEVENTF_KEYUP.0),
                    ki(VK_SHIFT, KEYEVENTF_KEYUP.0),
                ];
                SendInput(&sel, std::mem::size_of::<INPUT>() as i32);

                // Ctrl+C copies the selection.
                let copy = [
                    ki(VK_CONTROL, 0),
                    ki(VK_C, 0),
                    ki(VK_C, KEYEVENTF_KEYUP.0),
                    ki(VK_CONTROL, KEYEVENTF_KEYUP.0),
                ];
                SendInput(&copy, std::mem::size_of::<INPUT>() as i32);
                tokio::time::sleep(tokio::time::Duration::from_millis(60)).await;

                // Read the peeked character from the clipboard.
                let peeked: Option<char> = read_clipboard_unicode().and_then(|v| {
                    if v.is_empty() || v[0] == 0 {
                        None
                    } else {
                        char::from_u32(v[0] as u32)
                    }
                });

                // Right arrow collapses the selection and restores the cursor position.
                let desel = [ki(VK_RIGHT, 0), ki(VK_RIGHT, KEYEVENTF_KEYUP.0)];
                SendInput(&desel, std::mem::size_of::<INPUT>() as i32);

                // Lowercase the first character of the injection when the cursor is
                // mid-sentence (preceded by anything other than a sentence-ending mark).
                let sentence_enders: &[char] = &['.', '!', '?', '\n', '\r'];
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
                return Err(anyhow::anyhow!("OpenClipboard failed after 3 attempts — clipboard held by another process"));
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
        }
    }

    #[cfg(not(target_os = "windows"))]
    log::warn!("inject_text: not on Windows — skipping. text={text} target_hwnd={target_hwnd}");

    Ok(())
}
