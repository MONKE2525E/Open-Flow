use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex, MutexGuard};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_store::StoreExt;

use chrono::{DateTime, Duration, NaiveDateTime, Utc};
use crate::api::{auto_learn, cleanup, transcription};
use crate::core::{injection, window_context};
use crate::data::{db, dictionary, snippets, store};
use crate::media::audio;
use crate::system::apps::AppMapping;
use crate::DbHandle;

const MIN_RECORDING_MS: u64 = 700;
const MIN_RECORDING_RMS: f32 = 0.008;

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
    let lower = raw.to_lowercase();
    if !lower.contains(" plus ") {
        return raw.to_string();
    }
    if [" times ", " multiplied ", " multiply ", " divided ", " over ", " x "]
        .iter()
        .any(|needle| lower.contains(needle))
    {
        return raw.to_string();
    }

    let chars: Vec<char> = raw.chars().collect();
    let Some(_) = repeated_digit_x_digit_chunk(&chars) else {
        return raw.to_string();
    };
    fold_digit_x_digit_chunks_from_chars(&chars)
}

fn fold_digit_x_digit_chunks_from_chars(chars: &[char]) -> String {
    let mut out = String::with_capacity(chars.len());
    let mut i = 0usize;
    while i < chars.len() {
        if i + 2 < chars.len()
            && chars[i].is_ascii_digit()
            && (chars[i + 1] == 'x' || chars[i + 1] == 'X')
            && chars[i + 2].is_ascii_digit()
        {
            out.push(chars[i]);
            out.push(chars[i + 2]);
            i += 3;
            continue;
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

fn repeated_digit_x_digit_chunk(chars: &[char]) -> Option<(char, char)> {
    let mut first: Option<(char, char)> = None;
    let mut count = 0usize;
    let mut i = 0usize;
    while i + 2 < chars.len() {
        if chars[i].is_ascii_digit()
            && (chars[i + 1] == 'x' || chars[i + 1] == 'X')
            && chars[i + 2].is_ascii_digit()
        {
            let pair = (chars[i], chars[i + 2]);
            if let Some(existing) = first {
                if pair != existing {
                    return None;
                }
            } else {
                first = Some(pair);
            }
            count += 1;
            i += 3;
            continue;
        }
        i += 1;
    }
    if count >= 2 {
        first
    } else {
        None
    }
}

fn normalize_cleanup_cache_key(input: &str) -> String {
    let (tokens, separators) = tokenize_cache_key_parts(input);
    let mut out = String::new();
    let mut i = 0;

    while i < tokens.len() {
        let token = &tokens[i];
        if token.chars().any(|c| c.is_ascii_digit()) {
            let mut normalized = normalize_digit_token(token);
            let mut j = i + 1;
            while j < tokens.len() && tokens[j].chars().any(|c| c.is_ascii_digit()) {
                let sep = separators.get(j - 1).map(|s| s.trim()).unwrap_or("");
                if sep == "." {
                    normalized.push('.');
                    normalized.push_str(&normalize_digit_token(&tokens[j]));
                    j += 1;
                    continue;
                }
                if sep == ":" {
                    normalized.push_str(&normalize_digit_token(&tokens[j]));
                    j += 1;
                    continue;
                }
                break;
            }
            out.push_str("num");
            out.push_str(&normalized);
            i = j;
            continue;
        }

        if let Some((normalized, next_idx)) = normalize_number_word_run(&tokens, i) {
            out.push_str("num");
            out.push_str(&normalized);
            i = next_idx;
            continue;
        }

        out.push_str(token);
        i += 1;
    }

    out
}

fn should_use_cleanup_cache(raw: &str) -> bool {
    let tokens = tokenize_cache_key_input(raw);
    let mut numeric_count = 0usize;
    let mut has_math_operator = false;

    for t in &tokens {
        if t.chars().any(|c| c.is_ascii_digit()) || is_number_word_significant(t) {
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

fn normalize_digit_token(token: &str) -> String {
    let digits: String = token.chars().filter(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() {
        "0".to_string()
    } else {
        digits
    }
}

fn tokenize_cache_key_input(input: &str) -> Vec<String> {
    let (tokens, _) = tokenize_cache_key_parts(input);
    tokens
}

fn tokenize_cache_key_parts(input: &str) -> (Vec<String>, Vec<String>) {
    let mut tokens = Vec::new();
    let mut separators = Vec::new();
    let mut buf = String::new();
    let mut sep_buf = String::new();

    for ch in input.chars() {
        if ch.is_alphanumeric() {
            if buf.is_empty() && !tokens.is_empty() {
                separators.push(std::mem::take(&mut sep_buf));
            }
            buf.extend(ch.to_lowercase());
            continue;
        }

        if !buf.is_empty() {
            tokens.push(std::mem::take(&mut buf));
        }
        sep_buf.push(ch);
    }

    if !buf.is_empty() {
        tokens.push(buf);
    }

    (tokens, separators)
}

fn is_number_word_significant(token: &str) -> bool {
    matches!(
        token,
        "zero"
            | "one"
            | "two"
            | "three"
            | "four"
            | "five"
            | "six"
            | "seven"
            | "eight"
            | "nine"
            | "ten"
            | "eleven"
            | "twelve"
            | "thirteen"
            | "fourteen"
            | "fifteen"
            | "sixteen"
            | "seventeen"
            | "eighteen"
            | "nineteen"
            | "twenty"
            | "thirty"
            | "forty"
            | "fifty"
            | "sixty"
            | "seventy"
            | "eighty"
            | "ninety"
            | "hundred"
            | "thousand"
            | "million"
            | "billion"
            | "trillion"
            | "first"
            | "second"
            | "third"
            | "fourth"
            | "fifth"
            | "sixth"
            | "seventh"
            | "eighth"
            | "ninth"
            | "tenth"
            | "eleventh"
            | "twelfth"
            | "thirteenth"
            | "fourteenth"
            | "fifteenth"
            | "sixteenth"
            | "seventeenth"
            | "eighteenth"
            | "nineteenth"
            | "twentieth"
            | "thirtieth"
            | "fortieth"
            | "fiftieth"
            | "sixtieth"
            | "seventieth"
            | "eightieth"
            | "ninetieth"
            | "hundredth"
            | "thousandth"
            | "millionth"
            | "billionth"
            | "trillionth"
    )
}

fn normalize_number_word_run(tokens: &[String], start: usize) -> Option<(String, usize)> {
    if start >= tokens.len() {
        return None;
    }

    let mut i = start;
    let mut negative = false;
    if matches!(tokens[i].as_str(), "minus" | "negative") {
        negative = true;
        i += 1;
    }
    if i >= tokens.len() || !is_number_word_significant(&tokens[i]) {
        return None;
    }

    let (int_value, mut next, seen_any) = parse_number_word_integer(tokens, i);
    if !seen_any {
        return None;
    }

    let mut normalized = if negative {
        format!("-{int_value}")
    } else {
        int_value.to_string()
    };

    if next < tokens.len() && tokens[next] == "point" {
        next += 1;
        let mut frac = String::new();
        while next < tokens.len() {
            let t = tokens[next].as_str();
            if t.chars().all(|c| c.is_ascii_digit()) {
                frac.push_str(t);
                next += 1;
                continue;
            }
            if t == "oh" {
                frac.push('0');
                next += 1;
                continue;
            }
            if let Some(d) = unit_word_value(t) {
                frac.push(char::from(b'0' + d as u8));
                next += 1;
                continue;
            }
            break;
        }
        if !frac.is_empty() {
            normalized.push('.');
            normalized.push_str(&frac);
        }
    }

    Some((normalized, next))
}

fn parse_number_word_integer(tokens: &[String], mut i: usize) -> (i64, usize, bool) {
    let mut total: i64 = 0;
    let mut current: i64 = 0;
    let mut seen_any = false;

    while i < tokens.len() {
        let t = tokens[i].as_str();
        if t == "and" {
            i += 1;
            continue;
        }
        if let Some(v) = unit_word_value(t) {
            current += i64::from(v);
            seen_any = true;
            i += 1;
            continue;
        }
        if let Some(v) = teen_or_tens_word_value(t) {
            current += i64::from(v);
            seen_any = true;
            i += 1;
            continue;
        }
        if t == "hundred" {
            current = if current == 0 { 100 } else { current * 100 };
            seen_any = true;
            i += 1;
            continue;
        }
        if let Some(scale) = large_scale_word_value(t) {
            let part = if current == 0 { 1 } else { current };
            total = total.saturating_add(part.saturating_mul(scale));
            current = 0;
            seen_any = true;
            i += 1;
            continue;
        }
        if let Some(v) = ordinal_word_value(t) {
            current += i64::from(v);
            seen_any = true;
            i += 1;
            continue;
        }
        break;
    }

    (total + current, i, seen_any)
}

fn unit_word_value(token: &str) -> Option<i32> {
    match token {
        "zero" => Some(0),
        "one" | "first" => Some(1),
        "two" | "second" => Some(2),
        "three" | "third" => Some(3),
        "four" | "fourth" => Some(4),
        "five" | "fifth" => Some(5),
        "six" | "sixth" => Some(6),
        "seven" | "seventh" => Some(7),
        "eight" | "eighth" => Some(8),
        "nine" | "ninth" => Some(9),
        _ => None,
    }
}

fn teen_or_tens_word_value(token: &str) -> Option<i32> {
    match token {
        "ten" | "tenth" => Some(10),
        "eleven" | "eleventh" => Some(11),
        "twelve" | "twelfth" => Some(12),
        "thirteen" | "thirteenth" => Some(13),
        "fourteen" | "fourteenth" => Some(14),
        "fifteen" | "fifteenth" => Some(15),
        "sixteen" | "sixteenth" => Some(16),
        "seventeen" | "seventeenth" => Some(17),
        "eighteen" | "eighteenth" => Some(18),
        "nineteen" | "nineteenth" => Some(19),
        "twenty" | "twentieth" => Some(20),
        "thirty" | "thirtieth" => Some(30),
        "forty" | "fortieth" => Some(40),
        "fifty" | "fiftieth" => Some(50),
        "sixty" | "sixtieth" => Some(60),
        "seventy" | "seventieth" => Some(70),
        "eighty" | "eightieth" => Some(80),
        "ninety" | "ninetieth" => Some(90),
        _ => None,
    }
}

fn large_scale_word_value(token: &str) -> Option<i64> {
    match token {
        "thousand" | "thousandth" => Some(1_000),
        "million" | "millionth" => Some(1_000_000),
        "billion" | "billionth" => Some(1_000_000_000),
        "trillion" | "trillionth" => Some(1_000_000_000_000),
        _ => None,
    }
}

fn ordinal_word_value(token: &str) -> Option<i32> {
    match token {
        "hundredth" => Some(100),
        _ => None,
    }
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
    let started_at = std::time::Instant::now();
    let Some((session, target_hwnd)) = take_pipeline_session(&state) else {
        log::debug!("pipeline: no session — recording never started or was already consumed");
        return;
    };

    // Capture process name before any await points — foreground window may
    // change to a different app during async transcription/cleanup.
    let process_name = window_context::get_active_process_name()
        .unwrap_or_else(|| "unknown".into())
        .to_lowercase();
    log::info!("pipeline: start process={process_name} target_hwnd={target_hwnd}");

    std::thread::spawn(crate::system::volume::unmute);
    show_pill(&app, "processing");

    let stage_audio = std::time::Instant::now();
    let Some((wav, duration_ms)) = stop_and_validate_audio(&app, session).await else {
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
        "pipeline: config t_provider={} c_provider={} cleanup_enabled={} intensity={} fallback={} app_context_hint={} profile={}",
        cfg.transcription_provider,
        cfg.cleanup_provider,
        cfg.cleanup_enabled,
        cfg.cleanup_intensity,
        cfg.api_fallback_enabled,
        cfg.app_context_hint,
        profile
    );
    log::debug!(
        "pipeline: context resolved app_context={} stage_ms={}",
        app_context.as_deref().unwrap_or("none"),
        stage_config.elapsed().as_millis()
    );
    let stage_transcribe = std::time::Instant::now();
    let Some((raw, api_used)) = run_transcription(&app, &wav, &cfg).await else {
        return;
    };
    let raw = normalize_transcription_math_artifacts(&raw);
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
    let Some((final_text, dict_entries)) =
        run_cleanup_and_snippets(&app, &raw, &cfg, &profile, app_context.as_deref()).await
    else {
        return;
    };
    log::debug!(
        "pipeline: cleanup/snippets ok final_chars={} final_preview=\"{}\" dict_entries={}",
        final_text.chars().count(),
        preview_text(&final_text, 140),
        dict_entries.len()
    );
    if crate::system::logger::is_verbose() {
        log::debug!("pipeline: final_text_full=\"{}\"", final_text);
    }
    log::debug!("pipeline: cleanup stage_ms={}", stage_cleanup.elapsed().as_millis());

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

    let dict_stage = std::time::Instant::now();
    let final_before_dict = final_text.clone();
    let final_text = dictionary::apply_substitutions_from(&final_text, &dict_entries);
    let dict_changed = final_text != final_before_dict;
    log::debug!(
        "pipeline: dictionary apply changed={} before_chars={} after_chars={} stage_ms={}",
        dict_changed,
        final_before_dict.chars().count(),
        final_text.chars().count(),
        dict_stage.elapsed().as_millis()
    );
    if dict_changed && crate::system::logger::is_verbose() {
        log::debug!("pipeline: dictionary before_full=\"{}\"", final_before_dict);
        log::debug!("pipeline: dictionary after_full=\"{}\"", final_text);
    }

    hide_pill(&app);
    let inject_stage = std::time::Instant::now();
    let injected_text =
        match injection::inject_text(&final_text, target_hwnd, cfg.contextual_caps_enabled).await {
            Ok(text) => text,
            Err(e) => {
                log::error!("inject: {e}");
                show_error_pill(&app, "Failed to paste — text saved to history").await;
                final_text.clone()
            }
        };
    log::debug!(
        "pipeline: injection done contextual_caps={} output_chars={} stage_ms={}",
        cfg.contextual_caps_enabled,
        injected_text.chars().count(),
        inject_stage.elapsed().as_millis()
    );
    app.emit("open-flow:transcribed", &injected_text).ok();
    log::info!(
        "pipeline: completed words={} duration_ms={} elapsed_ms={}",
        words,
        duration_ms,
        started_at.elapsed().as_millis()
    );

    if cfg.auto_learn_enabled {
        log::debug!("pipeline: auto_learn monitor starting");
        auto_learn::start_monitor(injected_text, process_name, db.inner().clone(), app.clone());
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
    log::debug!(
        "pipeline: transcription stage start provider={} language={} bytes={}",
        cfg.transcription_provider,
        cfg.transcription_language,
        wav.len()
    );
    match try_providers(&[&cfg.transcription_provider], cfg, |provider_id, key| {
        let w = wav.clone();
        let provider = transcription_provider_from_str(provider_id);
        let language = cfg.transcription_language.clone();
        Box::pin(async move { transcription::transcribe(w, provider, &key, &language).await })
    })
    .await
    {
        Ok((raw, t_provider)) if !raw.is_empty() => {
            log::debug!(
                "pipeline: transcription provider success={} chars={}",
                t_provider,
                raw.chars().count()
            );
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
            log::error!("pipeline: transcription failed error={}", trim_err(&e.to_string()));
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
    log::debug!(
        "pipeline: cleanup inputs snippets={} dict_entries={}",
        db_snippets.len(),
        dict_entries.len()
    );

    let snippet_instructions = snippets::collect_snippet_instructions_from(raw, &db_snippets);
    log::debug!(
        "pipeline: cleanup stage start raw_chars={} snippet_override_lines={} cleanup_enabled={}",
        raw.chars().count(),
        snippet_instructions.lines().filter(|l| !l.trim().is_empty()).count(),
        cfg.cleanup_enabled
    );
    if crate::system::logger::is_verbose() {
        log::debug!("pipeline: cleanup raw_full=\"{}\"", raw);
        if !snippet_instructions.is_empty() {
            log::debug!("pipeline: cleanup snippet_instructions_full=\"{}\"", snippet_instructions);
        }
    }

    // Fast path: entire transcription was a single snippet trigger — skip the LLM.
    let pure_expansion = if snippet_instructions.is_empty() {
        snippets::try_pure_snippet_expand_from(raw, &db_snippets, &db)
    } else {
        None
    };
    let expanded = pure_expansion
        .clone()
        .unwrap_or_else(|| snippets::expand_snippets_from(raw, &mut db_snippets, &db));
    log::debug!(
        "pipeline: snippets expanded pure_fast_path={} expanded_chars={}",
        pure_expansion.is_some(),
        expanded.chars().count()
    );

    let c_key = cfg.key_for(&cfg.cleanup_provider).to_owned();
    let final_text = if cfg.cleanup_enabled
        && !c_key.is_empty()
        && pure_expansion.is_none()
        && cfg.cleanup_intensity != "none"
    {
        let allow_cache = should_use_cleanup_cache(raw);
        let cache_key = if allow_cache {
            normalize_cleanup_cache_key(raw)
        } else {
            String::new()
        };
        if !cache_key.is_empty() {
            if let Ok(Some(entry)) = db::cleanup_cache_get_active(&db, &cache_key) {
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
                    &db,
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
                let overridden =
                    snippets::apply_cleanup_instruction_overrides(&entry.clean_text, &snippet_instructions);
                return Some((overridden, dict_entries));
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
                )
                .await
            })
        })
        .await;

        match cleaned_res {
            Ok((cleaned, _)) if !cleaned.is_empty() => {
                log::debug!("pipeline: cleanup provider success cleaned_chars={}", cleaned.chars().count());
                let overridden =
                    snippets::apply_cleanup_instruction_overrides(&cleaned, &snippet_instructions);
                if !cache_key.is_empty() {
                    let expires = sqlite_utc_plus(7);
                    match db::cleanup_cache_insert_new(&db, &cache_key, &cleaned, &expires) {
                        Ok(_) => log::debug!("pipeline: cleanup cache insert ok expires_at={expires}"),
                        Err(err) => log::warn!("pipeline: cleanup cache insert failed: {err}"),
                    }
                }
                overridden
            }
            Ok(_) => {
                snippets::apply_cleanup_instruction_overrides(&expanded, &snippet_instructions)
            }
            Err(Some(e)) => {
                log::error!("pipeline: cleanup failed error={}", trim_err(&e.to_string()));
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
    log::debug!("pipeline: provider chain={}", to_try.join("->"));
    for (idx, provider_id) in to_try.iter().enumerate() {
        let provider_id = *provider_id;
        log::debug!(
            "pipeline: provider attempt {}/{} id={}",
            idx + 1,
            to_try.len(),
            provider_id
        );
        let key = cfg.key_for(provider_id).to_owned();
        if key.is_empty() {
            log::debug!("pipeline: provider {} skipped (missing key)", provider_id);
            continue;
        }

        match call(provider_id, key).await {
            Ok(result) => {
                log::debug!("pipeline: provider {} succeeded", provider_id);
                return Ok((result, provider_id.to_string()));
            }
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
        fold_digit_x_digit_chunks_from_chars, normalize_cleanup_cache_key,
        normalize_transcription_math_artifacts,
        should_use_cleanup_cache,
    };

    #[test]
    fn cache_key_normalizes_digit_vs_word_numbers() {
        let a = normalize_cleanup_cache_key("I have 12 apples");
        let b = normalize_cleanup_cache_key("I have twelve apples");
        assert_eq!(a, b);
    }

    #[test]
    fn cache_key_normalizes_decimal_digit_vs_word_form() {
        let a = normalize_cleanup_cache_key("version 2.5");
        let b = normalize_cleanup_cache_key("version two point five");
        assert_eq!(a, b);
    }

    #[test]
    fn cache_key_normalizes_time_and_date_like_forms() {
        let a = normalize_cleanup_cache_key("meet at 10:30 on 20260517");
        let b = normalize_cleanup_cache_key("meet at 1030 on 20260517");
        assert_eq!(a, b);
    }

    #[test]
    fn cache_key_keeps_non_numeric_text_distinct() {
        let a = normalize_cleanup_cache_key("model x");
        let b = normalize_cleanup_cache_key("model y");
        assert_ne!(a, b);
    }

    #[test]
    fn cache_key_still_ignores_case_and_punctuation() {
        let a = normalize_cleanup_cache_key("Hello, WORLD!");
        let b = normalize_cleanup_cache_key("hello world");
        assert_eq!(a, b);
    }

    #[test]
    fn cache_key_matches_digit_and_word_same_number() {
        let a = normalize_cleanup_cache_key("What's 45 plus 45?");
        let b = normalize_cleanup_cache_key("What's forty five plus forty five?");
        assert_eq!(a, b);
    }

    #[test]
    fn cache_key_distinguishes_different_numeric_values() {
        let a = normalize_cleanup_cache_key("What's 45 plus 45?");
        let b = normalize_cleanup_cache_key("What's 6 plus 6?");
        assert_ne!(a, b);
    }

    #[test]
    fn cache_key_distinguishes_decimal_from_whole_number() {
        let a = normalize_cleanup_cache_key("version 2.5");
        let b = normalize_cleanup_cache_key("version 25");
        assert_ne!(a, b);
    }

    #[test]
    fn cache_key_does_not_merge_comma_separated_digits() {
        let a = normalize_cleanup_cache_key("What's 4, 5 plus 4, 5?");
        let b = normalize_cleanup_cache_key("What's 45 plus 45?");
        assert_ne!(a, b);
    }

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
    fn transcription_normalizes_digit_x_digit_in_plus_queries() {
        let out = normalize_transcription_math_artifacts("What's 6x7 plus 6x7?");
        assert_eq!(out, "What's 67 plus 67?");
    }

    #[test]
    fn transcription_does_not_touch_non_plus_digit_x_digit() {
        let out = normalize_transcription_math_artifacts("Calculate 6x7");
        assert_eq!(out, "Calculate 6x7");
        let chars: Vec<char> = "6x7 and 3x4".chars().collect();
        let folded = fold_digit_x_digit_chunks_from_chars(&chars);
        assert_eq!(folded, "67 and 34");
    }

    #[test]
    fn transcription_does_not_fold_mixed_multiplication_chunks() {
        let out = normalize_transcription_math_artifacts("6x7 plus 3x4");
        assert_eq!(out, "6x7 plus 3x4");
    }
}
