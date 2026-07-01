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

    let (hotkey_tx, mut hotkey_rx) = tokio::sync::mpsc::channel::<HotkeyEvent>(8);
    let tx_press = hotkey_tx.clone();
    let tx_handless = hotkey_tx.clone();
    let tx_cancel = hotkey_tx.clone();
    let tx_escape = hotkey_tx.clone();
    let tx_release = hotkey_tx;

    match crate::core::hotkey::start(
        move || {
            let _ = tx_press.try_send(HotkeyEvent::Press);
        },
        move || {
            let _ = tx_release.try_send(HotkeyEvent::Release);
        },
        move || {
            let _ = tx_handless.try_send(HotkeyEvent::HandlessToggle);
        },
        move || {
            let _ = tx_cancel.try_send(HotkeyEvent::Cancel);
        },
        move || {
            let _ = tx_escape.try_send(HotkeyEvent::EscapeCancel);
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
                    let (already, is_handless) = {
                        let Some(st) = lock_app_state(&state_hk) else {
                            continue;
                        };
                        (st.session.is_some(), st.handless)
                    };
                    if !already && !is_handless {
                        // Capture the target window before recording starts so
                        // inject_text can restore focus to it after the pipeline,
                        // even if the user switched windows during transcription.
                        let target = WindowTarget::capture_foreground();
                        if let Some(mut st) = lock_app_state(&state_hk) {
                            st.target = target;
                            st.pill_placement_stale = true;
                        }
                        start_recording_session(&app_hk, &state_hk, "recording", false);
                    }
                }

                HotkeyEvent::Release => {
                    if let Some(mut st) = lock_app_state(&state_hk) {
                        st.handless = false;
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
                        (st.handless, st.session.is_some())
                    };
                    if is_handless {
                        crate::core::hotkey::set_handless_active(false);
                        if let Some(mut st) = lock_app_state(&state_hk) {
                            st.handless = false;
                        }
                        if has_session {
                            tauri::async_runtime::spawn(pipeline::run_pipeline(
                                app_hk.clone(),
                                state_hk.clone(),
                            ));
                        } else {
                            crate::system::media_control::end_dictation_media_pause();
                        }
                    } else if !has_session {
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
                    let Some(is_handless) = lock_app_state(&state_hk).map(|st| st.handless) else {
                        continue;
                    };
                    if is_handless {
                        // Quick tap while in handsfree = stop. Clear chord state
                        // immediately so the still-open double-tap window can't
                        // re-trigger a fresh handsfree session.
                        crate::core::hotkey::set_handless_active(false);
                        crate::core::hotkey::reset_chord_state();
                        let has_session = if let Some(mut st) = lock_app_state(&state_hk) {
                            st.handless = false;
                            st.session.is_some()
                        } else {
                            false
                        };
                        if has_session {
                            tauri::async_runtime::spawn(pipeline::run_pipeline(
                                app_hk.clone(),
                                state_hk.clone(),
                            ));
                        } else {
                            crate::system::media_control::end_dictation_media_pause();
                        }
                    } else {
                        // First click of a double-tap gesture outside handsfree:
                        // discard the short recording that just started.
                        let (session, exclusive_mic_session_id) = lock_app_state(&state_hk)
                            .map(|mut st| (st.session.take(), st.exclusive_mic_session_id.take()))
                            .unwrap_or((None, None));
                        if let Some(session) = session {
                            tauri::async_runtime::spawn_blocking(move || {
                                let _ = session.stop();
                                if let Some(session_id) = exclusive_mic_session_id {
                                    crate::system::volume::release_mic(session_id);
                                }
                                crate::media::sound::coordinated_unmute();
                                crate::system::media_control::end_dictation_media_pause();
                            });
                        } else if let Some(session_id) = exclusive_mic_session_id {
                            tauri::async_runtime::spawn_blocking(move || {
                                crate::system::volume::release_mic(session_id);
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
                    let session = {
                        let Some(mut st) = lock_app_state(&state_hk) else {
                            continue;
                        };
                        st.handless = false;
                        let session = st.session.take();
                        let exclusive_mic_session_id = st.exclusive_mic_session_id.take();
                        (session, exclusive_mic_session_id)
                    };
                    let (session, exclusive_mic_session_id) = session;
                    let had_recording = session.is_some() || exclusive_mic_session_id.is_some();
                    if let Some(s) = session {
                        tauri::async_runtime::spawn_blocking(move || {
                            let _ = s.stop();
                            if let Some(session_id) = exclusive_mic_session_id {
                                crate::system::volume::release_mic(session_id);
                            }
                            crate::media::sound::coordinated_unmute();
                            crate::system::media_control::end_dictation_media_pause();
                        });
                    } else if let Some(session_id) = exclusive_mic_session_id {
                        tauri::async_runtime::spawn_blocking(move || {
                            crate::system::volume::release_mic(session_id);
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
