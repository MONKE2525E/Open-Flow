//! Microphone + recording/calibration session control commands.

use super::*;
use crate::core::window_geometry::{WindowTarget, capture_webview_center};

fn lock_state<'a>(
    state: &'a tauri::State<'_, SharedState>,
) -> Result<std::sync::MutexGuard<'a, pipeline::AppState>, String> {
    state
        .lock()
        .map_err(|_| "Recording state lock was poisoned".to_string())
}

fn capture_in_app_target(app: &AppHandle) -> WindowTarget {
    let mut target = WindowTarget::capture_display_only();
    if let Some(window) = app.get_webview_window("main") {
        if let Some(display_point) = capture_webview_center(&window) {
            target.display_point = Some(display_point);
        }
    }
    target
}
// ---------- microphone ----------

#[tauri::command]
pub async fn get_microphones() -> Vec<String> {
    match tokio::task::spawn_blocking(audio::list_input_devices).await {
        Ok(devices) => devices,
        Err(e) => {
            log::error!("Task to get microphones panicked: {e}");
            Vec::new()
        }
    }
}

// ---------- recording control ----------

#[tauri::command]
pub async fn start_input_recording(
    app: AppHandle,
    state: tauri::State<'_, SharedState>,
) -> Result<(), String> {
    {
        let mut st = lock_state(&state)?;
        if st.session.is_some() || st.starting {
            return Err("Already recording".to_string());
        }
        st.starting = true;
    }

    let target = capture_in_app_target(&app);
    {
        let mut st = lock_state(&state)?;
        st.target = target;
        st.pill_placement_stale = true;
    }

    let state_clone = state.inner().clone();
    let app_clone = app.clone();
    let start_result = tokio::task::spawn_blocking(move || {
        pipeline::start_recording_session_ex(
            &app_clone,
            &state_clone,
            "recording",
            false,
            None,
            pipeline::RecordingStartOptions {
                show_recording_pill: true,
                emit_globally: false,
                start_cue_delay_ms: None,
            },
        )
    })
    .await;

    {
        let mut st = lock_state(&state)?;
        st.starting = false;
    }

    let start_result = start_result.map_err(|e| format!("Recording task panicked: {e}"))?;

    match start_result {
        Ok(()) => Ok(()),
        Err(e) => {
            let msg = format!("Failed to start recording: {e}");
            crate::pipeline::hide_pill(&app);
            app.emit("verenu:error", msg.clone()).ok();
            Err(msg)
        }
    }
}

#[tauri::command]
pub async fn start_setup_try_recording(
    app: AppHandle,
    state: tauri::State<'_, SharedState>,
) -> Result<(), String> {
    {
        let mut st = lock_state(&state)?;
        if st.session.is_some() || st.starting {
            return Err("Already recording".to_string());
        }
        st.starting = true;
    }

    let target = capture_in_app_target(&app);
    {
        let mut st = lock_state(&state)?;
        st.target = target;
        st.pill_placement_stale = true;
    }

    let state_clone = state.inner().clone();
    let app_clone = app.clone();
    let start_result = tokio::task::spawn_blocking(move || {
        pipeline::start_recording_session_ex(
            &app_clone,
            &state_clone,
            "recording",
            false,
            None,
            pipeline::RecordingStartOptions {
                show_recording_pill: true,
                emit_globally: false,
                start_cue_delay_ms: if pipeline::start_stop_sounds_enabled(&app_clone) {
                    Some(0)
                } else {
                    None
                },
            },
        )
    })
    .await;

    {
        let mut st = lock_state(&state)?;
        st.starting = false;
    }

    let start_result = start_result.map_err(|e| format!("Recording task panicked: {e}"))?;

    match start_result {
        Ok(()) => Ok(()),
        Err(e) => {
            let msg = format!("Failed to start recording: {e}");
            crate::pipeline::hide_pill(&app);
            app.emit("verenu:error", msg.clone()).ok();
            Err(msg)
        }
    }
}

