use serde_json::{Map, Value};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Manager};

const SETTINGS_FILE: &str = "settings.json";

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
        self.set(key, value)?;
        self.save()
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
            .map_err(|e| format!("Failed to create settings directory: {e}"))?;
    }
    let json = serde_json::to_string_pretty(values)
        .map_err(|e| format!("Failed to serialize settings.json: {e}"))?;
    // Write to a temp file then atomically rename so an interrupted write
    // (crash, power loss, disk full) can't truncate the live settings.json.
    let tmp_path = path.with_extension("json.tmp");
    if let Err(e) = std::fs::write(&tmp_path, json) {
        // A failed/partial write shouldn't leave a stale temp file behind.
        let _ = std::fs::remove_file(&tmp_path);
        return Err(format!("Failed to write temporary settings file: {e}"));
    }
    if let Err(e) = std::fs::rename(&tmp_path, path) {
        // Don't leave the temp file behind if the swap failed.
        let _ = std::fs::remove_file(&tmp_path);
        return Err(format!("Failed to replace settings.json atomically: {e}"));
    }
    Ok(())
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
pub const LEGACY_FEATURES_ENABLED: &str = "legacy_features_enabled";
pub const APP_CONTEXT_HINT: &str = "app_context_hint";
pub const AUTO_LEARN_ENABLED: &str = "auto_learn_enabled";
pub const AUTO_LEARN_EVENT_MODE: &str = "auto_learn_event_mode";
pub const CONTEXTUAL_CAPS: &str = "contextual_caps_enabled";
pub const AUTO_SPACING: &str = "auto_spacing_enabled";
pub const APPEARANCE_MODE: &str = "appearance_mode";
pub const FORCE_SETUP_ON_LAUNCH: &str = "force_setup_on_launch";
pub const ADVANCED_MODEL_UI: &str = "advanced_model_ui";
pub const CLEANUP_PROMPT_OVERRIDES: &str = "cleanup_prompt_overrides";
pub const CREDENTIALS_MIGRATED: &str = "credentials_migrated_v1";
pub const MACOS_CLIPBOARD_SNIFF: &str = "macos_clipboard_sniff_enabled";
pub const UPDATE_DISMISSED_VERSION: &str = "update_dismissed_version";
pub const UPDATE_NOTIFIED_VERSION: &str = "update_notified_version";
pub const BETA_UPDATES_ENABLED: &str = "beta_updates_enabled";
pub const VERENU_SERVICE_CHECKS_ENABLED: &str = "verenu_service_checks_enabled";
pub const HISTORY_RETENTION: &str = "history_retention";
pub const AUTOSTART_ENABLED: &str = "autostart_enabled";
pub const CAPS_LOCK_UPPERCASE: &str = "caps_lock_uppercase_enabled";
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

// ---------- pipeline config ----------

/// All settings values needed by run_pipeline, loaded in one place.
#[derive(Clone, Debug)]
pub struct PipelineConfig {
    pub transcription_provider: String,
    pub transcription_language: String,
    pub cleanup_provider: String,
    pub transcription_default_model: String,
    pub cleanup_default_model: String,
    pub transcription_fallback_models: Vec<String>,
    pub dual_transcription_enabled: bool,
    pub cleanup_fallback_models: Vec<String>,
    pub cleanup_enabled: bool,
    pub key_groq: String,
    pub key_openai: String,
    pub key_google: String,
    pub key_assemblyai: String,
    pub default_tone: String,
    pub cleanup_intensity: String,
    pub app_context_hint: bool,
    pub auto_learn_enabled: bool,
    pub contextual_caps_enabled: bool,
    pub auto_spacing_enabled: bool,
    pub caps_lock_uppercase_enabled: bool,
    pub macos_clipboard_sniff_enabled: bool,
    pub advanced_model_ui: bool,
    pub local_model_memory_policy: String,
    pub cleanup_prompt_overrides: std::collections::HashMap<String, String>,
}

pub const GROQ: &str = "groq";
pub const OPENAI: &str = "openai";
pub const GOOGLE: &str = "google";
pub const ASSEMBLYAI: &str = "assemblyai";
pub(crate) const LOCAL: &str = "local";
pub(crate) const GROQ_GPT_OSS_20B_MODEL: &str = "openai/gpt-oss-20b";
pub(crate) const GROQ_QWEN_3_6_27B_MODEL: &str = "qwen/qwen3.6-27b";
pub(crate) const DEPRECATED_GROQ_LLAMA_8B_MODEL: &str = "llama-3.1-8b-instant";
pub(crate) const DEPRECATED_GROQ_LLAMA_70B_MODEL: &str = "llama-3.3-70b-versatile";
pub const PROVIDERS: [&str; 5] = [GROQ, OPENAI, GOOGLE, ASSEMBLYAI, LOCAL];

