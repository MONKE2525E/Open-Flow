use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};

use chrono::Local;
use log::{LevelFilter, Log, Metadata, Record};
use tauri::{AppHandle, Emitter, Manager};

const MAX_LOG_LINES: usize = 1000;
const LOG_EVENT: &str = "verenu:log";

static LOG_BUFFER: OnceLock<Mutex<VecDeque<String>>> = OnceLock::new();
static APP_HANDLE: OnceLock<AppHandle> = OnceLock::new();
static LOGGER: SessionLogger = SessionLogger;
static VERBOSE_MODE: AtomicBool = AtomicBool::new(false);

pub fn init(app: &AppHandle) -> tauri::Result<()> {
    let _ = LOG_BUFFER.set(Mutex::new(VecDeque::with_capacity(MAX_LOG_LINES)));
    let _ = APP_HANDLE.set(app.clone());

    if log::set_logger(&LOGGER).is_ok() {
        log::set_max_level(LevelFilter::Debug);
        log::info!("session logger initialized");
    }
    Ok(())
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
    let payload = snapshot().join("\n");
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
    // Google's legacy `?key=` query param. Scope to googleapis URLs so a stray
    // `key=` elsewhere in a log line (cache keys, etc.) isn't clobbered.
    if out.to_ascii_lowercase().contains("googleapis.com") {
        out = redact_after_token_ci(&out, "key=");
    }
    out = redact_json_key_ci(&out, "api_key");
    out = redact_json_key_ci(&out, "authorization");
    out
}

fn redact_after_token_ci(input: &str, token: &str) -> String {
    let mut cursor = 0usize;
    let mut remaining = input.to_string();

    while let Some(found_idx) = find_ascii_case_insensitive_from(&remaining, token, cursor) {
        let idx = found_idx + token.len();
        let end_rel = remaining[idx..]
            .find(|ch: char| ch.is_whitespace() || ch == ',' || ch == ';' || ch == '"')
            .unwrap_or(remaining.len() - idx);
        let end = idx + end_rel;
        remaining.replace_range(idx..end, "[REDACTED]");
        cursor = idx + "[REDACTED]".len();
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
        let verbose = is_verbose();
        if !verbose
            && record.level() <= log::Level::Debug
            && !record.target().starts_with("verenu")
            && !record.target().starts_with("src_tauri")
        {
            return;
        }
        let msg = record.args().to_string();
        if !verbose && msg.starts_with("starting new connection:") {
            return;
        }
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
    use std::collections::VecDeque;
    use std::sync::Mutex;

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
