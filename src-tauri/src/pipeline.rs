use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex, MutexGuard};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_store::StoreExt;

use crate::api::{auto_learn, cleanup, transcription};
use crate::core::{injection, window_context};
use crate::data::{db, dictionary, snippets, store};
use crate::media::audio;
use crate::system::apps::AppMapping;
use crate::system::number_parser;
use crate::system::text::is_number_word_token;
use crate::DbHandle;
use chrono::{DateTime, Duration, NaiveDateTime, SecondsFormat, Utc};

const MIN_RECORDING_MS: u64 = 700;
const MIN_RECORDING_RMS: f32 = 0.008;
const RETRY_WINDOW: std::time::Duration = std::time::Duration::from_secs(600);
const PILL_WIDTH_POINTS: f64 = 140.0;
const PILL_HEIGHT_POINTS: f64 = 44.0;
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
const PILL_BOTTOM_GAP_POINTS: f64 = 16.0;

fn transcription_provider_from_str(s: &str) -> transcription::Provider {
    match s {
        "openai" => transcription::Provider::OpenAI,
        "google" => transcription::Provider::Google,
        _ => transcription::Provider::Groq,
    }
}

fn cleanup_provider_from_str(s: &str) -> cleanup::CleanupProvider {
    match s {
        "openai" => cleanup::CleanupProvider::OpenAI,
        "google" => cleanup::CleanupProvider::Google,
        _ => cleanup::CleanupProvider::Groq,
    }
}

// ---------- shared state ----------

pub struct AppState {
    pub session: Option<audio::RecordingSession>,
    pub starting: bool,
    pub handless: bool,
    pub target_hwnd: usize,
    pub retry_capture: Option<RetryCapture>,
}

pub type SharedState = Arc<Mutex<AppState>>;

#[derive(Clone)]
pub struct RetryCapture {
    pub wav: bytes::Bytes,
    pub captured_at: std::time::Instant,
    pub duration_ms: u64,
    pub target_hwnd: usize,
    pub process_name: String,
    pub profile: String,
    pub app_context: Option<String>,
}

fn lock_state(state: &SharedState) -> anyhow::Result<MutexGuard<'_, AppState>> {
    state
        .lock()
        .map_err(|_| anyhow::anyhow!("Recording state lock was poisoned"))
}

fn emit_pipeline_failed(app: &AppHandle) {
    app.emit(
        "verenu:pipeline-failed",
        Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true),
    )
    .ok();
}

