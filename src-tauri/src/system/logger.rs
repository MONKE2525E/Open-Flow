use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};

use chrono::Local;
use log::{LevelFilter, Log, Metadata, Record};
use tauri::{AppHandle, Emitter, Manager};

const MAX_LOG_LINES: usize = 1000;
const LOG_EVENT: &str = "verenu:log";
const REDACTED_TEXT_FIELD_TOKENS: &[&str] = &[
    "raw_full=",
    "input_full=",
    "prompt_full=",
    "output_full=",
    "final_text_full=",
    "app_context_full=",
    "before_full=",
    "after_full=",
    "raw_text=",
    "clean_text=",
    "dictation=",
    "clipboard=",
    // Preview fields log the first N chars of dictated/cleaned text. The
    // values are quoted, so redact_quoted_field_value_ci only ever strips
    // real dictation content — never bare numbers like raw_rms=.
    "raw=",
    "raw_preview=",
    "final_preview=",
];

static LOG_BUFFER: OnceLock<Mutex<VecDeque<String>>> = OnceLock::new();
static APP_HANDLE: OnceLock<AppHandle> = OnceLock::new();
static LOGGER: SessionLogger = SessionLogger;
static VERBOSE_MODE: AtomicBool = AtomicBool::new(false);

/// Initializes the in-memory log buffer and installs the session logger.
///
/// Call this as early as possible in `main()` — before the database opens —
/// so startup failures (DB open/migrations, settings load) are captured
/// instead of vanishing into the default no-op logger. The `AppHandle` is
/// attached later by [`attach_app`], which enables `verenu:log` emission.
pub fn init_early() {
    let _ = LOG_BUFFER.set(Mutex::new(VecDeque::with_capacity(MAX_LOG_LINES)));
    if log::set_logger(&LOGGER).is_ok() {
        log::set_max_level(LevelFilter::Debug);
    }
}

/// Attaches the app handle so buffered records are forwarded to the frontend
/// as `verenu:log` events. Safe to call once, after `init_early`.
pub fn attach_app(app: &AppHandle) {
    let _ = APP_HANDLE.set(app.clone());
    log::info!("session logger initialized");
}

pub fn recent(limit: Option<usize>) -> Vec<String> {
    let Some(buffer) = LOG_BUFFER.get() else {
        return vec![];
    };
    let Ok(guard) = buffer.lock() else {
        return vec![];
    };

    let requested = limit.unwrap_or(200).max(1);
    let count = requested.min(guard.len());
    guard
        .iter()
        .skip(guard.len().saturating_sub(count))
        .cloned()
        .collect()
}

pub fn snapshot() -> Vec<String> {
    recent(Some(MAX_LOG_LINES))
}

pub fn set_verbose(enabled: bool) {
    VERBOSE_MODE.store(enabled, Ordering::Relaxed);
    log::info!(
        "dev verbose logging {}",
        if enabled { "enabled" } else { "disabled" }
    );
}

pub fn is_verbose() -> bool {
    VERBOSE_MODE.load(Ordering::Relaxed)
}

/// Header for exported diagnostics. Metadata only — version, platform, and
/// export time; never settings, keys, paths, or user content. Kept pure so
/// the export path is testable without touching the filesystem.
pub fn export_header() -> String {
    let now = Local::now().format("%Y-%m-%d %H:%M:%S");
    let verbose = if is_verbose() { "on" } else { "off" };
    format!(
        "Verenu diagnostics v{} ({}/{}) — exported {} — verbose logging {verbose}\n----------------------------------------------------------------",
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS,
        std::env::consts::ARCH,
        now
    )
}

pub fn export_to_downloads(app: &AppHandle) -> Result<String, String> {
    let downloads = app
        .path()
        .download_dir()
        .map_err(|e| format!("Failed to resolve Downloads directory: {e}"))?;
    std::fs::create_dir_all(&downloads)
        .map_err(|e| format!("Failed to create Downloads path: {e}"))?;

    let ts = Local::now().format("%Y%m%d-%H%M%S");
    let file_name = format!("verenu-logs-{ts}.txt");
    let path: PathBuf = downloads.join(file_name);
    let mut payload = export_header();
    payload.push('\n');
    payload.push_str(&snapshot().join("\n"));
    payload.push('\n');
    std::fs::write(&path, payload).map_err(|e| format!("Failed to write logs file: {e}"))?;
    Ok(path.display().to_string())
}

