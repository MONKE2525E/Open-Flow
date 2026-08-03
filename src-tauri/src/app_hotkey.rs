use std::sync::MutexGuard;

use crate::core::window_geometry::WindowTarget;
use crate::pipeline::{self, hide_pill, start_recording_session, AppState, SharedState};
use tauri::Emitter;

fn lock_app_state(state: &SharedState) -> Option<MutexGuard<'_, AppState>> {
    match state.lock() {
        Ok(guard) => Some(guard),
        Err(_) => {
            log::error!("Recording state lock was poisoned");
            None
        }
    }
}

pub(crate) fn setup_hotkey(app: &mut tauri::App, shared: SharedState) {
    // The WH_KEYBOARD_LL hook callback must return within Windows' hook timeout
    // (~300ms) or the hook is silently removed. All real work happens in a Tokio
    // task below; callbacks only send a lightweight channel message.
    enum HotkeyEvent {
        Press,
        Release,
        HandlessToggle,
        Cancel,
        EscapeCancel,
    }

    let (hotkey_tx, mut hotkey_rx) = tokio::sync::mpsc::unbounded_channel::<HotkeyEvent>();
    let tx_press = hotkey_tx.clone();
    let tx_handless = hotkey_tx.clone();
    let tx_cancel = hotkey_tx.clone();
    let tx_escape = hotkey_tx.clone();
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
        while let Some(event) = hotkey_rx.recv().await {
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
                        crate::core::hotkey::set_handless_active(false);
                        crate::core::hotkey::reset_chord_state();
                        tauri::async_runtime::spawn(pipeline::run_pipeline(
                            app_hk.clone(),
                            state_hk.clone(),
                        ));
                    } else {
                        // First click of a double-tap gesture outside handsfree:
                        // discard the short recording that just started.
                        let taken = pipeline::take_recording_plain(&state_hk);
                        if let Some((session, exclusive_mic_session_id)) = taken {
                            tauri::async_runtime::spawn_blocking(move || {
                                let _ = session.stop();
                                if let Some(session_id) = exclusive_mic_session_id {
                                    crate::system::volume::release_mic(session_id);
                                }
                                crate::media::sound::coordinated_unmute();
                                crate::system::media_control::end_dictation_media_pause();
                            });
                        }
                        hide_pill(&app_hk);
                    }
                }

                HotkeyEvent::EscapeCancel => {
                    crate::core::hotkey::set_handless_active(false);
                    // If escape lands before the start cue fired, suppress it.
                    crate::media::sound::cancel_pending_start();

                    // Cancel in-flight processing outright — no append, no
                    // insertion. Distinct from the pre-Release recording-cancel
                    // path below (a dictation can be in at most one of the two).
                    if let Some(active) = pipeline::take_active_pipeline_for_escape(&state_hk) {
                        let _ = active.cancel_tx.send(true);
                        hide_pill(&app_hk);
                        continue;
                    }

                    let taken = pipeline::take_recording_plain(&state_hk);
                    let had_recording = taken.is_some();
                    if let Some((session, exclusive_mic_session_id)) = taken {
                        tauri::async_runtime::spawn_blocking(move || {
                            let _ = session.stop();
                            if let Some(session_id) = exclusive_mic_session_id {
                                crate::system::volume::release_mic(session_id);
                            }
                            crate::media::sound::coordinated_unmute();
                            crate::system::media_control::end_dictation_media_pause();
                        });
                    }
                    if had_recording && pipeline::start_stop_sounds_enabled(&app_hk) {
                        crate::media::sound::play(crate::media::sound::SoundCue::Cancel);
                    }
                    hide_pill(&app_hk);
                }
            }
        }
    });
}
