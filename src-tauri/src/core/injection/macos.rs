use super::*;

fn post_key_event(
    src: core_graphics::event_source::CGEventSource,
    keycode: core_graphics::event::CGKeyCode,
    down: bool,
    flags: core_graphics::event::CGEventFlags,
) {
    use core_graphics::event::{CGEvent, CGEventTapLocation};
    if let Ok(e) = CGEvent::new_keyboard_event(src, keycode, down) {
        e.set_flags(flags);
        e.post(CGEventTapLocation::HID);
    }
}

pub(super) async fn copy_to_clipboard(text: &str) -> anyhow::Result<()> {
    crate::system::mac_app::pasteboard_set_string(text);
    Ok(())
}

async fn macos_clipboard_sniff_context(target_hwnd: usize) -> Option<InjectionContextProbe> {
    use core_graphics::event::{CGEventFlags, CGKeyCode};
    use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};

    const KEY_LEFT_ARROW: CGKeyCode = 123;
    const KEY_RIGHT_ARROW: CGKeyCode = 124;
    const VK_ANSI_C: CGKeyCode = 8;

    if crate::core::window_context::get_foreground_hwnd() != target_hwnd {
        return None;
    }

    if crate::system::mac_app::pasteboard_has_non_text_formats() {
        log::info!(
            "bypassing macOS clipboard sniff fallback because pasteboard contains non-text or rich-text formats"
        );
        return None;
    }

    crate::system::mac_app::pasteboard_set_string("");

    {
        let src = CGEventSource::new(CGEventSourceStateID::CombinedSessionState).ok()?;
        post_key_event(
            src.clone(),
            KEY_LEFT_ARROW,
            true,
            CGEventFlags::CGEventFlagShift,
        );
        post_key_event(src, KEY_LEFT_ARROW, false, CGEventFlags::CGEventFlagShift);
    }
    tokio::time::sleep(Duration::from_millis(30)).await;

    {
        let src = CGEventSource::new(CGEventSourceStateID::CombinedSessionState).ok()?;
        post_key_event(
            src.clone(),
            VK_ANSI_C,
            true,
            CGEventFlags::CGEventFlagCommand,
        );
        post_key_event(src, VK_ANSI_C, false, CGEventFlags::CGEventFlagCommand);
    }
    tokio::time::sleep(Duration::from_millis(30)).await;

    let sniffed = crate::system::mac_app::pasteboard_get_string().unwrap_or_default();

    if !sniffed.is_empty() {
        {
            let src = CGEventSource::new(CGEventSourceStateID::CombinedSessionState).ok()?;
            post_key_event(src.clone(), KEY_RIGHT_ARROW, true, CGEventFlags::empty());
            post_key_event(src, KEY_RIGHT_ARROW, false, CGEventFlags::empty());
        }
        tokio::time::sleep(Duration::from_millis(15)).await;
    }

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

/// Guarantees the saved pasteboard string is restored exactly once, even on
/// an early return/panic — not just on the normal success path. `restore_now`
/// disarms it so the `Drop` fallback never double-restores.
struct ClipboardRestoreGuard {
    saved: Option<Option<String>>,
}
impl ClipboardRestoreGuard {
    fn new(saved: Option<String>) -> Self {
        Self { saved: Some(saved) }
    }
    fn restore_now(&mut self) {
        if let Some(saved) = self.saved.take() {
            match saved {
                Some(prev) => crate::system::mac_app::pasteboard_set_string(&prev),
                None => crate::system::mac_app::pasteboard_set_string(""),
            }
        }
    }
}
impl Drop for ClipboardRestoreGuard {
    fn drop(&mut self) {
        if let Some(saved) = self.saved.take() {
            match saved {
                Some(prev) => crate::system::mac_app::pasteboard_set_string(&prev),
                None => crate::system::mac_app::pasteboard_set_string(""),
            }
        }
    }
}

