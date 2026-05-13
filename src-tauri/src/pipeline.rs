use std::sync::{Arc, Mutex};
use std::sync::atomic::Ordering;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_store::StoreExt;

use crate::api::{cleanup, prompts, transcription};
use crate::core::{injection, window_context};
use crate::data::{db, store};
use crate::media::audio;
use crate::system::apps::AppMapping;
use crate::DbHandle;

// ---------- shared state ----------

pub struct AppState {
    pub session: Option<audio::RecordingSession>,
    pub handless: bool,
}

pub type SharedState = Arc<Mutex<AppState>>;

// ---------- pill helpers ----------

fn create_pill_if_needed(app: &AppHandle) {
    if app.get_webview_window("pill").is_some() { return; }
    let _ = tauri::WebviewWindowBuilder::new(
            app, "pill",
            tauri::WebviewUrl::App("/pill.html".into()))
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

        pill.emit("pill-state", state).ok();
        // SW_SHOWNOACTIVATE: pill appears without stealing keyboard focus from
        // whatever window the user is dictating into.
        #[cfg(target_os = "windows")]
        {
            use windows::Win32::UI::WindowsAndMessaging::{ShowWindow, SW_SHOWNOACTIVATE};
            if let Ok(hwnd) = pill.hwnd() {
                unsafe { let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE); }
            }
        }
        #[cfg(not(target_os = "windows"))]
        pill.show().ok();

        if let Ok(Some(m)) = pill.primary_monitor() {
            let sz = m.size();
            let sf = m.scale_factor();
            let x = ((sz.width  as f64 / sf - 140.0) / 2.0 * sf) as i32;
            let y = ((sz.height as f64 / sf - 44.0 - 64.0) * sf) as i32;
            pill.set_position(tauri::PhysicalPosition::new(x, y)).ok();
        }
    }
}

pub fn hide_pill(app: &AppHandle) {
    if let Some(pill) = app.get_webview_window("pill") {
        pill.emit("pill-state", "idle").ok();
        pill.hide().ok();
    }
}

// ---------- recording session helpers ----------

/// Starts a new recording session, stores it in shared state, shows the pill,
/// and spawns the audio-level emitter task.
pub fn start_recording_session(app: &AppHandle, state: &SharedState, pill_state: &str, handless: bool) {
    let settings = app.store("settings.json").ok();
    let device = settings.as_deref()
        .and_then(|s| s.get(store::MICROPHONE_DEVICE))
        .and_then(|v| v.as_str().map(String::from));
    let noise_reduction = settings.as_deref()
        .and_then(|s| s.get(store::NOISE_REDUCTION))
        .and_then(|v| v.as_bool())
        .unwrap_or(true);

    match audio::RecordingSession::start(device, noise_reduction) {
        Ok(session) => {
            let level_arc  = session.level.clone();
            let active_arc = session.active.clone();
            {
                let mut st = state.lock().unwrap();
                st.session = Some(session);
                st.handless = handless;
            }
            show_pill(app, pill_state);
            spawn_level_emitter(app.clone(), level_arc, active_arc);
        }
        Err(e) => log::error!("start recording: {e}"),
    }
}

