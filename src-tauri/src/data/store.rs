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
pub const MIC_GAIN: &str = "mic_gain";
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
pub const HISTORY_RETENTION: &str = "history_retention";
pub const AUTOSTART_ENABLED: &str = "autostart_enabled";
pub const CAPS_LOCK_UPPERCASE: &str = "caps_lock_uppercase_enabled";

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
    pub cleanup_prompt_overrides: std::collections::HashMap<String, String>,
}

pub const GROQ: &str = "groq";
pub const OPENAI: &str = "openai";
pub const GOOGLE: &str = "google";
pub const PROVIDERS: [&str; 3] = [GROQ, OPENAI, GOOGLE];

pub fn default_transcription_model_for(provider: &str) -> &'static str {
    match provider {
        OPENAI => "gpt-4o-transcribe",
        GOOGLE => "gemini-3.5-flash",
        _ => "whisper-large-v3-turbo",
    }
}

pub fn default_cleanup_model_for(provider: &str) -> &'static str {
    match provider {
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
        default_tone: str_or(DEFAULT_TONE, "casual"),
        cleanup_intensity: str_or(CLEANUP_INTENSITY, "medium"),
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
}

impl Default for AudioConfig {
    fn default() -> Self {
        Self {
            device: None,
            noise_reduction: true,
            mic_gain: DEFAULT_MIC_GAIN,
            mute_audio: false,
        }
    }
}

pub fn load_audio_config(store: &tauri_plugin_store::Store<tauri::Wry>) -> AudioConfig {
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

    AudioConfig {
        device,
        noise_reduction,
        mic_gain,
        mute_audio,
    }
}
