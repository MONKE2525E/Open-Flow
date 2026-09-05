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

    let disabled =
        SettingsSnapshot::from_pairs([(PLAY_START_STOP_SOUNDS.to_string(), json!(false))]);
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
    assert!(
        cfg.contextual_formatting_enabled,
        "contextual formatting default on"
    );
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
fn deprecated_groq_cleanup_models_migrate_to_no_thinking_qwen() {
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
fn unsupported_google_cleanup_models_migrate_to_flash_lite() {
    for legacy in ["gemini-3.7-flash", "gemini-2.5-pro"] {
        assert_eq!(
            migrate_deprecated_model_id(&format!("google/{legacy}")),
            "google/gemini-3.5-flash-lite"
        );
    }
    assert_eq!(
        migrate_deprecated_model_id("google/gemini-3.5-flash"),
        "google/gemini-3.5-flash"
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

/// The cleanup prompt override must be inert unless Advanced Models is on,
/// and a whitespace-only override must be ignored.
#[test]
fn setting_audit_cleanup_override_gated_by_advanced_ui() {
    let base = PipelineConfig {
        advanced_model_ui: true,
        cleanup_prompt_override: "Custom".to_string(),
        ..Default::default()
    };
    assert_eq!(base.cleanup_override(), Some("Custom"));

    let off = PipelineConfig {
        advanced_model_ui: false,
        cleanup_prompt_override: "Custom".to_string(),
        ..Default::default()
    };
    assert_eq!(off.cleanup_override(), None);

    let blank = PipelineConfig {
        advanced_model_ui: true,
        cleanup_prompt_override: "   ".to_string(),
        ..Default::default()
    };
    assert_eq!(blank.cleanup_override(), None);
}

/// An edit saved under the retired per-model map still applies after the
/// upgrade — losing a hand-written prompt to a schema change is not acceptable.
#[test]
fn legacy_per_model_cleanup_override_migrates() {
    let store = SettingsSnapshot::from_pairs([(
        CLEANUP_PROMPT_OVERRIDES.to_string(),
        serde_json::json!({
            "groq/openai/gpt-oss-20b": "  ",
            "google/gemini-3.7-flash": "Custom",
        }),
    )]);
    assert_eq!(
        load_pipeline_config(&store).cleanup_prompt_override,
        "Custom"
    );
}
