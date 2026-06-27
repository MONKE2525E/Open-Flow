//! Settings, API keys, prompt configuration, and data import/export.

use super::*;

mod api_keys;
mod import_export;
mod prompts;

pub use api_keys::*;
pub use import_export::*;
pub use prompts::*;

const CLEANUP_PROMPT_OVERRIDE_CHAR_LIMIT: usize = 20_000;

#[derive(Clone, Copy)]
enum SettingKind {
    Provider,
    TranscriptionLanguage,
    StringOrNull,
    DefaultTone,
    CleanupIntensity,
    HistoryRetention,
    ModelMap,
    StringArray,
    CleanupPromptOverrides,
    AppearanceMode,
    Bool,
    MicGain,
    AppMappings,
    Hotkey,
}

#[derive(Clone, Copy)]
pub struct SettingSpec {
    key: &'static str,
    kind: SettingKind,
    readable: bool,
    exportable: bool,
}

const fn setting_spec(
    key: &'static str,
    kind: SettingKind,
    readable: bool,
    exportable: bool,
) -> SettingSpec {
    SettingSpec {
        key,
        kind,
        readable,
        exportable,
    }
}

const SETTING_SPECS: &[SettingSpec] = &[
    setting_spec(
        store::TRANSCRIPTION_PROVIDER,
        SettingKind::Provider,
        true,
        true,
    ),
    setting_spec(
        store::TRANSCRIPTION_LANGUAGE,
        SettingKind::TranscriptionLanguage,
        true,
        true,
    ),
    setting_spec(store::CLEANUP_PROVIDER, SettingKind::Provider, true, true),
    setting_spec(
        store::TRANSCRIPTION_MODEL,
        SettingKind::StringOrNull,
        true,
        true,
    ),
    setting_spec(store::CLEANUP_MODEL, SettingKind::StringOrNull, true, true),
    setting_spec(
        store::TRANSCRIPTION_MODELS_BY_PROVIDER,
        SettingKind::ModelMap,
        true,
        true,
    ),
    setting_spec(
        store::CLEANUP_MODELS_BY_PROVIDER,
        SettingKind::ModelMap,
        true,
        true,
    ),
    setting_spec(
        store::TRANSCRIPTION_DEFAULT_MODEL,
        SettingKind::StringOrNull,
        true,
        true,
    ),
    setting_spec(
        store::CLEANUP_DEFAULT_MODEL,
        SettingKind::StringOrNull,
        true,
        true,
    ),
    setting_spec(
        store::TRANSCRIPTION_FALLBACK_MODELS,
        SettingKind::StringArray,
        true,
        true,
    ),
    setting_spec(
        store::CLEANUP_FALLBACK_MODELS,
        SettingKind::StringArray,
        true,
        true,
    ),
    setting_spec(store::CLEANUP_ENABLED, SettingKind::Bool, true, true),
    setting_spec(store::HOTKEY, SettingKind::Hotkey, true, true),
    setting_spec(
        store::MICROPHONE_DEVICE,
        SettingKind::StringOrNull,
        true,
        false,
    ),
    setting_spec(store::DEFAULT_TONE, SettingKind::DefaultTone, true, true),
    setting_spec(
        store::CLEANUP_INTENSITY,
        SettingKind::CleanupIntensity,
        true,
        true,
    ),
    setting_spec(store::APP_MAPPINGS, SettingKind::AppMappings, true, true),
    setting_spec(store::NOISE_REDUCTION, SettingKind::Bool, true, true),
    setting_spec(store::MUTE_AUDIO, SettingKind::Bool, true, true),
    setting_spec(
        store::PAUSE_MEDIA_DURING_DICTATION,
        SettingKind::Bool,
        true,
        true,
    ),
    setting_spec(store::MIC_GAIN, SettingKind::MicGain, true, false),
    setting_spec(store::PLAY_START_STOP_SOUNDS, SettingKind::Bool, true, true),
    setting_spec(store::SETUP_COMPLETE, SettingKind::Bool, true, false),
    setting_spec(store::APP_CONTEXT_HINT, SettingKind::Bool, true, true),
    setting_spec(store::AUTO_LEARN_ENABLED, SettingKind::Bool, true, true),
    setting_spec(store::AUTO_LEARN_EVENT_MODE, SettingKind::Bool, true, true),
    setting_spec(store::CONTEXTUAL_CAPS, SettingKind::Bool, true, true),
    setting_spec(store::AUTO_SPACING, SettingKind::Bool, true, true),
    setting_spec(
        store::APPEARANCE_MODE,
        SettingKind::AppearanceMode,
        true,
        true,
    ),
    setting_spec(store::FORCE_SETUP_ON_LAUNCH, SettingKind::Bool, true, false),
    setting_spec(store::ADVANCED_MODEL_UI, SettingKind::Bool, true, true),
    setting_spec(
        store::CLEANUP_PROMPT_OVERRIDES,
        SettingKind::CleanupPromptOverrides,
        true,
        true,
    ),
    setting_spec(
        store::UPDATE_DISMISSED_VERSION,
        SettingKind::StringOrNull,
        true,
        false,
    ),
    setting_spec(
        store::UPDATE_NOTIFIED_VERSION,
        SettingKind::StringOrNull,
        true,
        false,
    ),
    setting_spec(
        store::HISTORY_RETENTION,
        SettingKind::HistoryRetention,
        true,
        true,
    ),
    setting_spec(store::AUTOSTART_ENABLED, SettingKind::Bool, true, true),
    setting_spec(store::CAPS_LOCK_UPPERCASE, SettingKind::Bool, true, true),
];

