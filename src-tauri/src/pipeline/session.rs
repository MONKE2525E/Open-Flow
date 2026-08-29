use super::*;

// ---------- recording session helpers ----------

#[derive(Clone, Copy, Default)]
pub struct RecordingStartOptions {
    pub show_recording_pill: bool,
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
        RecordingStartOptions {
            show_recording_pill: true,
            start_cue_delay_ms,
        },
    ) {
        log::error!("start recording: {e}");
        hide_pill(app);
        app.emit(
            "verenu:error",
            format!(
                "Failed to start recording: {}",
                crate::api::user_facing_message(&e)
            ),
        )
        .ok();
    }
}

/// Generalized recording session function supporting a mic-gain override.
/// The caller must already have reserved `DictationLifecycle::Starting`.
pub fn start_recording_session_ex(
    app: &AppHandle,
    state: &SharedState,
    pill_state: &str,
    handless: bool,
    options: RecordingStartOptions,
) -> Result<(), String> {
    // A new recording supersedes any retryable audio from a previous one.
    // Without this, a dictation that VAD rejected stayed attached to the retry
    // slot, so a *later* recording that failed early would offer "Try Anyway"
    // and silently transcribe the older utterance instead — the kind of bug
    // that makes an adaptive system look haunted.
    if let Ok(mut st) = lock_state(state) {
        if st.retry_capture.take().is_some() {
            log::debug!(
                "recording: cleared previous retry capture (superseded by a new recording)"
            );
        }
    }

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
        // hotkey liveness as a proxy.
        // The global hotkey is Carbon `RegisterEventHotKey`, which needs no
        // permission at all — so a working hotkey does NOT prove Accessibility
        // is granted. Without Accessibility, synthetic Cmd+V (posting events to
        // the HID tap) silently fails. Using the real TCC check ensures we
        // surface the error instead of recording and never pasting.
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
    let pause_media = audio_config.pause_media_during_dictation;
    let exclusive_mic_session_id = if cfg!(target_os = "macos")
        && exclusive_mic
        && use_default_input_device
        
    {
        Some(crate::system::volume::register_session())
    } else {
        None
    };

    match audio::RecordingSession::start(device, noise_reduction) {
        Ok(session) => {
            let level_arc = session.level.clone();
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
                // Queued before show_pill (not after): show_pill's
                // cross-monitor move animates and defers its own pill-state
                // emission until the tween lands, well after this call
                // returns — queuing here, before that reveal ever happens,
                // is what makes the profile available in time regardless of
                // which reveal path this particular call takes (see
                // PENDING_PILL_CONTEXT for the full ordering rationale).
                let target_hwnd = lock_state(state).map(|st| st.target.id).unwrap_or(0);
                emit_context_for_window(app, target_hwnd);
                show_pill(app, pill_state);
            }
            spawn_level_emitter(app.clone(), level_arc, active_arc);
            if let Some(session_id) = exclusive_mic_session_id {
                tauri::async_runtime::spawn_blocking(move || {
                    crate::system::volume::hog_mic(session_id)
                });
            }
            if pause_media {
                crate::system::media_control::begin_dictation_media_pause();
            }
            if let Some(delay_ms) = options.start_cue_delay_ms {
                if mute_audio  {
                    crate::media::sound::play_start_delayed_then(delay_ms, move || {
                        if start_cue_active.load(Ordering::Relaxed) {
                            crate::media::sound::coordinated_mute(start_cue_active);
                        }
                    });
                } else {
                    crate::media::sound::play_start_delayed(delay_ms);
                }
            } else if mute_audio  {
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
/// `state::peek_cancelled_capture_if_fresh`). Otherwise behaves like a plain
/// discard: unmute, end any media pause, hide the pill.
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

    let min_rms = recording_gate_rms();
    let gate_rms = effective_recording_rms(rms, raw_rms);

    // Structural only: a whispered fragment must stay resumable. An absolute
    // loudness bar here would discard exactly the captures the adaptive
    // detector exists to rescue.
    let _ = (gate_rms, min_rms);
    if gates::capture_defect(
        captured_audio.duration_ms,
        captured_audio.samples_16k.len(),
        captured_audio.wav.len(),
        rms,
    )
    .is_some()
    {
        log::info!("recording: cancelled — capture discarded (structurally unusable)");
        if start_stop_sounds_enabled(app) {
            crate::media::sound::play(crate::media::sound::SoundCue::Cancel);
        }
        // A new dictation may have started while we were awaiting
        // stop_and_capture_audio — don't hide that session's pill.
        if state_is_idle(state) {
            hide_pill(app);
        }
        return;
    }

    log::info!("recording: cancelled — capture stashed for resume");
    stash_cancelled_capture(app, state, captured_audio);
}

/// Stops and discards a recording that belongs to a transient flow such as
/// repair feedback. Unlike normal dictation cancellation, this never stashes
/// audio for resume and never shows the cancelled-dictation affordance.
pub async fn discard_recording(app: &AppHandle, state: &SharedState) {
    let Some((session, exclusive_mic_session_id)) = state::take_recording_plain(state) else {
        if state_is_idle(state) {
            hide_pill(app);
        }
        return;
    };
    crate::media::sound::coordinated_unmute();
    crate::system::media_control::end_dictation_media_pause();
    let _ = stop_and_capture_audio(app, session, exclusive_mic_session_id).await;
    if state_is_idle(state) {
        hide_pill(app);
    }
}

/// True when the recording lifecycle is currently `Idle`.
fn state_is_idle(state: &SharedState) -> bool {
    lock_state(state).is_ok_and(|st| st.lifecycle.is_idle())
}

/// Stores `audio` as the resumable `cancelled_capture`, plays the cancel
/// cue, and shows the pill's "Cancelled" state. Callers are responsible for
/// having already decided the audio is worth keeping
/// (`cancel_recording_with_resume` applies the normal recording quality
/// gates before calling this; a cancel mid-processing has already cleared
/// the pipeline's own gate by the time it gets here).
pub fn stash_cancelled_capture(app: &AppHandle, state: &SharedState, audio: CapturedAudio) {
    enum StashOutcome {
        Stashed,
        LockPoisoned,
        SessionActive,
    }
    let outcome = match lock_state(state) {
        Ok(mut st) => {
            // Only stash while the system is genuinely idle. Between
            // `stop_and_capture_audio` (which released the Recording
            // lifecycle) and this call the user may have already started a
            // new dictation — overwriting the capture or forcing the pill
            // into "Cancelled" would clobber that fresh session.
            if !st.lifecycle.is_idle() {
                StashOutcome::SessionActive
            } else {
                st.cancelled_capture = Some(CancelledCapture {
                    audio,
                    captured_at: std::time::Instant::now(),
                });
                StashOutcome::Stashed
            }
        }
        Err(_) => StashOutcome::LockPoisoned,
    };
    match outcome {
        StashOutcome::Stashed => {}
        StashOutcome::SessionActive => {
            // A newer dictation owns the pill now — don't touch it, and
            // don't offer a resume that would fight the live session.
            log::debug!("skipping cancelled-capture pill: a new session is active");
            return;
        }
        StashOutcome::LockPoisoned => {
            // Poisoned lock: the pill's Continue button would offer a resume
            // that can't work since nothing was stashed. Hide the pill.
            log::warn!("failed to stash cancelled capture (state lock poisoned)");
            hide_pill(app);
            return;
        }
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
///
/// Pill-only. The app-wide `audio-level` / `audio-level-raw` broadcast existed
/// solely to drive the mic-calibration meter; adaptive voice detection replaced
/// that workflow, so nothing outside the pill listens any more.
pub fn spawn_level_emitter(
    app: AppHandle,
    level: Arc<std::sync::atomic::AtomicU32>,
    active: Arc<std::sync::atomic::AtomicBool>,
) {
    tauri::async_runtime::spawn(async move {
        // Give WebView2 a brief head start to wake up and process the
        // "recording" state event before we flood the IPC with 16ms updates.
        tokio::time::sleep(std::time::Duration::from_millis(80)).await;

        let emit_level = |level_val: f32| {
            if let Some(pill) = app.get_webview_window("pill") {
                pill.emit("audio-level", level_val).ok();
            }
        };

        loop {
            if !active.load(Ordering::Relaxed) {
                break;
            }
            emit_level(f32::from_bits(level.load(Ordering::Relaxed)));
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }

        // Emit final reset to ensure level goes to 0 regardless of timing
        emit_level(0.0);
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