/// Returns true if our own process currently owns the foreground window.
/// Catches the case where the user opened the Verenu main window while
/// transcribing — if we tried to Ctrl+V / Cmd+V in that state the paste would
/// land in our own WebView and silently disappear.
fn foreground_is_own_process() -> bool {
    #[cfg(windows)]
    unsafe {
        use windows::Win32::UI::WindowsAndMessaging::{
            GetForegroundWindow, GetWindowThreadProcessId,
        };
        let hwnd = GetForegroundWindow();
        if hwnd.0.is_null() {
            return false;
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        pid == std::process::id()
    }
    #[cfg(not(windows))]
    {
        false
    }
}

/// Returns true if `hwnd` belongs to our own process.
/// Catches the case where recording was started while Verenu itself had focus.
#[cfg_attr(not(windows), allow(unused_variables))]
fn hwnd_is_own_process(hwnd: usize) -> bool {
    if hwnd == 0 {
        return false;
    }
    #[cfg(windows)]
    unsafe {
        use windows::Win32::Foundation::HWND;
        use windows::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId;
        let mut pid = 0u32;
        GetWindowThreadProcessId(HWND(hwnd as *mut _), Some(&mut pid));
        pid == std::process::id()
    }
    #[cfg(not(windows))]
    {
        false
    }
}

// ---------- pill helpers ----------

fn create_pill_if_needed(app: &AppHandle) {
    if app.get_webview_window("pill").is_some() {
        return;
    }
    let _ =
        tauri::WebviewWindowBuilder::new(app, "pill", tauri::WebviewUrl::App("/pill.html".into()))
            .title("")
            .inner_size(140.0, 44.0)
            .decorations(false)
            .transparent(true)
            .always_on_top(true)
            .skip_taskbar(true)
            .visible(false)
            .resizable(false)
            .shadow(false)
            .focused(false)
            .build();
}

pub fn show_pill(app: &AppHandle, state: &str) {
    create_pill_if_needed(app);
    if let Some(pill) = app.get_webview_window("pill") {
        // Click-through for passive states so nothing behind the pill is blocked.
        // Handsfree needs real cursor events for its cancel/confirm buttons.
        pill.set_ignore_cursor_events(state != "handsfree").ok();

        // Show the window before emitting state so WebView2 is active when it
        // receives the event. WebView2 suspends event processing while hidden;
        // emitting into a suspended view causes the first state to be dropped or
        // overtaken by the next emit (e.g. "recording" lost, only "processing" seen).
        // SW_SHOWNOACTIVATE: appears without stealing keyboard focus from
        // whatever window the user is dictating into.
        #[cfg(target_os = "windows")]
        {
            use windows::Win32::UI::WindowsAndMessaging::{ShowWindow, SW_SHOWNOACTIVATE};
            if let Ok(hwnd) = pill.hwnd() {
                unsafe {
                    let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
                }
            }
        }
        #[cfg(not(target_os = "windows"))]
        pill.show().ok();

        // macOS: `show()` (orderFront:) is ignored for a background app, so the
        // pill only appeared when Verenu was frontmost. Force it above the
        // active app's windows without stealing focus. AppKit window calls must
        // run on the main thread — show_pill is invoked from pipeline worker
        // threads, so dispatch there (a raw msg_send off-thread raises an ObjC
        // exception and aborts the process).
        #[cfg(target_os = "macos")]
        {
            let pill_for_main = pill.clone();
            let _ = app.run_on_main_thread(move || {
                if let Ok(ns_window) = pill_for_main.ns_window() {
                    crate::system::mac_app::float_pill_window(ns_window);
                }
            });
        }

        if let Ok(Some(m)) = pill.primary_monitor() {
            let sz = m.size();
            let sf = m.scale_factor();
            let x = ((sz.width as f64 / sf - PILL_WIDTH_POINTS) / 2.0 * sf) as i32;
            let bottom_offset_points = pill_bottom_offset_points();
            let y =
                ((sz.height as f64 / sf - PILL_HEIGHT_POINTS - bottom_offset_points) * sf) as i32;
            pill.set_position(tauri::PhysicalPosition::new(x, y)).ok();
        }

        pill.emit("pill-state", state).ok();
    }
}

fn pill_bottom_offset_points() -> f64 {
    #[cfg(target_os = "macos")]
    {
        let dock_height = crate::system::mac_app::dock_height_points();
        dock_height + PILL_BOTTOM_GAP_POINTS
    }

    #[cfg(not(target_os = "macos"))]
    {
        64.0
    }
}

pub fn hide_pill(app: &AppHandle) {
    if let Some(pill) = app.get_webview_window("pill") {
        pill.emit("pill-state", "idle").ok();
        // Do not call pill.hide() — hiding the window suspends the WebView2
        // renderer. The next show_pill("recording") emit would then be lost
        // before WebView2 wakes up, causing only "processing" to appear.
        // The pill window is transparent + click-through in idle state, so
        // leaving it visible has no user-visible effect.
    }
}

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
    let settings = app.store("settings.json");
    let audio_config = match settings {
        Ok(ref store) => store::load_audio_config(store),
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
        if !crate::commands::check_accessibility_permission(false) {
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

// ---------- pipeline ----------

fn transcription_model_chain(cfg: &store::PipelineConfig) -> Vec<(String, String)> {
    let mut chain = Vec::<(String, String)>::new();
    if let Some((provider, model)) = store::parse_model_id(&cfg.transcription_default_model) {
        chain.push((provider, model));
    }
    for id in &cfg.transcription_fallback_models {
        if let Some((provider, model)) = store::parse_model_id(id) {
            if !chain.iter().any(|(p, m)| p == &provider && m == &model) {
                chain.push((provider, model));
            }
        }
    }
    chain
}

fn cleanup_model_chain(cfg: &store::PipelineConfig) -> Vec<(String, String)> {
    let mut chain = Vec::<(String, String)>::new();
    if let Some((provider, model)) = store::parse_model_id(&cfg.cleanup_default_model) {
        chain.push((provider, model));
    }
    for id in &cfg.cleanup_fallback_models {
        if let Some((provider, model)) = store::parse_model_id(id) {
            if !chain.iter().any(|(p, m)| p == &provider && m == &model) {
                chain.push((provider, model));
            }
        }
    }
    chain
}

fn has_transcription_key_in_chain(cfg: &store::PipelineConfig) -> bool {
    transcription_model_chain(cfg)
        .iter()
        .any(|(provider, _)| !cfg.key_for(provider).is_empty())
}

fn has_cleanup_key_in_chain(cfg: &store::PipelineConfig) -> bool {
    cleanup_model_chain(cfg)
        .iter()
        .any(|(provider, _)| !cfg.key_for(provider).is_empty())
}

fn trim_err(s: &str) -> String {
    let s = s.trim();
    if s.chars().count() > 120 {
        format!("{}…", s.chars().take(117).collect::<String>())
    } else {
        s.to_string()
    }
}

fn recording_gate_rms(active_gain: f32) -> f32 {
    let gain = active_gain.clamp(store::MIN_MIC_GAIN, store::MAX_MIC_GAIN);
    if gain <= store::DEFAULT_MIC_GAIN {
        MIN_RECORDING_RMS * gain / store::DEFAULT_MIC_GAIN
    } else {
        MIN_RECORDING_RMS * store::DEFAULT_MIC_GAIN / gain
    }
}

fn preview_text(s: &str, limit: usize) -> String {
    let compact = s.replace(['\n', '\r'], " ");
    let compact = compact.trim();
    if compact.chars().count() > limit {
        format!("{}...", compact.chars().take(limit).collect::<String>())
    } else {
        compact.to_string()
    }
}

fn normalize_transcription_math_artifacts(raw: &str) -> String {
    let chars: Vec<char> = raw.chars().collect();
    let mut out = String::with_capacity(chars.len());
    let mut i = 0usize;

    while i < chars.len() {
        if chars[i].is_ascii_digit() {
            let mut j = i + 1;
            while j < chars.len() && chars[j].is_whitespace() {
                j += 1;
            }
            if j < chars.len() && matches!(chars[j], 'x' | 'X') {
                let mut k = j + 1;
                while k < chars.len() && chars[k].is_whitespace() {
                    k += 1;
                }
                if k < chars.len() && chars[k].is_ascii_digit() {
                    let had_spacing = j > i + 1 || k > j + 1;
                    if had_spacing {
                        out.push(chars[i]);
                        out.push('x');
                        out.push(chars[k]);
                        i = k + 1;
                        continue;
                    }
                }
            }
        }
        out.push(chars[i]);
        i += 1;
    }

    out
}

#[cfg(test)]
fn should_use_cleanup_cache(raw: &str) -> bool {
    let (tokens, _) = number_parser::tokenize_cache_key_parts(raw);
    should_use_cleanup_cache_tokens(&tokens)
}

fn should_use_cleanup_cache_tokens(tokens: &[String]) -> bool {
    let mut numeric_count = 0usize;
    let mut has_math_operator = false;

    for t in tokens {
        if t.chars().any(|c| c.is_ascii_digit()) || is_number_word_token(t) {
            numeric_count += 1;
            continue;
        }
        if matches!(
            t.as_str(),
            "plus" | "minus" | "times" | "multiplied" | "multiply" | "divided" | "over" | "x"
        ) {
            has_math_operator = true;
        }
    }

    !(has_math_operator && numeric_count >= 2)
}

fn parse_sqlite_utc(s: &str) -> Option<DateTime<Utc>> {
    let naive = NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S").ok()?;
    Some(DateTime::from_naive_utc_and_offset(naive, Utc))
}

fn sqlite_utc_plus(days: i64) -> String {
    (Utc::now() + Duration::days(days))
        .format("%Y-%m-%d %H:%M:%S")
        .to_string()
}

fn next_cache_expiry(
    hit_count: i64,
    created_at: &str,
    existing_expires_at: &str,
    now: DateTime<Utc>,
) -> String {
    let base = now + Duration::days(7);
    let created = parse_sqlite_utc(created_at).unwrap_or(now);
    let age = now.signed_duration_since(created);

    let next = if hit_count >= 5 && age <= Duration::days(60) {
        if age <= Duration::days(30) {
            now + Duration::days(30)
        } else {
            now + Duration::days(365)
        }
    } else if hit_count >= 2 && age <= Duration::days(14) {
        if age <= Duration::days(7) {
            now + Duration::days(7)
        } else {
            now + Duration::days(30)
        }
    } else {
        let existing = parse_sqlite_utc(existing_expires_at).unwrap_or(base);
        if existing > base {
            existing
        } else {
            base
        }
    };

    next.format("%Y-%m-%d %H:%M:%S").to_string()
}

async fn show_error_pill(app: &AppHandle, msg: &str) {
    log::error!("pipeline error: {msg}");
    app.emit("verenu:error", msg).ok();
    show_pill(app, "error");
    if let Some(w) = app.get_webview_window("main") {
        w.show().ok();
        w.set_focus().ok();
    }
    tokio::time::sleep(std::time::Duration::from_secs(4)).await;
    hide_pill(app);
}

/// Shows the pill in error state for a quality-gate rejection without
/// focusing the main window or blocking the pipeline task.
fn reject_with_pill(app: &AppHandle, msg: &str) {
    app.emit("verenu:error", msg).ok();
    show_pill(app, "error");
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_secs(4)).await;
        // Only hide if no new recording session has started in the meantime
        if let Some(state) = app.try_state::<SharedState>() {
            if let Ok(st) = lock_state(&state) {
                if st.session.is_none() {
                    hide_pill(&app);
                }
            }
        }
    });
}

fn resolve_profile(
    store: Option<&tauri_plugin_store::Store<tauri::Wry>>,
    process_name: &str,
    default_tone: &str,
) -> String {
    let mapped = store.and_then(|s| {
        s.get(store::APP_MAPPINGS)
            .and_then(|v| serde_json::from_value::<Vec<AppMapping>>(v).ok())
            .and_then(|list| {
                list.into_iter()
                    .find_map(|m| (m.exe.to_lowercase() == process_name).then_some(m.profile))
            })
    });
    mapped.unwrap_or_else(|| default_tone.to_owned())
}

pub async fn transcribe_input_only(app: AppHandle, state: SharedState) -> anyhow::Result<String> {
    let session = {
        let mut st = lock_state(&state)?;
        st.session.take()
    };
    let Some(session) = session else {
        anyhow::bail!("No active recording");
    };

    std::thread::spawn(crate::system::volume::unmute);
    show_pill(&app, "processing");

    let settings_store = match app.store("settings.json") {
        Ok(s) => s,
        Err(e) => {
            hide_pill(&app);
            return Err(anyhow::anyhow!(e.to_string()));
        }
    };
    let active_gain = store::load_audio_config(&settings_store).mic_gain;
    let min_rms = recording_gate_rms(active_gain);
    log::debug!("pipeline: input gate active_gain={active_gain:.2} min_rms={min_rms:.6}");

    let stop_result = tokio::task::spawn_blocking(move || session.stop()).await?;
    let (wav, duration_ms, rms) = stop_result?;

    if duration_ms < MIN_RECORDING_MS || rms < min_rms {
        hide_pill(&app);
        anyhow::bail!("Recording too short");
    }
    let wav = bytes::Bytes::from(wav);

    let cfg = store::load_pipeline_config(&settings_store);

    if !has_transcription_key_in_chain(&cfg) {
        hide_pill(&app);
        anyhow::bail!("No API key configured for any model in the transcription chain");
    }

    let mut transcribed: Option<String> = None;
    let mut last_err: Option<anyhow::Error> = None;
    for (provider_id, model) in transcription_model_chain(&cfg) {
        let key = cfg.key_for(&provider_id).to_owned();
        if key.is_empty() {
            continue;
        }
        let provider = transcription_provider_from_str(&provider_id);
        let language = cfg.transcription_language.clone();
        match transcription::transcribe(wav.clone(), provider, &key, &language, &model).await {
            Ok(text) if !text.is_empty() => {
                transcribed = Some(text);
                break;
            }
            Ok(_) => {}
            Err(e) => {
                let retryable = crate::api::is_retryable_provider_error(&e);
                log::warn!(
                    "pipeline: transcription provider failed provider={} model={} retryable={} error={}",
                    provider_id,
                    model,
                    retryable,
                    trim_err(&e.to_string())
                );
                if retryable {
                    last_err = Some(e);
                    continue;
                }
                last_err = Some(e);
                break;
            }
        }
    }

    hide_pill(&app);

    match transcribed {
        Some(text) => Ok(text),
        None => Err(last_err.unwrap_or_else(|| {
            anyhow::anyhow!("Transcription failed: no model in chain produced output")
        })),
    }
}

