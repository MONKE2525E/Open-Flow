use super::*;

struct SavedClipboard {
    entries: Vec<(u32, Vec<u8>)>,
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
    use ::windows::Win32::System::Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalUnlock};

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
    use ::windows::Win32::System::Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalUnlock};

    const CF_UNICODETEXT: u32 = 13;

    if OpenClipboard(None).is_ok() {
        EmptyClipboard().ok();
        let hg = GlobalAlloc(GMEM_MOVEABLE, data.len() * 2)?;
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
        INPUT, INPUT_0, INPUT_KEYBOARD, KEYBD_EVENT_FLAGS, KEYBDINPUT, KEYEVENTF_KEYUP, SendInput,
        VK_CONTROL, VK_LMENU, VK_V,
    };
    use ::windows::Win32::UI::WindowsAndMessaging::SetForegroundWindow;

    unsafe {
        let saved = save_clipboard_all();

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

        let clear = [
            ki(VK_CONTROL, 0),
            ki(VK_LMENU, KEYEVENTF_KEYUP.0),
            ki(VK_CONTROL, KEYEVENTF_KEYUP.0),
        ];
        SendInput(&clear, std::mem::size_of::<INPUT>() as i32);

        tokio::time::sleep(tokio::time::Duration::from_millis(MODIFIER_GAP_MS)).await;

        let paste = [
            ki(VK_CONTROL, 0),
            ki(VK_V, 0),
            ki(VK_V, KEYEVENTF_KEYUP.0),
            ki(VK_CONTROL, KEYEVENTF_KEYUP.0),
        ];
        SendInput(&paste, std::mem::size_of::<INPUT>() as i32);
        tokio::time::sleep(tokio::time::Duration::from_millis(PASTE_SETTLE_MS)).await;

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
            probe_source: injection_probe.source.as_str(),
            selection_state: injection_probe.selection_state.as_str(),
        })
    }
}
