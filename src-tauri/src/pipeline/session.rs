use super::*;

// ---------- recording session helpers ----------

#[derive(Clone, Copy, Default)]
pub struct RecordingStartOptions {
    pub show_recording_pill: bool,
    pub emit_globally: bool,
    pub start_cue_delay_ms: Option<u64>,
}

/// Starts a new recording session, stores it in shared state, shows the pill,
/// and spawns the audio-level emitter task. The caller must already have
/// reserved `DictationLifecycle::Starting` (via `state::reserve_starting` or
/// `state::take_active_pipeline_for_interrupt`) before calling this.
pub fn start_recording_session(
    app: &AppHandle,
    state: &SharedState,
    pill_state: &str,
    handless: bool,
) {
    let start_cue_delay_ms = if start_stop_sounds_enabled(app) {
        Some(if handless {
            crate::media::sound::START_CUE_HANDSFREE_DELAY_MS
        } else {
            crate::media::sound::START_CUE_NORMAL_DELAY_MS
        })
    } else {
        None
    };

    if let Err(e) = start_recording_session_ex(
        app,
        state,
        pill_state,
        handless,
        None,
        RecordingStartOptions {
            show_recording_pill: true,
            emit_globally: false,
            start_cue_delay_ms,
        },
    ) {
        log::error!("start recording: {e}");
        hide_pill(app);
        app.emit("verenu:error", format!("Failed to start recording: {e}"))
            .ok();
    }
}

/// Generalized recording session function supporting calibration overrides.
/// The caller must already have reserved `DictationLifecycle::Starting`.
pub fn start_recording_session_ex(
    app: &AppHandle,
    state: &SharedState,
    pill_state: &str,
    handless: bool,
    gain_override: Option<f32>,
    options: RecordingStartOptions,
) -> Result<(), String> {
    let settings = store::settings_snapshot(app);
    let audio_config = match settings {
        Ok(ref settings) => store::load_audio_config(settings),
        Err(e) => {
            log::warn!(
                "Failed to load settings.json store for audio config: {:?}",
                e
            );
            store::AudioConfig::default()
        }
    };

    #[cfg(target_os = "macos")]
    {
        // `AXIsProcessTrustedWithOptions` can return a stale cached `false` for the
        // life of the process. Check Accessibility strictly rather than using
        // is_tap_active() as a proxy.
        // The CGEventTap can be active on Input Monitoring alone — so a running tap
        // does NOT prove Accessibility is granted. Without Accessibility, synthetic
        // Cmd+V (posting events to the HID tap) silently fails. Using the real TCC
        // check ensures we surface the error instead of recording and never pasting.
        if !crate::system::mac_app::is_accessibility_verified()
            && !crate::commands::check_accessibility_permission(false)
        {
            release_starting_reservation(state);
            return Err(
                "Accessibility permission is required for Verenu on macOS. Open System Settings > Privacy & Security > Accessibility and enable Verenu."
                    .to_string(),
            );
        }

        match crate::system::mac_app::microphone_permission_status() {
            "denied" | "restricted" => {
                release_starting_reservation(state);
                return Err(
                    "Microphone access is blocked on macOS. Open System Settings > Privacy & Security > Microphone and enable Verenu."
                        .to_string(),
                );
            }
            _ => {}
        }
    }

    let device = audio_config.device;
    let use_default_input_device = device.is_none();
    let noise_reduction = audio_config.noise_reduction;
    let mute_audio = audio_config.mute_audio;
    let exclusive_mic = audio_config.exclusive_mic;
    let pause_media = audio_config.pause_media_during_dictation && gain_override.is_none();
    let mic_gain = gain_override.unwrap_or(audio_config.mic_gain);
    let exclusive_mic_session_id = if cfg!(target_os = "macos")
        && exclusive_mic
        && use_default_input_device
        && gain_override.is_none()
    {
        Some(crate::system::volume::register_session())
    } else {
        None
    };

    match audio::RecordingSession::start(device, noise_reduction, mic_gain) {
        Ok(session) => {
            let level_arc = session.level.clone();
            let raw_level_arc = session.raw_level.clone();
            let active_arc = session.active.clone();
            let start_cue_active = session.active.clone();
            {
                let mut st = match lock_state(state) {
                    Ok(st) => st,
                    Err(e) => {
                        let _ = session.stop();
                        if let Some(session_id) = exclusive_mic_session_id {
                            crate::system::volume::release_mic(session_id);
                        }
                        release_starting_reservation(state);
                        return Err(e.to_string());
                    }
                };
                let prepend_audio = match &st.lifecycle {
                    DictationLifecycle::Starting { prepend_audio } => prepend_audio.clone(),
                    _ => {
                        log::warn!(
                            "start_recording_session_ex: lifecycle was not Starting when installing Recording"
                        );
                        drop(st);
                        let _ = session.stop();
                        if let Some(session_id) = exclusive_mic_session_id {
                            crate::system::volume::release_mic(session_id);
                        }
                        return Err(
                            "Recording start reservation was lost before the microphone opened"
                                .to_string(),
                        );
                    }
                };
                st.lifecycle = DictationLifecycle::Recording {
                    session,
                    exclusive_mic_session_id,
                    handless,
                    handless_from_hold: false,
                    prepend_audio,
                };
            }
            if let Some(manager) = app.try_state::<crate::local_stt::LocalTranscriptionManager>() {
                manager.set_recording_active(true);
            }
            if options.show_recording_pill {
                show_pill(app, pill_state);
            }
            spawn_level_emitter(
                app.clone(),
                level_arc,
                raw_level_arc,
                active_arc,
                options.emit_globally,
            );
            if let Some(session_id) = exclusive_mic_session_id {
                tauri::async_runtime::spawn_blocking(move || {
                    crate::system::volume::hog_mic(session_id)
                });
            }
            if pause_media {
                crate::system::media_control::begin_dictation_media_pause();
            }
            if let Some(delay_ms) = options.start_cue_delay_ms {
                if mute_audio && gain_override.is_none() {
                    crate::media::sound::play_start_delayed_then(delay_ms, move || {
                        if start_cue_active.load(Ordering::Relaxed) {
                            crate::media::sound::coordinated_mute(start_cue_active);
                        }
                    });
                } else {
                    crate::media::sound::play_start_delayed(delay_ms);
                }
            } else if mute_audio && gain_override.is_none() {
                tauri::async_runtime::spawn_blocking(crate::system::volume::mute);
            }
            Ok(())
        }
        Err(e) => {
            if let Some(session_id) = exclusive_mic_session_id {
                crate::system::volume::release_mic(session_id);
            }
            release_starting_reservation(state);
            Err(e.to_string())
        }
    }
}

