use serde_json::{Map, Value};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Manager};

const SETTINGS_FILE: &str = "settings.json";
pub(crate) const STORAGE_FULL_ERROR: &str = "STORAGE_FULL";

static SIMULATE_STORAGE_FULL: AtomicBool = AtomicBool::new(false);

pub(crate) fn set_storage_full_simulation(enabled: bool) {
    SIMULATE_STORAGE_FULL.store(enabled, Ordering::Relaxed);
}

pub(crate) fn storage_full_simulation_enabled() -> bool {
    SIMULATE_STORAGE_FULL.load(Ordering::Relaxed)
}

#[derive(Clone)]
pub struct SettingsHandle {
    path: Arc<PathBuf>,
    values: Arc<Mutex<Map<String, Value>>>,
}

#[derive(Clone, Debug, Default)]
pub struct SettingsSnapshot {
    values: Map<String, Value>,
}

impl SettingsSnapshot {
    pub fn get(&self, key: &str) -> Option<&Value> {
        self.values.get(key)
    }

    pub fn get_cloned(&self, key: &str) -> Option<Value> {
        self.values.get(key).cloned()
    }

    #[cfg(test)]
    pub fn from_pairs(pairs: impl IntoIterator<Item = (String, Value)>) -> Self {
        SettingsSnapshot {
            values: pairs.into_iter().collect(),
        }
    }
}

impl SettingsHandle {
    pub fn open(app: &AppHandle) -> Result<Self, String> {
        let path = settings_path(app)?;
        let values = read_settings_file(&path)?;
        Ok(Self {
            path: Arc::new(path),
            values: Arc::new(Mutex::new(values)),
        })
    }

    pub fn snapshot(&self) -> Result<SettingsSnapshot, String> {
        let values = self
            .values
            .lock()
            .map_err(|_| "Settings lock was poisoned".to_string())?
            .clone();
        Ok(SettingsSnapshot { values })
    }

    pub fn get(&self, key: &str) -> Option<Value> {
        match self.values.lock() {
            Ok(values) => values.get(key).cloned(),
            Err(_) => {
                log::error!("Settings lock was poisoned when reading key: {key}");
                None
            }
        }
    }

    pub fn set(&self, key: impl Into<String>, value: Value) -> Result<(), String> {
        self.values
            .lock()
            .map_err(|_| "Settings lock was poisoned".to_string())?
            .insert(key.into(), value);
        Ok(())
    }

    pub fn set_many<I, K>(&self, values: I) -> Result<(), String>
    where
        I: IntoIterator<Item = (K, Value)>,
        K: Into<String>,
    {
        let mut settings = self
            .values
            .lock()
            .map_err(|_| "Settings lock was poisoned".to_string())?;
        for (key, value) in values {
            settings.insert(key.into(), value);
        }
        Ok(())
    }

    pub fn delete(&self, key: &str) -> Result<Option<Value>, String> {
        Ok(self
            .values
            .lock()
            .map_err(|_| "Settings lock was poisoned".to_string())?
            .remove(key))
    }

    pub fn save(&self) -> Result<(), String> {
        let values = self
            .values
            .lock()
            .map_err(|_| "Settings lock was poisoned".to_string())?;
        write_settings_file(&self.path, &values)
    }

    pub fn save_value(&self, key: impl Into<String>, value: Value) -> Result<(), String> {
        self.save_values([(key, value)])
    }

    pub fn save_values<I, K>(&self, values: I) -> Result<(), String>
    where
        I: IntoIterator<Item = (K, Value)>,
        K: Into<String>,
    {
        let mut settings = self
            .values
            .lock()
            .map_err(|_| "Settings lock was poisoned".to_string())?;
        let mut next = settings.clone();
        for (key, value) in values {
            next.insert(key.into(), value);
        }
        write_settings_file(&self.path, &next)?;
        *settings = next;
        Ok(())
    }
}

