use std::sync::MutexGuard;
use std::time::{Duration, Instant};

use crate::core::window_geometry::WindowTarget;
use crate::pipeline::{self, hide_pill, start_recording_session, AppState, SharedState};
use crate::DbHandle;
use tauri::{Emitter, Manager};

fn lock_app_state(state: &SharedState) -> Option<MutexGuard<'_, AppState>> {
    match state.lock() {
        Ok(guard) => Some(guard),
        Err(_) => {
            log::error!("Recording state lock was poisoned");
            None
        }
    }
}

#[derive(Clone, Copy)]
enum HotkeyEvent {
    Press,
    Release,
    HandlessToggle,
    Cancel,
    EscapeCancel,
    CopyLast,
}

/// A hands-free stop is itself a quick tap, so the next click can still be
/// part of the same physical double-click. Without this fence, resetting the
/// hook's tap state makes that second click look like a fresh Press, which can
/// interrupt the transcription that the first click just started.
const HANDSFREE_STOP_GUARD: Duration = Duration::from_millis(350);

#[derive(Default)]
struct HandsfreeStopGuard {
    until: Option<Instant>,
}

impl HandsfreeStopGuard {
    fn arm(&mut self, now: Instant) {
        self.until = Some(now + HANDSFREE_STOP_GUARD);
    }

    fn suppresses(&mut self, event: HotkeyEvent, now: Instant) -> bool {
        let Some(until) = self.until else {
            return false;
        };
        if now >= until {
            self.until = None;
            return false;
        }
        matches!(
            event,
            HotkeyEvent::Press
                | HotkeyEvent::Release
                | HotkeyEvent::HandlessToggle
                | HotkeyEvent::Cancel
        )
    }
}

