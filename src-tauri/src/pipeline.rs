use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex, MutexGuard};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_store::StoreExt;

use chrono::{DateTime, Duration, NaiveDateTime, Utc};
use crate::api::{auto_learn, cleanup, transcription};
use crate::api::prompts::{AppContextMode, PromptProfile};
use crate::core::{injection, window_context};
use crate::data::{db, dictionary, snippets, store};
use crate::media::audio;
use crate::system::apps::AppMapping;
use crate::DbHandle;

const MIN_RECORDING_MS: u64 = 700;
const MIN_RECORDING_RMS: f32 = 0.008;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CleanupPath {
    LlmMinimalUnder50,
    LlmStandard,
    SkippedNone,
    PureSnippet,
    Disabled,
}

impl CleanupPath {
    fn as_str(self) -> &'static str {
        match self {
            CleanupPath::LlmMinimalUnder50 => "llm_minimal_under_50",
            CleanupPath::LlmStandard => "llm_standard",
            CleanupPath::SkippedNone => "skipped_none",
            CleanupPath::PureSnippet => "pure_snippet",
            CleanupPath::Disabled => "disabled",
        }
    }
}

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
    pub handless: bool,
    pub target_hwnd: usize,
}

pub type SharedState = Arc<Mutex<AppState>>;