pub fn settings_path(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .resolve(SETTINGS_FILE, tauri::path::BaseDirectory::AppData)
        .map_err(|e| e.to_string())
}

pub fn settings_handle(app: &AppHandle) -> Result<SettingsHandle, String> {
    if let Some(state) = app.try_state::<SettingsHandle>() {
        Ok(state.inner().clone())
    } else {
        SettingsHandle::open(app)
    }
}

pub fn settings_snapshot(app: &AppHandle) -> Result<SettingsSnapshot, String> {
    settings_handle(app)?.snapshot()
}

fn read_settings_file(path: &Path) -> Result<Map<String, Value>, String> {
    if !path.exists() {
        return Ok(Map::new());
    }
    let raw =
        std::fs::read_to_string(path).map_err(|e| format!("Failed to read settings.json: {e}"))?;
    if raw.trim().is_empty() {
        return Ok(Map::new());
    }
    // A corrupted or non-object settings.json must not crash the app at startup.
    // Back up the bad file so settings can be recovered manually, then start fresh.
    match serde_json::from_str::<Value>(&raw) {
        Ok(Value::Object(map)) => Ok(map),
        Ok(_) => {
            log::error!("settings.json did not contain a JSON object; backing up and resetting");
            backup_corrupt_settings(path);
            Ok(Map::new())
        }
        Err(e) => {
            log::error!("Failed to parse settings.json: {e}; backing up and resetting");
            backup_corrupt_settings(path);
            Ok(Map::new())
        }
    }
}

fn backup_corrupt_settings(path: &Path) {
    let backup_path = path.with_extension("json.bak");
    // Clear any prior backup first so the rename can't be blocked by a stale
    // .bak on platforms/filesystems where replace-on-rename isn't guaranteed.
    let _ = std::fs::remove_file(&backup_path);
    if let Err(e) = std::fs::rename(path, &backup_path) {
        // Non-critical startup cleanup — warn and continue rather than fail.
        log::warn!("Failed to back up corrupt settings.json: {e}");
    }
}

fn write_settings_file(path: &Path, values: &Map<String, Value>) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| settings_write_error("Failed to create settings directory", &e))?;
    }
    let json = serde_json::to_string_pretty(values)
        .map_err(|e| format!("Failed to serialize settings.json: {e}"))?;
    // Write to a temp file then atomically rename so an interrupted write
    // (crash, power loss, disk full) can't truncate the live settings.json.
    let tmp_path = path.with_extension("json.tmp");
    if let Err(e) = std::fs::write(&tmp_path, json) {
        // A failed/partial write shouldn't leave a stale temp file behind.
        let _ = std::fs::remove_file(&tmp_path);
        return Err(settings_write_error("Failed to write temporary settings file", &e));
    }
    if let Err(e) = std::fs::rename(&tmp_path, path) {
        // Don't leave the temp file behind if the swap failed.
        let _ = std::fs::remove_file(&tmp_path);
        return Err(settings_write_error("Failed to replace settings.json atomically", &e));
    }
    Ok(())
}

fn settings_write_error(operation: &str, error: &impl std::fmt::Display) -> String {
    let detail = error.to_string();
    if is_storage_full_error(&detail) {
        format!("{STORAGE_FULL_ERROR}: {operation}")
    } else {
        format!("{operation}: {detail}")
    }
}

pub(crate) fn is_storage_full_error(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    lower.contains("storage_full")
        || lower.contains("no space left on device")
        || lower.contains("not enough space")
        || lower.contains("disk is full")
        || lower.contains("disk full")
        || lower.contains("database or disk is full")
        || lower.contains("os error 112")
        || lower.contains("error 112")
}

/// API key names in the store — never expose values to the frontend after write.
pub const KEY_GROQ: &str = "api_key_groq";
pub const KEY_OPENAI: &str = "api_key_openai";
pub const KEY_GOOGLE: &str = "api_key_google";
pub const KEY_ASSEMBLYAI: &str = "api_key_assemblyai";

