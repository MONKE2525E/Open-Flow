use super::*;
use super::pill::{show_cancelled_pill, show_interrupted_pill};
use chrono::{SecondsFormat, Utc};

// ---------- recording session helpers ----------

#[derive(Clone, Copy, Default)]
pub struct RecordingStartOptions {
    pub show_recording_pill: bool,
    pub emit_globally: bool,
    pub start_cue_delay_ms: Option<u64>,
    /// Crash-recovery spool. Only hold-to-talk and hands-free dictation.
    pub durable: bool,
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
            durable: pill_state == "recording" || pill_state == "handsfree",
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

    // Keep the state lock off the disk path: collect durable session ids under
    // the lock, then open the live spool after dropping it.
    let (durable_id, prepend_for_lifecycle) = {
        let mut st = match lock_state(state) {
            Ok(st) => st,
            Err(e) => {
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
                if let Some(session_id) = exclusive_mic_session_id {
                    crate::system::volume::release_mic(session_id);
                }
                release_starting_reservation(state);
                return Err(
                    "Recording start reservation was lost before the microphone opened".to_string(),
                );
            }
        };
        let durable_id = if options.durable {
            let id = if st.failover_reuse_id {
                st.failover_reuse_id = false;
                st.failover_session_id
                    .clone()
                    .unwrap_or_else(super::failover::new_session_id)
            } else {
                super::failover::new_session_id()
            };
            st.failover_session_id = Some(id.clone());
            if st.failover_started_at_unix == 0 {
                st.failover_started_at_unix = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs() as i64)
                    .unwrap_or(0);
            }
            Some(id)
        } else {
            st.failover_session_id = None;
            st.failover_reuse_id = false;
            st.failover_started_at_unix = 0;
            None
        };
        (durable_id, prepend_audio)
    };
    let durable_sink = durable_id.and_then(|id| {
        super::failover::open_live_writer(
            id,
            prepend_for_lifecycle
                .as_ref()
                .map(|a| a.samples_16k.as_slice()),
            app,
            state,
        )
    });

    match audio::RecordingSession::start(device, noise_reduction, mic_gain, durable_sink) {
        Ok(session) => {
            let level_arc = session.level.clone();
            let raw_level_arc = session.raw_level.clone();
            let envelope_arc = session.envelope.clone();
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
                        if options.durable {
                            super::failover::abandon_live();
                        }
                        release_starting_reservation(state);
                        return Err(e.to_string());
                    }
                };
                if !matches!(&st.lifecycle, DictationLifecycle::Starting { .. }) {
                    log::warn!(
                        "start_recording_session_ex: lifecycle was not Starting when installing Recording"
                    );
                    st.failover_session_id = None;
                    st.failover_reuse_id = false;
                    st.failover_started_at_unix = 0;
                    drop(st);
                    let _ = session.stop();
                    if let Some(session_id) = exclusive_mic_session_id {
                        crate::system::volume::release_mic(session_id);
                    }
                    if options.durable {
                        super::failover::abandon_live();
                    }
                    return Err(
                        "Recording start reservation was lost before the microphone opened"
                            .to_string(),
                    );
                }
                st.lifecycle = DictationLifecycle::Recording {
                    session,
                    exclusive_mic_session_id,
                    handless,
                    handless_from_hold: false,
                    prepend_audio: prepend_for_lifecycle.clone(),
                };
            }
            if options.durable
                && prepend_for_lifecycle
                    .as_ref()
                    .is_some_and(|audio| audio.duration_ms >= MIN_RECORDING_MS)
            {
                super::failover::retire_committed();
                let emit_cleared = if let Ok(mut st) = lock_state(state) {
                    let current_id = st.failover_session_id.clone();
                    let stale = st.cancelled_capture.as_ref().is_some_and(|c| {
                        current_id.as_ref().is_some_and(|id| &c.id != id)
                    });
                    if stale {
                        st.cancelled_capture = None;
                        true
                    } else {
                        false
                    }
                } else {
                    false
                };
                if emit_cleared {
                    emit_cancelled_capture_cleared(app);
                }
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
            spawn_level_emitter(
                app.clone(),
                level_arc,
                raw_level_arc,
                envelope_arc,
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
            if options.durable {
                super::failover::abandon_live();
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

    let active_gain = store::settings_snapshot(app)
        .map(|s| store::load_audio_config(&s).mic_gain)
        .unwrap_or(store::DEFAULT_MIC_GAIN);
    let min_rms = recording_gate_rms(active_gain);
    let gate_rms = effective_recording_rms(rms, raw_rms, active_gain);

    if captured_audio.duration_ms < MIN_RECORDING_MS || gate_rms < min_rms {
        log::info!("recording: cancelled — capture discarded (duration/quiet gate)");
        super::failover::abandon_live();
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
    stash_cancelled_capture(app, state, captured_audio, CaptureOrigin::UserCancelled);
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
pub fn stash_cancelled_capture(
    app: &AppHandle,
    state: &SharedState,
    audio: CapturedAudio,
    origin: CaptureOrigin,
) {
    enum StashOutcome {
        Stashed(CancelledCapture),
        LockPoisoned,
        SessionActive,
    }
    let outcome = match lock_state(state) {
        Ok(st) => {
            // Only stash while the system is genuinely idle. Between
            // `stop_and_capture_audio` (which released the Recording
            // lifecycle) and this call the user may have already started a
            // new dictation — overwriting the capture or forcing the pill
            // into "Cancelled" would clobber that fresh session.
            if !st.lifecycle.is_idle() {
                StashOutcome::SessionActive
            } else {
                let id = st
                    .failover_session_id
                    .clone()
                    .unwrap_or_else(super::failover::new_session_id);
                let started_at_unix = if st.failover_started_at_unix != 0 {
                    st.failover_started_at_unix
                } else {
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_secs() as i64)
                        .unwrap_or(0)
                };
                let kind = match origin {
                    CaptureOrigin::UserCancelled => super::failover::FailoverKind::Cancelled,
                    CaptureOrigin::Interrupted => super::failover::FailoverKind::Recording,
                };
                // Drop the state lock before disk I/O so hotkey/UI paths are
                // not stalled on fsync.
                drop(st);
                super::failover::commit_capture(&audio, &id, kind, started_at_unix);
                let capture = CancelledCapture {
                    audio,
                    captured_at: std::time::Instant::now(),
                    id,
                    origin,
                    created_at_rfc3339: Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true),
                    started_at_unix,
                };
                match lock_state(state) {
                    Ok(mut st) => {
                        if !st.lifecycle.is_idle() {
                            StashOutcome::SessionActive
                        } else {
                            st.cancelled_capture = Some(capture.clone());
                            st.failover_session_id = None;
                            st.failover_reuse_id = false;
                            st.failover_started_at_unix = 0;
                            StashOutcome::Stashed(capture)
                        }
                    }
                    Err(_) => StashOutcome::LockPoisoned,
                }
            }
        }
        Err(_) => StashOutcome::LockPoisoned,
    };
    match outcome {
        StashOutcome::Stashed(capture) => {
            if start_stop_sounds_enabled(app)
                && matches!(capture.origin, CaptureOrigin::UserCancelled)
            {
                crate::media::sound::play(crate::media::sound::SoundCue::Cancel);
            }
            emit_cancelled_capture(app, &capture);
            match capture.origin {
                CaptureOrigin::UserCancelled => show_cancelled_pill(app),
                CaptureOrigin::Interrupted => show_interrupted_pill(app),
            }
        }
        StashOutcome::SessionActive => {
            // A newer dictation owns the pill now — don't touch it, and
            // don't offer a resume that would fight the live session.
            log::debug!("skipping cancelled-capture pill: a new session is active");
        }
        StashOutcome::LockPoisoned => {
            // Poisoned lock: the pill's Continue button would offer a resume
            // that can't work since nothing was stashed. Hide the pill.
            log::warn!("failed to stash cancelled capture (state lock poisoned)");
            hide_pill(app);
        }
    }
}

/// Releases a `Starting` reservation back to `Idle` when the mic failed to
/// open — the carried prepend audio (if any) is simply dropped, not
/// retained anywhere a later, unrelated dictation could inherit it.
///
/// Also clears in-memory failover fields so a failed durable start cannot
/// leave a stale `failover_started_at_unix` that later sessions inherit.
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
    st.failover_session_id = None;
    st.failover_reuse_id = false;
    st.failover_started_at_unix = 0;
}

/// Spawns a Tokio task that emits `audio-level` events to the pill every 50ms
/// until the recording's `active` flag goes false.
///
/// Also drains the short-window envelope (see `EnvelopeTap` in media/audio.rs)
/// and ships it as `audio-envelope`. That payload is what lets the pill show
/// audio *flowing* rather than a level meter: one RMS scalar per tick averages
/// away everything inside its own window, so a sustained vowel is a constant
/// number no motion model can animate. Each batch is a handful of f32 peaks at
/// a fixed ENVELOPE_WINDOW_MS cadence, ~100/sec -- small enough to send
/// alongside the existing level without being PCM streaming.
pub fn spawn_level_emitter(
    app: AppHandle,
    level: Arc<std::sync::atomic::AtomicU32>,
    raw_level: Arc<std::sync::atomic::AtomicU32>,
    envelope: Arc<crate::media::audio::EnvelopeTap>,
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

        // The pill is the only consumer of the envelope, so this never goes out
        // globally even when the level does -- no other window needs 100
        // floats a second.
        let emit_envelope = |batch: Vec<f32>| {
            if batch.is_empty() {
                return;
            }
            if let Some(pill) = app.get_webview_window("pill") {
                pill.emit("audio-envelope", batch).ok();
            }
        };

        loop {
            if !active.load(Ordering::Relaxed) {
                break;
            }
            let level_val = f32::from_bits(level.load(Ordering::Relaxed));
            let raw_level_val = f32::from_bits(raw_level.load(Ordering::Relaxed));
            emit_level(level_val);
            emit_envelope(envelope.drain());
            if emit_globally {
                let _ = app.emit("audio-level-raw", raw_level_val);
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }

        // Emit final reset to ensure level goes to 0 regardless of timing
        emit_envelope(envelope.drain());
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