fn redact_message(input: &str) -> String {
    let mut out = input.to_string();
    out = redact_after_token_ci(&out, "authorization:");
    out = redact_after_token_ci(&out, "bearer ");
    out = redact_after_token_ci(&out, "api_key=");
    out = redact_after_token_ci(&out, "x-api-key:");
    out = redact_after_token_ci(&out, "x-goog-api-key:");
    // Google's legacy query-param key. Match the `?key=`/`&key=` URL markers
    // specifically: avoids a per-message lowercase allocation and won't clobber
    // unrelated params like `cache_key=` the way a bare `key=` would.
    out = redact_after_token_ci(&out, "?key=");
    out = redact_after_token_ci(&out, "&key=");
    out = redact_json_key_ci(&out, "api_key");
    out = redact_json_key_ci(&out, "authorization");
    for token in REDACTED_TEXT_FIELD_TOKENS {
        out = redact_quoted_field_value_ci(&out, token);
    }
    out
}

fn redact_after_token_ci(input: &str, token: &str) -> String {
    let mut cursor = 0usize;
    let mut remaining = input.to_string();

    while let Some(found_idx) = find_ascii_case_insensitive_from(&remaining, token, cursor) {
        let mut value_start = found_idx + token.len();
        while value_start < remaining.len()
            && remaining.as_bytes()[value_start].is_ascii_whitespace()
        {
            value_start += 1;
        }
        if value_start >= remaining.len() {
            break;
        }
        let end_rel = remaining[value_start..]
            .find(|ch: char| ch.is_whitespace() || ch == ',' || ch == ';' || ch == '"')
            .unwrap_or(remaining.len() - value_start);
        let end = value_start + end_rel;
        remaining.replace_range(value_start..end, "[REDACTED]");
        cursor = value_start + "[REDACTED]".len();
    }
    remaining
}

fn redact_json_key_ci(input: &str, key: &str) -> String {
    let pattern = format!("\"{}\":", key);
    let mut cursor = 0usize;
    let mut remaining = input.to_string();

    while let Some(found_idx) = find_ascii_case_insensitive_from(&remaining, &pattern, cursor) {
        let start = found_idx + pattern.len();
        let mut value_start = start;
        while value_start < remaining.len()
            && remaining.as_bytes()[value_start].is_ascii_whitespace()
        {
            value_start += 1;
        }
        if value_start < remaining.len() && remaining.as_bytes()[value_start] == b'"' {
            let content_start = value_start + 1;
            if let Some(content_end) = find_json_string_end(&remaining, content_start) {
                remaining.replace_range(content_start..content_end, "[REDACTED]");
                cursor = content_start + "[REDACTED]".len();
                continue;
            }
        }
        cursor = value_start;
    }
    remaining
}

fn redact_quoted_field_value_ci(input: &str, token: &str) -> String {
    let mut cursor = 0usize;
    let mut remaining = input.to_string();

    while let Some(found_idx) = find_ascii_case_insensitive_from(&remaining, token, cursor) {
        let mut value_start = found_idx + token.len();
        while value_start < remaining.len()
            && remaining.as_bytes()[value_start].is_ascii_whitespace()
        {
            value_start += 1;
        }

        if value_start < remaining.len() && remaining.as_bytes()[value_start] == b'"' {
            let content_start = value_start + 1;
            if let Some(content_end) = find_json_string_end(&remaining, content_start) {
                remaining.replace_range(content_start..content_end, "[REDACTED]");
                cursor = content_start + "[REDACTED]".len();
                continue;
            }
        }

        // Advance by a whole char so cursor stays on a UTF-8 boundary.
        cursor = remaining[value_start..]
            .char_indices()
            .nth(1)
            .map(|(idx, _)| value_start + idx)
            .unwrap_or(remaining.len());
    }

    remaining
}

fn find_ascii_case_insensitive_from(haystack: &str, needle: &str, start: usize) -> Option<usize> {
    if needle.is_empty() || start >= haystack.len() {
        return None;
    }
    let h = haystack.as_bytes();
    let n = needle.as_bytes();
    if n.len() > h.len().saturating_sub(start) {
        return None;
    }

    for i in start..=h.len() - n.len() {
        if !haystack.is_char_boundary(i) {
            continue;
        }
        let mut matches = true;
        for j in 0..n.len() {
            if !h[i + j].eq_ignore_ascii_case(&n[j]) {
                matches = false;
                break;
            }
        }
        if matches {
            return Some(i);
        }
    }
    None
}

fn find_json_string_end(input: &str, content_start: usize) -> Option<usize> {
    let bytes = input.as_bytes();
    let mut i = content_start;
    let mut escaped = false;
    while i < bytes.len() {
        let b = bytes[i];
        if escaped {
            escaped = false;
            i += 1;
            continue;
        }
        if b == b'\\' {
            escaped = true;
            i += 1;
            continue;
        }
        if b == b'"' {
            return Some(i);
        }
        i += 1;
    }
    None
}