pub async fn run_pipeline(app: AppHandle, state: SharedState) {
    let started_at = std::time::Instant::now();
    let Some((session, target_hwnd)) = take_pipeline_session(&state) else {
        log::debug!("pipeline: no session - recording never started or was already consumed");
        return;
    };

    let process_name = window_context::get_active_process_name()
        .unwrap_or_else(|| "unknown".into())
        .to_lowercase();
    log::info!("pipeline: start process={process_name} target_hwnd={target_hwnd}");

    std::thread::spawn(crate::system::volume::unmute);
    show_pill(&app, "processing");

    // Keep the quiet-audio gate permissive at high gain. Whisper recordings can
    // still have low post-denoise RMS, even after amplification.
    let active_gain = match app.store("settings.json") {
        Ok(s) => store::load_audio_config(&s).mic_gain,
        Err(e) => {
            log::warn!("pipeline: failed to load audio config, using default gain: {e}");
            store::DEFAULT_MIC_GAIN
        }
    };
    let min_rms = recording_gate_rms(active_gain);
    log::debug!("pipeline: audio gate active_gain={active_gain:.2} min_rms={min_rms:.6}");

    let stage_audio = std::time::Instant::now();
    let Some((wav, duration_ms)) = stop_and_validate_audio(&app, session, min_rms).await else {
        return;
    };
    log::debug!(
        "pipeline: audio accepted duration_ms={duration_ms} wav_bytes={} stage_ms={}",
        wav.len(),
        stage_audio.elapsed().as_millis()
    );

    let stage_config = std::time::Instant::now();
    let Some((cfg, profile, app_context)) = open_config_and_context(&app, &process_name).await
    else {
        return;
    };
    log::debug!(
        "pipeline: config t_provider={} c_provider={} t_model={} c_model={} cleanup_enabled={} intensity={} app_context_hint={} profile={}",
        cfg.transcription_provider,
        cfg.cleanup_provider,
        cfg.transcription_default_model,
        cfg.cleanup_default_model,
        cfg.cleanup_enabled,
        cfg.cleanup_intensity,
        cfg.app_context_hint,
        profile
    );
    log::debug!(
        "pipeline: context resolved app_context={} stage_ms={}",
        app_context.as_deref().unwrap_or("none"),
        stage_config.elapsed().as_millis()
    );

    let retry_captured_at = std::time::Instant::now();
    if let Ok(mut st) = lock_state(&state) {
        st.retry_capture = Some(RetryCapture {
            wav: wav.clone(),
            captured_at: retry_captured_at,
            duration_ms,
            target_hwnd,
            process_name: process_name.clone(),
            profile: profile.clone(),
            app_context: app_context.clone(),
        });
    }

    let stage_transcribe = std::time::Instant::now();
    let Some((raw, api_used, final_text, dict_entries, cleanup_cache_key)) =
        run_transcription_and_cleanup(&app, &wav, &cfg, &profile, app_context.as_deref()).await
    else {
        emit_pipeline_failed(&app);
        return;
    };
    log::debug!(
        "pipeline: transcription ok provider={} raw_chars={} raw_preview=\"{}\"",
        api_used,
        raw.chars().count(),
        preview_text(&raw, 140)
    );
    if crate::system::logger::is_verbose() {
        log::debug!("pipeline: transcription raw_full=\"{}\"", raw);
    }
    log::debug!(
        "pipeline: transcription stage_ms={}",
        stage_transcribe.elapsed().as_millis()
    );

    let stage_cleanup = std::time::Instant::now();
    log::debug!(
        "pipeline: cleanup/snippets ok final_chars={} final_preview=\"{}\" dict_entries={}",
        final_text.chars().count(),
        preview_text(&final_text, 140),
        dict_entries.len()
    );
    if crate::system::logger::is_verbose() {
        log::debug!("pipeline: final_text_full=\"{}\"", final_text);
    }
    log::debug!(
        "pipeline: cleanup stage_ms={}",
        stage_cleanup.elapsed().as_millis()
    );

    let words = raw.split_whitespace().count() as i64;
    if let Err(e) = finalize_pipeline_completion(
        &app,
        &state,
        PipelineCompletionContext {
            raw: &raw,
            final_text_before_dict: &final_text,
            dict_entries: &dict_entries,
            duration_ms,
            api_used: &api_used,
            target_hwnd,
            cfg: &cfg,
            profile: &profile,
            process_name,
            cleanup_cache_key,
            captured_at: retry_captured_at,
        },
    )
    .await
    {
        log::error!("pipeline finalize failed: {e}");
        return;
    }

    log::info!(
        "pipeline: completed words={} duration_ms={} elapsed_ms={}",
        words,
        duration_ms,
        started_at.elapsed().as_millis()
    );
}