pub fn default_transcription_model_for(provider: &str) -> &'static str {
    match provider {
        LOCAL => "parakeet-v3",
        OPENAI => "gpt-4o-transcribe",
        GOOGLE => "gemini-3.5-flash",
        ASSEMBLYAI => "universal-3-5-pro",
        _ => "whisper-large-v3-turbo",
    }
}

pub fn default_cleanup_model_for(provider: &str) -> &'static str {
    match provider {
        LOCAL => "gemma-4-e2b",
        OPENAI => "gpt-4o-mini",
        GOOGLE => "gemini-3.5-flash",
        _ => GROQ_QWEN_3_6_27B_MODEL,
    }
}

pub fn migrate_deprecated_model_id(id: &str) -> String {
    let Some((provider, model)) = parse_model_id(id) else {
        return id.trim().to_string();
    };
    if provider == GROQ && model == DEPRECATED_GROQ_LLAMA_8B_MODEL {
        format!("{GROQ}/{GROQ_GPT_OSS_20B_MODEL}")
    } else if provider == GROQ && model == DEPRECATED_GROQ_LLAMA_70B_MODEL {
        format!("{GROQ}/{GROQ_QWEN_3_6_27B_MODEL}")
    } else {
        format!("{provider}/{model}")
    }
}

pub const TRANSCRIPTION_LANGUAGE_OPTIONS: &[(&str, &str)] = &[
    ("af", "Afrikaans"),
    ("ar", "Arabic"),
    ("hy", "Armenian"),
    ("az", "Azerbaijani"),
    ("be", "Belarusian"),
    ("bs", "Bosnian"),
    ("bg", "Bulgarian"),
    ("ca", "Catalan"),
    ("zh", "Chinese"),
    ("hr", "Croatian"),
    ("cs", "Czech"),
    ("da", "Danish"),
    ("nl", "Dutch"),
    ("en", "English"),
    ("et", "Estonian"),
    ("fi", "Finnish"),
    ("fr", "French"),
    ("gl", "Galician"),
    ("de", "German"),
    ("el", "Greek"),
    ("he", "Hebrew"),
    ("hi", "Hindi"),
    ("hu", "Hungarian"),
    ("is", "Icelandic"),
    ("id", "Indonesian"),
    ("it", "Italian"),
    ("ja", "Japanese"),
    ("kn", "Kannada"),
    ("kk", "Kazakh"),
    ("ko", "Korean"),
    ("lv", "Latvian"),
    ("lt", "Lithuanian"),
    ("mk", "Macedonian"),
    ("ms", "Malay"),
    ("mr", "Marathi"),
    ("mi", "Maori"),
    ("ne", "Nepali"),
    ("no", "Norwegian"),
    ("fa", "Persian"),
    ("pl", "Polish"),
    ("pt", "Portuguese"),
    ("ro", "Romanian"),
    ("ru", "Russian"),
    ("sr", "Serbian"),
    ("sk", "Slovak"),
    ("sl", "Slovenian"),
    ("es", "Spanish"),
    ("sw", "Swahili"),
    ("sv", "Swedish"),
    ("tl", "Tagalog"),
    ("ta", "Tamil"),
    ("th", "Thai"),
    ("tr", "Turkish"),
    ("uk", "Ukrainian"),
    ("ur", "Urdu"),
    ("vi", "Vietnamese"),
    ("cy", "Welsh"),
];

pub fn is_supported_transcription_language(code: &str) -> bool {
    TRANSCRIPTION_LANGUAGE_OPTIONS
        .iter()
        .any(|(candidate, _)| *candidate == code)
}

pub fn transcription_language_label(code: &str) -> &'static str {
    TRANSCRIPTION_LANGUAGE_OPTIONS
        .iter()
        .find_map(|(candidate, label)| (*candidate == code).then_some(*label))
        .unwrap_or("English")
}