fn lock_state(state: &SharedState) -> anyhow::Result<MutexGuard<'_, AppState>> {
    state
        .lock()
        .map_err(|_| anyhow::anyhow!("Recording state lock was poisoned"))
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
        std::thread::spawn(crate::system::volume::mute);
    }

    match audio::RecordingSession::start(device, noise_reduction, mic_gain) {
        Ok(session) => {
            let level_arc = session.level.clone();
            let active_arc = session.active.clone();
            {
                let mut st = match lock_state(state) {
                    Ok(st) => st,
                    Err(e) => {
                        log::error!("recording state: {e}");
                        return;
                    }
                };
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

fn fallback_providers(primary: &str) -> &'static [&'static str] {
    match primary {
        "groq" => &["openai", "google"],
        "openai" => &["groq", "google"],
        "google" => &["groq", "openai"],
        _ => &["openai", "google"],
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

fn estimate_tokens(text: &str) -> u32 {
    let words = text.split_whitespace().count() as f32;
    (words * 1.3).floor() as u32
}

fn select_prompt_profile(estimated_tokens: u32) -> PromptProfile {
    if estimated_tokens < 50 {
        PromptProfile::Minimal
    } else {
        PromptProfile::Standard
    }
}

fn select_app_context_mode(
    app_context_enabled: bool,
    app_context_available: bool,
    estimated_tokens: u32,
) -> AppContextMode {
    if !app_context_enabled || !app_context_available {
        return AppContextMode::None;
    }
    if estimated_tokens >= 100 {
        AppContextMode::Full4Row
    } else {
        AppContextMode::Compact
    }
}

fn cleanup_path_for(
    cleanup_enabled: bool,
    has_cleanup_key: bool,
    has_pure_expansion: bool,
    cleanup_intensity: &str,
    estimated_tokens: u32,
) -> CleanupPath {
    if has_pure_expansion {
        return CleanupPath::PureSnippet;
    }
    if !cleanup_enabled || !has_cleanup_key {
        return CleanupPath::Disabled;
    }
    if cleanup_intensity == "none" {
        return CleanupPath::SkippedNone;
    }
    if estimated_tokens < 50 {
        CleanupPath::LlmMinimalUnder50
    } else {
        CleanupPath::LlmStandard
    }
}

fn normalize_cleanup_cache_key(input: &str) -> String {
    input
        .chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(|c| c.to_lowercase())
        .collect()
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
        now + Duration::days(365)
    } else if hit_count >= 5 && age <= Duration::days(30) {
        now + Duration::days(30)
    } else if hit_count >= 2 && age <= Duration::days(14) {
        now + Duration::days(30)
    } else if hit_count >= 2 && age <= Duration::days(7) {
        now + Duration::days(7)
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

    let stop_result = tokio::task::spawn_blocking(move || session.stop()).await?;
    let (wav, duration_ms, rms) = stop_result?;

    if duration_ms < MIN_RECORDING_MS || rms < MIN_RECORDING_RMS {
        hide_pill(&app);
        anyhow::bail!("Recording too short");
    }
    let wav = bytes::Bytes::from(wav);

    let settings_store = match app.store("settings.json") {
        Ok(s) => s,
        Err(e) => {
            hide_pill(&app);
            return Err(anyhow::anyhow!(e.to_string()));
        }
    };
    let cfg = store::load_pipeline_config(&settings_store);

    let t_key = cfg.key_for(&cfg.transcription_provider).to_owned();
    if t_key.is_empty() {
        hide_pill(&app);
        anyhow::bail!("No API key saved for {}", cfg.transcription_provider);
    }

    let result = try_providers(&[&cfg.transcription_provider], &cfg, |provider_id, key| {
        let w = wav.clone();
        let provider = transcription_provider_from_str(provider_id);
        let language = cfg.transcription_language.clone();
        Box::pin(async move { transcription::transcribe(w, provider, &key, &language).await })
    })
    .await;

    hide_pill(&app);

    match result {
        Ok((text, _)) if !text.is_empty() => Ok(text),
        Ok(_) => anyhow::bail!("Nothing transcribed"),
        Err(Some(e)) => Err(e),
        Err(None) => anyhow::bail!("Transcription failed"),
    }
}

pub async fn run_pipeline(app: AppHandle, state: SharedState) {
    let Some((session, target_hwnd)) = take_pipeline_session(&state) else {
        log::debug!("pipeline: no session — recording never started or was already consumed");
        return;
    };

    // Capture process name before any await points — foreground window may
    // change to a different app during async transcription/cleanup.
    let process_name = window_context::get_active_process_name()
        .unwrap_or_else(|| "unknown".into())
        .to_lowercase();

    std::thread::spawn(crate::system::volume::unmute);
    show_pill(&app, "processing");

    let Some((wav, duration_ms)) = stop_and_validate_audio(&app, session).await else {
        return;
    };
    let Some((cfg, profile, app_context)) = open_config_and_context(&app, &process_name).await
    else {
        return;
    };
    let Some((raw, api_used)) = run_transcription(&app, &wav, &cfg).await else {
        return;
    };
    let Some((final_text, dict_entries)) =
        run_cleanup_and_snippets(&app, &raw, &cfg, &profile, app_context.as_deref()).await
    else {
        return;
    };

    let db = app.state::<DbHandle>();
    let words = final_text.split_whitespace().count() as i64;
    if let Err(e) =
        db::insert_transcription(&db, &raw, &final_text, words, duration_ms as i64, &api_used)
    {
        show_error_pill(
            &app,
            &format!("Failed to save transcription: {}", trim_err(&e.to_string())),
        )
        .await;
        return;
    }

    let final_text = dictionary::apply_substitutions_from(&final_text, &dict_entries);

    hide_pill(&app);
    let injected_text =
        match injection::inject_text(&final_text, target_hwnd, cfg.contextual_caps_enabled).await {
            Ok(text) => text,
            Err(e) => {
                log::error!("inject: {e}");
                show_error_pill(&app, "Failed to paste — text saved to history").await;
                final_text.clone()
            }
        };
    app.emit("open-flow:transcribed", &injected_text).ok();

    if cfg.auto_learn_enabled {
        auto_learn::start_monitor(injected_text, db.inner().clone(), app.clone());
    }
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
    if duration_ms < MIN_RECORDING_MS || rms < MIN_RECORDING_RMS {
        log::debug!("pipeline: rejected — duration={duration_ms}ms rms={rms:.4}");
        hide_pill(app);
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
    if cfg.key_for(&cfg.transcription_provider).is_empty() {
        show_error_pill(
            app,
            &format!("No API key saved for {}", cfg.transcription_provider),
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
    match try_providers(&[&cfg.transcription_provider], cfg, |provider_id, key| {
        let w = wav.clone();
        let provider = transcription_provider_from_str(provider_id);
        let language = cfg.transcription_language.clone();
        Box::pin(async move { transcription::transcribe(w, provider, &key, &language).await })
    })
    .await
    {
        Ok((raw, t_provider)) if !raw.is_empty() => {
            Some((raw, format!("{t_provider}/transcription")))
        }
        Ok(_) => {
            show_error_pill(
                app,
                "Nothing transcribed — please try speaking more clearly",
            )
            .await;
            None
        }
        Err(Some(e)) => {
            show_error_pill(app, &trim_err(&e.to_string())).await;
            None
        }
        Err(None) => {
            show_error_pill(app, "Transcription failed").await;
            None
        }
    }
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
) -> Option<(String, Vec<db::DictionaryEntry>)> {
    let db = app.state::<DbHandle>();
    let mut db_snippets = db::query_snippets(&db).unwrap_or_default();
    let dict_entries = db::query_dictionary(&db).unwrap_or_default();

    let snippet_instructions = snippets::collect_snippet_instructions_from(raw, &db_snippets);

    // Fast path: entire transcription was a single snippet trigger — skip the LLM.
    let pure_expansion = if snippet_instructions.is_empty() {
        snippets::try_pure_snippet_expand_from(raw, &db_snippets, &db)
    } else {
        None
    };
    let expanded = pure_expansion
        .clone()
        .unwrap_or_else(|| snippets::expand_snippets_from(raw, &mut db_snippets, &db));
    let estimated_tokens = estimate_tokens(&expanded);

    let c_key = cfg.key_for(&cfg.cleanup_provider).to_owned();
    let app_context_mode = select_app_context_mode(
        cfg.app_context_hint,
        app_context.is_some(),
        estimated_tokens,
    );
    let cleanup_path = cleanup_path_for(
        cfg.cleanup_enabled,
        !c_key.is_empty(),
        pure_expansion.is_some(),
        &cfg.cleanup_intensity,
        estimated_tokens,
    );
    let prompt_profile = select_prompt_profile(estimated_tokens);

    let app_context_mode_label = match app_context_mode {
        AppContextMode::None => "none",
        AppContextMode::Compact => "compact",
        AppContextMode::Full4Row => "full_4row",
    };
    log::info!(
        "cleanup_routing estimated_tokens={} cleanup_path={} app_context_setting_enabled={} app_context_available={} app_context_sent={} app_context_mode={}",
        estimated_tokens,
        cleanup_path.as_str(),
        cfg.app_context_hint,
        app_context.is_some(),
        app_context_mode != AppContextMode::None,
        app_context_mode_label
    );

    let final_text = if matches!(
        cleanup_path,
        CleanupPath::LlmMinimalUnder50 | CleanupPath::LlmStandard
    ) {
        let cache_key = normalize_cleanup_cache_key(raw);
        if !cache_key.is_empty() {
            if let Ok(Some(entry)) = db::cleanup_cache_get_active(&db, &cache_key) {
                let now = Utc::now();
                let new_hit_count = entry.hit_count + 1;
                let now_str = now.format("%Y-%m-%d %H:%M:%S").to_string();
                let new_expires_at =
                    next_cache_expiry(new_hit_count, &entry.created_at, &entry.expires_at, now);
                let _ = db::cleanup_cache_touch_hit(
                    &db,
                    &cache_key,
                    new_hit_count,
                    &now_str,
                    &new_expires_at,
                );
                let overridden =
                    snippets::apply_cleanup_instruction_overrides(&entry.clean_text, &snippet_instructions);
                return Some((overridden, dict_entries));
            }
        }

        let dict_instructions =
            dictionary::build_relevant_dictionary_prompt_from(&dict_entries, raw);
        let extra_rules = [snippet_instructions.as_str(), dict_instructions.as_str()]
            .iter()
            .filter(|s| !s.is_empty())
            .copied()
            .collect::<Vec<_>>()
            .join("\n\n");

        let cleaned_res = try_providers(&[&cfg.cleanup_provider], cfg, |provider_id, key| {
            let cp = cleanup_provider_from_str(provider_id);
            let expanded_ref = expanded.clone();
            let profile_ref = profile.to_owned();
            let intensity_ref = cfg.cleanup_intensity.clone();
            let rules_ref = extra_rules.clone();
            let ctx_ref = app_context.map(|s| s.to_owned());
            Box::pin(async move {
                cleanup::cleanup(
                    &expanded_ref,
                    cp,
                    &key,
                    &profile_ref,
                    &intensity_ref,
                    &rules_ref,
                    ctx_ref.as_deref(),
                    app_context_mode,
                    prompt_profile,
                )
                .await
            })
        })
        .await;

        match cleaned_res {
            Ok((cleaned, _)) if !cleaned.is_empty() => {
                let overridden =
                    snippets::apply_cleanup_instruction_overrides(&cleaned, &snippet_instructions);
                let cache_key = normalize_cleanup_cache_key(raw);
                if !cache_key.is_empty() {
                    let _ = db::cleanup_cache_insert_new(&db, &cache_key, &cleaned, &sqlite_utc_plus(7));
                }
                overridden
            }
            Ok(_) => {
                snippets::apply_cleanup_instruction_overrides(&expanded, &snippet_instructions)
            }
            Err(Some(e)) => {
                show_error_pill(
                    app,
                    &format!("Cleanup failed: {}", trim_err(&e.to_string())),
                )
                .await;
                return None;
            }
            Err(None) => {
                snippets::apply_cleanup_instruction_overrides(&expanded, &snippet_instructions)
            }
        }
    } else {
        snippets::apply_cleanup_instruction_overrides(&expanded, &snippet_instructions)
    };

    Some((final_text, dict_entries))
}

use std::future::Future;
use std::pin::Pin;

async fn try_providers<F>(
    providers: &[&str],
    cfg: &store::PipelineConfig,
    mut call: F,
) -> Result<(String, String), Option<anyhow::Error>>
where
    F: FnMut(&str, String) -> Pin<Box<dyn Future<Output = anyhow::Result<String>> + Send>>,
{
    let mut to_try = vec![providers[0]];
    if cfg.api_fallback_enabled {
        for fb in fallback_providers(providers[0]) {
            if !cfg.key_for(fb).is_empty() {
                to_try.push(fb);
            }
        }
    }

    let mut last_err = None;
    for provider_id in to_try {
        let key = cfg.key_for(provider_id).to_owned();
        if key.is_empty() {
            continue;
        }

        match call(provider_id, key).await {
            Ok(result) => return Ok((result, provider_id.to_string())),
            Err(e) => {
                if cfg.api_fallback_enabled && crate::api::is_retryable_provider_error(&e) {
                    log::warn!("retryable provider error on {provider_id}, trying fallback: {e}");
                    last_err = Some(e);
                } else {
                    return Err(Some(e));
                }
            }
        }
    }
    Err(last_err)
}

#[cfg(test)]
mod tests {
    use super::{
        cleanup_path_for, estimate_tokens, next_cache_expiry, normalize_cleanup_cache_key,
        select_app_context_mode, select_prompt_profile, CleanupPath,
    };
    use crate::api::prompts::{AppContextMode, PromptProfile};
    use chrono::{DateTime, Utc};

    #[test]
    fn token_estimator_uses_word_count_times_point_13() {
        assert_eq!(estimate_tokens("one two three"), 3);
        assert_eq!(estimate_tokens("one two three four five"), 6);
    }

    #[test]
    fn prompt_profile_thresholds_work() {
        assert_eq!(select_prompt_profile(49), PromptProfile::Minimal);
        assert_eq!(select_prompt_profile(50), PromptProfile::Standard);
        assert_eq!(select_prompt_profile(100), PromptProfile::Standard);
    }

    #[test]
    fn app_context_mode_thresholds_work() {
        assert_eq!(
            select_app_context_mode(false, true, 120),
            AppContextMode::None
        );
        assert_eq!(
            select_app_context_mode(true, false, 120),
            AppContextMode::None
        );
        assert_eq!(
            select_app_context_mode(true, true, 80),
            AppContextMode::Compact
        );
        assert_eq!(
            select_app_context_mode(true, true, 100),
            AppContextMode::Full4Row
        );
    }

    #[test]
    fn cleanup_path_selection_matches_contract() {
        assert_eq!(
            cleanup_path_for(true, true, true, "medium", 10),
            CleanupPath::PureSnippet
        );
        assert_eq!(
            cleanup_path_for(false, true, false, "medium", 10),
            CleanupPath::Disabled
        );
        assert_eq!(
            cleanup_path_for(true, false, false, "medium", 10),
            CleanupPath::Disabled
        );
        assert_eq!(
            cleanup_path_for(true, true, false, "none", 10),
            CleanupPath::SkippedNone
        );
        assert_eq!(
            cleanup_path_for(true, true, false, "medium", 49),
            CleanupPath::LlmMinimalUnder50
        );
        assert_eq!(
            cleanup_path_for(true, true, false, "medium", 50),
            CleanupPath::LlmStandard
        );
    }

    #[test]
    fn cleanup_cache_key_normalization_works() {
        assert_eq!(
            normalize_cleanup_cache_key("Okay, great. It looks amazing, honestly."),
            "okaygreatitlooksamazinghonestly"
        );
        assert_eq!(normalize_cleanup_cache_key(" \n\t!!! "), "");
    }

    #[test]
    fn cache_ttl_promotion_matches_contract() {
        let now = DateTime::parse_from_rfc3339("2026-01-15T00:00:00Z")
            .expect("now")
            .with_timezone(&Utc);
        assert_eq!(
            next_cache_expiry(5, "2026-01-01 00:00:00", "2026-01-20 00:00:00", now),
            "2027-01-15 00:00:00"
        );
        assert_eq!(
            next_cache_expiry(2, "2026-01-05 00:00:00", "2026-01-20 00:00:00", now),
            "2026-02-14 00:00:00"
        );
        assert_eq!(
            next_cache_expiry(1, "2025-10-01 00:00:00", "2026-02-01 00:00:00", now),
            "2026-02-01 00:00:00"
        );
    }
}
