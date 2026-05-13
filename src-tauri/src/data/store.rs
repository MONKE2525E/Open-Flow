/// API key names in the store — never expose values to the frontend after write.
pub const KEY_GROQ:   &str = "api_key_groq";
pub const KEY_OPENAI: &str = "api_key_openai";
pub const KEY_GOOGLE: &str = "api_key_google";

pub const TRANSCRIPTION_PROVIDER: &str = "transcription_provider";
pub const CLEANUP_PROVIDER:       &str = "cleanup_provider";
pub const TRANSCRIPTION_MODEL:    &str = "transcription_model";
pub const CLEANUP_MODEL:          &str = "cleanup_model";
pub const CLEANUP_ENABLED:        &str = "cleanup_enabled";
pub const HOTKEY:                 &str = "hotkey";
pub const MICROPHONE_DEVICE:      &str = "microphone_device";
pub const DEFAULT_TONE:           &str = "default_tone";
pub const CLEANUP_INTENSITY:      &str = "cleanup_intensity";
pub const APP_MAPPINGS:           &str = "app_mappings";
pub const NOISE_REDUCTION:        &str = "noise_reduction";
pub const SETUP_COMPLETE:         &str = "setup_complete";

// ---------- pipeline config ----------

/// All settings values needed by run_pipeline, loaded in one place.
pub struct PipelineConfig {
    pub transcription_provider: String,
    pub cleanup_provider: String,
    pub cleanup_enabled: bool,
    pub key_groq: String,
    pub key_openai: String,
    pub key_google: String,
    pub default_tone: String,
    pub cleanup_intensity: String,
}

impl PipelineConfig {
    pub fn key_for(&self, provider: &str) -> &str {
        match provider {
            "openai" => &self.key_openai,
            "google" => &self.key_google,
            _        => &self.key_groq,
        }
    }
}

pub fn load_pipeline_config(store: &tauri_plugin_store::Store<tauri::Wry>) -> PipelineConfig {
    let str_val = |key: &str| -> String {
        store.get(key).and_then(|v| v.as_str().map(String::from)).unwrap_or_default()
    };
    let str_or = |key: &str, default: &str| -> String {
        let v = str_val(key);
        if v.is_empty() { default.into() } else { v }
    };

    PipelineConfig {
        transcription_provider: str_or(TRANSCRIPTION_PROVIDER, "groq"),
        cleanup_provider:       str_or(CLEANUP_PROVIDER, "groq"),
        cleanup_enabled: store.get(CLEANUP_ENABLED).and_then(|v| v.as_bool()).unwrap_or(true),
        key_groq:   str_val(KEY_GROQ),
        key_openai: str_val(KEY_OPENAI),
        key_google: str_val(KEY_GOOGLE),
        default_tone:      str_or(DEFAULT_TONE, "casual"),
        cleanup_intensity: str_or(CLEANUP_INTENSITY, "medium"),
    }
}