fn take_pipeline_session(state: &SharedState) -> Option<(audio::RecordingSession, usize)> {
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

// session.stop() blocks until the audio thread finishes (denoise + resample + WAV encode).
// spawn_blocking keeps the tokio worker free during that wait.
async fn stop_and_validate_audio(
    app: &AppHandle,
    session: audio::RecordingSession,
    min_rms: f32,
) -> Option<(bytes::Bytes, u64)> {
    let stop_result = tokio::task::spawn_blocking(move || session.stop()).await;
    let (wav, duration_ms, rms) = match stop_result {
        Ok(Ok(v)) => v,
        Ok(Err(e)) => {
            log::error!("audio stop: {e}");
            hide_pill(app);
            return None;
        }
        Err(e) => {
            log::error!("audio stop task panicked: {e}");
            hide_pill(app);
            return None;
        }
    };
    if duration_ms < MIN_RECORDING_MS || rms < min_rms {
        let msg = if duration_ms < MIN_RECORDING_MS {
            "Recording too short"
        } else {
            "Audio too quiet — check your mic"
        };
        log::debug!(
            "pipeline: rejected — duration={duration_ms}ms rms={rms:.4} min_rms={min_rms:.4}"
        );
        reject_with_pill(app, msg);
        return None;
    }
    Some((bytes::Bytes::from(wav), duration_ms))
}

async fn open_config_and_context(
    app: &AppHandle,
    process_name: &str,
) -> Option<(store::PipelineConfig, String, Option<String>)> {
    let settings_store = match app.store("settings.json") {
        Ok(s) => s,
        Err(e) => {
            log::error!("store: {e}");
            hide_pill(app);
            return None;
        }
    };
    let cfg = store::load_pipeline_config(&settings_store);
    if !has_transcription_key_in_chain(&cfg) {
        show_error_pill(
            app,
            "No API key saved for selected transcription model chain",
        )
        .await;
        return None;
    }
    let profile = resolve_profile(Some(&settings_store), process_name, &cfg.default_tone);
    let app_context = if cfg.app_context_hint {
        window_context::get_app_context_hint(process_name)
    } else {
        None
    };
    Some((cfg, profile, app_context))
}

async fn run_transcription(
    app: &AppHandle,
    wav: &bytes::Bytes,
    cfg: &store::PipelineConfig,
) -> Option<(String, String)> {
    let wav = wav.clone();
    log::debug!(
        "pipeline: transcription stage start provider={} model={} language={} bytes={}",
        cfg.transcription_provider,
        cfg.transcription_default_model,
        cfg.transcription_language,
        wav.len()
    );

    let mut last_err: Option<anyhow::Error> = None;
    for (provider_id, model) in transcription_model_chain(cfg) {
        let key = cfg.key_for(&provider_id).to_owned();
        if key.is_empty() {
            continue;
        }
        let provider = transcription_provider_from_str(&provider_id);
        let language = cfg.transcription_language.clone();
        match transcription::transcribe(wav.clone(), provider, &key, &language, &model).await {
            Ok(raw) if !raw.is_empty() => {
                log::debug!(
                    "pipeline: transcription provider success={} model={} chars={}",
                    provider_id,
                    model,
                    raw.chars().count()
                );
                return Some((raw, format!("{provider_id}/{model}/transcription")));
            }
            Ok(_) => {}
            Err(e) => {
                let retryable = crate::api::is_retryable_provider_error(&e);
                log::warn!(
                    "pipeline: transcription provider failed provider={} model={} retryable={} error={}",
                    provider_id,
                    model,
                    retryable,
                    trim_err(&e.to_string())
                );
                if retryable {
                    last_err = Some(e);
                    continue;
                }
                last_err = Some(e);
                break;
            }
        }
    }

    if let Some(e) = last_err {
        let mut user_msg = trim_err(&e.to_string());
        if let Some(parsed) = crate::api::parse_auth_401_error(&e.to_string()) {
            user_msg = crate::api::auth_401_display_message(&parsed);
        }
        log::error!(
            "pipeline: transcription failed error={}",
            trim_err(&e.to_string())
        );
        show_error_pill(app, &user_msg).await;
    } else {
        show_error_pill(
            app,
            "Nothing transcribed - please try speaking more clearly",
        )
        .await;
    }
    None
}

async fn run_transcription_and_cleanup(
    app: &AppHandle,
    wav: &bytes::Bytes,
    cfg: &store::PipelineConfig,
    profile: &str,
    app_context: Option<&str>,
) -> Option<(String, String, String, Vec<db::DictionaryEntry>, String)> {
    let (raw, api_used) = run_transcription(app, wav, cfg).await?;
    let raw = normalize_transcription_math_artifacts(&raw);

    let (final_text, dict_entries, cleanup_cache_key) =
        run_cleanup_and_snippets(app, &raw, cfg, profile, app_context).await?;
    Some((raw, api_used, final_text, dict_entries, cleanup_cache_key))
}

// Handles snippet fast-path, snippet instruction collection, LLM cleanup, and
// instruction override application. Returns (final_text_before_dict, dict_entries)
// so the caller can apply dictionary substitutions after saving to DB.
async fn run_cleanup_and_snippets(
    app: &AppHandle,
    raw: &str,
    cfg: &store::PipelineConfig,
    profile: &str,
    app_context: Option<&str>,
) -> Option<(String, Vec<db::DictionaryEntry>, String)> {
    let db_handle = app.state::<DbHandle>();
    match run_cleanup_and_snippets_for_db(db_handle.inner(), raw, cfg, profile, app_context).await {
        Ok(result) => Some(result),
        Err(e) => {
            let mut user_msg = format!("Cleanup failed: {}", trim_err(&e.to_string()));
            if let Some(parsed) = crate::api::parse_auth_401_error(&e.to_string()) {
                user_msg = format!(
                    "Cleanup failed: {}",
                    crate::api::auth_401_display_message(&parsed)
                );
            }
            log::error!(
                "pipeline: cleanup failed error={}",
                trim_err(&e.to_string())
            );
            show_error_pill(app, &user_msg).await;
            None
        }
    }
}

async fn run_cleanup_and_snippets_for_db(
    db_handle: &DbHandle,
    raw: &str,
    cfg: &store::PipelineConfig,
    profile: &str,
    app_context: Option<&str>,
) -> anyhow::Result<(String, Vec<db::DictionaryEntry>, String)> {
    let mut db_snippets = db::query_snippets(db_handle).unwrap_or_default();
    let dict_entries = db::query_dictionary(db_handle).unwrap_or_default();
    log::debug!(
        "pipeline: cleanup inputs snippets={} dict_entries={}",
        db_snippets.len(),
        dict_entries.len()
    );

    let snippet_instructions = snippets::collect_snippet_instructions_from(raw, &db_snippets);
    log::debug!(
        "pipeline: cleanup stage start raw_chars={} snippet_override_lines={} cleanup_enabled={}",
        raw.chars().count(),
        snippet_instructions
            .lines()
            .filter(|l| !l.trim().is_empty())
            .count(),
        cfg.cleanup_enabled
    );
    if crate::system::logger::is_verbose() {
        log::debug!("pipeline: cleanup raw_full=\"{}\"", raw);
        if !snippet_instructions.is_empty() {
            log::debug!(
                "pipeline: cleanup snippet_instructions_full=\"{}\"",
                snippet_instructions
            );
        }
    }

    // Fast path: entire transcription was a single snippet trigger — skip the LLM.
    let pure_expansion = if snippet_instructions.is_empty() {
        snippets::try_pure_snippet_expand_from(raw, &db_snippets, db_handle)
    } else {
        None
    };
    let expanded = pure_expansion
        .clone()
        .unwrap_or_else(|| snippets::expand_snippets_from(raw, &mut db_snippets, db_handle));
    log::debug!(
        "pipeline: snippets expanded pure_fast_path={} expanded_chars={}",
        pure_expansion.is_some(),
        expanded.chars().count()
    );

    let mut used_cache_key = String::new();
    let final_text = if should_run_cleanup_llm(
        cfg.cleanup_enabled,
        has_cleanup_key_in_chain(cfg),
        pure_expansion.is_none(),
        &cfg.cleanup_intensity,
        profile,
    ) {
        let has_snippets = !snippet_instructions.is_empty();
        let (cache_tokens, cache_separators) = number_parser::tokenize_cache_key_parts(&expanded);
        let allow_cache = should_use_cleanup_cache_tokens(&cache_tokens)
            && (expanded.chars().count() <= 200 || has_snippets);
        let cache_key = if allow_cache {
            let base_cache_key =
                number_parser::normalize_cleanup_cache_key_parts(&cache_tokens, &cache_separators);
            let mut key =
                style_scoped_cleanup_cache_key(&base_cache_key, profile, &cfg.cleanup_intensity);
            if !key.is_empty() && has_snippets {
                let fp = snippet_instructions_fingerprint(&snippet_instructions);
                key = format!("{key}|snip:{fp:x}");
            }
            key
        } else {
            String::new()
        };
        if !cache_key.is_empty() {
            used_cache_key = cache_key.clone();
            if let Ok(Some(entry)) = db::cleanup_cache_get_active(db_handle, &cache_key) {
                log::debug!(
                    "pipeline: cleanup cache hit key_len={} hit_count={}",
                    cache_key.len(),
                    entry.hit_count
                );
                let now = Utc::now();
                let new_hit_count = entry.hit_count + 1;
                let now_str = now.format("%Y-%m-%d %H:%M:%S").to_string();
                let new_expires_at =
                    next_cache_expiry(new_hit_count, &entry.created_at, &entry.expires_at, now);
                let _ = db::cleanup_cache_touch_hit(
                    db_handle,
                    &cache_key,
                    new_hit_count,
                    &now_str,
                    &new_expires_at,
                );
                log::debug!(
                    "pipeline: cleanup cache touch hit_count={} expires_at={}",
                    new_hit_count,
                    new_expires_at
                );
                let overridden = snippets::apply_cleanup_instruction_overrides(
                    &entry.clean_text,
                    &snippet_instructions,
                );
                return Ok((overridden, dict_entries, cache_key));
            }
        }
        log::debug!(
            "pipeline: cleanup cache {} key_len={}",
            if allow_cache { "miss" } else { "bypass" },
            cache_key.len()
        );
        let dict_instructions =
            dictionary::build_relevant_dictionary_prompt_from(&dict_entries, raw);
        let extra_rules = [snippet_instructions.as_str(), dict_instructions.as_str()]
            .iter()
            .filter(|s| !s.is_empty())
            .copied()
            .collect::<Vec<_>>()
            .join("\n\n");
        log::debug!(
            "pipeline: cleanup extra_rules chars={} lines={}",
            extra_rules.chars().count(),
            extra_rules.lines().filter(|l| !l.trim().is_empty()).count()
        );

        let mut cleaned_res: Option<String> = None;
        let mut last_cleanup_err: Option<anyhow::Error> = None;
        for (provider_id, model) in cleanup_model_chain(cfg) {
            let key = cfg.key_for(&provider_id).to_owned();
            if key.is_empty() {
                continue;
            }
            let cp = cleanup_provider_from_str(&provider_id);
            match cleanup::cleanup(
                &expanded,
                cp,
                &key,
                &model,
                profile,
                &cfg.cleanup_intensity,
                &extra_rules,
                app_context,
            )
            .await
            {
                Ok(cleaned) if !cleaned.is_empty() => {
                    log::debug!(
                        "pipeline: cleanup provider success={} model={} cleaned_chars={}",
                        provider_id,
                        model,
                        cleaned.chars().count()
                    );
                    cleaned_res = Some(cleaned);
                    break;
                }
                Ok(_) => {
                    last_cleanup_err = None;
                }
                Err(e) => {
                    let retryable = crate::api::is_retryable_provider_error(&e);
                    log::warn!(
                        "pipeline: cleanup provider failed provider={} model={} retryable={} error={}",
                        provider_id,
                        model,
                        retryable,
                        trim_err(&e.to_string())
                    );
                    if retryable {
                        last_cleanup_err = Some(e);
                        continue;
                    }
                    last_cleanup_err = Some(e);
                    break;
                }
            }
        }

        match cleaned_res {
            Some(cleaned) => {
                let overridden =
                    snippets::apply_cleanup_instruction_overrides(&cleaned, &snippet_instructions);
                if !cache_key.is_empty() {
                    let expires = sqlite_utc_plus(7);
                    match db::cleanup_cache_insert_new(
                        db_handle,
                        &cache_key,
                        &cleaned,
                        &expires,
                        has_snippets,
                    ) {
                        Ok(_) => {
                            log::debug!("pipeline: cleanup cache insert ok expires_at={expires}")
                        }
                        Err(err) => log::warn!("pipeline: cleanup cache insert failed: {err}"),
                    }
                }
                overridden
            }
            None if last_cleanup_err.is_some() => return Err(last_cleanup_err.expect("checked")),
            None => snippets::apply_cleanup_instruction_overrides(&expanded, &snippet_instructions),
        }
    } else {
        snippets::apply_cleanup_instruction_overrides(&expanded, &snippet_instructions)
    };

    Ok((final_text, dict_entries, used_cache_key))
}

fn should_run_cleanup_llm(
    cleanup_enabled: bool,
    has_cleanup_key: bool,
    no_pure_expansion: bool,
    cleanup_intensity: &str,
    profile: &str,
) -> bool {
    cleanup_enabled
        && has_cleanup_key
        && no_pure_expansion
        && (cleanup_intensity != "none" || profile == "formal")
}

fn style_scoped_cleanup_cache_key(
    base_key: &str,
    profile: &str,
    cleanup_intensity: &str,
) -> String {
    if base_key.is_empty() {
        return String::new();
    }
    format!("{base_key}|profile:{profile}|intensity:{cleanup_intensity}")
}

fn snippet_instructions_fingerprint(instructions: &str) -> u64 {
    // djb2 hash — deterministic across runs, no external dep
    let mut h: u64 = 5381;
    for b in instructions.bytes() {
        h = h.wrapping_shl(5).wrapping_add(h).wrapping_add(b as u64);
    }
    h
}

#[cfg(any(test, debug_assertions))]
#[allow(dead_code)]
#[derive(Clone, Debug)]
pub struct PipelineTestSnippet {
    pub trigger: String,
    pub expansion: String,
    pub instructions: String,
}

#[cfg(any(test, debug_assertions))]
#[allow(dead_code)]
#[derive(Clone, Debug)]
pub struct PipelineTestDictionaryEntry {
    pub term: String,
    pub mistake: Option<String>,
}

#[cfg(any(test, debug_assertions))]
#[allow(dead_code)]
#[derive(Clone, Debug)]
pub struct PipelineTestRequest {
    pub db: Option<DbHandle>,
    pub wav: bytes::Bytes,
    pub duration_ms: u64,
    pub rms: f32,
    pub config: store::PipelineConfig,
    pub profile: String,
    pub target_hwnd: usize,
    pub app_context: Option<String>,
    pub snippets: Vec<PipelineTestSnippet>,
    pub dictionary: Vec<PipelineTestDictionaryEntry>,
}

#[cfg(any(test, debug_assertions))]
#[allow(dead_code)]
#[derive(Clone, Debug)]
pub struct PipelineTestResult {
    pub raw_text: String,
    pub final_text_before_dictionary: String,
    pub injected_text: String,
    pub api_used: String,
    pub cleanup_cache_key: String,
    pub history_entry: db::RecentEntry,
    pub recent: Vec<db::RecentEntry>,
    pub stats: db::Stats,
}

#[cfg(any(test, debug_assertions))]
#[allow(dead_code)]
pub async fn run_pipeline_fixture(
    request: PipelineTestRequest,
) -> anyhow::Result<PipelineTestResult> {
    if request.duration_ms < MIN_RECORDING_MS {
        anyhow::bail!("Recording too short");
    }
    if request.rms < MIN_RECORDING_RMS {
        anyhow::bail!("Audio too quiet - check your mic");
    }
    if !has_transcription_key_in_chain(&request.config) {
        anyhow::bail!("No API key configured for any model in the transcription chain");
    }

    let db_handle = match request.db {
        Some(d) => d,
        None => db::open(":memory:")?,
    };
    for snippet in &request.snippets {
        db::insert_snippet_returning(
            &db_handle,
            &snippet.trigger,
            &snippet.expansion,
            &snippet.instructions,
        )?;
    }
    for entry in &request.dictionary {
        db::insert_dictionary_entry_returning(&db_handle, &entry.term, entry.mistake.as_deref())?;
    }

    let mut transcribed: Option<(String, String)> = None;
    let mut last_err: Option<anyhow::Error> = None;
    for (provider_id, model) in transcription_model_chain(&request.config) {
        let key = request.config.key_for(&provider_id).to_owned();
        if key.is_empty() {
            continue;
        }
        let provider = transcription_provider_from_str(&provider_id);
        match transcription::transcribe(
            request.wav.clone(),
            provider,
            &key,
            &request.config.transcription_language,
            &model,
        )
        .await
        {
            Ok(raw) if !raw.is_empty() => {
                transcribed = Some((
                    normalize_transcription_math_artifacts(&raw),
                    format!("{provider_id}/{model}/transcription"),
                ));
                break;
            }
            Ok(_) => {}
            Err(e) => {
                if crate::api::is_retryable_provider_error(&e) {
                    last_err = Some(e);
                    continue;
                }
                last_err = Some(e);
                break;
            }
        }
    }

    let (raw_text, api_used) = transcribed.ok_or_else(|| {
        last_err.unwrap_or_else(|| {
            anyhow::anyhow!("Transcription failed: no model in chain produced output")
        })
    })?;

    let (final_text_before_dictionary, dict_entries, cleanup_cache_key) =
        run_cleanup_and_snippets_for_db(
            &db_handle,
            &raw_text,
            &request.config,
            &request.profile,
            request.app_context.as_deref(),
        )
        .await?;
    let (injected_text, _applied_dict_ids) =
        dictionary::apply_substitutions_from(&final_text_before_dictionary, &dict_entries);
    let words = raw_text.split_whitespace().count() as i64;
    let history_entry = db::insert_transcription_returning(
        &db_handle,
        &raw_text,
        &final_text_before_dictionary,
        words,
        request.duration_ms as i64,
        &api_used,
    )?;
    let injected = injection::inject_text(
        &injected_text,
        request.target_hwnd,
        request.config.contextual_caps_enabled,
        request.config.auto_spacing_enabled,
        &request.profile,
        request.config.macos_clipboard_sniff_enabled,
    )
    .await?;
    let recent = db::query_recent(&db_handle)?;
    let stats = db::query_stats(&db_handle)?;

    Ok(PipelineTestResult {
        raw_text,
        final_text_before_dictionary,
        injected_text: injected.text,
        api_used,
        cleanup_cache_key,
        history_entry,
        recent,
        stats,
    })
}

#[cfg(test)]
mod tests {
    use super::{
        normalize_transcription_math_artifacts, recording_gate_rms, run_pipeline_fixture,
        should_run_cleanup_llm, should_use_cleanup_cache, style_scoped_cleanup_cache_key,
        PipelineTestDictionaryEntry, PipelineTestRequest, PipelineTestSnippet,
    };
    use crate::data::store;
    use crate::testing::{
        fixture_hit_count, register_fixture, reset, set_enabled, take_injections, FixtureSpec,
    };
    use bytes::Bytes;

    #[test]
    fn cleanup_cache_bypasses_math_like_queries() {
        assert!(!should_use_cleanup_cache("What's 67 plus 67?"));
        assert!(!should_use_cleanup_cache("what is six times seven"));
    }

    #[test]
    fn cleanup_cache_keeps_non_math_numeric_queries() {
        assert!(should_use_cleanup_cache("version 2.5 release notes"));
        assert!(should_use_cleanup_cache("meeting on 2026-05-17 at 10:30"));
    }

    #[test]
    fn transcription_preserves_digit_x_digit_in_plus_queries() {
        let out = normalize_transcription_math_artifacts("What's 6x7 plus 6x7?");
        assert_eq!(out, "What's 6x7 plus 6x7?");
    }

    #[test]
    fn transcription_does_not_touch_non_plus_digit_x_digit() {
        let out = normalize_transcription_math_artifacts("Calculate 6x7");
        assert_eq!(out, "Calculate 6x7");
    }

    #[test]
    fn transcription_compacts_spaced_digit_x_digit_chunks() {
        let out = normalize_transcription_math_artifacts("What is 6 x 7 plus 6 x 7?");
        assert_eq!(out, "What is 6x7 plus 6x7?");
    }

    #[test]
    fn transcription_does_not_fold_mixed_multiplication_chunks() {
        let out = normalize_transcription_math_artifacts("6x7 plus 3x4");
        assert_eq!(out, "6x7 plus 3x4");
    }

    #[test]
    fn cleanup_llm_runs_for_formal_even_when_none_intensity() {
        assert!(should_run_cleanup_llm(true, true, true, "none", "formal"));
    }

    #[test]
    fn cleanup_llm_skips_for_non_formal_when_none_intensity() {
        assert!(!should_run_cleanup_llm(true, true, true, "none", "casual"));
    }

    #[test]
    fn style_scoped_cache_key_changes_with_profile_and_intensity() {
        let casual_medium = style_scoped_cleanup_cache_key("abc123", "casual", "medium");
        let formal_medium = style_scoped_cleanup_cache_key("abc123", "formal", "medium");
        let casual_high = style_scoped_cleanup_cache_key("abc123", "casual", "high");
        assert_ne!(casual_medium, formal_medium);
        assert_ne!(casual_medium, casual_high);
    }

    #[test]
    fn style_scoped_cache_key_preserves_empty_base_key() {
        assert_eq!(style_scoped_cleanup_cache_key("", "casual", "medium"), "");
    }

    #[test]
    fn recording_gate_gets_more_permissive_at_high_gain() {
        let default_gate = recording_gate_rms(store::DEFAULT_MIC_GAIN);
        let high_gain_gate = recording_gate_rms(store::MAX_MIC_GAIN);

        assert!((default_gate - 0.008).abs() < f32::EPSILON);
        assert!(high_gain_gate < default_gate);
        assert!((high_gain_gate - 0.0035).abs() < 0.0001);
    }

    fn base_config() -> store::PipelineConfig {
        store::PipelineConfig {
            transcription_provider: "groq".into(),
            transcription_language: "en".into(),
            cleanup_provider: "groq".into(),
            transcription_default_model: "groq/whisper-large-v3-turbo".into(),
            cleanup_default_model: "groq/llama-3.3-70b-versatile".into(),
            transcription_fallback_models: Vec::new(),
            cleanup_fallback_models: Vec::new(),
            cleanup_enabled: true,
            key_groq: "fixture-groq-key".into(),
            key_openai: "fixture-openai-key".into(),
            key_google: "fixture-google-key".into(),
            default_tone: "casual".into(),
            cleanup_intensity: "medium".into(),
            app_context_hint: false,
            auto_learn_enabled: false,
            contextual_caps_enabled: true,
            auto_spacing_enabled: true,
            macos_clipboard_sniff_enabled: false,
        }
    }

    fn base_request(config: store::PipelineConfig) -> PipelineTestRequest {
        PipelineTestRequest {
            db: None,
            wav: Bytes::from_static(b"fixture-wav"),
            duration_ms: 1200,
            rms: 0.2,
            config,
            profile: "casual".into(),
            target_hwnd: 77,
            app_context: None,
            snippets: Vec::new(),
            dictionary: Vec::new(),
        }
    }

    fn harness_test_lock() -> &'static std::sync::Mutex<()> {
        static LOCK: std::sync::OnceLock<std::sync::Mutex<()>> = std::sync::OnceLock::new();
        LOCK.get_or_init(|| std::sync::Mutex::new(()))
    }

    #[tokio::test(flavor = "current_thread")]
    async fn pipeline_fixture_rejects_short_recordings_before_provider_calls() {
        let _guard = harness_test_lock().lock().expect("harness lock");
        reset();
        set_enabled(true);
        register_fixture(FixtureSpec {
            task: "transcription".into(),
            provider: "groq".into(),
            model: "whisper-large-v3-turbo".into(),
            response: Some("should never be used".into()),
            error_kind: None,
            error_message: None,
        });

        let mut request = base_request(base_config());
        request.duration_ms = 300;
        let err = run_pipeline_fixture(request)
            .await
            .expect_err("short recording should fail");
        assert!(err.to_string().contains("Recording too short"));
        assert_eq!(
            fixture_hit_count("transcription", "groq", "whisper-large-v3-turbo"),
            0
        );
        reset();
    }

    #[tokio::test(flavor = "current_thread")]
    async fn pipeline_fixture_rejects_quiet_recordings_before_provider_calls() {
        let _guard = harness_test_lock().lock().expect("harness lock");
        reset();
        set_enabled(true);
        register_fixture(FixtureSpec {
            task: "transcription".into(),
            provider: "groq".into(),
            model: "whisper-large-v3-turbo".into(),
            response: Some("should never be used".into()),
            error_kind: None,
            error_message: None,
        });

        let mut request = base_request(base_config());
        request.rms = 0.001;
        let err = run_pipeline_fixture(request)
            .await
            .expect_err("quiet recording should fail");
        assert!(err.to_string().contains("Audio too quiet"));
        assert_eq!(
            fixture_hit_count("transcription", "groq", "whisper-large-v3-turbo"),
            0
        );
        reset();
    }

    #[tokio::test(flavor = "current_thread")]
    async fn pipeline_fixture_requires_transcription_key_before_provider_calls() {
        let _guard = harness_test_lock().lock().expect("harness lock");
        reset();
        set_enabled(true);
        let mut config = base_config();
        config.key_groq.clear();
        config.key_openai.clear();
        config.key_google.clear();
        let err = run_pipeline_fixture(base_request(config))
            .await
            .expect_err("missing key should fail");
        assert!(err.to_string().contains("No API key configured"));
        reset();
    }

    #[tokio::test(flavor = "current_thread")]
    async fn pipeline_fixture_uses_transcription_fallback_for_retryable_errors() {
        let _guard = harness_test_lock().lock().expect("harness lock");
        reset();
        set_enabled(true);
        let mut config = base_config();
        config.transcription_fallback_models = vec!["openai/gpt-4o-transcribe".into()];
        register_fixture(FixtureSpec {
            task: "transcription".into(),
            provider: "groq".into(),
            model: "whisper-large-v3-turbo".into(),
            response: None,
            error_kind: Some("timeout".into()),
            error_message: Some("groq timed out".into()),
        });
        register_fixture(FixtureSpec {
            task: "transcription".into(),
            provider: "openai".into(),
            model: "gpt-4o-transcribe".into(),
            response: Some("fallback transcript".into()),
            error_kind: None,
            error_message: None,
        });
        register_fixture(FixtureSpec {
            task: "cleanup".into(),
            provider: "groq".into(),
            model: "llama-3.3-70b-versatile".into(),
            response: Some("fallback transcript".into()),
            error_kind: None,
            error_message: None,
        });

        let result = run_pipeline_fixture(base_request(config))
            .await
            .expect("fallback should succeed");
        assert_eq!(result.raw_text, "fallback transcript");
        assert_eq!(result.api_used, "openai/gpt-4o-transcribe/transcription");
        assert_eq!(
            fixture_hit_count("transcription", "groq", "whisper-large-v3-turbo"),
            1
        );
        assert_eq!(
            fixture_hit_count("transcription", "openai", "gpt-4o-transcribe"),
            1
        );
        reset();
    }

    #[tokio::test(flavor = "current_thread")]
    async fn pipeline_fixture_stops_on_non_retryable_transcription_error() {
        let _guard = harness_test_lock().lock().expect("harness lock");
        reset();
        set_enabled(true);
        let mut config = base_config();
        config.transcription_fallback_models = vec!["openai/gpt-4o-transcribe".into()];
        register_fixture(FixtureSpec {
            task: "transcription".into(),
            provider: "groq".into(),
            model: "whisper-large-v3-turbo".into(),
            response: None,
            error_kind: Some("auth_invalid".into()),
            error_message: None,
        });
        register_fixture(FixtureSpec {
            task: "transcription".into(),
            provider: "openai".into(),
            model: "gpt-4o-transcribe".into(),
            response: Some("should not be used".into()),
            error_kind: None,
            error_message: None,
        });

        let err = run_pipeline_fixture(base_request(config))
            .await
            .expect_err("auth error should stop fallback");
        assert!(err.to_string().starts_with("AUTH_401|provider=Groq"));
        assert_eq!(
            fixture_hit_count("transcription", "openai", "gpt-4o-transcribe"),
            0
        );
        reset();
    }

    #[tokio::test(flavor = "current_thread")]
    async fn pipeline_fixture_uses_cleanup_fallback_and_persists_history() {
        let _guard = harness_test_lock().lock().expect("harness lock");
        reset();
        set_enabled(true);
        let mut config = base_config();
        config.cleanup_fallback_models = vec!["openai/gpt-4o-mini".into()];
        register_fixture(FixtureSpec {
            task: "transcription".into(),
            provider: "groq".into(),
            model: "whisper-large-v3-turbo".into(),
            response: Some("raw fallback cleanup test".into()),
            error_kind: None,
            error_message: None,
        });
        register_fixture(FixtureSpec {
            task: "cleanup".into(),
            provider: "groq".into(),
            model: "llama-3.3-70b-versatile".into(),
            response: None,
            error_kind: Some("status_503".into()),
            error_message: Some("temporary overload".into()),
        });
        register_fixture(FixtureSpec {
            task: "cleanup".into(),
            provider: "openai".into(),
            model: "gpt-4o-mini".into(),
            response: Some("clean fallback result".into()),
            error_kind: None,
            error_message: None,
        });

        let result = run_pipeline_fixture(base_request(config))
            .await
            .expect("cleanup fallback should succeed");
        assert_eq!(result.final_text_before_dictionary, "clean fallback result");
        assert_eq!(result.history_entry.clean_text, "clean fallback result");
        assert_eq!(result.stats.total_words, 4);
        assert_eq!(result.recent.len(), 1);
        assert_eq!(
            fixture_hit_count("cleanup", "groq", "llama-3.3-70b-versatile"),
            1
        );
        assert_eq!(fixture_hit_count("cleanup", "openai", "gpt-4o-mini"), 1);
        reset();
    }

    #[tokio::test(flavor = "current_thread")]
    async fn pipeline_fixture_skips_cleanup_for_pure_snippet_fast_path() {
        let _guard = harness_test_lock().lock().expect("harness lock");
        reset();
        set_enabled(true);
        register_fixture(FixtureSpec {
            task: "transcription".into(),
            provider: "groq".into(),
            model: "whisper-large-v3-turbo".into(),
            response: Some("sig".into()),
            error_kind: None,
            error_message: None,
        });
        register_fixture(FixtureSpec {
            task: "cleanup".into(),
            provider: "groq".into(),
            model: "llama-3.3-70b-versatile".into(),
            response: Some("should not be called".into()),
            error_kind: None,
            error_message: None,
        });

        let mut request = base_request(base_config());
        request.snippets.push(PipelineTestSnippet {
            trigger: "sig".into(),
            expansion: "Best regards, Noah".into(),
            instructions: String::new(),
        });

        let result = run_pipeline_fixture(request)
            .await
            .expect("pure snippet fast path should succeed");
        assert_eq!(result.final_text_before_dictionary, "Best regards, Noah");
        assert_eq!(
            fixture_hit_count("cleanup", "groq", "llama-3.3-70b-versatile"),
            0
        );
        assert_eq!(take_injections().len(), 1);
        reset();
    }

    #[tokio::test(flavor = "current_thread")]
    async fn pipeline_fixture_applies_instruction_snippets_and_dictionary_last() {
        let _guard = harness_test_lock().lock().expect("harness lock");
        reset();
        set_enabled(true);
        register_fixture(FixtureSpec {
            task: "transcription".into(),
            provider: "groq".into(),
            model: "whisper-large-v3-turbo".into(),
            response: Some("say acme alert".into()),
            error_kind: None,
            error_message: None,
        });
        register_fixture(FixtureSpec {
            task: "cleanup".into(),
            provider: "groq".into(),
            model: "llama-3.3-70b-versatile".into(),
            response: Some("acme alert".into()),
            error_kind: None,
            error_message: None,
        });

        let mut request = base_request(base_config());
        request.snippets.push(PipelineTestSnippet {
            trigger: "alert".into(),
            expansion: "alert".into(),
            instructions: "all capitals".into(),
        });
        request.dictionary.push(PipelineTestDictionaryEntry {
            term: "OpenFlow".into(),
            mistake: Some("ACME".into()),
        });

        let result = run_pipeline_fixture(request)
            .await
            .expect("instruction and dictionary path should succeed");
        assert_eq!(result.final_text_before_dictionary, "ACME ALERT");
        assert_eq!(result.injected_text, "OpenFlow ALERT");
        reset();
    }

    #[tokio::test(flavor = "current_thread")]
    async fn pipeline_fixture_honors_formal_cleanup_even_with_none_intensity() {
        let _guard = harness_test_lock().lock().expect("harness lock");
        reset();
        set_enabled(true);
        let mut config = base_config();
        config.cleanup_intensity = "none".into();
        register_fixture(FixtureSpec {
            task: "transcription".into(),
            provider: "groq".into(),
            model: "whisper-large-v3-turbo".into(),
            response: Some("im sending the note".into()),
            error_kind: None,
            error_message: None,
        });
        register_fixture(FixtureSpec {
            task: "cleanup".into(),
            provider: "groq".into(),
            model: "llama-3.3-70b-versatile".into(),
            response: Some("I am sending the note.".into()),
            error_kind: None,
            error_message: None,
        });

        let mut request = base_request(config);
        request.profile = "formal".into();
        let result = run_pipeline_fixture(request)
            .await
            .expect("formal none intensity should still run cleanup");
        assert_eq!(
            result.final_text_before_dictionary,
            "I am sending the note."
        );
        assert_eq!(
            fixture_hit_count("cleanup", "groq", "llama-3.3-70b-versatile"),
            1
        );
        reset();
    }

    #[tokio::test(flavor = "current_thread")]
    async fn pipeline_fixture_uses_cleanup_cache_on_repeat_runs() {
        let _guard = harness_test_lock().lock().expect("harness lock");
        reset();
        set_enabled(true);
        register_fixture(FixtureSpec {
            task: "transcription".into(),
            provider: "groq".into(),
            model: "whisper-large-v3-turbo".into(),
            response: Some("cache me please".into()),
            error_kind: None,
            error_message: None,
        });
        register_fixture(FixtureSpec {
            task: "cleanup".into(),
            provider: "groq".into(),
            model: "llama-3.3-70b-versatile".into(),
            response: Some("cache me please".into()),
            error_kind: None,
            error_message: None,
        });

        let mut request = base_request(base_config());
        request.db = Some(crate::data::db::open(":memory:").expect("shared test db"));
        let first = run_pipeline_fixture(request.clone())
            .await
            .expect("first run should succeed");
        let second = run_pipeline_fixture(request)
            .await
            .expect("second run should succeed");
        assert!(!first.cleanup_cache_key.is_empty());
        assert_eq!(first.cleanup_cache_key, second.cleanup_cache_key);
        assert_eq!(
            fixture_hit_count("cleanup", "groq", "llama-3.3-70b-versatile"),
            1
        );
        reset();
    }
}