pub const TRANSCRIPTION_PROVIDER: &str = "transcription_provider";
pub const TRANSCRIPTION_LANGUAGE: &str = "transcription_language";
pub const CLEANUP_PROVIDER: &str = "cleanup_provider";
pub const TRANSCRIPTION_MODEL: &str = "transcription_model";
pub const CLEANUP_MODEL: &str = "cleanup_model";
pub const TRANSCRIPTION_MODELS_BY_PROVIDER: &str = "transcription_models_by_provider";
pub const CLEANUP_MODELS_BY_PROVIDER: &str = "cleanup_models_by_provider";
pub const TRANSCRIPTION_DEFAULT_MODEL: &str = "transcription_default_model";
pub const CLEANUP_DEFAULT_MODEL: &str = "cleanup_default_model";
pub const TRANSCRIPTION_FALLBACK_MODELS: &str = "transcription_fallback_models";
pub const DUAL_TRANSCRIPTION_ENABLED: &str = "dual_transcription_enabled";
pub const CLEANUP_FALLBACK_MODELS: &str = "cleanup_fallback_models";
pub const CLEANUP_ENABLED: &str = "cleanup_enabled";
pub const HOTKEY: &str = "hotkey";
pub const MICROPHONE_DEVICE: &str = "microphone_device";
pub const DEFAULT_TONE: &str = "default_tone";
pub const CLEANUP_INTENSITY: &str = "cleanup_intensity";
pub const APP_MAPPINGS: &str = "app_mappings";
pub const NOISE_REDUCTION: &str = "noise_reduction";
pub const MUTE_AUDIO: &str = "mute_audio";
pub const EXCLUSIVE_MIC: &str = "exclusive_mic";
pub const PAUSE_MEDIA_DURING_DICTATION: &str = "pause_media_during_dictation";
pub const MIC_GAIN: &str = "mic_gain";
pub const PLAY_START_STOP_SOUNDS: &str = "play_start_stop_sounds";
pub const SOUND_EFFECTS_VOLUME: &str = "sound_effects_volume";
pub const SETUP_COMPLETE: &str = "setup_complete";
pub const CLIPBOARD_PHRASE: &str = "clipboard_phrase";
pub const CLIPBOARD_PHRASE_ENABLED: &str = "clipboard_phrase_enabled";
pub const LEGACY_FEATURES_ENABLED: &str = "legacy_features_enabled";
pub const SYNC_ENABLED: &str = "sync_enabled";
pub const APP_CONTEXT_HINT: &str = "app_context_hint";
pub const AUTO_LEARN_ENABLED: &str = "auto_learn_enabled";
pub const AUTO_LEARN_EVENT_MODE: &str = "auto_learn_event_mode";
pub const CONTEXTUAL_FORMATTING: &str = "contextual_formatting_enabled";
/// Legacy mirror of [`CONTEXTUAL_FORMATTING`], still written on every save so a
/// downgrade to an older build keeps working.
pub const CONTEXTUAL_CAPS: &str = "contextual_caps_enabled";
/// Legacy mirror of [`CONTEXTUAL_FORMATTING`]. See [`CONTEXTUAL_CAPS`].
pub const AUTO_SPACING: &str = "auto_spacing_enabled";
pub const APPEARANCE_MODE: &str = "appearance_mode";
pub const ACCENT_COLOR: &str = "accent_color";
pub const FORCE_SETUP_ON_LAUNCH: &str = "force_setup_on_launch";
pub const ADVANCED_MODEL_UI: &str = "advanced_model_ui";
/// One cleanup prompt for every model. Fallback chains made per-model prompts
/// a trap: edit the prompt on your default, fall back to another model, and the
/// edit silently vanished. `CLEANUP_PROMPT_OVERRIDES` is the retired per-model
/// map, still read once so an existing edit survives the change.
pub const CLEANUP_PROMPT_OVERRIDE: &str = "cleanup_prompt_override";
pub const CLEANUP_PROMPT_OVERRIDES: &str = "cleanup_prompt_overrides";
/// Per-provider snapshot of the live model lists, written only by the model
/// catalog store. Derived cache state, so it is readable but never exported —
/// one machine's stale view of a provider must not travel to another.
pub const PROVIDER_MODEL_CACHE: &str = "provider_model_cache";
pub const CREDENTIALS_MIGRATED: &str = "credentials_migrated_v1";
pub const UPDATE_DISMISSED_VERSION: &str = "update_dismissed_version";
pub const UPDATE_NOTIFIED_VERSION: &str = "update_notified_version";
pub const BETA_UPDATES_ENABLED: &str = "beta_updates_enabled";
pub const VERENU_SERVICE_CHECKS_ENABLED: &str = "verenu_service_checks_enabled";
pub const HISTORY_RETENTION: &str = "history_retention";
pub const AUTOSTART_ENABLED: &str = "autostart_enabled";
pub const CAPS_LOCK_UPPERCASE: &str = "caps_lock_uppercase_enabled";
pub const DEFAULT_CLIPBOARD_PHRASE: &str = "paste clipboard here";
pub const LOCAL_MODEL_MEMORY_POLICY: &str = "local_model_memory_policy";