fn spec_for(key: &str) -> Option<&'static SettingSpec> {
    SETTING_SPECS.iter().find(|spec| spec.key == key)
}

pub fn is_readable_setting_key(key: &str) -> bool {
    spec_for(key).is_some_and(|spec| spec.readable)
}

pub fn is_exportable_setting_key(key: &str) -> bool {
    spec_for(key).is_some_and(|spec| spec.exportable)
}

fn exportable_setting_keys() -> impl Iterator<Item = &'static str> {
    SETTING_SPECS
        .iter()
        .filter(|spec| spec.exportable)
        .map(|spec| spec.key)
}

pub fn validate_setting(key: &str, value: &serde_json::Value) -> Result<(), String> {
    let is_model_map = |v: &serde_json::Value| {
        let Some(obj) = v.as_object() else {
            return false;
        };
        obj.keys().all(|k| store::PROVIDERS.contains(&k.as_str()))
            && obj.values().all(|val| {
                val.as_array().is_some_and(|arr| {
                    arr.iter()
                        .all(|x| x.as_str().is_some_and(|s| !s.trim().is_empty()))
                })
            })
    };
    let is_non_empty_string_array = |v: &serde_json::Value| {
        v.as_array().is_some_and(|arr| {
            arr.iter()
                .all(|x| x.as_str().is_some_and(|s| !s.trim().is_empty()))
        })
    };
    let is_cleanup_prompt_override_map = |v: &serde_json::Value| {
        v.as_object().is_some_and(|obj| {
            obj.iter().all(|(model_id, template)| {
                store::parse_model_id(model_id).is_some()
                    && template.as_str().is_some_and(|text| {
                        text.chars().count() <= CLEANUP_PROMPT_OVERRIDE_CHAR_LIMIT
                    })
            })
        })
    };
    let is_valid_app_mappings = |v: &serde_json::Value| {
        let Ok(mappings) = serde_json::from_value::<Vec<AppMapping>>(v.clone()) else {
            return false;
        };
        let mut seen = std::collections::HashSet::new();
        mappings.iter().all(|mapping| {
            let exe = mapping.exe.trim().to_lowercase();
            let profile = mapping.profile.trim();
            !exe.is_empty()
                && seen.insert(exe)
                && store::is_supported_default_tone(profile)
                && mapping
                    .cleanup_intensity
                    .as_deref()
                    .map(str::trim)
                    .is_none_or(|value| {
                        value.is_empty() || store::is_supported_cleanup_intensity(value)
                    })
        })
    };
    let Some(spec) = spec_for(key) else {
        return Err(format!("Invalid or unsupported setting: {key}"));
    };
    let valid = match spec.kind {
        SettingKind::Provider => value
            .as_str()
            .is_some_and(|v| store::PROVIDERS.contains(&v)),
        SettingKind::TranscriptionLanguage => value
            .as_str()
            .is_some_and(store::is_supported_transcription_language),
        SettingKind::StringOrNull => value.is_string() || value.is_null(),
        SettingKind::DefaultTone => value.as_str().is_some_and(store::is_supported_default_tone),
        SettingKind::CleanupIntensity => value
            .as_str()
            .is_some_and(store::is_supported_cleanup_intensity),
        SettingKind::HistoryRetention => value
            .as_str()
            .is_some_and(store::is_supported_history_retention),
        SettingKind::ModelMap => is_model_map(value),
        SettingKind::StringArray => is_non_empty_string_array(value),
        SettingKind::CleanupPromptOverrides => is_cleanup_prompt_override_map(value),
        SettingKind::AppearanceMode => value
            .as_str()
            .is_some_and(|v| matches!(v, "system" | "light" | "dark")),
        SettingKind::Bool => value.is_boolean(),
        SettingKind::MicGain => value.as_f64().is_some_and(|v| (1.0..=8.0).contains(&v)),
        SettingKind::AppMappings => is_valid_app_mappings(value),
        SettingKind::Hotkey => value
            .as_array()
            .is_some_and(|keys| keys.len() == 2 && keys.iter().all(serde_json::Value::is_string)),
    };

    if valid {
        Ok(())
    } else {
        Err(format!("Invalid or unsupported setting: {key}"))
    }
}