/// Stops a just-cancelled recording session, and — if the captured audio is
/// long/loud enough to clear the normal recording quality gates — stashes it
/// as a `CancelledCapture` and shows the pill's "Cancelled" state (Continue
/// button resumes hands-free with this audio prepended, see
/// `state::reserve_starting_with_cancelled_capture`). Otherwise behaves like
/// a plain discard: unmute, end any media pause, hide the pill.
///
/// Mirrors `stages::stop_and_capture_audio`'s gating (same constants
/// `run_pipeline`/`transcribe_input_only` use), applied here instead of to a
/// pipeline that's actually about to transcribe.
pub async fn cancel_recording_with_resume(
    app: &AppHandle,
    state: &SharedState,
    session: audio::RecordingSession,
    exclusive_mic_session_id: Option<u64>,
) {
    crate::media::sound::coordinated_unmute();
    crate::system::media_control::end_dictation_media_pause();

    let Some((captured_audio, rms, raw_rms)) =
        stop_and_capture_audio(app, session, exclusive_mic_session_id).await
    else {
        // stop_and_capture_audio already hid the pill on failure.
        return;
    };

    let active_gain = store::settings_snapshot(app)
        .map(|s| store::load_audio_config(&s).mic_gain)
        .unwrap_or(store::DEFAULT_MIC_GAIN);
    let min_rms = recording_gate_rms(active_gain);
    let gate_rms = effective_recording_rms(rms, raw_rms, active_gain);

    if captured_audio.duration_ms < MIN_RECORDING_MS || gate_rms < min_rms {
        if start_stop_sounds_enabled(app) {
            crate::media::sound::play(crate::media::sound::SoundCue::Cancel);
        }
        hide_pill(app);
        return;
    }

    stash_cancelled_capture(app, state, captured_audio);
}