struct SessionLogger;

impl Log for SessionLogger {
    fn enabled(&self, _metadata: &Metadata) -> bool {
        true
    }

    fn log(&self, record: &Record) {
        if !self.enabled(record.metadata()) {
            return;
        }
        // Non-verbose mode keeps info/warn/error only. Debug/trace is gated
        // behind the explicit Developer-panel toggle. The app's own debug
        // logs (per-poll detail, stage internals, per-request metadata) must
        // not flood the ring buffer by default — they would crowd out the
        // info-level lifecycle events that make a session reconstructable.
        // Level ordinals: Error=1 < Warn=2 < Info=3 < Debug=4 < Trace=5, so
        // debug/trace are the levels AT OR ABOVE Debug in ordinal terms.
        let verbose = is_verbose();
        if !verbose && record.level() >= log::Level::Debug {
            return;
        }
        let msg = record.args().to_string();
        let timestamp = Local::now().format("%Y-%m-%d %H:%M:%S%.3f");
        let line = format!(
            "[{}] {:<5} {}",
            timestamp,
            record.level(),
            redact_message(&msg)
        );

        if let Some(buffer) = LOG_BUFFER.get() {
            if let Ok(mut guard) = buffer.lock() {
                guard.push_back(line.clone());
                if guard.len() > MAX_LOG_LINES {
                    let _ = guard.pop_front();
                }
            }
        }

        if let Some(app) = APP_HANDLE.get() {
            let _ = app.emit(LOG_EVENT, line);
        }
    }

    fn flush(&self) {}
}

#[cfg(test)]
mod tests {
    use super::{find_json_string_end, recent, redact_json_key_ci, LOG_BUFFER};
    use log::Log;
    use std::collections::VecDeque;
    use std::sync::Mutex;

    fn reset_buffer() {
        let _ = LOG_BUFFER.set(Mutex::new(VecDeque::new()));
        let buf = LOG_BUFFER.get().expect("buffer");
        let mut guard = buf.lock().expect("lock");
        guard.clear();
    }

    fn log_record(level: log::Level, target: &str, message: &str) {
        let args = format_args!("{message}");
        let record = log::Record::builder()
            .args(args)
            .level(level)
            .target(target)
            .build();
        super::SessionLogger.log(&record);
    }

    /// Registers the session logger so `log::info!` etc. inside the code under
    /// test actually reach the ring buffer (the app normally does this in
    /// `init()` with a real AppHandle, which tests don't have).
    fn ensure_logger_registered() {
        static REGISTERED: std::sync::Once = std::sync::Once::new();
        REGISTERED.call_once(|| {
            let _ = log::set_logger(&super::LOGGER);
            log::set_max_level(log::LevelFilter::Debug);
        });
    }

    fn buffer_lines() -> Vec<String> {
        recent(Some(50))
    }

    #[test]
    fn verbose_off_drops_debug_from_app_targets_too() {
        reset_buffer();
        super::set_verbose(false);
        log_record(
            log::Level::Debug,
            "verenu::pipeline",
            "pipeline: stage detail",
        );
        log_record(
            log::Level::Debug,
            "hyper",
            "starting new connection: 127.0.0.1",
        );
        log_record(log::Level::Info, "verenu::pipeline", "pipeline: start");
        let lines = buffer_lines();
        assert!(
            lines.iter().all(|l| !l.contains("stage detail")),
            "app debug must not reach the buffer in non-verbose mode: {lines:?}"
        );
        assert!(
            lines.iter().all(|l| !l.contains("starting new connection")),
            "third-party debug must not reach the buffer in non-verbose mode: {lines:?}"
        );
        assert!(
            lines.iter().any(|l| l.contains("pipeline: start")),
            "info must always reach the buffer: {lines:?}"
        );
    }

    #[test]
    fn verbose_on_keeps_app_debug() {
        reset_buffer();
        super::set_verbose(true);
        log_record(
            log::Level::Debug,
            "verenu::pipeline",
            "pipeline: stage detail",
        );
        super::set_verbose(false);
        assert!(
            buffer_lines().iter().any(|l| l.contains("stage detail")),
            "app debug must reach the buffer in verbose mode"
        );
    }

    #[test]
    fn verbose_toggle_writes_a_marker() {
        reset_buffer();
        ensure_logger_registered();
        super::set_verbose(true);
        super::set_verbose(false);
        let lines = buffer_lines();
        assert!(lines.iter().any(|l| l.contains("verbose logging enabled")));
        assert!(lines.iter().any(|l| l.contains("verbose logging disabled")));
    }