pub(super) async fn inject_text(
    text: &str,
    target_hwnd: usize,
    contextual_caps: bool,
    auto_spacing: bool,
    profile: &str,
    clipboard_sniff_enabled: bool,
) -> anyhow::Result<InjectionOutcome> {
    use core_graphics::event::{CGEvent, CGEventFlags, CGEventTapLocation, CGKeyCode};
    use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};

    const VK_ANSI_V: CGKeyCode = 9;

    // Declared before the restore guard so it releases *after* the pasteboard
    // has been restored (Rust drops in reverse declaration order).
    let _injection_guard = super::injection_lock().lock().await;

    if target_hwnd != 0 {
        let pid = (target_hwnd & 0xFFFFFFFF) as i32;
        crate::system::mac_app::activate_pid(pid);
        tokio::time::sleep(Duration::from_millis(120)).await;
    }

    let saved = crate::system::mac_app::pasteboard_get_string();
    let mut restore_guard = ClipboardRestoreGuard::new(saved);

    let mut injection_probe = if contextual_caps || auto_spacing {
        crate::core::context_probe::read_injection_context_probe().await
    } else {
        unavailable_injection_probe()
    };
    if (contextual_caps || auto_spacing) && injection_probe.source.allows_history_fallback() {
        if let Some(history_probe) = fallback_probe_from_history(target_hwnd) {
            injection_probe = history_probe;
        } else if clipboard_sniff_enabled {
            if let Some(sniff_probe) = macos_clipboard_sniff_context(target_hwnd).await {
                injection_probe = sniff_probe;
            }
        }
    }
    let (adjusted, context_kind, case_decision) = apply_probe_adjustments(
        text,
        contextual_caps,
        auto_spacing,
        profile,
        &injection_probe,
    );

    crate::system::mac_app::pasteboard_set_string(&adjusted);
    tokio::time::sleep(Duration::from_millis(20)).await;

    if !crate::system::mac_app::is_accessibility_verified()
        && !crate::commands::check_accessibility_permission(false)
    {
        log::error!(
            "inject_text: Cmd+V injection attempted but Accessibility is not granted — \
             grant Verenu Accessibility permission in System Settings → Privacy & Security → Accessibility"
        );
    }

    const VK_COMMAND: CGKeyCode = 55;
    let posted = tokio::task::spawn_blocking(move || -> Option<()> {
        use std::{thread::sleep, time::Duration as Std};
        let src = CGEventSource::new(CGEventSourceStateID::CombinedSessionState).ok()?;
        crate::core::hotkey::begin_synthetic_paste_suppression(400);
        let cmd_down = CGEvent::new_keyboard_event(src.clone(), VK_COMMAND, true).ok()?;
        cmd_down.set_flags(CGEventFlags::CGEventFlagCommand);
        cmd_down.post(CGEventTapLocation::HID);
        sleep(Std::from_millis(8));
        let result = (|| -> Option<()> {
            let v_down = CGEvent::new_keyboard_event(src.clone(), VK_ANSI_V, true).ok()?;
            v_down.set_flags(CGEventFlags::CGEventFlagCommand);
            v_down.post(CGEventTapLocation::HID);
            sleep(Std::from_millis(8));
            if let Ok(v_up) = CGEvent::new_keyboard_event(src.clone(), VK_ANSI_V, false) {
                v_up.set_flags(CGEventFlags::CGEventFlagCommand);
                v_up.post(CGEventTapLocation::HID);
                sleep(Std::from_millis(8));
                Some(())
            } else {
                None
            }
        })();
        if let Ok(cmd_up) = CGEvent::new_keyboard_event(src, VK_COMMAND, false) {
            cmd_up.set_flags(CGEventFlags::empty());
            cmd_up.post(CGEventTapLocation::HID);
        }
        result
    })
    .await
    .ok()
    .flatten();

    log::info!(
        "inject_text(macos): target_pid={} text_len={} posted={}",
        (target_hwnd & 0xFFFFFFFF) as i32,
        adjusted.len(),
        posted.is_some(),
    );

    tokio::time::sleep(Duration::from_millis(120)).await;

    restore_guard.restore_now();

    if posted.is_none() {
        return Err(anyhow::anyhow!(
            "inject_text: failed to synthesise Cmd+V — grant Verenu Accessibility permission"
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
        probe_source: injection_probe.source.as_str(),
        selection_state: injection_probe.selection_state.as_str(),
    })
}
