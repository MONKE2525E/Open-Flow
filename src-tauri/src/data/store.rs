use tauri_plugin_store::StoreExt;

/// API key names in the store — never expose values to the frontend after write.
pub const KEY_GROQ: &str = "api_key_groq";
pub const KEY_OPENAI: &str = "api_key_openai";
pub const KEY_GOOGLE: &str = "api_key_google";

pub const TRANSCRIPTION_PROVIDER: &str = "transcription_provider";
pub const TRANSCRIPTION_LANGUAGE: &str = "transcription_language";
pub const CLEANUP_PROVIDER: &str = "cleanup_provider";
pub const TRANSCRIPTION_MODEL: &str = "transcription_model";
pub const CLEANUP_MODEL: &str = "cleanup_model";
pub const CLEANUP_ENABLED: &str = "cleanup_enabled";
pub const HOTKEY: &str = "hotkey";
pub const MICROPHONE_DEVICE: &str = "microphone_device";
pub const DEFAULT_TONE: &str = "default_tone";
pub const CLEANUP_INTENSITY: &str = "cleanup_intensity";
pub const APP_MAPPINGS: &str = "app_mappings";
pub const NOISE_REDUCTION: &str = "noise_reduction";
pub const MUTE_AUDIO: &str = "mute_audio";
pub const MIC_GAIN: &str = "mic_gain";
pub const SETUP_COMPLETE: &str = "setup_complete";
pub const APP_CONTEXT_HINT: &str = "app_context_hint";
pub const API_FALLBACK_ENABLED: &str = "api_fallback_enabled";
pub const AUTO_LEARN_ENABLED: &str = "auto_learn_enabled";
pub const CONTEXTUAL_CAPS: &str = "contextual_caps_enabled";
pub const APPEARANCE_MODE: &str = "appearance_mode";

// ---------- pipeline config ----------

/// All settings values needed by run_pipeline, loaded in one place.
pub struct PipelineConfig {
    pub transcription_provider: String,
    pub transcription_language: String,
    pub cleanup_provider: String,
    pub cleanup_enabled: bool,
    pub key_groq: String,
    pub key_openai: String,
    pub key_google: String,
    pub default_tone: String,
    pub cleanup_intensity: String,
    pub app_context_hint: bool,
    pub api_fallback_enabled: bool,
    pub auto_learn_enabled: bool,
    pub contextual_caps_enabled: bool,
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
            _ => &self.key_groq,
        }
    }
}

pub fn load_pipeline_config(store: &tauri_plugin_store::Store<tauri::Wry>) -> PipelineConfig {
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
    let language_or_default = |key: &str, default: &str| -> String {
        let v = str_or(key, default);
        if is_supported_transcription_language(&v) {
            v
        } else {
            default.into()
        }
    };

    PipelineConfig {
        transcription_provider: str_or(TRANSCRIPTION_PROVIDER, "groq"),
        transcription_language: language_or_default(TRANSCRIPTION_LANGUAGE, "en"),
        cleanup_provider: str_or(CLEANUP_PROVIDER, "groq"),
        cleanup_enabled: store
            .get(CLEANUP_ENABLED)
            .and_then(|v| v.as_bool())
            .unwrap_or(true),
        key_groq: str_val(KEY_GROQ),
        key_openai: str_val(KEY_OPENAI),
        key_google: str_val(KEY_GOOGLE),
        default_tone: str_or(DEFAULT_TONE, "casual"),
        cleanup_intensity: str_or(CLEANUP_INTENSITY, "medium"),
        app_context_hint: store
            .get(APP_CONTEXT_HINT)
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        api_fallback_enabled: store
            .get(API_FALLBACK_ENABLED)
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
    }
}

pub struct AudioConfig {
    pub device: Option<String>,
    pub noise_reduction: bool,
    pub mic_gain: f32,
    pub mute_audio: bool,
}

pub fn load_audio_config(app: &tauri::AppHandle) -> AudioConfig {
    let settings = match app.store("settings.json") {
        Ok(store) => Some(store),
        Err(e) => {
            log::warn!("Failed to load settings.json store for audio config: {:?}", e);
            None
        }
    };
    let device = settings
        .as_deref()
        .and_then(|s| s.get(MICROPHONE_DEVICE))
        .and_then(|v| v.as_str().map(String::from));
    let noise_reduction = settings
        .as_deref()
        .and_then(|s| s.get(NOISE_REDUCTION))
        .and_then(|v| v.as_bool())
        .unwrap_or(true);
    let mic_gain = settings
        .as_deref()
        .and_then(|s| s.get(MIC_GAIN))
        .and_then(|v| v.as_f64())
        .map(|v| v as f32)
        .unwrap_or(3.5)
        .clamp(1.0, 8.0);
    let mute_audio = settings
        .as_deref()
        .and_then(|s| s.get(MUTE_AUDIO))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    AudioConfig {
        device,
        noise_reduction,
        mic_gain,
        mute_audio,
    }
}