pub async fn retry_transcription_impl(
    app: &AppHandle,
    state: &SharedState,
) -> anyhow::Result<db::RecentEntry> {
    show_pill(app, "processing");
    let mut retry_expired = false;
    let capture = {
        let mut st = lock_state(state)?;
        match &st.retry_capture {
            Some(retry) => {
                if retry.captured_at.elapsed() > RETRY_WINDOW {
                    st.retry_capture = None;
                    retry_expired = true;
                    None
                } else {
                    Some(retry.clone())
                }
            }
            None => None,
        }
    };
    if retry_expired {
        show_error_pill(app, "Retry window expired").await;
        anyhow::bail!("Retry window expired");
    }
    let Some(capture) = capture else {
        hide_pill(app);
        anyhow::bail!("No retry available");
    };

    let settings_store = app.store("settings.json")?;
    let cfg = store::load_pipeline_config(&settings_store);

    if !has_transcription_key_in_chain(&cfg) {
        show_error_pill(app, "No API key configured").await;
        anyhow::bail!("No API key configured");
    }

    let Some((raw, api_used, final_text, dict_entries, cleanup_cache_key)) =
        run_transcription_and_cleanup(
            app,
            &capture.wav,
            &cfg,
            &capture.profile,
            capture.app_context.as_deref(),
        )
        .await
    else {
        anyhow::bail!("Retry processing failed");
    };

    finalize_pipeline_completion(
        app,
        state,
        PipelineCompletionContext {
            raw: &raw,
            final_text_before_dict: &final_text,
            dict_entries: &dict_entries,
            duration_ms: capture.duration_ms,
            api_used: &api_used,
            target_hwnd: capture.target_hwnd,
            cfg: &cfg,
            profile: &capture.profile,
            process_name: capture.process_name,
            cleanup_cache_key,
            captured_at: capture.captured_at,
        },
    )
    .await
}