/// Stores `audio` as the resumable `cancelled_capture`, plays the cancel
/// cue, and shows the pill's "Cancelled" state. Callers are responsible for
/// having already decided the audio is worth keeping
/// (`cancel_recording_with_resume` applies the normal recording quality
/// gates before calling this; a cancel mid-processing has already cleared
/// the pipeline's own gate by the time it gets here).
pub fn stash_cancelled_capture(app: &AppHandle, state: &SharedState, audio: CapturedAudio) {
    let stashed = lock_state(state)
        .map(|mut st| {
            st.cancelled_capture = Some(CancelledCapture {
                audio,
                captured_at: std::time::Instant::now(),
            });
        })
        .is_ok();
    if !stashed {
        // Lock poisoned — the pill's Continue button would offer a resume
        // that can't work since nothing was stashed. Hide the pill instead.
        log::warn!("failed to stash cancelled capture (state lock poisoned)");
        hide_pill(app);
        return;
    }
    if start_stop_sounds_enabled(app) {
        crate::media::sound::play(crate::media::sound::SoundCue::Cancel);
    }
    emit_cancelled_capture(app);
    show_cancelled_pill(app);
}

/// Releases a `Starting` reservation back to `Idle` when the mic failed to
/// open — the carried prepend audio (if any) is simply dropped, not
/// retained anywhere a later, unrelated dictation could inherit it.
pub(crate) fn release_starting_reservation(state: &SharedState) {
    let mut st = match state.lock() {
        Ok(st) => st,
        Err(poisoned) => {
            log::warn!(
                "Recording state lock was poisoned while releasing start reservation; recovering"
            );
            poisoned.into_inner()
        }
    };
    if matches!(st.lifecycle, DictationLifecycle::Starting { .. }) {
        st.lifecycle = DictationLifecycle::Idle;
    }
}

/// Spawns a Tokio task that emits `audio-level` events to the pill every 50ms
/// until the recording's `active` flag goes false.
pub fn spawn_level_emitter(
    app: AppHandle,
    level: Arc<std::sync::atomic::AtomicU32>,
    raw_level: Arc<std::sync::atomic::AtomicU32>,
    active: Arc<std::sync::atomic::AtomicBool>,
    emit_globally: bool,
) {
    tauri::async_runtime::spawn(async move {
        // Give WebView2 a brief head start to wake up and process the
        // "recording" state event before we flood the IPC with 16ms updates.
        tokio::time::sleep(std::time::Duration::from_millis(80)).await;

        let emit_level = |level_val: f32| {
            if emit_globally {
                let _ = app.emit("audio-level", level_val);
            } else if let Some(pill) = app.get_webview_window("pill") {
                pill.emit("audio-level", level_val).ok();
            }
        };

        loop {
            if !active.load(Ordering::Relaxed) {
                break;
            }
            let level_val = f32::from_bits(level.load(Ordering::Relaxed));
            let raw_level_val = f32::from_bits(raw_level.load(Ordering::Relaxed));
            emit_level(level_val);
            if emit_globally {
                let _ = app.emit("audio-level-raw", raw_level_val);
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }

        // Emit final reset to ensure level goes to 0 regardless of timing
        emit_level(0.0);
        if emit_globally {
            let _ = app.emit("audio-level-raw", 0.0f32);
        }
    });
}
/// Whether dictation start/stop sound cues are enabled (defaults to true when
/// the settings store cannot be read).
pub(crate) fn start_stop_sounds_enabled(app: &AppHandle) -> bool {
    store::settings_snapshot(app)
        .map(|s| {
            let config = store::load_audio_config(&s);
            crate::media::sound::set_volume(config.sound_effects_volume);
            config.sound_effects_volume > 0.0
        })
        .unwrap_or(true)
}
