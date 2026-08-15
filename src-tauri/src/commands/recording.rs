//! Microphone + recording/calibration session control commands.

use super::*;
use crate::core::window_geometry::{capture_webview_center, WindowTarget};

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
    pipeline::reserve_starting(state.inner())?;

    let target = capture_in_app_target(&app);
    {
        let mut st = match lock_state(&state) {
            Ok(st) => st,
            Err(err) => {
                pipeline::release_starting_reservation(state.inner());
                return Err(err);
            }
        };
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

    let start_result = match start_result {
        Ok(result) => result,
        Err(e) => {
            pipeline::release_starting_reservation(state.inner());
            return Err(format!("Recording task panicked: {e}"));
        }
    };

    match start_result {
        Ok(()) => Ok(()),
        Err(e) => {
            // The returned error is the only surface the caller shows (inline
            // mic-button message / setup TryIt step). Emitting verenu:error
            // here too would show the same failure twice — a toast on top of
            // the inline message. The hotkey path that has no caller-facing
            // surface emits its own event in pipeline::session instead.
            let msg = format!(
                "Failed to start recording: {}",
                crate::api::user_facing_message(&e)
            );
            crate::pipeline::hide_pill(&app);
            Err(msg)
        }
    }
}

#[tauri::command]
pub async fn start_setup_try_recording(
    app: AppHandle,
    state: tauri::State<'_, SharedState>,
) -> Result<(), String> {
    pipeline::reserve_starting(state.inner())?;

    let target = capture_in_app_target(&app);
    {
        let mut st = match lock_state(&state) {
            Ok(st) => st,
            Err(err) => {
                pipeline::release_starting_reservation(state.inner());
                return Err(err);
            }
        };
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

    let start_result = match start_result {
        Ok(result) => result,
        Err(e) => {
            pipeline::release_starting_reservation(state.inner());
            return Err(format!("Recording task panicked: {e}"));
        }
    };

    match start_result {
        Ok(()) => Ok(()),
        Err(e) => {
            // Single-surface rule: the caller (setup TryIt step) shows the
            // returned error inline; no duplicate verenu:error toast here.
            let msg = format!(
                "Failed to start recording: {}",
                crate::api::user_facing_message(&e)
            );
            crate::pipeline::hide_pill(&app);
            Err(msg)
        }
    }
}

#[tauri::command]
pub async fn stop_setup_try_recording(
    app: AppHandle,
    state: tauri::State<'_, SharedState>,
) -> Result<(), String> {
    if pipeline::cancel_starting_reservation(state.inner()) {
        pipeline::hide_pill(&app);
        return Ok(());
    }
    crate::core::hotkey::set_handless_active(false);
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
    pipeline::reserve_starting(state.inner())?;

    let target = capture_in_app_target(&app);
    {
        let mut st = match lock_state(&state) {
            Ok(st) => st,
            Err(err) => {
                pipeline::release_starting_reservation(state.inner());
                return Err(err);
            }
        };
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

    let start_result = match start_result {
        Ok(result) => result,
        Err(e) => {
            pipeline::release_starting_reservation(state.inner());
            return Err(format!("Calibration task panicked: {e}"));
        }
    };
    start_result.map_err(|e| e.to_string())
}

/// What calibration learned about the capture it just took.
///
/// `contains_speech` comes from the same Silero VAD the dictation pipeline
/// uses, not from a peak-RMS threshold — a door slam or a desk bump clears an
/// RMS bar exactly like a voice does, and it also drags the computed gain down
/// with it. `null` means VAD could not run (model staging failed); the caller
/// should fall back rather than treat it as silence.
#[derive(Debug, Clone, Copy, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CalibrationResult {
    pub contains_speech: Option<bool>,
    pub speech_ms: u64,
    pub speech_ratio: f32,
    pub peak_probability: f32,
    pub longest_segment_ms: u64,
    pub duration_ms: u64,
}

#[tauri::command]
pub async fn stop_calibration_monitoring(
    app: AppHandle,
    state: tauri::State<'_, SharedState>,
) -> Result<CalibrationResult, String> {
    let mut result = CalibrationResult {
        contains_speech: None,
        speech_ms: 0,
        speech_ratio: 0.0,
        peak_probability: 0.0,
        longest_segment_ms: 0,
        duration_ms: 0,
    };

    let taken = pipeline::take_recording_plain(state.inner());
    if let Some((session, exclusive_mic_session_id)) = taken {
        // The capture used to be discarded here. Keeping it costs nothing —
        // the samples are already in memory — and gives the frontend a real
        // answer to "did they actually speak?".
        let analysis = tauri::async_runtime::spawn_blocking(move || {
            let recording = session.stop();
            if let Some(session_id) = exclusive_mic_session_id {
                crate::system::volume::release_mic(session_id);
            }
            recording.ok().map(|recording| {
                // Calibration forces gain 1.0 (see start_calibration_monitoring),
                // so VAD sees the unamplified signal and needs no leniency scaling.
                let speech = crate::media::vad::analyze_speech(&recording.samples_16k, 1.0);
                (recording.duration_ms, speech)
            })
        })
        .await;

        match analysis {
            Ok(Some((duration_ms, speech))) => {
                result.duration_ms = duration_ms;
                match speech {
                    Ok(speech) => {
                        result.contains_speech = Some(speech.contains_speech);
                        result.speech_ms = speech.speech_ms;
                        result.speech_ratio = speech.speech_ratio;
                        result.peak_probability = speech.peak_probability;
                        result.longest_segment_ms = speech.longest_segment_ms;
                        log::debug!(
                            "calibration: vad speech={} speech_ms={} ratio={:.3} peak={:.3} longest_ms={}",
                            speech.contains_speech,
                            speech.speech_ms,
                            speech.speech_ratio,
                            speech.peak_probability,
                            speech.longest_segment_ms
                        );
                    }
                    Err(e) => {
                        log::warn!("calibration: VAD unavailable, falling back to level check: {e}")
                    }
                }
            }
            Ok(None) => log::warn!("calibration: capture returned no audio"),
            Err(e) => log::error!("calibration: stop task panicked: {e}"),
        }
    }

    if let Some(manager) = app.try_state::<crate::local_stt::LocalTranscriptionManager>() {
        manager.set_recording_active(false);
    }
    Ok(result)
}

#[tauri::command]
pub async fn stop_and_transcribe_input(
    app: AppHandle,
    state: tauri::State<'_, SharedState>,
) -> Result<String, String> {
    pipeline::transcribe_input_only(app, state.inner().clone())
        .await
        // Sanitize before crossing IPC: the raw error can embed provider
        // response bodies or the AUTH_401 wire format, which must not reach
        // the inline mic-button message.
        .map_err(|e| crate::api::user_facing_error(&e))
}

#[tauri::command]
pub async fn start_repair_complaint_recording(
    app: AppHandle,
    state: tauri::State<'_, SharedState>,
) -> Result<(), String> {
    pipeline::reserve_starting(state.inner())?;
    pipeline::start_recording_session_ex(
        &app,
        state.inner(),
        "repair_recording",
        false,
        None,
        pipeline::RecordingStartOptions {
            show_recording_pill: true,
            emit_globally: false,
            start_cue_delay_ms: None,
        },
    )
}

#[tauri::command]
pub async fn stop_repair_complaint_recording(
    app: AppHandle,
    state: tauri::State<'_, SharedState>,
) -> Result<(), String> {
    crate::core::hotkey::set_handless_active(false);
    tauri::async_runtime::spawn(pipeline::finish_complaint_recording(
        app.clone(),
        state.inner().clone(),
    ));
    Ok(())
}

#[tauri::command]
pub async fn repair_positive_feedback(
    app: AppHandle,
    state: tauri::State<'_, SharedState>,
) -> Result<(), String> {
    pipeline::clear_repair(state.inner());
    pipeline::hide_pill(&app);
    Ok(())
}

#[tauri::command]
pub async fn repair_enter_input(
    app: AppHandle,
    _state: tauri::State<'_, SharedState>,
) -> Result<(), String> {
    pipeline::enter_repair_input(&app);
    Ok(())
}

#[tauri::command]
pub async fn repair_cancel(
    app: AppHandle,
    state: tauri::State<'_, SharedState>,
) -> Result<(), String> {
    // A stale pill event must never stop a newer normal dictation. A repair
    // session is cleared synchronously when a new dictation reserves its
    // lifecycle, so only discard an active microphone session while repair
    // still owns the transient state.
    let owns_repair = state
        .inner()
        .lock()
        .map(|locked| locked.repair.is_some())
        .unwrap_or(false);
    if owns_repair {
        pipeline::clear_repair(state.inner());
        pipeline::discard_recording(&app, state.inner()).await;
    }
    Ok(())
}

#[tauri::command]
pub async fn repair_analyze(
    app: AppHandle,
    state: tauri::State<'_, SharedState>,
    complaint: String,
) -> Result<(), String> {
    pipeline::diagnose_repair(app.clone(), state.inner().clone(), complaint)
        .await
        .map_err(|error| {
            let message = crate::api::user_facing_error(&error);
            pipeline::emit_repair_error(&app, &message);
            message
        })
}

#[tauri::command]
pub async fn repair_apply(
    app: AppHandle,
    state: tauri::State<'_, SharedState>,
    proposal_id: u64,
) -> Result<String, String> {
    pipeline::apply_repair(app.clone(), state.inner().clone(), proposal_id)
        .await
        .map_err(|error| {
            let message = crate::api::user_facing_error(&error);
            pipeline::emit_repair_error(&app, &message);
            message
        })
}

#[tauri::command]
pub async fn start_repair_complaint_recording(
    app: AppHandle,
    state: tauri::State<'_, SharedState>,
) -> Result<(), String> {
    pipeline::reserve_starting(state.inner())?;
    pipeline::start_recording_session(&app, state.inner(), "repair_recording", false);
    Ok(())
}

#[tauri::command]
pub async fn stop_repair_complaint_recording(
    app: AppHandle,
    state: tauri::State<'_, SharedState>,
) -> Result<(), String> {
    tauri::async_runtime::spawn(pipeline::finish_complaint_recording(
        app,
        state.inner().clone(),
    ));
    Ok(())
}

#[tauri::command]
pub async fn repair_positive_feedback(
    app: AppHandle,
    state: tauri::State<'_, SharedState>,
) -> Result<(), String> {
    pipeline::clear_repair(state.inner());
    pipeline::hide_pill(&app);
    Ok(())
}

#[tauri::command]
pub async fn repair_enter_input(app: AppHandle) -> Result<(), String> {
    pipeline::enter_repair_input(&app);
    Ok(())
}

#[tauri::command]
pub async fn repair_cancel(
    app: AppHandle,
    state: tauri::State<'_, SharedState>,
) -> Result<(), String> {
    pipeline::clear_repair(state.inner());
    pipeline::hide_pill(&app);
    Ok(())
}

#[tauri::command]
pub async fn repair_analyze(
    app: AppHandle,
    state: tauri::State<'_, SharedState>,
    complaint: String,
) -> Result<(), String> {
    pipeline::diagnose_repair(app.clone(), state.inner().clone(), complaint)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn repair_apply(
    app: AppHandle,
    state: tauri::State<'_, SharedState>,
    proposal_id: u64,
) -> Result<String, String> {
    pipeline::apply_repair(app, state.inner().clone(), proposal_id)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn stop_recording(
    app: AppHandle,
    state: tauri::State<'_, SharedState>,
) -> Result<(), String> {
    crate::core::hotkey::set_handless_active(false);
    let taken = pipeline::take_recording_plain(state.inner());
    if let Some(manager) = app.try_state::<crate::local_stt::LocalTranscriptionManager>() {
        manager.set_recording_active(false);
    }
    if let Some((session, exclusive_mic_session_id)) = taken {
        let app_for_cancel = app.clone();
        let state_for_cancel = state.inner().clone();
        let handle = tauri::async_runtime::spawn(async move {
            pipeline::cancel_recording_with_resume(
                &app_for_cancel,
                &state_for_cancel,
                session,
                exclusive_mic_session_id,
            )
            .await;
        });
        // The join handle is deliberately not awaited inline (stop_recording
        // returns immediately), but a panicked/aborted task would otherwise
        // be swallowed silently — log it so a broken cancel path is visible.
        tauri::async_runtime::spawn(async move {
            if handle.await.is_err() {
                log::error!("cancel_recording_with_resume task panicked or was aborted");
            }
        });
    } else {
        crate::media::sound::coordinated_unmute();
        crate::system::media_control::end_dictation_media_pause();
        pipeline::hide_pill(&app);
    }
    Ok(())
}

#[tauri::command]
pub async fn stop_handless_mode(
    app: AppHandle,
    state: tauri::State<'_, SharedState>,
) -> Result<(), String> {
    crate::core::hotkey::set_handless_active(false);
    crate::core::hotkey::reset_chord_state();
    let has_session = lock_state(&state)?.lifecycle.is_recording();
    if has_session {
        tauri::async_runtime::spawn(pipeline::run_pipeline(app, state.inner().clone()));
    } else {
        pipeline::cancel_starting_reservation(state.inner());
        crate::system::media_control::end_dictation_media_pause();
    }
    Ok(())
}

/// Resumes a cancelled recording's audio: starts a fresh hands-free
/// recording that will be prepended with the previously-captured audio when
/// it stops, so the finished dictation reads as one continuous take. Errs if
/// the capture already expired or was already resumed/dismissed.
#[tauri::command]
pub async fn resume_cancelled_capture(
    app: AppHandle,
    state: tauri::State<'_, SharedState>,
) -> Result<(), String> {
    // Reserve first so a busy state fails before anything is consumed. The
    // stashed capture is only cleared once the new session is actually
    // underway — a mid-start failure (mic permissions, poisoned lock) must
    // leave it resumable rather than destroying the audio.
    pipeline::reserve_starting(state.inner())?;
    let Some(audio) = pipeline::peek_cancelled_capture_if_fresh(state.inner()) else {
        pipeline::cancel_starting_reservation(state.inner());
        return Err("Nothing to resume".to_string());
    };
    pipeline::set_starting_prepend_audio(state.inner(), audio);
    let target = WindowTarget::capture_foreground();
    {
        let mut st = match lock_state(&state) {
            Ok(st) => st,
            Err(err) => {
                pipeline::cancel_starting_reservation(state.inner());
                return Err(err);
            }
        };
        st.target = target;
        st.pill_placement_stale = true;
    }
    pipeline::start_recording_session(&app, state.inner(), "handsfree", true);
    crate::core::hotkey::set_handless_active(true);
    pipeline::clear_cancelled_capture(state.inner());
    pipeline::emit_cancelled_capture_cleared(&app);
    Ok(())
}

/// Discards a cancelled recording's stashed audio without resuming it —
/// called when the user explicitly dismisses the offer, either the pill's
/// own dismiss (X) button or Home's dismiss button. The pill toast's plain
/// 10s auto-hide does *not* call this: the capture stays resumable from Home
/// for the full `CANCEL_RESUME_WINDOW` regardless of whether the toast is
/// still on screen.
#[tauri::command]
pub async fn dismiss_cancelled_capture(
    app: AppHandle,
    state: tauri::State<'_, SharedState>,
) -> Result<(), String> {
    if pipeline::take_cancelled_capture_if_fresh(state.inner()).is_some() {
        pipeline::emit_cancelled_capture_cleared(&app);
    }
    Ok(())
}

/// Puts a failed dictation's text back on the clipboard so the user can
/// paste it manually — called from the pill's "Paste failed" Copy button.
/// The text itself never crosses to the frontend; only this command's
/// success/failure result does.
#[tauri::command]
pub async fn copy_paste_failure_to_clipboard(
    state: tauri::State<'_, SharedState>,
) -> Result<(), String> {
    // Peek rather than take: if the clipboard write fails (transient system
    // clipboard lock/error), the text stays available so the user can retry.
    // The slot is only cleared once the copy actually succeeds.
    let Some(text) = pipeline::peek_paste_failure_if_fresh(state.inner()) else {
        return Err("Nothing to copy".to_string());
    };
    crate::core::injection::copy_to_clipboard(&text)
        .await
        .map_err(|e| e.to_string())?;
    pipeline::clear_paste_failure(state.inner());
    Ok(())
}

/// Resizes the pill window to fit its visible content. The frontend measures
/// the actual rendered content (pill capsule + profile label above it + error
/// text) and reports the size in CSS px (== logical points on the pill's
/// monitor), so the transparent click-capture zone around the pill stays
/// exactly as wide as the pill itself instead of a fixed 380px band that
/// swallows or forwards stray clicks — the floating pill grows for wide
/// content (long error messages, handsfree buttons) and shrinks back when
/// it's just the bare recording capsule. Height changes grow the window
/// upward so the pill itself stays visually pinned in place.
#[tauri::command]
pub fn set_pill_size(
    app: AppHandle,
    state: tauri::State<'_, SharedState>,
    width: f64,
    height: f64,
) -> Result<(), String> {
    let Some(pill) = app.get_webview_window("pill") else {
        return Ok(());
    };

    // Upper bounds are backstops against a bad frontend measurement, not the
    // expected size — they must clear the largest real layout, which is the
    // repair error/proposal card: its message text is provider-supplied and
    // effectively unbounded in length, so it needs far more vertical room
    // than the single-line capsule every other state uses.
    let width_points = if width.is_finite() {
        width.clamp(60.0, 440.0).round()
    } else {
        60.0
    };
    let height_points = if height.is_finite() {
        height.clamp(44.0, 420.0).round()
    } else {
        44.0
    };

    // Recompute the ideal centered placement from the monitor directly
    // rather than offsetting from the window's current position — content-fit
    // resizing fires many times per second during a width transition, and
    // deriving each new position from the previous one let small rounding/race
    // errors compound into a visible rightward drift. This recomputation is
    // idempotent: every call lands on the same correct center.
    //
    // If no monitor resolves at all, still apply the *size* (anchored on the
    // window's current center) rather than bailing. The frontend has already
    // recorded this size as sent and won't re-send it, so dropping the resize
    // here would strand the window too small for its content and clip the pill
    // until something else happened to change its size.
    let placement =
        crate::pipeline::placement_for_current_monitor(&pill, width_points, height_points)
            .unwrap_or_else(|| {
                let scale = pill.scale_factor().unwrap_or(1.0).max(0.1);
                let cur_pos = pill.outer_position().unwrap_or_default();
                let w = (width_points * scale).round() as i32;
                let h = (height_points * scale).round() as i32;
                let (cur_w, cur_h) = pill
                    .outer_size()
                    .map(|size| (size.width as f64, size.height as f64))
                    .unwrap_or((w as f64, h as f64));
                crate::pipeline::PillPlacement {
                    x: cur_pos.x + ((cur_w - w as f64) / 2.0).round() as i32,
                    y: cur_pos.y + (cur_h - h as f64).round() as i32,
                    width: w,
                    height: h,
                }
            });
    crate::pipeline::apply_pill_placement(&pill, placement);

    let mut st = lock_state(&state)?;
    st.pill_width_points = width_points;
    st.pill_height_points = height_points;
    st.pill_placement = Some(placement);
    Ok(())
}