#[tauri::command]
pub async fn stop_setup_try_recording(
    app: AppHandle,
    state: tauri::State<'_, SharedState>,
) -> Result<(), String> {
    crate::core::hotkey::set_handless_active(false);
    {
        let mut st = lock_state(&state)?;
        st.handless = false;
    }
    tauri::async_runtime::spawn(pipeline::run_pipeline_event_only(
        app,
        state.inner().clone(),
    ));
    Ok(())
}

#[tauri::command]
pub async fn start_calibration_monitoring(
    app: AppHandle,
    state: tauri::State<'_, SharedState>,
) -> Result<(), String> {
    {
        let mut st = lock_state(&state)?;
        if st.session.is_some() || st.starting {
            return Err("Already recording".to_string());
        }
        st.starting = true;
    }

    let target = capture_in_app_target(&app);
    {
        let mut st = lock_state(&state)?;
        st.target = target;
        st.pill_placement_stale = true;
    }

    let state_clone = state.inner().clone();
    let app_clone = app.clone();
    let start_result = tokio::task::spawn_blocking(move || {
        pipeline::start_recording_session_ex(
            &app_clone,
            &state_clone,
            "calibration",
            false,
            Some(1.0),
            pipeline::RecordingStartOptions {
                show_recording_pill: false,
                emit_globally: true,
                start_cue_delay_ms: None,
            },
        )
    })
    .await;

    {
        let mut st = lock_state(&state)?;
        st.starting = false;
    }

    let start_result = start_result.map_err(|e| format!("Calibration task panicked: {e}"))?;
    start_result.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn stop_calibration_monitoring(
    state: tauri::State<'_, SharedState>,
) -> Result<(), String> {
    let (session, exclusive_mic_session_id) = {
        let mut st = lock_state(&state)?;
        let session = st.session.take();
        let exclusive_mic_session_id = st.exclusive_mic_session_id.take();
        (session, exclusive_mic_session_id)
    };
    if let Some(s) = session {
        tauri::async_runtime::spawn_blocking(move || {
            let _ = s.stop();
            if let Some(session_id) = exclusive_mic_session_id {
                crate::system::volume::release_mic(session_id);
            }
        });
    } else if let Some(session_id) = exclusive_mic_session_id {
        tauri::async_runtime::spawn_blocking(move || {
            crate::system::volume::release_mic(session_id)
        });
    }
    Ok(())
}

#[tauri::command]
pub async fn stop_and_transcribe_input(
    app: AppHandle,
    state: tauri::State<'_, SharedState>,
) -> Result<String, String> {
    pipeline::transcribe_input_only(app, state.inner().clone())
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn stop_recording(
    app: AppHandle,
    state: tauri::State<'_, SharedState>,
) -> Result<(), String> {
    crate::core::hotkey::set_handless_active(false);
    let session = {
        let mut st = lock_state(&state)?;
        st.handless = false;
        let session = st.session.take();
        let exclusive_mic_session_id = st.exclusive_mic_session_id.take();
        (session, exclusive_mic_session_id)
    };
    let (session, exclusive_mic_session_id) = session;
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
    } else {
        crate::media::sound::coordinated_unmute();
        crate::system::media_control::end_dictation_media_pause();
    }
    pipeline::hide_pill(&app);
    Ok(())
}

#[tauri::command]
pub async fn stop_handless_mode(
    app: AppHandle,
    state: tauri::State<'_, SharedState>,
) -> Result<(), String> {
    crate::core::hotkey::set_handless_active(false);
    crate::core::hotkey::reset_chord_state();
    let has_session = {
        let mut st = lock_state(&state)?;
        st.handless = false;
        st.session.is_some()
    };
    if has_session {
        tauri::async_runtime::spawn(pipeline::run_pipeline(app, state.inner().clone()));
    } else {
        crate::system::media_control::end_dictation_media_pause();
    }
    Ok(())
}
