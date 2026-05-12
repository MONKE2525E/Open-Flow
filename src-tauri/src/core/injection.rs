pub async fn inject_text(text: &str) -> anyhow::Result<()> {
    #[cfg(target_os = "windows")]
    {
        use windows::Win32::Foundation::HANDLE;
        use windows::Win32::System::DataExchange::{
            CloseClipboard, EmptyClipboard, GetClipboardData, IsClipboardFormatAvailable,
            OpenClipboard, SetClipboardData,
        };
        use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
        use windows::Win32::UI::Input::KeyboardAndMouse::{
            SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS,
            KEYEVENTF_KEYUP, VK_CONTROL, VK_LMENU, VK_V,
        };

        const CF_UNICODETEXT: u32 = 13;

        unsafe {
            // Save existing clipboard
            let saved: Option<Vec<u16>> = if IsClipboardFormatAvailable(CF_UNICODETEXT).is_ok() {
                if OpenClipboard(None).is_ok() {
                    let saved = GetClipboardData(CF_UNICODETEXT)
                        .ok()
                        .and_then(|h| {
                            let ptr = GlobalLock(
                                windows::Win32::Foundation::HGLOBAL(h.0)
                            ) as *const u16;
                            if ptr.is_null() { return None; }
                            let mut len = 0usize;
                            while *ptr.add(len) != 0 { len += 1; }
                            let v = std::slice::from_raw_parts(ptr, len + 1).to_vec();
                            GlobalUnlock(windows::Win32::Foundation::HGLOBAL(h.0));
                            Some(v)
                        });
                    CloseClipboard().ok();
                    saved
                } else { None }
            } else { None };

            // Write new text
            let wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
            if OpenClipboard(None).is_ok() {
                EmptyClipboard().ok();
                let hg = GlobalAlloc(GMEM_MOVEABLE, wide.len() * 2)?;
                let ptr = GlobalLock(hg) as *mut u16;
                std::ptr::copy_nonoverlapping(wide.as_ptr(), ptr, wide.len());
                GlobalUnlock(hg);
                SetClipboardData(CF_UNICODETEXT, Some(HANDLE(hg.0))).ok();
                CloseClipboard().ok();
            }

            tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

            // Key sequence to paste without triggering menu-bar activation:
            //
            //  1. Ctrl-down  — breaks the "Alt was pressed alone" invariant that
            //                  DefWindowProc uses to decide whether to activate the
            //                  menu bar on Alt-up. Previous message is now Ctrl-down,
            //                  not Alt-down, so WM_SYSKEYUP(Alt) won't fire SC_KEYMENU.
            //  2. Alt-up     — clears the dangling Alt-down the app received during
            //                  recording. Without this, GetKeyState(VK_MENU) is still
            //                  "held" and Ctrl+V is seen as Ctrl+Alt+V (no paste).
            //  3. V-down/up  — Alt is now released; Ctrl is still held → Ctrl+V → paste.
            //  4. Ctrl-up
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
            let inputs = [
                ki(VK_CONTROL,           0),
                ki(VK_LMENU,             KEYEVENTF_KEYUP.0),
                ki(VK_V,                 0),
                ki(VK_V,                 KEYEVENTF_KEYUP.0),
                ki(VK_CONTROL,           KEYEVENTF_KEYUP.0),
            ];
            SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
            tokio::time::sleep(tokio::time::Duration::from_millis(80)).await;

            // Restore clipboard
            if let Some(saved_wide) = saved {
                if OpenClipboard(None).is_ok() {
                    EmptyClipboard().ok();
                    let hg = GlobalAlloc(GMEM_MOVEABLE, saved_wide.len() * 2)?;
                    let ptr = GlobalLock(hg) as *mut u16;
                    std::ptr::copy_nonoverlapping(saved_wide.as_ptr(), ptr, saved_wide.len());
                    GlobalUnlock(hg);
                    SetClipboardData(CF_UNICODETEXT, Some(HANDLE(hg.0))).ok();
                    CloseClipboard().ok();
                }
            }
        }
    }

    #[cfg(not(target_os = "windows"))]
    log::warn!("inject_text: not on Windows — skipping. text={text}");

    Ok(())
}
