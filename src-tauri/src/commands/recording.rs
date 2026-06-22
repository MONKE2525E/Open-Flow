//! Microphone + recording/calibration session control commands.

use super::*;

fn lock_state<'a>(
    state: &'a tauri::State<'_, SharedState>,
) -> Result<std::sync::MutexGuard<'a, pipeline::AppState>, String> {
    state
        .lock()
        .map_err(|_| "Recording state lock was poisoned".to_string())
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

    let state_clone = state.inner().clone();
    let app_clone = app.clone();
    let start_result = tokio::task::spawn_blocking(move || {
        pipeline::start_recording_session_ex(
            &app_clone,
            &state_clone,
            "recording",
            false,
            None,
            true,
            false,
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
        st.target_hwnd = 0;
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
            true,
            false,
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

    let state_clone = state.inner().clone();
    let app_clone = app.clone();
    let start_result = tokio::task::spawn_blocking(move || {
        pipeline::start_recording_session_ex(
            &app_clone,
            &state_clone,
            "calibration",
            false,
            Some(1.0),
            false,
            true,
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
    let session = {
        let mut st = lock_state(&state)?;
        st.session.take()
    };
    if let Some(s) = session {
        tauri::async_runtime::spawn_blocking(move || {
            let _ = s.stop();
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
        st.session.take()
    };
    if let Some(s) = session {
        let _ = s.stop();
        std::thread::spawn(crate::system::volume::unmute);
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
    lock_state(&state)?.handless = false;
    tauri::async_runtime::spawn(pipeline::run_pipeline(app, state.inner().clone()));
    Ok(())
}
