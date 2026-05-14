use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_store::StoreExt;

use crate::api::{auto_learn, cleanup, transcription};
use crate::core::{injection, window_context};
use crate::data::{db, dictionary, snippets, store};
use crate::media::audio;
use crate::system::apps::AppMapping;
use crate::DbHandle;

// ---------- shared state ----------

pub struct AppState {
    pub session: Option<audio::RecordingSession>,
    pub handless: bool,
    pub target_hwnd: usize,
}

pub type SharedState = Arc<Mutex<AppState>>;

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

        if let Ok(Some(m)) = pill.primary_monitor() {
            let sz = m.size();
            let sf = m.scale_factor();
            let x = ((sz.width as f64 / sf - 140.0) / 2.0 * sf) as i32;
            let y = ((sz.height as f64 / sf - 44.0 - 64.0) * sf) as i32;
            pill.set_position(tauri::PhysicalPosition::new(x, y)).ok();
        }

        pill.emit("pill-state", state).ok();
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
    let settings = app.store("settings.json").ok();
    let device = settings
        .as_deref()
        .and_then(|s| s.get(store::MICROPHONE_DEVICE))
        .and_then(|v| v.as_str().map(String::from));
    let noise_reduction = settings
        .as_deref()
        .and_then(|s| s.get(store::NOISE_REDUCTION))
        .and_then(|v| v.as_bool())
        .unwrap_or(true);
    let mute_audio = settings
        .as_deref()
        .and_then(|s| s.get(store::MUTE_AUDIO))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let mic_gain = settings
        .as_deref()
        .and_then(|s| s.get(store::MIC_GAIN))
        .and_then(|v| v.as_f64())
        .map(|v| v as f32)
        .unwrap_or(3.5)
        .clamp(1.0, 8.0);

    if mute_audio {
        std::thread::spawn(|| crate::system::volume::mute());
    }

    match audio::RecordingSession::start(device, noise_reduction, mic_gain) {
        Ok(session) => {
            let level_arc = session.level.clone();
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
        // Give WebView2 a brief head start to wake up and process the
        // "recording" state event before we flood the IPC with 16ms updates.
        tokio::time::sleep(std::time::Duration::from_millis(80)).await;

        loop {
            if !active.load(Ordering::Relaxed) {
                break;
            }
            let level_val = f32::from_bits(level.load(Ordering::Relaxed));
            if let Some(pill) = app.get_webview_window("pill") {
                pill.emit("audio-level", level_val).ok();
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }

        // Emit final reset to ensure level goes to 0 regardless of timing
        if let Some(pill) = app.get_webview_window("pill") {
            pill.emit("audio-level", 0.0).ok();
        }
    });
}

// ---------- pipeline ----------

fn is_quota_error(e: &anyhow::Error) -> bool {
    e.to_string().starts_with("QUOTA_EXCEEDED:")
}

