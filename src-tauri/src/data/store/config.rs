//! Pipeline and audio configuration types, provider/model constants, and
//! the single-place settings loaders used by the pipeline.

use super::*;
// ---------- pipeline config ----------

/// All settings values needed by run_pipeline, loaded in one place.
#[derive(Clone, Debug, Default)]
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
    pub contextual_formatting_enabled: bool,
    pub caps_lock_uppercase_enabled: bool,
    pub clipboard_phrase_enabled: bool,
    pub clipboard_phrase: String,
    pub advanced_model_ui: bool,
    pub local_model_memory_policy: String,
    pub cleanup_prompt_overrides: std::collections::HashMap<String, String>,
}

pub const GROQ: &str = "groq";
pub const OPENAI: &str = "openai";
pub const GOOGLE: &str = "google";
pub const ASSEMBLYAI: &str = "assemblyai";
pub(crate) const LOCAL: &str = "local";
pub const GROQ_GPT_OSS_20B_MODEL: &str = "openai/gpt-oss-20b";
pub const GROQ_QWEN_3_6_27B_MODEL: &str = "qwen/qwen3.6-27b";
pub const DEPRECATED_GROQ_LLAMA_8B_MODEL: &str = "llama-3.1-8b-instant";
pub const DEPRECATED_GROQ_LLAMA_70B_MODEL: &str = "llama-3.3-70b-versatile";
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
    let cleanup_prompt_overrides = store
        .get(CLEANUP_PROMPT_OVERRIDES)
        .and_then(|v| v.as_object().cloned())
        .unwrap_or_default()
        .into_iter()
        .filter_map(|(k, v)| {
            v.as_str()
                .map(|s| (migrate_deprecated_model_id(&k), s.to_string()))
        })
        .collect();

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
        contextual_formatting_enabled: store
            .get(CONTEXTUAL_FORMATTING)
            .and_then(|v| v.as_bool())
            .unwrap_or(true),
        caps_lock_uppercase_enabled: store
            .get(CAPS_LOCK_UPPERCASE)
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        clipboard_phrase_enabled: store
            .get(CLIPBOARD_PHRASE_ENABLED)
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        clipboard_phrase: store
            .get(CLIPBOARD_PHRASE)
            .and_then(|v| v.as_str())
            .map(normalize_clipboard_phrase)
            .filter(|v| is_valid_clipboard_phrase(v))
            .unwrap_or_else(|| DEFAULT_CLIPBOARD_PHRASE.to_string()),
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

pub fn normalize_clipboard_phrase(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub fn is_valid_clipboard_phrase(value: &str) -> bool {
    let count = value.chars().count();
    (5..=80).contains(&count) && value.chars().any(char::is_alphanumeric)
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
