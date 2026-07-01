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
pub const CLEANUP_FALLBACK_MODELS: &str = "cleanup_fallback_models";
pub const CLEANUP_ENABLED: &str = "cleanup_enabled";
pub const HOTKEY: &str = "hotkey";
pub const MICROPHONE_DEVICE: &str = "microphone_device";
pub const DEFAULT_TONE: &str = "default_tone";
pub const CLEANUP_INTENSITY: &str = "cleanup_intensity";
pub const APP_MAPPINGS: &str = "app_mappings";
pub const NOISE_REDUCTION: &str = "noise_reduction";
pub const MUTE_AUDIO: &str = "mute_audio";
pub const PAUSE_MEDIA_DURING_DICTATION: &str = "pause_media_during_dictation";
pub const MIC_GAIN: &str = "mic_gain";
pub const PLAY_START_STOP_SOUNDS: &str = "play_start_stop_sounds";
pub const SETUP_COMPLETE: &str = "setup_complete";
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
    pub cleanup_fallback_models: Vec<String>,
    pub cleanup_enabled: bool,
    pub key_groq: String,
    pub key_openai: String,
    pub key_google: String,
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
pub(crate) const LOCAL: &str = "local";
pub const PROVIDERS: [&str; 4] = [GROQ, OPENAI, GOOGLE, LOCAL];

pub fn default_transcription_model_for(provider: &str) -> &'static str {
    match provider {
        LOCAL => "parakeet-v3",
        OPENAI => "gpt-4o-transcribe",
        GOOGLE => "gemini-3.5-flash",
        _ => "whisper-large-v3-turbo",
    }
}

pub fn default_cleanup_model_for(provider: &str) -> &'static str {
    match provider {
        LOCAL => "gemma-4-e2b",
        OPENAI => "gpt-4o-mini",
        GOOGLE => "gemini-3.5-flash",
        _ => "llama-3.3-70b-versatile",
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
                .map(|(p, m)| format!("{p}/{m}"))
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

    let transcription_fallback_models = parse_string_array(TRANSCRIPTION_FALLBACK_MODELS);
    let cleanup_fallback_models = parse_string_array(CLEANUP_FALLBACK_MODELS);
    let cleanup_prompt_overrides = store
        .get(CLEANUP_PROMPT_OVERRIDES)
        .and_then(|v| v.as_object().cloned())
        .unwrap_or_default()
        .into_iter()
        .filter_map(|(k, v)| v.as_str().map(|s| (k, s.to_string())))
        .collect();

    PipelineConfig {
        transcription_provider,
        transcription_language: language_or_default(TRANSCRIPTION_LANGUAGE, "en"),
        cleanup_provider,
        transcription_default_model,
        cleanup_default_model,
        transcription_fallback_models,
        cleanup_fallback_models,
        cleanup_enabled: store
            .get(CLEANUP_ENABLED)
            .and_then(|v| v.as_bool())
            .unwrap_or(true),
        key_groq: crate::data::credentials::get(GROQ),
        key_openai: crate::data::credentials::get(OPENAI),
        key_google: crate::data::credentials::get(GOOGLE),
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

pub struct AudioConfig {
    pub device: Option<String>,
    pub noise_reduction: bool,
    pub mic_gain: f32,
    pub mute_audio: bool,
    pub pause_media_during_dictation: bool,
    pub play_start_stop_sounds: bool,
}

impl Default for AudioConfig {
    fn default() -> Self {
        Self {
            device: None,
            noise_reduction: true,
            mic_gain: DEFAULT_MIC_GAIN,
            mute_audio: false,
            pause_media_during_dictation: false,
            play_start_stop_sounds: true,
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
    let pause_media_during_dictation = store
        .get(PAUSE_MEDIA_DURING_DICTATION)
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let play_start_stop_sounds = store
        .get(PLAY_START_STOP_SOUNDS)
        .and_then(|v| v.as_bool())
        .unwrap_or(true);

    AudioConfig {
        device,
        noise_reduction,
        mic_gain,
        mute_audio,
        pause_media_during_dictation,
        play_start_stop_sounds,
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

    // Start/stop sound cues default ON when the key is absent, and honor an
    // explicit false.
    #[test]
    fn load_audio_config_play_start_stop_sounds_default_and_override() {
        let empty = SettingsSnapshot::from_pairs([]);
        assert!(
            load_audio_config(&empty).play_start_stop_sounds,
            "should default to enabled"
        );

        let disabled = SettingsSnapshot::from_pairs([(
            PLAY_START_STOP_SOUNDS.to_string(),
            json!(false),
        )]);
        assert!(
            !load_audio_config(&disabled).play_start_stop_sounds,
            "explicit false must be honored"
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
}