pub const DEFAULT_TONES: &[&str] = &["casual", "formal", "very_casual"];
pub const CLEANUP_INTENSITIES: &[&str] = &["none", "light", "medium", "high"];
pub const HISTORY_RETENTION_OPTIONS: &[&str] = &["7 days", "30 days", "90 days", "Forever"];
pub const LOCAL_MODEL_MEMORY_POLICY_OPTIONS: &[&str] = &[
    "keep_loaded",
    "unload_after_5m",
    "unload_after_15m",
    "unload_immediately",
];

pub fn is_supported_default_tone(value: &str) -> bool {
    DEFAULT_TONES.contains(&value)
}

pub fn is_supported_cleanup_intensity(value: &str) -> bool {
    CLEANUP_INTENSITIES.contains(&value)
}

pub fn is_supported_history_retention(value: &str) -> bool {
    HISTORY_RETENTION_OPTIONS.contains(&value)
}

pub fn is_supported_local_model_memory_policy(value: &str) -> bool {
    LOCAL_MODEL_MEMORY_POLICY_OPTIONS.contains(&value)
}

/// Maps a `history_retention` setting value to a day count. `None` means
/// "Forever" (or an unrecognized value) — never prune.
pub fn history_retention_days(value: &str) -> Option<i64> {
    match value {
        "7 days" => Some(7),
        "30 days" => Some(30),
        "90 days" => Some(90),
        _ => None,
    }
}

mod config;
#[cfg(test)]
mod tests;

pub use config::*;

/// Folds the two legacy booleans into the single `contextual_formatting_enabled`
/// setting they were merged into.
///
/// Runs once at launch and is idempotent: if the new key already exists it does
/// nothing. Old installs get `caps AND spacing` — formatting is only considered
/// on if the user had both halves on, so nobody who deliberately turned one off
/// gets it silently switched back on. The legacy keys are left in place as
/// downgrade mirrors; every later save rewrites all three.
pub fn migrate_contextual_formatting(settings: &SettingsHandle) -> Result<(), String> {
    if settings.get(CONTEXTUAL_FORMATTING).is_some() {
        return Ok(());
    }

    let legacy = |key: &str| settings.get(key).and_then(|v| v.as_bool());
    let (caps, spacing) = (legacy(CONTEXTUAL_CAPS), legacy(AUTO_SPACING));
    if caps.is_none() && spacing.is_none() {
        // Fresh install: no legacy value to fold, so leave the key unset and
        // let the normal default apply.
        return Ok(());
    }

    let merged = caps.unwrap_or(true) && spacing.unwrap_or(true);
    settings.set(CONTEXTUAL_FORMATTING, Value::Bool(merged))?;
    settings.save()
}
