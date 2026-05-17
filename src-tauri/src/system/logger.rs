use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};

use chrono::Local;
use log::{LevelFilter, Log, Metadata, Record};
use tauri::{AppHandle, Emitter, Manager};

const MAX_LOG_LINES: usize = 1000;
const LOG_EVENT: &str = "open-flow:log";

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
        .rev()
        .take(count)
        .cloned()
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect()
}

pub fn snapshot() -> Vec<String> {
    recent(Some(MAX_LOG_LINES))
}

pub fn set_verbose(enabled: bool) {
    VERBOSE_MODE.store(enabled, Ordering::Relaxed);
    log::info!("dev verbose logging {}", if enabled { "enabled" } else { "disabled" });
}

pub fn is_verbose() -> bool {
    VERBOSE_MODE.load(Ordering::Relaxed)
}

pub fn export_to_downloads(app: &AppHandle) -> Result<String, String> {
    let downloads = app
        .path()
        .download_dir()
        .map_err(|e| format!("Failed to resolve Downloads directory: {e}"))?;
    std::fs::create_dir_all(&downloads).map_err(|e| format!("Failed to create Downloads path: {e}"))?;

    let ts = Local::now().format("%Y%m%d-%H%M%S");
    let file_name = format!("open-flow-logs-{ts}.txt");
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
    out = redact_json_key_ci(&out, "api_key");
    out = redact_json_key_ci(&out, "authorization");
    out
}

fn redact_after_token_ci(input: &str, token: &str) -> String {
    let mut cursor = 0usize;
    let mut remaining = input.to_string();
    let token_l = token.to_ascii_lowercase();

    loop {
        let lower = remaining.to_ascii_lowercase();
        let Some(found_rel) = lower[cursor..].find(&token_l) else {
            break;
        };
        let idx = cursor + found_rel + token.len();
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
    let pattern = format!("\"{}\":", key.to_ascii_lowercase());
    let mut cursor = 0usize;
    let mut remaining = input.to_string();

    loop {
        let lower = remaining.to_ascii_lowercase();
        let Some(found_rel) = lower[cursor..].find(&pattern) else {
            break;
        };
        let start = cursor + found_rel + pattern.len();
        let mut value_start = start;
        while value_start < remaining.len() && remaining.as_bytes()[value_start].is_ascii_whitespace() {
            value_start += 1;
        }
        if value_start < remaining.len() && remaining.as_bytes()[value_start] == b'"' {
            let content_start = value_start + 1;
            if let Some(next_quote_rel) = remaining[content_start..].find('"') {
                let content_end = content_start + next_quote_rel;
                remaining.replace_range(content_start..content_end, "[REDACTED]");
                cursor = content_start + "[REDACTED]".len();
                continue;
            }
        }
        cursor = value_start;
    }
    remaining
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
        let msg = record.args().to_string();
        if !is_verbose() && msg.starts_with("starting new connection:") {
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