struct PipelineCompletionContext<'a> {
    raw: &'a str,
    final_text_before_dict: &'a str,
    dict_entries: &'a [db::DictionaryEntry],
    duration_ms: u64,
    api_used: &'a str,
    target_hwnd: usize,
    cfg: &'a store::PipelineConfig,
    profile: &'a str,
    process_name: String,
    cleanup_cache_key: String,
    captured_at: std::time::Instant,
}

async fn finalize_pipeline_completion(
    app: &AppHandle,
    state: &SharedState,
    ctx: PipelineCompletionContext<'_>,
) -> anyhow::Result<db::RecentEntry> {
    let dict_stage = std::time::Instant::now();
    let (final_text_substituted, applied_dict_ids) =
        dictionary::apply_substitutions_from(ctx.final_text_before_dict, ctx.dict_entries);
    let dict_changed = !applied_dict_ids.is_empty();
    log::debug!(
        "pipeline: dictionary apply changed={} before_chars={} after_chars={} stage_ms={}",
        dict_changed,
        ctx.final_text_before_dict.chars().count(),
        final_text_substituted.chars().count(),
        dict_stage.elapsed().as_millis()
    );
    if dict_changed && crate::system::logger::is_verbose() {
        log::debug!(
            "pipeline: dictionary before_full=\"{}\"",
            ctx.final_text_before_dict
        );
        log::debug!(
            "pipeline: dictionary after_full=\"{}\"",
            final_text_substituted
        );
    }

    let db_handle = app.state::<DbHandle>();
    let words = ctx.raw.split_whitespace().count() as i64;
    let db_for_insert = db_handle.inner().clone();
    let raw_for_insert = ctx.raw.to_string();
    let clean_for_insert = ctx.final_text_before_dict.to_string();
    let api_used_for_insert = ctx.api_used.to_string();
    let duration_for_insert = ctx.duration_ms as i64;
    let entry = match tokio::task::spawn_blocking(move || {
        db::insert_transcription_returning(
            &db_for_insert,
            &raw_for_insert,
            &clean_for_insert,
            words,
            duration_for_insert,
            &api_used_for_insert,
        )
    })
    .await
    {
        Ok(Ok(entry)) => entry,
        Ok(Err(e)) => {
            show_error_pill(
                app,
                &format!("Failed to save transcription: {}", trim_err(&e.to_string())),
            )
            .await;
            return Err(e);
        }
        Err(e) => {
            show_error_pill(app, "Failed to save transcription: background task crashed").await;
            return Err(anyhow::anyhow!("insert_transcription task panicked: {e}"));
        }
    };

    if let Ok(mut st) = lock_state(state) {
        if st.retry_capture.as_ref().map(|v| v.captured_at) == Some(ctx.captured_at) {
            st.retry_capture = None;
        }
    }

    hide_pill(app);
    let inject_stage = std::time::Instant::now();

    // If Verenu itself has foreground focus, a Ctrl+V / Cmd+V paste would
    // land in our own WebView with no active text field and silently disappear.
    // Detect this by PID and fall back to clipboard-only so the user can paste manually.
    let self_inject = foreground_is_own_process() || hwnd_is_own_process(ctx.target_hwnd);

    let injected = if self_inject {
        log::info!("pipeline: self-inject detected — clipboard fallback");
        if let Err(e) = injection::copy_to_clipboard(&final_text_substituted).await {
            log::warn!("pipeline: clipboard fallback write failed: {e}");
        }
        app.emit("verenu:error", "Text copied — press Ctrl+V to paste").ok();
        injection::InjectionOutcome {
            text: final_text_substituted.clone(),
            context_state: "self_inject",
            case_decision: "clipboard_fallback",
            probe_source: "unavailable",
            selection_state: "unknown",
        }
    } else {
        match injection::inject_text(
            &final_text_substituted,
            ctx.target_hwnd,
            ctx.cfg.contextual_caps_enabled,
            ctx.cfg.auto_spacing_enabled,
            ctx.profile,
            ctx.cfg.macos_clipboard_sniff_enabled,
        )
        .await
        {
            Ok(outcome) => outcome,
            Err(e) => {
                log::error!("inject: {e}");
                show_error_pill(app, "Failed to paste - text saved to history").await;
                injection::InjectionOutcome {
                    text: final_text_substituted.clone(),
                    context_state: "unknown",
                    case_decision: "inject_failed",
                    probe_source: "unavailable",
                    selection_state: "unknown",
                }
            }
        }
    };
    let injected_text = injected.text;
    log::debug!(
        "pipeline: injection done contextual_caps={} auto_spacing={} context_state={} case_decision={} probe_source={} selection_state={} output_chars={} stage_ms={}",
        ctx.cfg.contextual_caps_enabled,
        ctx.cfg.auto_spacing_enabled,
        injected.context_state,
        injected.case_decision,
        injected.probe_source,
        injected.selection_state,
        injected_text.chars().count(),
        inject_stage.elapsed().as_millis()
    );
    app.emit("verenu:transcribed", &injected_text).ok();

    if !ctx.cleanup_cache_key.is_empty() {
        auto_learn::start_cache_rejection_monitor(
            injected_text.clone(),
            ctx.cleanup_cache_key,
            ctx.target_hwnd,
            db_handle.inner().clone(),
            app.clone(),
        );
    }
    if ctx.cfg.auto_learn_enabled {
        if !applied_dict_ids.is_empty() {
            auto_learn::start_rejection_monitor(
                injected_text.clone(),
                applied_dict_ids,
                ctx.target_hwnd,
                db_handle.inner().clone(),
                app.clone(),
            );
        }
        auto_learn::start_monitor(
            injected_text,
            ctx.process_name,
            db_handle.inner().clone(),
            app.clone(),
        );
    }

    Ok(entry)
}