    #[test]
    fn export_header_carries_metadata_only() {
        super::set_verbose(false);
        let header = super::export_header();
        assert!(header.contains(env!("CARGO_PKG_VERSION")));
        assert!(header.contains(std::env::consts::OS));
        assert!(header.contains("verbose logging off"));
        assert!(!header.contains("api_key"));
        assert!(!header.contains("C:\\"));
        assert!(!header.contains("/Users/"));
    }

    #[test]
    fn redacts_json_value_with_escaped_quote() {
        let input = r#"{"api_key":"abc\"def","other":"ok"}"#;
        let out = redact_json_key_ci(input, "api_key");
        assert!(out.contains(r#""api_key":"[REDACTED]""#));
        assert!(!out.contains(r#"abc\"def"#));
    }

    #[test]
    fn json_string_end_handles_escape_sequences() {
        let s = r#""abc\"def""#;
        let end = find_json_string_end(s, 1).expect("expected end quote");
        assert_eq!(&s[end..=end], "\"");
    }

    #[test]
    fn redacts_google_url_key_query_param() {
        let input = "POST https://generativelanguage.googleapis.com/v1beta/models/gemini:generateContent?key=AIzaSECRET status=200";
        let out = super::redact_message(input);
        assert!(!out.contains("AIzaSECRET"));
        assert!(out.contains("key=[REDACTED]"));
    }

    #[test]
    fn does_not_redact_unrelated_key_param() {
        let input = "cleanup cache key=mysession123 stored";
        let out = super::redact_message(input);
        assert!(out.contains("mysession123"));
    }

    #[test]
    fn redacts_verbose_dictation_fields() {
        let input = r#"pipeline: transcription raw_full="my private dictated text""#;
        let out = super::redact_message(input);
        assert!(!out.contains("my private dictated text"));
        assert!(out.contains(r#"raw_full="[REDACTED]""#));
    }

    #[test]
    fn redacts_dictation_preview_fields() {
        let input = r#"pipeline: transcription ok raw_preview="hello world" final_preview="hello world!" before_full="raw dictation" after_full="cleaned text" raw="dropped hallucination""#;
        let out = super::redact_message(input);
        assert!(!out.contains("hello world"));
        assert!(!out.contains("cleaned text"));
        assert!(!out.contains("dropped hallucination"));
        assert!(out.contains(r#"raw_preview="[REDACTED]""#));
        assert!(out.contains(r#"final_preview="[REDACTED]""#));
        assert!(out.contains(r#"before_full="[REDACTED]""#));
        assert!(out.contains(r#"after_full="[REDACTED]""#));
    }

    #[test]
    fn redact_leaves_unquoted_raw_metrics_alone() {
        // raw= followed by a number (no quote) must not be clobbered.
        let input = "pipeline: raw=7 rms=0.0042 raw_rms=0.0012";
        let out = super::redact_message(input);
        assert!(out.contains("raw=7"));
        assert!(out.contains("raw_rms=0.0012"));
    }

    #[test]
    fn provider_body_previews_keep_diagnostics_but_redact_secrets() {
        // Provider error bodies pass through `body_preview="..."` fields. The
        // 180-char cap in api/mod.rs prevents payload dumps, and the content
        // is diagnostic (account/model errors), so it must SURVIVE redaction —
        // but any embedded secret still gets caught by the generic tokens.
        let input = r#"transcription: unauthorized body_preview="{\"error\":\"model not found\"}""#;
        let out = super::redact_message(input);
        assert!(
            out.contains("model not found"),
            "diagnostics preserved: {out}"
        );
        assert!(out.contains("unauthorized"));
    }

    #[test]
    fn redacts_auth_urls_within_body_previews() {
        // Even if a provider body ever echoed a key back (defense in depth),
        // the generic key tokens still fire inside body_preview values.
        let input = r#"cleanup: error body_preview="{\"detail\":\"invalid ?key=AIzaSECRET\"}""#;
        let out = super::redact_message(input);
        assert!(!out.contains("AIzaSECRET"));
    }

    #[test]
    fn recent_returns_tail_in_order() {
        let _ = LOG_BUFFER.set(Mutex::new(VecDeque::new()));
        let buf = LOG_BUFFER.get().expect("buffer");
        let mut guard = buf.lock().expect("lock");
        guard.clear();
        guard.push_back("a".into());
        guard.push_back("b".into());
        guard.push_back("c".into());
        drop(guard);

        assert_eq!(recent(Some(2)), vec!["b".to_string(), "c".to_string()]);
    }
}
