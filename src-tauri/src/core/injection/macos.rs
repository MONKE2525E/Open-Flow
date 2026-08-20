use super::*;
use std::time::Duration;

pub(super) async fn copy_to_clipboard(text: &str) -> anyhow::Result<()> {
    crate::system::mac_app::pasteboard_write_string(text)
        .map(|_| ())
        .map_err(|_| anyhow::anyhow!("Could not write pasteboard text"))
}

/// Restores every pasteboard representation exactly once, provided the user
/// has not copied something else since Verenu wrote its temporary payload.
struct ClipboardRestoreGuard {
    saved: Option<crate::system::mac_app::PasteboardSnapshot>,
    expected_change_count: Option<isize>,
}
impl ClipboardRestoreGuard {
    fn new(saved: Option<crate::system::mac_app::PasteboardSnapshot>) -> Self {
        Self {
            saved,
            expected_change_count: None,
        }
    }
    fn mark_temporary_write(&mut self, change_count: isize) {
        self.expected_change_count = Some(change_count);
    }
    fn restore_now(&mut self) {
        if let (Some(saved), Some(expected)) =
            (self.saved.take(), self.expected_change_count.take())
        {
            saved.restore_if_unchanged(expected);
        }
    }
}
impl Drop for ClipboardRestoreGuard {
    fn drop(&mut self) {
        self.restore_now();
    }
}

pub(super) async fn inject_text(
    text: &str,
    target_hwnd: usize,
    contextual_caps: bool,
    auto_spacing: bool,
    profile: &str,
    language: &str,
    protected_initial_case: bool,
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

    let saved = crate::system::mac_app::pasteboard_snapshot();
    let mut restore_guard = ClipboardRestoreGuard::new(saved);

    let mut injection_probe = if contextual_caps || auto_spacing {
        crate::core::context_probe::read_injection_context_probe().await
    } else {
        unavailable_injection_probe()
    };
    let target_pid = target_hwnd & 0xFFFF_FFFF;
    if injection_probe.target_id != 0 && injection_probe.target_id != target_pid {
        log::debug!(
            "injection: rejecting stale AX probe target_pid={} probe_pid={}",
            target_pid,
            injection_probe.target_id
        );
        injection_probe = unavailable_injection_probe();
    }
    let (adjusted, context_kind, case_decision) = apply_probe_adjustments(
        text,
        contextual_caps,
        auto_spacing,
        profile,
        language,
        protected_initial_case,
        &injection_probe,
    );

    let change_count = match crate::system::mac_app::pasteboard_write_string(&adjusted) {
        Ok(change_count) => change_count,
        Err(change_count) => {
            if change_count >= 0 {
                restore_guard.mark_temporary_write(change_count);
            }
            anyhow::bail!("Could not write temporary pasteboard payload");
        }
    };
    restore_guard.mark_temporary_write(change_count);
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