fn fallback_providers(primary: &str) -> &'static [&'static str] {
    match primary {
        "groq"   => &["openai", "google"],
        "openai" => &["groq",   "google"],
        "google" => &["groq",   "openai"],
        _        => &["openai", "google"],
    }
}

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
    // Callers reset `handless` before spawning; take the session and the HWND
    // that was captured at recording-start time.
    let (session, target_hwnd) = {
        let mut st = state.lock().unwrap();
        (st.session.take(), st.target_hwnd)
    };
    let Some(session) = session else {
        log::debug!("pipeline: no session — recording never started or was already consumed");
        return;
    };

    // Capture the foreground process name NOW, before any await points.
    // After transcription/cleanup the foreground window may have changed to a
    // different app, giving us the wrong profile and app-context hint.
    let process_name = window_context::get_active_process_name()
        .unwrap_or_else(|| "unknown".into())
        .to_lowercase();

    // Attempt to unmute immediately since the dictation phase has formally ended
    std::thread::spawn(|| crate::system::volume::unmute());

    show_pill(&app, "processing");

    // session.stop() blocks on std::sync::mpsc::recv() until the audio thread
    // finishes processing (denoise + resample + WAV encode). Use spawn_blocking
    // so the tokio worker thread stays free for other tasks during that wait.
    let stop_result = tokio::task::spawn_blocking(move || session.stop()).await;
    let (wav, duration_ms, rms) = match stop_result {
        Ok(Ok(v)) => v,
        Ok(Err(e)) => {
            log::error!("audio stop: {e}");
            hide_pill(&app);
            return;
        }
        Err(e) => {
            log::error!("audio stop task panicked: {e}");
            hide_pill(&app);
            return;
        }
    };
    // Reject recordings that are too short or too quiet to contain real speech.
    // Short taps hallucinate words; near-silence recordings do the same.
    if duration_ms < 700 || rms < 0.008 {
        log::debug!("pipeline: rejected — duration={duration_ms}ms rms={rms:.4}");
        hide_pill(&app);
        return;
    }

    let cfg = match app.store("settings.json") {
        Ok(s) => store::load_pipeline_config(&s),
        Err(e) => {
            log::error!("store: {e}");
            hide_pill(&app);
            return;
        }
    };

    let t_key = cfg.key_for(&cfg.transcription_provider).to_owned();
    if t_key.is_empty() {
        show_error_pill(
            &app,
            &format!("No API key saved for {}", cfg.transcription_provider),
        )
        .await;
        return;
    }

    // Resolve profile: user-defined app mapping → default tone.
    let settings_store = app.store("settings.json").ok();
    let profile = resolve_profile(settings_store.as_deref(), &process_name, &cfg.default_tone);

    let app_context: Option<String> = if cfg.app_context_hint {
        window_context::get_app_context_hint(&process_name)
    } else {
        None
    };

    let c_key = cfg.key_for(&cfg.cleanup_provider).to_owned();

    // Build provider order for transcription: primary first, then fallbacks if enabled.
    let mut t_providers: Vec<&str> = vec![cfg.transcription_provider.as_str()];
    if cfg.api_fallback_enabled {
        for fb in fallback_providers(&cfg.transcription_provider) {
            if !cfg.key_for(fb).is_empty() {
                t_providers.push(fb);
            }
        }
    }

    let mut raw_result: Option<String> = None;
    let mut t_last_err: Option<anyhow::Error> = None;
    let mut used_t_provider = cfg.transcription_provider.as_str();
    for provider_id in &t_providers {
        let key = cfg.key_for(provider_id);
        if key.is_empty() { continue; }
        let provider = match *provider_id {
            "openai" => transcription::Provider::OpenAI,
            "google" => transcription::Provider::Google,
            _ => transcription::Provider::Groq,
        };
        match transcription::transcribe(wav.clone(), provider, key).await {
            Ok(t) => {
                used_t_provider = provider_id;
                raw_result = Some(t);
                break;
            }
            Err(e) => {
                if cfg.api_fallback_enabled && is_quota_error(&e) {
                    log::warn!("transcription quota on {provider_id}, trying fallback");
                    t_last_err = Some(e);
                } else {
                    show_error_pill(&app, &trim_err(&e.to_string())).await;
                    return;
                }
            }
        }
    }

    let raw = match raw_result {
        Some(t) => t,
        None => {
            let msg = t_last_err.map(|e| trim_err(&e.to_string())).unwrap_or_else(|| "Transcription failed".into());
            show_error_pill(&app, &msg).await;
            return;
        }
    };

    let api_used = format!("{}/transcription", used_t_provider);
    if raw.is_empty() {
        show_error_pill(&app, "Nothing transcribed — please try speaking more clearly").await;
        return;
    }

    // Snippet expansion happens BEFORE cleanup so the cleanup model can apply
    // the snippet's instructions (uppercase, no period, etc.) to the expansion
    // text itself. Otherwise the literal expansion would overwrite any work
    // the model did on the trigger area, making instructions feel ignored.
    let db = app.state::<DbHandle>();
    let snippet_instructions = snippets::collect_snippet_instructions(&raw, &db);

    // Fast path: the entire transcription was just a snippet trigger.
    // try_pure_snippet_expand strips trailing punctuation the transcription model
    // added (e.g. "roblox." → matches trigger "roblox") and returns the expansion
    // directly, so no orphaned period can bleed through into the final text.
    // If the snippet has cleanup instructions, fall through to the normal path so
    // the LLM can apply them.
    let pure_expansion = if snippet_instructions.is_empty() {
        snippets::try_pure_snippet_expand(&raw, &db)
    } else {
        None
    };

    let expanded = pure_expansion
        .clone()
        .unwrap_or_else(|| snippets::expand_snippets(&raw, &db));

    let final_text = if cfg.cleanup_enabled && !c_key.is_empty() && pure_expansion.is_none() && cfg.cleanup_intensity != "none" {
        let dict_instructions = dictionary::build_dictionary_prompt(&db);
        let extra_rules = [snippet_instructions.as_str(), dict_instructions.as_str()]
            .iter()
            .filter(|s| !s.is_empty())
            .copied()
            .collect::<Vec<_>>()
            .join("\n\n");

        // Build provider order for cleanup: primary first, then fallbacks if enabled.
        let mut c_providers: Vec<&str> = vec![cfg.cleanup_provider.as_str()];
        if cfg.api_fallback_enabled {
            for fb in fallback_providers(&cfg.cleanup_provider) {
                if !cfg.key_for(fb).is_empty() {
                    c_providers.push(fb);
                }
            }
        }

        let mut cleaned_result: Option<String> = None;
        let mut c_last_err: Option<anyhow::Error> = None;
        for provider_id in &c_providers {
            let key = cfg.key_for(provider_id);
            if key.is_empty() { continue; }
            let cp = match *provider_id {
                "openai" => cleanup::CleanupProvider::OpenAI,
                "google" => cleanup::CleanupProvider::Google,
                _ => cleanup::CleanupProvider::Groq,
            };
            match cleanup::cleanup(&expanded, cp, key, &profile, &cfg.cleanup_intensity, &extra_rules, app_context.as_deref()).await {
                Ok(t) => { cleaned_result = Some(t); break; }
                Err(e) => {
                    if cfg.api_fallback_enabled && is_quota_error(&e) {
                        log::warn!("cleanup quota on {provider_id}, trying fallback");
                        c_last_err = Some(e);
                    } else {
                        show_error_pill(&app, &format!("Cleanup failed: {}", trim_err(&e.to_string()))).await;
                        return;
                    }
                }
            }
        }

        match cleaned_result {
            Some(cleaned) if !cleaned.is_empty() => {
                snippets::apply_cleanup_instruction_overrides(&cleaned, &snippet_instructions)
            }
            Some(_) => {
                snippets::apply_cleanup_instruction_overrides(&expanded, &snippet_instructions)
            }
            None => {
                if let Some(e) = c_last_err {
                    show_error_pill(&app, &format!("Cleanup failed: {}", trim_err(&e.to_string()))).await;
                    return;
                }
                snippets::apply_cleanup_instruction_overrides(&expanded, &snippet_instructions)
            }
        }
    } else {
        snippets::apply_cleanup_instruction_overrides(&expanded, &snippet_instructions)
    };

    let words = final_text.split_whitespace().count() as i64;
    if let Err(e) = db::insert_transcription(&db, &raw, &final_text, words, duration_ms as i64, &api_used) {
        show_error_pill(&app, &format!("Failed to save transcription: {}", trim_err(&e.to_string()))).await;
        return;
    }

    let final_text = dictionary::apply_substitutions(&final_text, &db);

    hide_pill(&app);
    if let Err(e) = injection::inject_text(&final_text, target_hwnd).await {
        log::error!("inject: {e}");
        show_error_pill(&app, "Failed to paste — text saved to history").await;
    }
    app.emit("open-flow:transcribed", &final_text).ok();

    if cfg.auto_learn_enabled {
        auto_learn::start_monitor(final_text.clone(), db.inner().clone(), app.clone());
    }
}