impl PipelineConfig {
    pub fn key_for(&self, provider: &str) -> &str {
        match provider {
            "openai" => &self.key_openai,
            "google" => &self.key_google,
            "assemblyai" => &self.key_assemblyai,
            "local" => "",
            _ => &self.key_groq,
        }
    }

    /// Returns the user's custom cleanup prompt template for `provider/model`,
    /// or `None` if Advanced Models is off or no override is stored for this model.
    pub fn cleanup_override_for(&self, provider: &str, model: &str) -> Option<&str> {
        if !self.advanced_model_ui {
            return None;
        }
        let key = format!("{provider}/{model}");
        self.cleanup_prompt_overrides
            .get(&key)
            .map(String::as_str)
            .filter(|s| !s.trim().is_empty())
    }
}

pub fn parse_model_id(id: &str) -> Option<(String, String)> {
    let mut parts = id.splitn(2, '/');
    let provider = parts.next()?.trim().to_lowercase();
    let model = parts.next()?.trim().to_string();
    if PROVIDERS.contains(&provider.as_str()) && !model.is_empty() {
        Some((provider, model))
    } else {
        None
    }
}

pub fn load_pipeline_config(store: &SettingsSnapshot) -> PipelineConfig {
    let str_val = |key: &str| -> String {
        store
            .get(key)
            .and_then(|v| v.as_str().map(String::from))
            .unwrap_or_default()
    };
    let str_or = |key: &str, default: &str| -> String {
        let v = str_val(key);
        if v.is_empty() {
            default.into()
        } else {
            v
        }
    };
    let supported_or_default = |key: &str, default: &str, is_supported: fn(&str) -> bool| {
        let v = str_or(key, default);
        if is_supported(&v) {
            v
        } else {
            default.into()
        }
    };
    let language_or_default = |key: &str, default: &str| -> String {
        let v = str_or(key, default);
        if is_supported_transcription_language(&v) {
            v
        } else {
            default.into()
        }
    };
    let parse_string_array = |key: &str| -> Vec<String> {
        store
            .get(key)
            .and_then(|v| v.as_array().cloned())
            .unwrap_or_default()
            .into_iter()
            .filter_map(|v| v.as_str().map(str::trim).map(String::from))
            .filter(|v| !v.is_empty())
            .collect()
    };
    let transcription_provider = str_or(TRANSCRIPTION_PROVIDER, GROQ);
    let cleanup_provider = str_or(CLEANUP_PROVIDER, GROQ);
    let legacy_transcription_model = str_or(
        TRANSCRIPTION_MODEL,
        &format!("{}/{}", GROQ, default_transcription_model_for(GROQ)),
    );
    let legacy_cleanup_model = str_or(
        CLEANUP_MODEL,
        &format!("{}/{}", GROQ, default_cleanup_model_for(GROQ)),
    );

    let transcription_default_from_new = str_val(TRANSCRIPTION_DEFAULT_MODEL);
    let cleanup_default_from_new = str_val(CLEANUP_DEFAULT_MODEL);

    let resolve_default =
        |new_val: &str, legacy_val: &str, provider: &str, default_fn: fn(&str) -> &'static str| {
            parse_model_id(new_val)
                .or_else(|| parse_model_id(legacy_val))
                .map(|(p, m)| migrate_deprecated_model_id(&format!("{p}/{m}")))
                .unwrap_or_else(|| format!("{provider}/{}", default_fn(provider)))
        };

    let transcription_default_model = resolve_default(
        &transcription_default_from_new,
        &legacy_transcription_model,
        &transcription_provider,
        default_transcription_model_for,
    );

    let cleanup_default_model = resolve_default(
        &cleanup_default_from_new,
        &legacy_cleanup_model,
        &cleanup_provider,
        default_cleanup_model_for,
    );

    let transcription_fallback_models = parse_string_array(TRANSCRIPTION_FALLBACK_MODELS)
        .into_iter()
        .map(|id| migrate_deprecated_model_id(&id))
        .collect();
    let dual_transcription_enabled = store
        .get(DUAL_TRANSCRIPTION_ENABLED)
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let cleanup_fallback_models = parse_string_array(CLEANUP_FALLBACK_MODELS)
        .into_iter()
        .map(|id| migrate_deprecated_model_id(&id))
        .collect();
    let raw_cleanup_prompt_overrides = store
        .get(CLEANUP_PROMPT_OVERRIDES)
        .and_then(|v| v.as_object().cloned())
        .unwrap_or_default();
    let mut cleanup_prompt_overrides = std::collections::HashMap::new();
    for (key, value) in &raw_cleanup_prompt_overrides {
        if let Some(text) = value.as_str() {
            let migrated_key = migrate_deprecated_model_id(key);
            if migrated_key == *key {
                cleanup_prompt_overrides.insert(migrated_key, text.to_string());
            }
        }
    }
    for (key, value) in raw_cleanup_prompt_overrides {
        if let Some(text) = value.as_str() {
            let migrated_key = migrate_deprecated_model_id(&key);
            if migrated_key != key && !cleanup_prompt_overrides.contains_key(&migrated_key) {
                cleanup_prompt_overrides.insert(migrated_key, text.to_string());
            }
        }
    }

    PipelineConfig {
        transcription_provider,
        transcription_language: language_or_default(TRANSCRIPTION_LANGUAGE, "en"),
        cleanup_provider,
        transcription_default_model,
        cleanup_default_model,
        transcription_fallback_models,
        dual_transcription_enabled,
        cleanup_fallback_models,
        cleanup_enabled: store
            .get(CLEANUP_ENABLED)
            .and_then(|v| v.as_bool())
            .unwrap_or(true),
        key_groq: crate::data::credentials::get(GROQ),
        key_openai: crate::data::credentials::get(OPENAI),
        key_google: crate::data::credentials::get(GOOGLE),
        key_assemblyai: crate::data::credentials::get(ASSEMBLYAI),
        default_tone: supported_or_default(DEFAULT_TONE, "casual", is_supported_default_tone),
        cleanup_intensity: supported_or_default(
            CLEANUP_INTENSITY,
            "medium",
            is_supported_cleanup_intensity,
        ),
        app_context_hint: store
            .get(APP_CONTEXT_HINT)
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        auto_learn_enabled: store
            .get(AUTO_LEARN_ENABLED)
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        contextual_caps_enabled: store
            .get(CONTEXTUAL_CAPS)
            .and_then(|v| v.as_bool())
            .unwrap_or(true),
        auto_spacing_enabled: store
            .get(AUTO_SPACING)
            .and_then(|v| v.as_bool())
            .unwrap_or(true),
        caps_lock_uppercase_enabled: store
            .get(CAPS_LOCK_UPPERCASE)
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        macos_clipboard_sniff_enabled: store
            .get(MACOS_CLIPBOARD_SNIFF)
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        advanced_model_ui: store
            .get(ADVANCED_MODEL_UI)
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        cleanup_prompt_overrides,
        local_model_memory_policy: supported_or_default(
            LOCAL_MODEL_MEMORY_POLICY,
            "unload_after_5m",
            is_supported_local_model_memory_policy,
        ),
    }
}