#[cfg(test)]
mod setting_key_tests {
    use super::*;

    #[test]
    fn readable_settings_exclude_credential_keys() {
        assert!(is_readable_setting_key(store::APPEARANCE_MODE));
        assert!(!is_readable_setting_key(store::KEY_GROQ));
        assert!(!is_readable_setting_key(store::KEY_OPENAI));
        assert!(!is_readable_setting_key(store::KEY_GOOGLE));
    }

    #[test]
    fn exportable_settings_exclude_credential_keys() {
        assert!(is_exportable_setting_key(store::APP_MAPPINGS));
        assert!(!is_exportable_setting_key(store::KEY_GROQ));
        assert!(!is_exportable_setting_key(store::KEY_OPENAI));
        assert!(!is_exportable_setting_key(store::KEY_GOOGLE));
    }

    #[test]
    fn pause_media_during_dictation_is_boolean_setting() {
        assert!(is_readable_setting_key(store::PAUSE_MEDIA_DURING_DICTATION));
        assert!(is_exportable_setting_key(
            store::PAUSE_MEDIA_DURING_DICTATION
        ));
        assert!(validate_setting(
            store::PAUSE_MEDIA_DURING_DICTATION,
            &serde_json::json!(true)
        )
        .is_ok());
        assert!(validate_setting(
            store::PAUSE_MEDIA_DURING_DICTATION,
            &serde_json::json!("yes")
        )
        .is_err());
    }
}
// ---------- generic settings ----------

#[tauri::command]
pub async fn save_setting(
    app: AppHandle,
    key: String,
    value: serde_json::Value,
) -> Result<(), String> {
    validate_setting(&key, &value)?;
    let history_prune_days = if key == store::HISTORY_RETENTION {
        value.as_str().and_then(store::history_retention_days)
    } else {
        None
    };
    let settings = store::settings_handle(&app)?;
    let key_clone = key.clone();
    run_blocking("save_setting", move || {
        settings.save_value(key_clone, value)
    })
    .await?;

    if key == store::APPEARANCE_MODE {
        crate::apply_runtime_icons(&app, None);
    }

    if let Some(days) = history_prune_days {
        let db = app.state::<DbHandle>().inner().clone();
        let deleted =
            tokio::task::spawn_blocking(move || db::prune_transcriptions_older_than(&db, days))
                .await
                .map_err(|e| e.to_string())?
                .map_err(|e| e.to_string())?;
        if deleted > 0 {
            let _ = app.emit("verenu:history-pruned", ());
        }
    }

    Ok(())
}

#[tauri::command]
pub async fn get_setting(app: AppHandle, key: String) -> Result<Option<serde_json::Value>, String> {
    if !is_readable_setting_key(&key) {
        return Err(format!("Unsupported setting key: {key}"));
    }
    Ok(store::settings_handle(&app)?.get(&key))
}

#[derive(serde::Serialize)]
pub struct AllSettings {
    pub transcription_provider: Option<String>,
    pub transcription_model: Option<String>,
    pub transcription_language: Option<String>,
    pub cleanup_provider: Option<String>,
    pub cleanup_model: Option<String>,
    pub transcription_models_by_provider: Option<serde_json::Value>,
    pub cleanup_models_by_provider: Option<serde_json::Value>,
    pub transcription_default_model: Option<String>,
    pub cleanup_default_model: Option<String>,
    pub transcription_fallback_models: Option<Vec<String>>,
    pub cleanup_fallback_models: Option<Vec<String>>,
    pub advanced_model_ui: Option<bool>,
    pub cleanup_enabled: Option<bool>,
    pub noise_reduction: Option<bool>,
    pub mute_audio: Option<bool>,
    pub pause_media_during_dictation: Option<bool>,
    pub play_start_stop_sounds: Option<bool>,
    pub autostart_enabled: Option<bool>,
    pub app_context_hint: Option<bool>,
    pub auto_learn_enabled: Option<bool>,
    pub contextual_caps_enabled: Option<bool>,
    pub auto_spacing_enabled: Option<bool>,
    pub caps_lock_uppercase_enabled: Option<bool>,
    pub mic_gain: Option<f64>,
    pub history_retention: Option<String>,
    pub microphone_device: Option<String>,
    pub update_dismissed_version: Option<String>,
    pub update_notified_version: Option<String>,
    pub hotkey: Option<Vec<String>>,
    pub appearance_mode: Option<String>,
    pub cleanup_prompt_overrides: Option<serde_json::Value>,
}