/// Spawns a Tokio task that emits `audio-level` events to the pill every 50ms
/// until the recording's `active` flag goes false.
pub fn spawn_level_emitter(
    app: AppHandle,
    level: Arc<std::sync::atomic::AtomicU32>,
    active: Arc<std::sync::atomic::AtomicBool>,
) {
    tauri::async_runtime::spawn(async move {
        loop {
            if !active.load(Ordering::Relaxed) { break; }
            let level_val = f32::from_bits(level.load(Ordering::Relaxed));
            if let Some(pill) = app.get_webview_window("pill") {
                pill.emit("audio-level", level_val).ok();
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
    });
}

// ---------- pipeline ----------

fn trim_err(s: &str) -> String {
    let s = s.trim();
    if s.chars().count() > 120 {
        format!("{}…", s.chars().take(117).collect::<String>())
    } else {
        s.to_string()
    }
}

async fn show_error_pill(app: &AppHandle, msg: &str) {
    log::error!("pipeline error: {msg}");
    app.emit("open-flow:error", msg).ok();
    show_pill(app, "error");
    if let Some(w) = app.get_webview_window("main") {
        w.show().ok();
        w.set_focus().ok();
    }
    tokio::time::sleep(std::time::Duration::from_secs(4)).await;
    hide_pill(app);
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

pub async fn run_pipeline(app: AppHandle, state: SharedState) {
    // Callers reset `handless` before spawning; take the session here.
    let session = {
        let mut st = state.lock().unwrap();
        st.session.take()
    };
    let Some(session) = session else { return };

    show_pill(&app, "processing");

    let (wav, duration_ms, rms) = match session.stop() {
        Ok(v) => v,
        Err(e) => { log::error!("audio stop: {e}"); hide_pill(&app); return; }
    };
    // Reject recordings that are too short or too quiet to contain real speech.
    // Short taps hallucinate words; near-silence recordings do the same.
    if duration_ms < 700 || rms < 0.008 { hide_pill(&app); return; }

    let cfg = match app.store("settings.json") {
        Ok(s) => store::load_pipeline_config(&s),
        Err(e) => { log::error!("store: {e}"); hide_pill(&app); return; }
    };

    let t_key = cfg.key_for(&cfg.transcription_provider).to_owned();
    if t_key.is_empty() {
        show_error_pill(&app, &format!("No API key saved for {}", cfg.transcription_provider)).await;
        return;
    }

    let t_provider = match cfg.transcription_provider.as_str() {
        "openai" => transcription::Provider::OpenAI,
        "google" => transcription::Provider::Google,
        _        => transcription::Provider::Groq,
    };

    let process_name = window_context::get_active_process_name()
        .unwrap_or_else(|| "unknown".into())
        .to_lowercase();

    // Resolve profile: user-defined app mapping → default tone.
    let settings_store = app.store("settings.json").ok();
    let profile = resolve_profile(settings_store.as_deref(), &process_name, &cfg.default_tone);

    let c_key = cfg.key_for(&cfg.cleanup_provider).to_owned();
    let api_used = format!("{}/transcription", cfg.transcription_provider);

    // When both providers are Google and cleanup is on, fuse into a single Gemini call.
    let (raw, final_text) = if cfg.transcription_provider == "google"
        && cfg.cleanup_provider == "google"
        && cfg.cleanup_enabled
        && !c_key.is_empty()
    {
        let profile_prompt = prompts::get_system_prompt(&profile, &cfg.cleanup_intensity);
        match transcription::transcribe_and_cleanup_gemini(wav, &t_key, &profile_prompt).await {
            Ok(text) => {
                if text.is_empty() { hide_pill(&app); return; }
                (text.clone(), text)
            }
            Err(e) => {
                show_error_pill(&app, &trim_err(&e.to_string())).await;
                return;
            }
        }
    } else {
        let raw = match transcription::transcribe(wav, t_provider, &t_key).await {
            Ok(t) => t,
            Err(e) => {
                show_error_pill(&app, &trim_err(&e.to_string())).await;
                return;
            }
        };
        if raw.is_empty() { hide_pill(&app); return; }

        let final_text = if cfg.cleanup_enabled && !c_key.is_empty() {
            let cp = match cfg.cleanup_provider.as_str() {
                "openai" => cleanup::CleanupProvider::OpenAI,
                "google" => cleanup::CleanupProvider::Google,
                _        => cleanup::CleanupProvider::Groq,
            };
            cleanup::cleanup(&raw, cp, &c_key, &profile, &cfg.cleanup_intensity)
                .await
                .unwrap_or(raw.clone())
        } else {
            raw.clone()
        };

        (raw, final_text)
    };

    let words = final_text.split_whitespace().count() as i64;
    let db = app.state::<DbHandle>();
    let _ = db::insert_transcription(&db, &raw, &final_text, words, duration_ms as i64, &api_used);

    hide_pill(&app);
    if let Err(e) = injection::inject_text(&final_text).await {
        log::error!("inject: {e}");
    }
    app.emit("open-flow:transcribed", &final_text).ok();
}