pub const DEFAULT_MIC_GAIN: f32 = 3.5;
pub const MIN_MIC_GAIN: f32 = 1.0;
pub const MAX_MIC_GAIN: f32 = 8.0;
pub const DEFAULT_SOUND_EFFECTS_VOLUME: f32 = 1.0;

pub struct AudioConfig {
    pub device: Option<String>,
    pub noise_reduction: bool,
    pub mic_gain: f32,
    pub mute_audio: bool,
    pub exclusive_mic: bool,
    pub pause_media_during_dictation: bool,
    pub sound_effects_volume: f32,
}

impl Default for AudioConfig {
    fn default() -> Self {
        Self {
            device: None,
            noise_reduction: true,
            mic_gain: DEFAULT_MIC_GAIN,
            mute_audio: false,
            exclusive_mic: false,
            pause_media_during_dictation: false,
            sound_effects_volume: DEFAULT_SOUND_EFFECTS_VOLUME,
        }
    }
}

pub fn load_audio_config(store: &SettingsSnapshot) -> AudioConfig {
    let device = store
        .get(MICROPHONE_DEVICE)
        .and_then(|v| v.as_str().map(String::from));
    let noise_reduction = store
        .get(NOISE_REDUCTION)
        .and_then(|v| v.as_bool())
        .unwrap_or(true);
    let mic_gain = store
        .get(MIC_GAIN)
        .and_then(|v| v.as_f64())
        .map(|v| v as f32)
        .unwrap_or(DEFAULT_MIC_GAIN)
        .clamp(MIN_MIC_GAIN, MAX_MIC_GAIN);
    let mute_audio = store
        .get(MUTE_AUDIO)
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let exclusive_mic = store
        .get(EXCLUSIVE_MIC)
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let pause_media_during_dictation = store
        .get(PAUSE_MEDIA_DURING_DICTATION)
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let sound_effects_volume = store
        .get(SOUND_EFFECTS_VOLUME)
        .and_then(|v| v.as_f64())
        .map(|v| (v as f32 / 100.0).clamp(0.0, 1.0))
        .or_else(|| {
            store
                .get(PLAY_START_STOP_SOUNDS)
                .and_then(|v| v.as_bool())
                .map(|enabled| if enabled { 1.0 } else { 0.0 })
        })
        .unwrap_or(DEFAULT_SOUND_EFFECTS_VOLUME);

    AudioConfig {
        device,
        noise_reduction,
        mic_gain,
        mute_audio,
        exclusive_mic,
        pause_media_during_dictation,
        sound_effects_volume,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn unique_tmp_path() -> PathBuf {
        let mut p = std::env::temp_dir();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        p.push(format!("verenu_settings_test_{nanos}.json"));
        p
    }

    // Regression test: the atomic write must overwrite an existing settings.json
    // on every platform (std::fs::rename replaces the destination, including on
    // Windows). A second save to the same path must succeed, not fail.
    #[test]
    fn write_settings_file_overwrites_existing() {
        let path = unique_tmp_path();
        let mut first = Map::new();
        first.insert("hotkey".to_string(), json!(["CtrlLeft", "Space"]));
        write_settings_file(&path, &first).expect("first save should succeed");

        let mut second = Map::new();
        second.insert("hotkey".to_string(), json!(["AltLeft", "Space"]));
        write_settings_file(&path, &second).expect("overwrite save should succeed");

        let reloaded = read_settings_file(&path).expect("read back");
        assert_eq!(reloaded.get("hotkey"), Some(&json!(["AltLeft", "Space"])));

        let _ = std::fs::remove_file(&path);
    }

    // A corrupt settings.json must not surface an error; it is backed up and the
    // app starts from empty defaults.
    #[test]
    fn read_settings_file_recovers_from_corrupt_json() {
        let path = unique_tmp_path();
        std::fs::write(&path, b"{ not valid json").unwrap();

        let recovered = read_settings_file(&path).expect("corrupt file recovers to defaults");
        assert!(recovered.is_empty());

        let backup = path.with_extension("json.bak");
        assert!(backup.exists(), "corrupt file should be backed up");

        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(&backup);
    }

    // The new volume setting takes precedence, while the old boolean setting
    // remains a one-time compatibility fallback for existing users.
    #[test]
    fn load_audio_config_sound_effects_volume_default_and_legacy_override() {
        let empty = SettingsSnapshot::from_pairs([]);
        assert_eq!(load_audio_config(&empty).sound_effects_volume, 1.0);

        let disabled = SettingsSnapshot::from_pairs([(
            PLAY_START_STOP_SOUNDS.to_string(),
            json!(false),
        )]);
        assert_eq!(load_audio_config(&disabled).sound_effects_volume, 0.0);

        let explicit_volume = SettingsSnapshot::from_pairs([
            (PLAY_START_STOP_SOUNDS.to_string(), json!(false)),
            (SOUND_EFFECTS_VOLUME.to_string(), json!(35)),
        ]);
        assert_eq!(
            load_audio_config(&explicit_volume).sound_effects_volume,
            0.35
        );
    }

    #[test]
    fn load_audio_config_pause_media_default_and_override() {
        let empty = SettingsSnapshot::from_pairs([]);
        assert!(
            !load_audio_config(&empty).pause_media_during_dictation,
            "media pause should default to disabled"
        );

        let enabled =
            SettingsSnapshot::from_pairs([(PAUSE_MEDIA_DURING_DICTATION.to_string(), json!(true))]);
        assert!(
            load_audio_config(&enabled).pause_media_during_dictation,
            "explicit true must be honored"
        );
    }

    // ── setting_audit_* regression tests (targeted by the OnePyFone harness) ──

    /// Fresh install (empty settings.json) must resolve to the documented
    /// product defaults, not empty strings or false positives.
    #[test]
    fn setting_audit_empty_store_resolves_to_documented_defaults() {
        let empty = SettingsSnapshot::from_pairs([]);
        let cfg = load_pipeline_config(&empty);
        let audio = load_audio_config(&empty);

        assert_eq!(cfg.transcription_provider, GROQ);
        assert_eq!(cfg.cleanup_provider, GROQ);
        assert_eq!(cfg.transcription_language, "en");
        assert_eq!(
            cfg.transcription_default_model,
            format!("{GROQ}/whisper-large-v3-turbo")
        );
        assert_eq!(
            cfg.cleanup_default_model,
            format!("{GROQ}/{GROQ_QWEN_3_6_27B_MODEL}")
        );
        assert!(cfg.transcription_fallback_models.is_empty());
        assert!(cfg.cleanup_fallback_models.is_empty());
        assert!(!cfg.dual_transcription_enabled);
        assert!(cfg.cleanup_enabled, "cleanup should default to on");
        assert_eq!(cfg.default_tone, "casual");
        assert_eq!(cfg.cleanup_intensity, "medium");
        assert!(!cfg.app_context_hint);
        assert!(!cfg.auto_learn_enabled);
        assert!(cfg.contextual_caps_enabled, "contextual caps default on");
        assert!(cfg.auto_spacing_enabled, "auto spacing default on");
        assert!(!cfg.caps_lock_uppercase_enabled);
        assert!(!cfg.advanced_model_ui);
        assert_eq!(cfg.local_model_memory_policy, "unload_after_5m");

        assert!(audio.noise_reduction, "noise reduction default on");
        assert_eq!(audio.mic_gain, DEFAULT_MIC_GAIN);
        assert_eq!(audio.sound_effects_volume, 1.0);
        assert!(!audio.mute_audio);
        assert!(!audio.exclusive_mic);
        assert!(!audio.pause_media_during_dictation);
        assert!(audio.device.is_none());
    }

    /// A legacy `transcription_model`/`cleanup_model` (provider-prefixed) must
    /// migrate into the new `*_default_model` resolution even when the new key
    /// is absent. Older builds always wrote the full `provider/model` id.
    #[test]
    fn setting_audit_legacy_model_keys_migrate_to_default() {
        let store = SettingsSnapshot::from_pairs([
            (
                TRANSCRIPTION_MODEL.to_string(),
                json!("openai/gpt-4o-transcribe"),
            ),
            (CLEANUP_MODEL.to_string(), json!("openai/gpt-4o-mini")),
        ]);
        let cfg = load_pipeline_config(&store);
        assert_eq!(cfg.transcription_default_model, "openai/gpt-4o-transcribe");
        assert_eq!(cfg.cleanup_default_model, "openai/gpt-4o-mini");
    }

    /// An unparseable model id must not panic or pass through. Resolution
    /// prefers new key → legacy key → provider default; a legacy key that is
    /// absent resolves to the groq default (the legacy default), so an invalid
    /// new key with no legacy value also lands on the groq default.
    #[test]
    fn setting_audit_malformed_model_id_resolves_to_safe_default() {
        let store = SettingsSnapshot::from_pairs([
            (
                TRANSCRIPTION_DEFAULT_MODEL.to_string(),
                json!("not-a-model-id"),
            ),
            (CLEANUP_DEFAULT_MODEL.to_string(), json!("")),
            (TRANSCRIPTION_PROVIDER.to_string(), json!(OPENAI)),
        ]);
        let cfg = load_pipeline_config(&store);
        assert_eq!(
            cfg.transcription_default_model,
            format!("{GROQ}/whisper-large-v3-turbo"),
            "malformed new key + absent legacy key must fall back to the legacy groq default"
        );
        assert_eq!(
            cfg.cleanup_default_model,
            format!("{GROQ}/{GROQ_QWEN_3_6_27B_MODEL}")
        );
    }

    #[test]
    fn deprecated_groq_cleanup_models_migrate_to_gpt_oss() {
        let store = SettingsSnapshot::from_pairs([
            (
                CLEANUP_DEFAULT_MODEL.to_string(),
                json!("groq/llama-3.1-8b-instant"),
            ),
            (
                CLEANUP_FALLBACK_MODELS.to_string(),
                json!(["groq/llama-3.1-8b-instant", "openai/gpt-4o-mini"]),
            ),
        ]);
        let cfg = load_pipeline_config(&store);
        assert_eq!(
            cfg.cleanup_default_model,
            format!("{GROQ}/{GROQ_GPT_OSS_20B_MODEL}")
        );
        assert_eq!(
            cfg.cleanup_fallback_models,
            vec![
                format!("{GROQ}/{GROQ_GPT_OSS_20B_MODEL}"),
                "openai/gpt-4o-mini".to_string()
            ]
        );
    }

    #[test]
    fn deprecated_groq_llama_70b_migrates_to_qwen() {
        let store = SettingsSnapshot::from_pairs([
            (
                CLEANUP_DEFAULT_MODEL.to_string(),
                json!("groq/llama-3.3-70b-versatile"),
            ),
            (
                CLEANUP_FALLBACK_MODELS.to_string(),
                json!(["groq/llama-3.3-70b-versatile", "openai/gpt-4o-mini"]),
            ),
        ]);
        let cfg = load_pipeline_config(&store);
        assert_eq!(
            cfg.cleanup_default_model,
            format!("{GROQ}/{GROQ_QWEN_3_6_27B_MODEL}")
        );
        assert_eq!(
            cfg.cleanup_fallback_models,
            vec![
                format!("{GROQ}/{GROQ_QWEN_3_6_27B_MODEL}"),
                "openai/gpt-4o-mini".to_string()
            ]
        );
    }

    #[test]
    fn migrated_cleanup_override_prefers_existing_current_model_key() {
        let store = SettingsSnapshot::from_pairs([(
            CLEANUP_PROMPT_OVERRIDES.to_string(),
            json!({
                "groq/llama-3.3-70b-versatile": "legacy",
                "groq/qwen/qwen3.6-27b": "current"
            }),
        )]);
        let cfg = load_pipeline_config(&store);
        assert_eq!(
            cfg.cleanup_prompt_overrides.get("groq/qwen/qwen3.6-27b"),
            Some(&"current".to_string())
        );
    }

    /// Unknown enum values must be coerced back to the backend default rather
    /// than passed through to the pipeline.
    #[test]
    fn setting_audit_unknown_enum_values_fall_back_to_default() {
        let store = SettingsSnapshot::from_pairs([
            (DEFAULT_TONE.to_string(), json!("business")),
            (CLEANUP_INTENSITY.to_string(), json!("extreme")),
            (
                LOCAL_MODEL_MEMORY_POLICY.to_string(),
                json!("always_loaded"),
            ),
            (TRANSCRIPTION_LANGUAGE.to_string(), json!("xx")),
        ]);
        let cfg = load_pipeline_config(&store);
        assert_eq!(cfg.default_tone, "casual");
        assert_eq!(cfg.cleanup_intensity, "medium");
        assert_eq!(cfg.local_model_memory_policy, "unload_after_5m");
        assert_eq!(cfg.transcription_language, "en");
    }

    /// `mic_gain` stored outside the valid range must be clamped at load time,
    /// matching the slider's 1.0..=8.0 contract.
    #[test]
    fn setting_audit_mic_gain_clamped_at_load() {
        let below = SettingsSnapshot::from_pairs([(MIC_GAIN.to_string(), json!(0.2))]);
        assert_eq!(load_audio_config(&below).mic_gain, MIN_MIC_GAIN);

        let above = SettingsSnapshot::from_pairs([(MIC_GAIN.to_string(), json!(99.0))]);
        assert_eq!(load_audio_config(&above).mic_gain, MAX_MIC_GAIN);

        let in_range = SettingsSnapshot::from_pairs([(MIC_GAIN.to_string(), json!(4.5))]);
        assert_eq!(load_audio_config(&in_range).mic_gain, 4.5);
    }

    /// A corrupt `mic_gain` type (string) must fall back to the default, not panic.
    #[test]
    fn setting_audit_mic_gain_wrong_type_falls_back_to_default() {
        let store = SettingsSnapshot::from_pairs([(MIC_GAIN.to_string(), json!("loud"))]);
        assert_eq!(load_audio_config(&store).mic_gain, DEFAULT_MIC_GAIN);
    }

    /// history_retention_days must map every supported label and return None
    /// (never prune) for "Forever" and anything unrecognized.
    #[test]
    fn setting_audit_history_retention_days_mapping() {
        assert_eq!(history_retention_days("7 days"), Some(7));
        assert_eq!(history_retention_days("30 days"), Some(30));
        assert_eq!(history_retention_days("90 days"), Some(90));
        assert_eq!(history_retention_days("Forever"), None);
        assert_eq!(history_retention_days("365 days"), None);
        assert_eq!(history_retention_days(""), None);
    }

}