#[derive(serde::Serialize)]
pub struct CleanupCacheStatus {
    pub entry_count: i64,
    pub is_space_constrained: bool,
    pub free_bytes: u64,
}

#[tauri::command]
pub async fn get_all_settings(app: AppHandle) -> Result<AllSettings, String> {
    let s = store::settings_snapshot(&app)?;
    let bool_val = |key: &str| s.get(key).and_then(|v| v.as_bool());
    let str_val = |key: &str| s.get(key).and_then(|v| v.as_str().map(String::from));
    let f64_val = |key: &str| s.get(key).and_then(|v| v.as_f64());
    let json_val = |key: &str| s.get_cloned(key);
    let str_array_val = |key: &str| {
        s.get(key).and_then(|v| {
            v.as_array().map(|arr| {
                arr.iter()
                    .filter_map(|x| x.as_str().map(String::from))
                    .collect()
            })
        })
    };
    Ok(AllSettings {
        transcription_provider: str_val(store::TRANSCRIPTION_PROVIDER),
        transcription_model: str_val(store::TRANSCRIPTION_MODEL),
        transcription_language: str_val(store::TRANSCRIPTION_LANGUAGE),
        cleanup_provider: str_val(store::CLEANUP_PROVIDER),
        cleanup_model: str_val(store::CLEANUP_MODEL),
        transcription_models_by_provider: json_val(store::TRANSCRIPTION_MODELS_BY_PROVIDER),
        cleanup_models_by_provider: json_val(store::CLEANUP_MODELS_BY_PROVIDER),
        transcription_default_model: str_val(store::TRANSCRIPTION_DEFAULT_MODEL),
        cleanup_default_model: str_val(store::CLEANUP_DEFAULT_MODEL),
        transcription_fallback_models: str_array_val(store::TRANSCRIPTION_FALLBACK_MODELS),
        cleanup_fallback_models: str_array_val(store::CLEANUP_FALLBACK_MODELS),
        advanced_model_ui: bool_val(store::ADVANCED_MODEL_UI),
        cleanup_enabled: bool_val(store::CLEANUP_ENABLED),
        noise_reduction: bool_val(store::NOISE_REDUCTION),
        mute_audio: bool_val(store::MUTE_AUDIO),
        pause_media_during_dictation: bool_val(store::PAUSE_MEDIA_DURING_DICTATION),
        play_start_stop_sounds: bool_val(store::PLAY_START_STOP_SOUNDS),
        autostart_enabled: bool_val(store::AUTOSTART_ENABLED),
        app_context_hint: bool_val(store::APP_CONTEXT_HINT),
        auto_learn_enabled: bool_val(store::AUTO_LEARN_ENABLED),
        contextual_caps_enabled: bool_val(store::CONTEXTUAL_CAPS),
        auto_spacing_enabled: bool_val(store::AUTO_SPACING),
        caps_lock_uppercase_enabled: bool_val(store::CAPS_LOCK_UPPERCASE),
        mic_gain: f64_val(store::MIC_GAIN),
        history_retention: str_val(store::HISTORY_RETENTION),
        microphone_device: str_val(store::MICROPHONE_DEVICE),
        update_dismissed_version: str_val(store::UPDATE_DISMISSED_VERSION),
        update_notified_version: str_val(store::UPDATE_NOTIFIED_VERSION),
        hotkey: s.get(store::HOTKEY).and_then(|v| {
            v.as_array().map(|arr| {
                arr.iter()
                    .filter_map(|x| x.as_str().map(String::from))
                    .collect()
            })
        }),
        appearance_mode: str_val(store::APPEARANCE_MODE),
        cleanup_prompt_overrides: json_val(store::CLEANUP_PROMPT_OVERRIDES),
    })
}