pub(crate) fn setup_hotkey(app: &mut tauri::App, shared: SharedState) {
    // The WH_KEYBOARD_LL hook callback must return within Windows' hook timeout
    // (~300ms) or the hook is silently removed. All real work happens in a Tokio
    // task below; callbacks only send a lightweight channel message.
    let (hotkey_tx, mut hotkey_rx) = tokio::sync::mpsc::unbounded_channel::<HotkeyEvent>();
    let tx_press = hotkey_tx.clone();
    let tx_handless = hotkey_tx.clone();
    let tx_cancel = hotkey_tx.clone();
    let tx_escape = hotkey_tx.clone();
    let tx_copy_last = hotkey_tx.clone();
    let tx_release = hotkey_tx;

    match crate::core::hotkey::start(
        move || {
            let _ = tx_press.send(HotkeyEvent::Press);
        },
        move || {
            let _ = tx_release.send(HotkeyEvent::Release);
        },
        move || {
            let _ = tx_handless.send(HotkeyEvent::HandlessToggle);
        },
        move || {
            let _ = tx_cancel.send(HotkeyEvent::Cancel);
        },
        move || {
            let _ = tx_escape.send(HotkeyEvent::EscapeCancel);
        },
        move || {
            let _ = tx_copy_last.send(HotkeyEvent::CopyLast);
        },
    ) {
        Ok(_handle) => { /* hook thread running */ }
        Err(e) => {
            log::error!("Hotkey hook failed to start: {e}");
            let app_h = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                // Give the webview a moment to initialise before emitting.
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                app_h
                    .emit(
                        "verenu:error",
                        format!("Keyboard hook failed to install — hotkey unavailable. {e}"),
                    )
                    .ok();
            });
            return;
        }
    }

    let app_hk = app.handle().clone();
    let state_hk = shared;

    tauri::async_runtime::spawn(async move {
        let mut handsfree_stop_guard = HandsfreeStopGuard::default();
        while let Some(event) = hotkey_rx.recv().await {
            if handsfree_stop_guard.suppresses(event, Instant::now()) {
                log::debug!("hotkey: ignored follow-up input after hands-free stop");
                continue;
            }
            match event {
                HotkeyEvent::Press => {
                    pipeline::clear_handless_hold_marker(&state_hk);
                    enum PressAction {
                        None,
                        Fresh,
                        Interrupt(pipeline::ActivePipeline),
                    }
                    let action = {
                        let Some(st) = lock_app_state(&state_hk) else {
                            continue;
                        };
                        match &st.lifecycle {
                            pipeline::DictationLifecycle::Idle => PressAction::Fresh,
                            pipeline::DictationLifecycle::Processing(_) => {
                                drop(st);
                                match pipeline::take_active_pipeline_for_interrupt(&state_hk) {
                                    Some(active) => PressAction::Interrupt(active),
                                    None => PressAction::None,
                                }
                            }
                            // Recording/Starting: already busy. Stopping/Finalizing:
                            // short-lived, deliberately dropped — see the UX note
                            // in the plan (no queueing, no concurrent recording).
                            _ => PressAction::None,
                        }
                    };
                    match action {
                        PressAction::None => {}
                        PressAction::Fresh => {
                            if pipeline::reserve_starting(&state_hk).is_ok() {
                                // Capture the target window before recording starts
                                // so inject_text can restore focus to it after the
                                // pipeline, even if the user switched windows
                                // during transcription.
                                let target = WindowTarget::capture_foreground();
                                if let Some(mut st) = lock_app_state(&state_hk) {
                                    st.target = target;
                                    st.pill_placement_stale = true;
                                }
                                start_recording_session(&app_hk, &state_hk, "recording", false);
                            }
                        }
                        PressAction::Interrupt(active) => {
                            // Informational — the old task already lost ownership
                            // of this generation the instant it was taken above,
                            // so its own ownership checks fail regardless of
                            // whether it observes this signal in time.
                            let _ = active.cancel_tx.send(true);
                            let target = WindowTarget::capture_foreground();
                            if let Some(mut st) = lock_app_state(&state_hk) {
                                st.target = target;
                                st.pill_placement_stale = true;
                            }
                            start_recording_session(&app_hk, &state_hk, "recording", false);
                        }
                    }
                }

                HotkeyEvent::Release => {
                    if pipeline::consume_handless_hold_stop(&state_hk) {
                        log::debug!("hotkey: ignored stale hold release after Space hands-free conversion");
                        continue;
                    }
                    tauri::async_runtime::spawn(pipeline::run_pipeline(
                        app_hk.clone(),
                        state_hk.clone(),
                    ));
                }

                HotkeyEvent::HandlessToggle => {
                    let (is_handless, has_session) = {
                        let Some(st) = lock_app_state(&state_hk) else {
                            continue;
                        };
                        (
                            st.lifecycle.is_handless_recording(),
                            st.lifecycle.is_recording(),
                        )
                    };
                    if is_handless {
                        crate::core::hotkey::set_handless_active(false);
                        if has_session {
                            tauri::async_runtime::spawn(pipeline::run_pipeline(
                                app_hk.clone(),
                                state_hk.clone(),
                            ));
                        } else {
                            crate::system::media_control::end_dictation_media_pause();
                        }
                    } else if has_session && pipeline::promote_recording_to_handless(&state_hk) {
                        // Space converts an active hold-to-talk recording in
                        // place. The existing session stays open, and the
                        // hook suppresses the original chord release.
                        crate::core::hotkey::set_handless_active(true);
                        pipeline::update_pill_state(&app_hk, "handsfree");
                    } else if !has_session && pipeline::reserve_starting(&state_hk).is_ok() {
                        let target = WindowTarget::capture_foreground();
                        if let Some(mut st) = lock_app_state(&state_hk) {
                            st.target = target;
                            st.pill_placement_stale = true;
                        }
                        start_recording_session(&app_hk, &state_hk, "handsfree", true);
                        crate::core::hotkey::set_handless_active(true);
                    }
                }

                HotkeyEvent::Cancel => {
                    // A discarded first tap (or a quick handsfree stop) must not
                    // let the pending start cue sound.
                    crate::media::sound::cancel_pending_start();
                    if pipeline::consume_handless_hold_stop(&state_hk) {
                        log::debug!("hotkey: ignored stale hold cancel after Space hands-free conversion");
                        continue;
                    }
                    let is_handless = {
                        let Some(st) = lock_app_state(&state_hk) else {
                            continue;
                        };
                        st.lifecycle.is_handless_recording()
                    };
                    if is_handless {
                        // Quick tap while in handsfree = stop. Clear chord state
                        // immediately so the still-open double-tap window can't
                        // re-trigger a fresh handsfree session.
                        handsfree_stop_guard.arm(Instant::now());
                        crate::core::hotkey::set_handless_active(false);
                        crate::core::hotkey::reset_chord_state();
                        tauri::async_runtime::spawn(pipeline::run_pipeline(
                            app_hk.clone(),
                            state_hk.clone(),
                        ));
                    } else {
                        // First click of a double-tap gesture outside handsfree:
                        // cancel the short recording that just started, but
                        // stash its audio (if long/loud enough) so the pill's
                        // Continue button can resume it instead of losing it.
                        let taken = pipeline::take_recording_plain(&state_hk);
                        if let Some((session, exclusive_mic_session_id)) = taken {
                            let app_for_cancel = app_hk.clone();
                            let state_for_cancel = state_hk.clone();
                            tauri::async_runtime::spawn(async move {
                                pipeline::cancel_recording_with_resume(
                                    &app_for_cancel,
                                    &state_for_cancel,
                                    session,
                                    exclusive_mic_session_id,
                                )
                                .await;
                            });
                        } else {
                            hide_pill(&app_hk);
                        }
                    }
                }

                HotkeyEvent::EscapeCancel => {
                    crate::core::hotkey::set_handless_active(false);
                    // If escape lands before the start cue fired, suppress it.
                    crate::media::sound::cancel_pending_start();

                    // Cancel in-flight processing outright — no append, no
                    // insertion. Distinct from the pre-Release recording-cancel
                    // path below (a dictation can be in at most one of the two).
                    // Its audio already cleared the pipeline's own quality gate
                    // to get this far, so stash it unconditionally for Continue.
                    if let Some(active) = pipeline::take_active_pipeline_for_escape(&state_hk) {
                        let _ = active.cancel_tx.send(true);
                        pipeline::stash_cancelled_capture(
                            &app_hk,
                            &state_hk,
                            active.captured_audio,
                        );
                        continue;
                    }

                    // Cancel sound cue (if any) is now played inside
                    // cancel_recording_with_resume/stash_cancelled_capture,
                    // consistently across every cancel path.
                    let taken = pipeline::take_recording_plain(&state_hk);
                    if let Some((session, exclusive_mic_session_id)) = taken {
                        let app_for_cancel = app_hk.clone();
                        let state_for_cancel = state_hk.clone();
                        tauri::async_runtime::spawn(async move {
                            pipeline::cancel_recording_with_resume(
                                &app_for_cancel,
                                &state_for_cancel,
                                session,
                                exclusive_mic_session_id,
                            )
                            .await;
                        });
                    } else {
                        // Nothing was recording/processing — if the pill is
                        // currently showing a pending "Cancelled -> Undo"
                        // offer, Escape dismisses it (same as clicking the
                        // pill's own dismiss button).
                        if pipeline::take_cancelled_capture_if_fresh(&state_hk).is_some() {
                            pipeline::emit_cancelled_capture_cleared(&app_hk);
                        }
                        hide_pill(&app_hk);
                    }
                }

                HotkeyEvent::CopyLast => {
                    // Global fallback for a paste that failed silently (not
                    // caught by the pipeline's own detection): re-copy the
                    // most recent dictation to the clipboard on demand.
                    // try_state — a hotkey can fire during teardown when
                    // managed state is already gone, and panicking in the
                    // hook thread would take the app down.
                    let Some(db_handle) = app_hk.try_state::<DbHandle>() else {
                        log::warn!("CopyLast hotkey: DbHandle not managed, skipping");
                        continue;
                    };
                    let db_handle = db_handle.inner().clone();
                    let app_for_copy = app_hk.clone();
                    tauri::async_runtime::spawn(async move {
                        let recent = tokio::task::spawn_blocking(move || {
                            crate::data::db::query_recent_page(&db_handle, 1, 0)
                        })
                        .await
                        .ok()
                        .and_then(|r| r.ok())
                        .unwrap_or_default();
                        match recent.into_iter().next() {
                            Some(entry) if !entry.clean_text.trim().is_empty() => {
                                if let Err(e) =
                                    crate::core::injection::copy_to_clipboard(&entry.clean_text)
                                        .await
                                {
                                    log::warn!(
                                        "hotkey: copy-last-dictation clipboard write failed: {e}"
                                    );
                                    return;
                                }
                                pipeline::show_copied_pill(
                                    &app_for_copy,
                                    "Copied last dictation to clipboard",
                                );
                            }
                            _ => {
                                pipeline::show_copied_pill(&app_for_copy, "Nothing to copy yet");
                            }
                        }
                    });
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handsfree_stop_guard_consumes_the_followup_click_but_not_escape() {
        let now = Instant::now();
        let mut guard = HandsfreeStopGuard::default();
        guard.arm(now);

        for event in [
            HotkeyEvent::Press,
            HotkeyEvent::Release,
            HotkeyEvent::HandlessToggle,
            HotkeyEvent::Cancel,
        ] {
            assert!(guard.suppresses(event, now + Duration::from_millis(1)));
        }
        assert!(!guard.suppresses(HotkeyEvent::EscapeCancel, now + Duration::from_millis(1)));
        assert!(!guard.suppresses(HotkeyEvent::CopyLast, now + Duration::from_millis(1)));
    }

    #[test]
    fn handsfree_stop_guard_expires_after_the_double_tap_window() {
        let now = Instant::now();
        let mut guard = HandsfreeStopGuard::default();
        guard.arm(now);

        assert!(!guard.suppresses(
            HotkeyEvent::Press,
            now + HANDSFREE_STOP_GUARD + Duration::from_millis(1),
        ));
        assert!(!guard.suppresses(
            HotkeyEvent::Press,
            now + HANDSFREE_STOP_GUARD + Duration::from_millis(2),
        ));
    }
}
