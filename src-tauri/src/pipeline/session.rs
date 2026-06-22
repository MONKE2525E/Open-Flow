use super::*;

// ---------- recording session helpers ----------

/// Starts a new recording session, stores it in shared state, shows the pill,
/// and spawns the audio-level emitter task.
pub fn start_recording_session(
    app: &AppHandle,
    state: &SharedState,
    pill_state: &str,
    handless: bool,
) {
    if let Err(e) = start_recording_session_ex(app, state, pill_state, handless, None, true, false)
    {
        log::error!("start recording: {e}");
        hide_pill(app);
        app.emit("verenu:error", format!("Failed to start recording: {e}"))
            .ok();
    }
}

/// Generalized recording session function supporting calibration overrides.
pub fn start_recording_session_ex(
    app: &AppHandle,
    state: &SharedState,
    pill_state: &str,
    handless: bool,
    gain_override: Option<f32>,
    show_recording_pill: bool,
    emit_globally: bool,
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
        // Check Accessibility strictly rather than using is_tap_active() as a proxy.
        // The CGEventTap can be active on Input Monitoring alone — so a running tap
        // does NOT prove Accessibility is granted. Without Accessibility, synthetic
        // Cmd+V (posting events to the HID tap) silently fails. Using the real TCC
        // check ensures we surface the error instead of recording and never pasting.
        if !crate::system::mac_app::is_accessibility_verified()
            && !crate::commands::check_accessibility_permission(false)
        {
            return Err(
                "Accessibility permission is required for Verenu on macOS. Open System Settings > Privacy & Security > Accessibility and enable Verenu."
                    .to_string(),
            );
        }

        match crate::system::mac_app::microphone_permission_status() {
            "denied" | "restricted" => {
                return Err(
                    "Microphone access is blocked on macOS. Open System Settings > Privacy & Security > Microphone and enable Verenu."
                        .to_string(),
                );
            }
            _ => {}
        }
    }

    let device = audio_config.device;
    let noise_reduction = audio_config.noise_reduction;
    let mute_audio = audio_config.mute_audio;
    let mic_gain = gain_override.unwrap_or(audio_config.mic_gain);

    match audio::RecordingSession::start(device, noise_reduction, mic_gain) {
        Ok(session) => {
            if mute_audio && gain_override.is_none() {
                std::thread::spawn(crate::system::volume::mute);
            }
            let level_arc = session.level.clone();
            let raw_level_arc = session.raw_level.clone();
            let active_arc = session.active.clone();
            {
                let mut st = match lock_state(state) {
                    Ok(st) => st,
                    Err(e) => return Err(e.to_string()),
                };
                st.session = Some(session);
                st.handless = handless;
            }
            if show_recording_pill {
                show_pill(app, pill_state);
            }
            spawn_level_emitter(
                app.clone(),
                level_arc,
                raw_level_arc,
                active_arc,
                emit_globally,
            );
            Ok(())
        }
        Err(e) => Err(e.to_string()),
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
pub(super) fn take_pipeline_session(
    state: &SharedState,
) -> Option<(audio::RecordingSession, usize)> {
    let mut st = match lock_state(state) {
        Ok(st) => st,
        Err(e) => {
            log::error!("recording state: {e}");
            return None;
        }
    };
    let session = st.session.take()?;
    Some((session, st.target_hwnd))
}
