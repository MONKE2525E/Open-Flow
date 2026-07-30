use super::gates::strip_provider_artifacts;
use super::stages::{cleanup_cache_plan, dual_cleanup_context_fingerprint, speech_gate_accepts};
use super::{
    apply_app_style_overrides, effective_recording_rms, ensure_terminal_punctuation,
    has_spoken_content, is_transcription_hallucination, normalize_transcription_math_artifacts,
    preview_text, recording_gate_rms, resolve_app_mapping, run_pipeline_fixture,
    should_run_cleanup_llm, should_use_cleanup_cache, strip_hallucinated_suffix,
    style_scoped_cleanup_cache_key, PipelineTestDictionaryEntry, PipelineTestRequest,
    PipelineTestSnippet,
};
use crate::system::apps::AppMapping;

#[test]
fn preview_text_redacts_dictation_content_when_not_verbose() {
    // Verbose logging is off by default; dictation text must never appear in
    // the rendered preview (it would otherwise reach the log buffer + export).
    let out = preview_text("my secret dictated password is hunter2", 140);
    assert!(!out.contains("secret"));
    assert!(!out.contains("hunter2"));
    assert!(out.contains("chars redacted"));
}

#[test]
fn provider_artifact_filter_is_conservative() {
    assert_eq!(
        strip_provider_artifacts("Hello there. Transcribed by AssemblyAI."),
        "Hello there."
    );
    assert_eq!(
        strip_provider_artifacts("Please subscribe to the newsletter."),
        "Please subscribe to the newsletter."
    );
    assert_eq!(
        strip_provider_artifacts("AssemblyAI documentation is useful."),
        "AssemblyAI documentation is useful."
    );
}

#[test]
fn dual_cleanup_cache_key_changes_with_cleanup_context() {
    let mut config = base_config();
    let context = dual_cleanup_context_fingerprint(&config, "dictionary rules", Some("editor"));
    let first = cleanup_cache_plan(
        "hello world",
        "casual",
        "medium",
        "",
        Some("alternate world"),
        Some(context),
    );

    config.cleanup_default_model = "google/gemini-2.5-flash".into();
    let changed_context =
        dual_cleanup_context_fingerprint(&config, "dictionary rules", Some("editor"));
    let second = cleanup_cache_plan(
        "hello world",
        "casual",
        "medium",
        "",
        Some("alternate world"),
        Some(changed_context),
    );

    assert_ne!(first.key, second.key);
}
use crate::api::prompts::looks_like_refusal;
use crate::data::store;
use crate::testing::{
    fixture_hit_count, register_fixture, reset, set_enabled, take_injections, FixtureSpec,
};
use bytes::Bytes;
use std::ffi::OsString;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn hallucination_gate_catches_prompt_echo() {
    assert!(is_transcription_hallucination("Return only spoken words."));
    assert!(is_transcription_hallucination("Return only spoken words"));
    assert!(is_transcription_hallucination(
        "Return only the words spoken."
    ));
    assert!(is_transcription_hallucination(
        "Verenu dictation in English."
    ));
    assert!(is_transcription_hallucination(
        "Transcribe the audio in English."
    ));
}

#[test]
fn hallucination_gate_catches_common_whisper_artifacts() {
    assert!(is_transcription_hallucination("Thank you for watching!"));
    assert!(is_transcription_hallucination("[silence]"));
    assert!(is_transcription_hallucination("[Music playing]"));
}

#[test]
fn hallucination_gate_catches_pure_glossary_echo_on_silence() {
    // After switching Whisper/Groq priming to vocabulary-only text (no more
    // "Return only spoken words" for the model to echo), silent audio is
    // expected to instead echo the vocabulary list itself.
    assert!(is_transcription_hallucination(
        "Verenu. Tauri. Svelte. Groq. Gemini. OpenAI."
    ));
    assert!(is_transcription_hallucination("Verenu."));
    assert!(is_transcription_hallucination("groq, gemini, openai"));
}

#[test]
fn hallucination_gate_catches_a_near_miss_of_a_prompt_glossary_term() {
    assert!(is_transcription_hallucination("svlet"));
    assert!(!is_transcription_hallucination("svelte"));
    assert!(!is_transcription_hallucination("please select the file"));
}

#[test]
fn spoken_content_gate_rejects_punctuation_only_transcripts_but_keeps_letters() {
    assert!(!has_spoken_content("."));
    assert!(!has_spoken_content("?!..."));
    assert!(!has_spoken_content("   "));
    assert!(has_spoken_content("I"));
    assert!(has_spoken_content("42"));
}

#[test]
fn hallucination_gate_passes_real_speech() {
    assert!(!is_transcription_hallucination(
        "Return the package to me by Thursday."
    ));
    assert!(!is_transcription_hallucination(
        "Why does YouTube's algorithm feel like doo-doo?"
    ));
    assert!(!is_transcription_hallucination("Thank you for your help."));
}

#[test]
fn strip_hallucinated_suffix_removes_trailing_amara_credit() {
    let raw =
        "What do you mean by gated it? Like, it won't work? Subtitles by the Amara.org community.";
    assert_eq!(
        strip_hallucinated_suffix(raw),
        "What do you mean by gated it? Like, it won't work?"
    );
}

#[test]
fn strip_hallucinated_suffix_leaves_real_speech_untouched() {
    let raw = "Return the package to me by Thursday.";
    assert_eq!(strip_hallucinated_suffix(raw), raw);
}

#[test]
fn strip_hallucinated_suffix_handles_whole_output_hallucination() {
    assert_eq!(strip_hallucinated_suffix("Thanks for watching!"), "");
}

#[test]
fn strip_hallucinated_suffix_does_not_break_on_internal_periods() {
    let raw = "Check out amara.org for subtitle tools.";
    assert_eq!(strip_hallucinated_suffix(raw), raw);
}

#[test]
fn strip_hallucinated_suffix_preserves_legitimate_trailing_sentences() {
    // These start with the same short, generic phrases as known Whisper
    // hallucinations, but are real dictated speech and must survive.
    let raw_subscribe = "We have a new blog post. Please subscribe to our newsletter.";
    assert_eq!(strip_hallucinated_suffix(raw_subscribe), raw_subscribe);

    let raw_watching = "I am so grateful for your help. Thank you for watching over my dog.";
    assert_eq!(strip_hallucinated_suffix(raw_watching), raw_watching);
}

#[test]
fn strip_hallucinated_suffix_still_strips_bare_hallucination_sentence() {
    let raw = "I finished the report. Thank you for watching!";
    assert_eq!(strip_hallucinated_suffix(raw), "I finished the report.");
}

#[test]
fn strip_hallucinated_suffix_finds_boundary_after_cjk_fullwidth_period() {
    // CJK sentences end with a fullwidth terminator and no trailing space, so
    // the ASCII "punctuation + whitespace" boundary rule alone would miss the
    // split point and fail to isolate the trailing English hallucination.
    let raw = "今晩予定がある。Thank you for watching!";
    assert_eq!(strip_hallucinated_suffix(raw), "今晩予定がある。");
}

#[test]
fn strip_hallucinated_suffix_trims_trailing_fullwidth_punctuation_before_exact_match() {
    let raw = "会議の議事録です。Thank you for watching！";
    assert_eq!(strip_hallucinated_suffix(raw), "会議の議事録です。");
}

#[test]
fn strip_hallucinated_suffix_handles_closing_quotes_after_sentence_boundary() {
    let raw = "He said, \"No!\" Thank you for watching!";
    assert_eq!(strip_hallucinated_suffix(raw), "He said, \"No!\"");

    let raw2 = "This is a test (with parentheses). Please subscribe!";
    assert_eq!(
        strip_hallucinated_suffix(raw2),
        "This is a test (with parentheses)."
    );
}

#[test]
fn strip_hallucinated_suffix_handles_cjk_closing_quotes_after_sentence_boundary() {
    let raw = "他说：“不行！”Thank you for watching!";
    assert_eq!(strip_hallucinated_suffix(raw), "他说：“不行！”");
}

#[test]
fn cleanup_cache_bypasses_math_like_queries() {
    assert!(!should_use_cleanup_cache("What's 67 plus 67?"));
    assert!(!should_use_cleanup_cache("what is six times seven"));
}

#[test]
fn cleanup_cache_keeps_non_math_numeric_queries() {
    assert!(should_use_cleanup_cache("version 2.5 release notes"));
    assert!(should_use_cleanup_cache("meeting on 2026-05-17 at 10:30"));
}

#[test]
fn transcription_preserves_digit_x_digit_in_plus_queries() {
    let out = normalize_transcription_math_artifacts("What's 6x7 plus 6x7?");
    assert_eq!(out, "What's 6x7 plus 6x7?");
}

#[test]
fn transcription_does_not_touch_non_plus_digit_x_digit() {
    let out = normalize_transcription_math_artifacts("Calculate 6x7");
    assert_eq!(out, "Calculate 6x7");
}

#[test]
fn transcription_compacts_spaced_digit_x_digit_chunks() {
    let out = normalize_transcription_math_artifacts("What is 6 x 7 plus 6 x 7?");
    assert_eq!(out, "What is 6x7 plus 6x7?");
}

#[test]
fn transcription_does_not_fold_mixed_multiplication_chunks() {
    let out = normalize_transcription_math_artifacts("6x7 plus 3x4");
    assert_eq!(out, "6x7 plus 3x4");
}

#[test]
fn cleanup_llm_skips_when_cleanup_is_off_even_for_formal() {
    assert!(!should_run_cleanup_llm(true, true, true, "none", "formal"));
}

#[test]
fn cleanup_llm_skips_for_non_formal_when_none_intensity() {
    assert!(!should_run_cleanup_llm(true, true, true, "none", "casual"));
}

#[test]
fn style_scoped_cache_key_changes_with_profile_and_intensity() {
    let casual_medium = style_scoped_cleanup_cache_key("abc123", "casual", "medium");
    let formal_medium = style_scoped_cleanup_cache_key("abc123", "formal", "medium");
    let casual_high = style_scoped_cleanup_cache_key("abc123", "casual", "high");
    assert_ne!(casual_medium, formal_medium);
    assert_ne!(casual_medium, casual_high);
}

#[test]
fn style_scoped_cache_key_preserves_empty_base_key() {
    assert_eq!(style_scoped_cleanup_cache_key("", "casual", "medium"), "");
}

#[test]
fn recording_gate_gets_more_permissive_at_high_gain() {
    let default_gate = recording_gate_rms(store::DEFAULT_MIC_GAIN);
    let high_gain_gate = recording_gate_rms(store::MAX_MIC_GAIN);

    assert!((default_gate - 0.005).abs() < f32::EPSILON);
    assert!(high_gain_gate < default_gate);
    assert!((high_gain_gate - 0.0021875).abs() < 0.0001);
}

#[test]
fn speech_gate_does_not_let_loud_rms_bypass_a_vad_rejection() {
    let vad_rejected = crate::media::vad::SpeechDetectionResult {
        contains_speech: false,
        speech_ms: 0,
        speech_ratio: 0.0,
        peak_probability: 0.9,
        longest_segment_ms: 0,
    };

    assert!(!speech_gate_accepts(Some(&vad_rejected), 1.0, 0.001));
    assert!(speech_gate_accepts(None, 1.0, 0.001));
    assert!(!speech_gate_accepts(None, 0.0001, 0.001));
}

#[test]
fn effective_recording_rms_keeps_quiet_gain_boosted_speech_from_failing() {
    let effective = effective_recording_rms(0.002, 0.001, store::MAX_MIC_GAIN);

    assert!((effective - 0.008).abs() < 0.0001);
}

#[test]
fn terminal_punctuation_added_for_casual_bare_word() {
    assert_eq!(
        ensure_terminal_punctuation("smart decision", "casual", "medium"),
        "smart decision."
    );
    assert_eq!(
        ensure_terminal_punctuation("the report is done", "formal", "high"),
        "the report is done."
    );
}

#[test]
fn terminal_punctuation_left_alone_when_already_terminated() {
    assert_eq!(
        ensure_terminal_punctuation("all good.", "casual", "medium"),
        "all good."
    );
    assert_eq!(
        ensure_terminal_punctuation("really?", "casual", "medium"),
        "really?"
    );
    assert_eq!(
        ensure_terminal_punctuation("wait,", "casual", "medium"),
        "wait,"
    );
    assert_eq!(
        ensure_terminal_punctuation("as follows:", "casual", "medium"),
        "as follows:"
    );
}

#[test]
fn terminal_punctuation_skipped_for_very_casual_and_verbatim() {
    assert_eq!(
        ensure_terminal_punctuation("smart decision", "very_casual", "medium"),
        "smart decision"
    );
    assert_eq!(
        ensure_terminal_punctuation("smart decision", "casual", "none"),
        "smart decision"
    );
}

#[test]
fn terminal_punctuation_preserves_trailing_whitespace_and_empty() {
    assert_eq!(
        ensure_terminal_punctuation("hello ", "casual", "medium"),
        "hello. "
    );
    assert_eq!(ensure_terminal_punctuation("", "casual", "medium"), "");
    assert_eq!(
        ensure_terminal_punctuation("   ", "casual", "medium"),
        "   "
    );
}

#[test]
fn terminal_punctuation_uses_fullwidth_period_for_cjk() {
    assert_eq!(
        ensure_terminal_punctuation("今晩予定がある", "casual", "medium"),
        "今晩予定がある。"
    );
    assert_eq!(
        ensure_terminal_punctuation("这是一个决定", "formal", "high"),
        "这是一个决定。"
    );
    assert_eq!(
        ensure_terminal_punctuation("コーヒーを飲む", "casual", "medium"),
        "コーヒーを飲む。"
    );
    // Already terminated with a fullwidth period stays untouched.
    assert_eq!(
        ensure_terminal_punctuation("今晩予定がある。", "casual", "medium"),
        "今晩予定がある。"
    );
}

fn mapping(exe: &str, profile: &str, cleanup_intensity: Option<&str>) -> AppMapping {
    AppMapping {
        exe: exe.into(),
        profile: profile.into(),
        name: String::new(),
        cleanup_intensity: cleanup_intensity.map(Into::into),
    }
}

#[test]
fn app_mapping_override_applies_for_matching_app() {
    // base_config(): default_tone = "casual", cleanup_intensity = "medium".
    let mut cfg = base_config();
    let m = mapping("appa.exe", "very_casual", Some("high"));
    let profile = apply_app_style_overrides(&mut cfg, Some(&m));
    assert_eq!(profile, "very_casual");
    assert_eq!(cfg.cleanup_intensity, "high");
}

#[test]
fn app_mapping_no_match_leaves_global_defaults_untouched() {
    // Regression for issue #144: when no mapping matches the active app, the
    // effective tone falls back to default_tone and the global cleanup
    // intensity must NOT be mutated by a different app's override.
    let mut cfg = base_config();
    let profile = apply_app_style_overrides(&mut cfg, None);
    assert_eq!(profile, "casual");
    assert_eq!(cfg.cleanup_intensity, "medium");
}

#[test]
fn app_mapping_empty_override_fields_fall_back_to_globals() {
    let mut cfg = base_config();
    let m = mapping("appa.exe", "   ", Some("   "));
    let profile = apply_app_style_overrides(&mut cfg, Some(&m));
    assert_eq!(profile, "casual");
    assert_eq!(cfg.cleanup_intensity, "medium");
}

#[test]
fn resolve_app_mapping_is_scoped_to_matching_exe() {
    let mappings = serde_json::json!([
        { "exe": "appa.exe", "profile": "very_casual", "cleanup_intensity": "high" }
    ]);
    let snap = store::SettingsSnapshot::from_pairs([(store::APP_MAPPINGS.to_string(), mappings)]);

    let matched = resolve_app_mapping(Some(&snap), "appa.exe").expect("App A should match");
    assert_eq!(matched.profile, "very_casual");
    assert_eq!(matched.cleanup_intensity.as_deref(), Some("high"));

    // A different foreground app must not resolve App A's mapping.
    assert!(resolve_app_mapping(Some(&snap), "appb.exe").is_none());
}

fn base_config() -> store::PipelineConfig {
    store::PipelineConfig {
        transcription_provider: "groq".into(),
        transcription_language: "en".into(),
        cleanup_provider: "groq".into(),
        transcription_default_model: "groq/whisper-large-v3-turbo".into(),
        cleanup_default_model: "groq/llama-3.3-70b-versatile".into(),
        transcription_fallback_models: Vec::new(),
        dual_transcription_enabled: false,
        cleanup_fallback_models: Vec::new(),
        cleanup_enabled: true,
        key_groq: "fixture-groq-key".into(),
        key_openai: "fixture-openai-key".into(),
        key_google: "fixture-google-key".into(),
        key_assemblyai: "fixture-assemblyai-key".into(),
        default_tone: "casual".into(),
        cleanup_intensity: "medium".into(),
        app_context_hint: false,
        auto_learn_enabled: false,
        contextual_caps_enabled: true,
        auto_spacing_enabled: true,
        caps_lock_uppercase_enabled: false,
        macos_clipboard_sniff_enabled: false,
        advanced_model_ui: false,
        local_model_memory_policy: "unload_after_5m".into(),
        cleanup_prompt_overrides: std::collections::HashMap::new(),
    }
}

fn test_audio(duration_ms: u64) -> super::CapturedAudio {
    super::CapturedAudio {
        wav: Bytes::from_static(b"fixture-wav"),
        samples_16k: Arc::new(vec![0.0; 16_000]),
        sample_rate: 16_000,
        duration_ms,
    }
}

fn base_request(config: store::PipelineConfig) -> PipelineTestRequest {
    PipelineTestRequest {
        db: None,
        audio: test_audio(1200),
        rms: 0.2,
        config,
        profile: "casual".into(),
        target_hwnd: 77,
        app_context: None,
        snippets: Vec::new(),
        dictionary: Vec::new(),
        caps_lock_on: false,
    }
}

struct LocalModelsTestGuard {
    root: PathBuf,
    previous_override: Option<OsString>,
}

impl Drop for LocalModelsTestGuard {
    fn drop(&mut self) {
        // Tests hold the harness lock while this override is active.
        unsafe {
            match &self.previous_override {
                Some(value) => std::env::set_var("VERENU_APP_DATA_DIR_OVERRIDE", value),
                None => std::env::remove_var("VERENU_APP_DATA_DIR_OVERRIDE"),
            }
        }
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn install_local_models(downloaded_model_ids: &[&str]) -> LocalModelsTestGuard {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before unix epoch")
        .as_nanos();
    let root = std::env::temp_dir().join(format!("verenu-local-stt-tests-{unique}"));
    let previous_override = std::env::var_os("VERENU_APP_DATA_DIR_OVERRIDE");
    std::fs::create_dir_all(root.join("models").join("stt")).expect("create local model root");
    // Tests hold the harness lock while this override is active.
    unsafe {
        std::env::set_var("VERENU_APP_DATA_DIR_OVERRIDE", &root);
    }

    let models_root = crate::local_stt::LocalTranscriptionManager::models_root();
    for model_id in downloaded_model_ids {
        let manifest = crate::local_stt::model::manifest_by_id(model_id)
            .unwrap_or_else(|| panic!("missing local manifest for {model_id}"));
        let final_path = manifest.final_path(&models_root);
        if manifest.is_directory {
            std::fs::create_dir_all(&final_path)
                .unwrap_or_else(|err| panic!("create local model dir failed: {err}"));
        } else {
            if let Some(parent) = final_path.parent() {
                std::fs::create_dir_all(parent).expect("create local model parent");
            }
            std::fs::write(&final_path, b"fixture-model")
                .unwrap_or_else(|err| panic!("write local model file failed: {err}"));
        }
    }

    LocalModelsTestGuard {
        root,
        previous_override,
    }
}

fn install_local_cleanup_models(downloaded_model_ids: &[&str]) -> LocalModelsTestGuard {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before unix epoch")
        .as_nanos();
    let root = std::env::temp_dir().join(format!("verenu-local-cleanup-tests-{unique}"));
    let previous_override = std::env::var_os("VERENU_APP_DATA_DIR_OVERRIDE");
    std::fs::create_dir_all(root.join("models").join("cleanup"))
        .expect("create local cleanup model root");
    unsafe {
        std::env::set_var("VERENU_APP_DATA_DIR_OVERRIDE", &root);
    }

    let models_root = crate::local_llm::LocalLlmManager::models_root();
    for model_id in downloaded_model_ids {
        let manifest = crate::local_llm::model::manifest_by_id(model_id)
            .unwrap_or_else(|| panic!("missing local cleanup manifest for {model_id}"));
        let final_path = manifest.final_path(&models_root);
        std::fs::create_dir_all(&final_path)
            .unwrap_or_else(|err| panic!("create local cleanup model dir failed: {err}"));
        for artifact in manifest.artifacts {
            std::fs::write(final_path.join(artifact.filename), b"fixture-model")
                .unwrap_or_else(|err| panic!("write local cleanup model file failed: {err}"));
        }
    }

    LocalModelsTestGuard {
        root,
        previous_override,
    }
}

fn fixture(
    task: &str,
    provider: &str,
    model: &str,
    response: Option<&str>,
    error_kind: Option<&str>,
    error_message: Option<&str>,
) {
    register_fixture(FixtureSpec {
        task: task.into(),
        provider: provider.into(),
        model: model.into(),
        response: response.map(Into::into),
        error_kind: error_kind.map(Into::into),
        error_message: error_message.map(Into::into),
    });
}

fn harness_test_lock() -> &'static std::sync::Mutex<()> {
    static LOCK: std::sync::OnceLock<std::sync::Mutex<()>> = std::sync::OnceLock::new();
    LOCK.get_or_init(|| std::sync::Mutex::new(()))
}

#[tokio::test(flavor = "current_thread")]
async fn pipeline_fixture_rejects_short_recordings_before_provider_calls() {
    let _guard = harness_test_lock().lock().expect("harness lock");
    reset();
    set_enabled(true);
    fixture(
        "transcription",
        "groq",
        "whisper-large-v3-turbo",
        Some("should never be used"),
        None,
        None,
    );

    let mut request = base_request(base_config());
    request.audio.duration_ms = 300;
    let err = run_pipeline_fixture(request)
        .await
        .expect_err("short recording should fail");
    assert!(err.to_string().contains("Recording too short"));
    assert_eq!(
        fixture_hit_count("transcription", "groq", "whisper-large-v3-turbo"),
        0
    );
    reset();
}

#[tokio::test(flavor = "current_thread")]
async fn pipeline_fixture_rejects_quiet_recordings_before_provider_calls() {
    let _guard = harness_test_lock().lock().expect("harness lock");
    reset();
    set_enabled(true);
    fixture(
        "transcription",
        "groq",
        "whisper-large-v3-turbo",
        Some("should never be used"),
        None,
        None,
    );

    let mut request = base_request(base_config());
    request.rms = 0.001;
    let err = run_pipeline_fixture(request)
        .await
        .expect_err("quiet recording should fail");
    assert!(err.to_string().contains("Audio too quiet"));
    assert_eq!(
        fixture_hit_count("transcription", "groq", "whisper-large-v3-turbo"),
        0
    );
    reset();
}

#[tokio::test(flavor = "current_thread")]
async fn pipeline_fixture_requires_transcription_key_before_provider_calls() {
    let _guard = harness_test_lock().lock().expect("harness lock");
    reset();
    set_enabled(true);
    let mut config = base_config();
    config.key_groq.clear();
    config.key_openai.clear();
    config.key_google.clear();
    let err = run_pipeline_fixture(base_request(config))
        .await
        .expect_err("missing key should fail");
    assert!(err
        .to_string()
        .contains("No configured transcription backend is available"));
    reset();
}

#[tokio::test(flavor = "current_thread")]
async fn pipeline_fixture_uses_transcription_fallback_for_retryable_errors() {
    let _guard = harness_test_lock().lock().expect("harness lock");
    reset();
    set_enabled(true);
    let mut config = base_config();
    config.transcription_fallback_models = vec!["openai/gpt-4o-transcribe".into()];
    fixture(
        "transcription",
        "groq",
        "whisper-large-v3-turbo",
        None,
        Some("timeout"),
        Some("groq timed out"),
    );
    fixture(
        "transcription",
        "openai",
        "gpt-4o-transcribe",
        Some("fallback transcript"),
        None,
        None,
    );
    fixture(
        "cleanup",
        "groq",
        "llama-3.3-70b-versatile",
        Some("fallback transcript"),
        None,
        None,
    );

    let result = run_pipeline_fixture(base_request(config))
        .await
        .expect("fallback should succeed");
    assert_eq!(result.raw_text, "fallback transcript");
    assert_eq!(result.api_used, "openai/gpt-4o-transcribe/transcription");
    assert_eq!(
        fixture_hit_count("transcription", "groq", "whisper-large-v3-turbo"),
        1
    );
    assert_eq!(
        fixture_hit_count("transcription", "openai", "gpt-4o-transcribe"),
        1
    );
    reset();
}

#[tokio::test(flavor = "current_thread")]
async fn pipeline_fixture_falls_back_past_a_non_retryable_transcription_error() {
    // A non-retryable error (missing/invalid key, bad request, etc.) on the
    // primary provider says nothing about whether a *different* fallback
    // provider would succeed — e.g. a cloud primary with no API key saved
    // and a local fallback that needs no key at all. The chain must still
    // try the fallback instead of aborting outright.
    let _guard = harness_test_lock().lock().expect("harness lock");
    reset();
    set_enabled(true);
    let mut config = base_config();
    config.transcription_fallback_models = vec!["openai/gpt-4o-transcribe".into()];
    register_fixture(FixtureSpec {
        task: "transcription".into(),
        provider: "groq".into(),
        model: "whisper-large-v3-turbo".into(),
        response: None,
        error_kind: Some("auth_invalid".into()),
        error_message: None,
    });
    register_fixture(FixtureSpec {
        task: "transcription".into(),
        provider: "openai".into(),
        model: "gpt-4o-transcribe".into(),
        response: Some("fallback transcript".into()),
        error_kind: None,
        error_message: None,
    });
    fixture(
        "cleanup",
        "groq",
        "llama-3.3-70b-versatile",
        Some("fallback transcript"),
        None,
        None,
    );

    let result = run_pipeline_fixture(base_request(config))
        .await
        .expect("fallback should succeed past the non-retryable error");
    assert_eq!(result.raw_text, "fallback transcript");
    assert_eq!(result.api_used, "openai/gpt-4o-transcribe/transcription");
    assert_eq!(
        fixture_hit_count("transcription", "openai", "gpt-4o-transcribe"),
        1
    );
    reset();
}

#[tokio::test(flavor = "current_thread")]
async fn pipeline_fixture_uses_cleanup_fallback_and_persists_history() {
    let _guard = harness_test_lock().lock().expect("harness lock");
    reset();
    set_enabled(true);
    let mut config = base_config();
    config.cleanup_fallback_models = vec!["openai/gpt-4o-mini".into()];
    register_fixture(FixtureSpec {
        task: "transcription".into(),
        provider: "groq".into(),
        model: "whisper-large-v3-turbo".into(),
        response: Some("raw fallback cleanup test".into()),
        error_kind: None,
        error_message: None,
    });
    register_fixture(FixtureSpec {
        task: "cleanup".into(),
        provider: "groq".into(),
        model: "llama-3.3-70b-versatile".into(),
        response: None,
        error_kind: Some("status_503".into()),
        error_message: Some("temporary overload".into()),
    });
    register_fixture(FixtureSpec {
        task: "cleanup".into(),
        provider: "openai".into(),
        model: "gpt-4o-mini".into(),
        response: Some("clean fallback result".into()),
        error_kind: None,
        error_message: None,
    });

    let result = run_pipeline_fixture(base_request(config))
        .await
        .expect("cleanup fallback should succeed");
    assert_eq!(
        result.final_text_before_dictionary,
        "clean fallback result."
    );
    assert_eq!(result.history_entry.clean_text, "clean fallback result.");
    assert_eq!(result.stats.total_words, 4);
    assert_eq!(result.recent.len(), 1);
    assert_eq!(
        fixture_hit_count("cleanup", "groq", "llama-3.3-70b-versatile"),
        1
    );
    assert_eq!(fixture_hit_count("cleanup", "openai", "gpt-4o-mini"), 1);
    reset();
}

#[test]
fn looks_like_refusal_matches_known_markers() {
    assert!(looks_like_refusal(
        "I am an AI and I do not have access to real-time data."
    ));
    assert!(looks_like_refusal("As an AI, I can't help with that."));
    assert!(looks_like_refusal("I cannot provide that information."));
    assert!(!looks_like_refusal("Send me the file when you can."));
}

#[tokio::test(flavor = "current_thread")]
async fn pipeline_fixture_keeps_self_described_refusal_when_dictated_by_user() {
    let _guard = harness_test_lock().lock().expect("harness lock");
    reset();
    set_enabled(true);
    register_fixture(FixtureSpec {
        task: "transcription".into(),
        provider: "groq".into(),
        model: "whisper-large-v3-turbo".into(),
        response: Some("I cannot believe it is already five o'clock".into()),
        error_kind: None,
        error_message: None,
    });
    register_fixture(FixtureSpec {
        task: "cleanup".into(),
        provider: "groq".into(),
        model: "llama-3.3-70b-versatile".into(),
        response: Some("I cannot believe it's already five o'clock.".into()),
        error_kind: None,
        error_message: None,
    });

    let result = run_pipeline_fixture(base_request(base_config()))
        .await
        .expect("dictated refusal-shaped text should pass through unchanged");
    assert_eq!(
        result.final_text_before_dictionary,
        "I cannot believe it's already five o'clock."
    );
    assert_eq!(
        fixture_hit_count("cleanup", "groq", "llama-3.3-70b-versatile"),
        1
    );
    reset();
}

#[tokio::test(flavor = "current_thread")]
async fn pipeline_fixture_retries_then_falls_back_on_model_refusal() {
    let _guard = harness_test_lock().lock().expect("harness lock");
    reset();
    set_enabled(true);
    register_fixture(FixtureSpec {
        task: "transcription".into(),
        provider: "groq".into(),
        model: "whisper-large-v3-turbo".into(),
        response: Some("what time is it in tokyo right now".into()),
        error_kind: None,
        error_message: None,
    });
    register_fixture(FixtureSpec {
        task: "cleanup".into(),
        provider: "groq".into(),
        model: "llama-3.3-70b-versatile".into(),
        response: Some("I am an AI and I do not have access to real-time information.".into()),
        error_kind: None,
        error_message: None,
    });

    let result = run_pipeline_fixture(base_request(base_config()))
        .await
        .expect("model refusal should fall back to pre-cleanup text");
    // Cleanup refused, so the pipeline falls back to the pre-cleanup text via
    // the non-cleaned path — which is intentionally left unpunctuated.
    assert_eq!(
        result.final_text_before_dictionary,
        "what time is it in tokyo right now"
    );
    assert_eq!(
        fixture_hit_count("cleanup", "groq", "llama-3.3-70b-versatile"),
        2
    );
    reset();
}

#[tokio::test(flavor = "current_thread")]
async fn pipeline_fixture_skips_cleanup_for_pure_snippet_fast_path() {
    let _guard = harness_test_lock().lock().expect("harness lock");
    reset();
    set_enabled(true);
    register_fixture(FixtureSpec {
        task: "transcription".into(),
        provider: "groq".into(),
        model: "whisper-large-v3-turbo".into(),
        response: Some("sig".into()),
        error_kind: None,
        error_message: None,
    });
    register_fixture(FixtureSpec {
        task: "cleanup".into(),
        provider: "groq".into(),
        model: "llama-3.3-70b-versatile".into(),
        response: Some("should not be called".into()),
        error_kind: None,
        error_message: None,
    });

    let mut request = base_request(base_config());
    request.snippets.push(PipelineTestSnippet {
        trigger: "sig".into(),
        expansion: "Best regards, Noah".into(),
        instructions: String::new(),
    });

    let result = run_pipeline_fixture(request)
        .await
        .expect("pure snippet fast path should succeed");
    assert_eq!(result.final_text_before_dictionary, "Best regards, Noah");
    assert_eq!(
        fixture_hit_count("cleanup", "groq", "llama-3.3-70b-versatile"),
        0
    );
    assert_eq!(take_injections().len(), 1);
    reset();
}

#[tokio::test(flavor = "current_thread")]
async fn pipeline_fixture_applies_instruction_snippets_and_dictionary_last() {
    let _guard = harness_test_lock().lock().expect("harness lock");
    reset();
    set_enabled(true);
    register_fixture(FixtureSpec {
        task: "transcription".into(),
        provider: "groq".into(),
        model: "whisper-large-v3-turbo".into(),
        response: Some("say acme alert".into()),
        error_kind: None,
        error_message: None,
    });
    register_fixture(FixtureSpec {
        task: "cleanup".into(),
        provider: "groq".into(),
        model: "llama-3.3-70b-versatile".into(),
        response: Some("acme alert".into()),
        error_kind: None,
        error_message: None,
    });

    let mut request = base_request(base_config());
    request.snippets.push(PipelineTestSnippet {
        trigger: "alert".into(),
        expansion: "alert".into(),
        instructions: "all capitals".into(),
    });
    request.dictionary.push(PipelineTestDictionaryEntry {
        term: "Verenu".into(),
        mistake: Some("ACME".into()),
    });

    let result = run_pipeline_fixture(request)
        .await
        .expect("instruction and dictionary path should succeed");
    // Casual cleanup now guarantees a terminal period (so consecutive
    // dictations capitalize); the "all capitals" override still applies.
    assert_eq!(result.final_text_before_dictionary, "ACME ALERT.");
    assert_eq!(result.injected_text, "Verenu ALERT.");
    // History must match what was actually injected, dictionary correction
    // included — it previously saved final_text_before_dictionary instead,
    // so a correction that changed what got pasted never showed up here.
    assert_eq!(result.history_entry.clean_text, "Verenu ALERT.");
    reset();
}

#[tokio::test(flavor = "current_thread")]
async fn pipeline_fixture_uppercases_output_only_when_setting_and_caps_lock_both_on() {
    let _guard = harness_test_lock().lock().expect("harness lock");
    reset();
    set_enabled(true);
    register_fixture(FixtureSpec {
        task: "transcription".into(),
        provider: "groq".into(),
        model: "whisper-large-v3-turbo".into(),
        response: Some("send the report now".into()),
        error_kind: None,
        error_message: None,
    });
    register_fixture(FixtureSpec {
        task: "cleanup".into(),
        provider: "groq".into(),
        model: "llama-3.3-70b-versatile".into(),
        response: Some("Send the report now.".into()),
        error_kind: None,
        error_message: None,
    });

    // Setting on + Caps Lock on -> uppercase the injected text and the
    // persisted history record, but not the pre-dictionary cleanup value.
    let mut config = base_config();
    config.caps_lock_uppercase_enabled = true;
    let mut request = base_request(config.clone());
    request.caps_lock_on = true;
    let result = run_pipeline_fixture(request)
        .await
        .expect("caps lock uppercase path should succeed");
    assert_eq!(result.final_text_before_dictionary, "Send the report now.");
    assert_eq!(result.injected_text, "SEND THE REPORT NOW.");
    assert_eq!(result.history_entry.clean_text, "SEND THE REPORT NOW.");

    // Setting on + Caps Lock off -> unchanged.
    let mut request = base_request(config);
    request.caps_lock_on = false;
    let result = run_pipeline_fixture(request)
        .await
        .expect("caps lock off should leave output unchanged");
    assert_eq!(result.injected_text, "Send the report now.");
    assert_eq!(result.history_entry.clean_text, "Send the report now.");

    // Setting off -> unchanged regardless of Caps Lock state.
    let mut request = base_request(base_config());
    request.caps_lock_on = true;
    let result = run_pipeline_fixture(request)
        .await
        .expect("setting disabled should leave output unchanged");
    assert_eq!(result.injected_text, "Send the report now.");
    assert_eq!(result.history_entry.clean_text, "Send the report now.");
    reset();
}

#[tokio::test(flavor = "current_thread")]
async fn pipeline_fixture_skips_cleanup_for_formal_when_intensity_is_off() {
    let _guard = harness_test_lock().lock().expect("harness lock");
    reset();
    set_enabled(true);
    let mut config = base_config();
    config.cleanup_intensity = "none".into();
    register_fixture(FixtureSpec {
        task: "transcription".into(),
        provider: "groq".into(),
        model: "whisper-large-v3-turbo".into(),
        response: Some("im sending the note".into()),
        error_kind: None,
        error_message: None,
    });
    register_fixture(FixtureSpec {
        task: "cleanup".into(),
        provider: "groq".into(),
        model: "llama-3.3-70b-versatile".into(),
        response: Some("I am sending the note.".into()),
        error_kind: None,
        error_message: None,
    });

    let mut request = base_request(config);
    request.profile = "formal".into();
    let result = run_pipeline_fixture(request)
        .await
        .expect("formal cleanup should stay off when intensity is none");
    assert_eq!(result.final_text_before_dictionary, "im sending the note");
    assert_eq!(
        fixture_hit_count("cleanup", "groq", "llama-3.3-70b-versatile"),
        0
    );
    reset();
}

#[tokio::test(flavor = "current_thread")]
async fn pipeline_fixture_accepts_downloaded_local_model_without_api_keys() {
    let _guard = harness_test_lock().lock().expect("harness lock");
    let _local_models = install_local_models(&["parakeet-v3"]);
    reset();
    set_enabled(true);

    let mut config = base_config();
    config.transcription_provider = store::LOCAL.into();
    config.transcription_default_model = "local/parakeet-v3".into();
    config.cleanup_enabled = false;
    config.cleanup_intensity = "none".into();
    config.key_groq.clear();
    config.key_openai.clear();
    config.key_google.clear();

    fixture(
        "transcription",
        "local",
        "parakeet-v3",
        Some("local transcript"),
        None,
        None,
    );

    let result = run_pipeline_fixture(base_request(config))
        .await
        .expect("downloaded local model should run without cloud keys");
    assert_eq!(result.raw_text, "local transcript");
    assert_eq!(result.final_text_before_dictionary, "local transcript");
    assert_eq!(result.api_used, "local/parakeet-v3/transcription");
    reset();
}

#[tokio::test(flavor = "current_thread")]
async fn pipeline_fixture_strips_filler_words_mechanically_when_cleanup_disabled_but_intensity_is_not_none(
) {
    let _guard = harness_test_lock().lock().expect("harness lock");
    let _local_models = install_local_models(&["parakeet-v3"]);
    reset();
    set_enabled(true);

    let mut config = base_config();
    config.transcription_provider = store::LOCAL.into();
    config.transcription_default_model = "local/parakeet-v3".into();
    config.cleanup_enabled = false;
    config.cleanup_intensity = "medium".into();
    config.key_groq.clear();
    config.key_openai.clear();
    config.key_google.clear();

    fixture(
        "transcription",
        "local",
        "parakeet-v3",
        Some("Just have um basically sync users, uh, to the database."),
        None,
        None,
    );

    let result = run_pipeline_fixture(base_request(config))
        .await
        .expect("should run");
    assert_eq!(
        result.raw_text,
        "Just have um basically sync users, uh, to the database."
    );
    assert_eq!(
        result.final_text_before_dictionary,
        "Just have basically sync users, to the database."
    );
    reset();
}

#[tokio::test(flavor = "current_thread")]
async fn pipeline_fixture_reports_missing_selected_local_model() {
    let _guard = harness_test_lock().lock().expect("harness lock");
    let _local_models = install_local_models(&[]);
    reset();
    set_enabled(true);

    let mut config = base_config();
    config.transcription_provider = store::LOCAL.into();
    config.transcription_default_model = "local/parakeet-v3".into();
    config.cleanup_enabled = false;
    config.cleanup_intensity = "none".into();
    config.key_groq.clear();
    config.key_openai.clear();
    config.key_google.clear();

    let err = run_pipeline_fixture(base_request(config))
        .await
        .expect_err("missing local model should fail with a download message");
    assert!(err
        .to_string()
        .contains("Download the selected local model."));
    reset();
}

#[tokio::test(flavor = "current_thread")]
async fn pipeline_fixture_falls_back_from_retryable_local_failure_to_cloud() {
    let _guard = harness_test_lock().lock().expect("harness lock");
    let _local_models = install_local_models(&["parakeet-v3"]);
    reset();
    set_enabled(true);

    let mut config = base_config();
    config.transcription_provider = store::LOCAL.into();
    config.transcription_default_model = "local/parakeet-v3".into();
    config.transcription_fallback_models = vec!["openai/gpt-4o-transcribe".into()];
    config.cleanup_enabled = false;
    config.cleanup_intensity = "none".into();
    config.key_groq.clear();
    config.key_google.clear();

    fixture(
        "transcription",
        "local",
        "parakeet-v3",
        None,
        Some("timeout"),
        Some("local runtime timed out"),
    );
    fixture(
        "transcription",
        "openai",
        "gpt-4o-transcribe",
        Some("cloud fallback transcript"),
        None,
        None,
    );

    let result = run_pipeline_fixture(base_request(config))
        .await
        .expect("retryable local failure should fall back to cloud");
    assert_eq!(result.raw_text, "cloud fallback transcript");
    assert_eq!(result.api_used, "openai/gpt-4o-transcribe/transcription");
    assert_eq!(
        fixture_hit_count("transcription", "local", "parakeet-v3"),
        1
    );
    assert_eq!(
        fixture_hit_count("transcription", "openai", "gpt-4o-transcribe"),
        1
    );
    reset();
}

#[tokio::test(flavor = "current_thread")]
async fn pipeline_fixture_runs_local_cleanup_without_cloud_credentials() {
    let _guard = harness_test_lock().lock().expect("harness lock");
    let _local_models = install_local_cleanup_models(&["gemma-4-e2b"]);
    reset();
    set_enabled(true);

    let mut config = base_config();
    config.cleanup_provider = store::LOCAL.into();
    config.cleanup_default_model = "local/gemma-4-e2b".into();
    config.key_openai.clear();
    config.key_google.clear();

    fixture(
        "transcription",
        "groq",
        "whisper-large-v3-turbo",
        Some("send the report now"),
        None,
        None,
    );
    fixture(
        "cleanup",
        "local",
        "gemma-4-e2b",
        Some("Send the report now."),
        None,
        None,
    );

    let result = run_pipeline_fixture(base_request(config))
        .await
        .expect("downloaded local cleanup model should run without cloud cleanup keys");
    assert_eq!(result.raw_text, "send the report now");
    assert_eq!(result.final_text_before_dictionary, "Send the report now.");
    assert_eq!(fixture_hit_count("cleanup", "local", "gemma-4-e2b"), 1);
    reset();
}

#[tokio::test(flavor = "current_thread")]
async fn pipeline_fixture_falls_back_from_retryable_local_cleanup_failure_to_cloud() {
    let _guard = harness_test_lock().lock().expect("harness lock");
    let _local_models = install_local_cleanup_models(&["gemma-4-e2b"]);
    reset();
    set_enabled(true);

    let mut config = base_config();
    config.cleanup_provider = store::LOCAL.into();
    config.cleanup_default_model = "local/gemma-4-e2b".into();
    config.cleanup_fallback_models = vec!["openai/gpt-4o-mini".into()];
    config.key_groq = "fixture-groq-key".into();

    fixture(
        "transcription",
        "groq",
        "whisper-large-v3-turbo",
        Some("please send that note to sam"),
        None,
        None,
    );
    fixture(
        "cleanup",
        "local",
        "gemma-4-e2b",
        None,
        Some("timeout"),
        Some("local cleanup runtime timed out"),
    );
    fixture(
        "cleanup",
        "openai",
        "gpt-4o-mini",
        Some("Please send that note to Sam."),
        None,
        None,
    );

    let result = run_pipeline_fixture(base_request(config))
        .await
        .expect("retryable local cleanup failure should fall back to cloud");
    assert_eq!(
        result.final_text_before_dictionary,
        "Please send that note to Sam."
    );
    assert_eq!(fixture_hit_count("cleanup", "local", "gemma-4-e2b"), 1);
    assert_eq!(fixture_hit_count("cleanup", "openai", "gpt-4o-mini"), 1);
    reset();
}

#[tokio::test(flavor = "current_thread")]
async fn pipeline_fixture_uses_cleanup_cache_on_repeat_runs() {
    let _guard = harness_test_lock().lock().expect("harness lock");
    reset();
    set_enabled(true);
    register_fixture(FixtureSpec {
        task: "transcription".into(),
        provider: "groq".into(),
        model: "whisper-large-v3-turbo".into(),
        response: Some("cache me please".into()),
        error_kind: None,
        error_message: None,
    });
    register_fixture(FixtureSpec {
        task: "cleanup".into(),
        provider: "groq".into(),
        model: "llama-3.3-70b-versatile".into(),
        response: Some("cache me please".into()),
        error_kind: None,
        error_message: None,
    });

    let mut request = base_request(base_config());
    request.db = Some(crate::data::db::open(":memory:").expect("shared test db"));
    let first = run_pipeline_fixture(request.clone())
        .await
        .expect("first run should succeed");
    let second = run_pipeline_fixture(request)
        .await
        .expect("second run should succeed");
    assert!(!first.cleanup_cache_key.is_empty());
    assert_eq!(first.cleanup_cache_key, second.cleanup_cache_key);
    assert_eq!(
        fixture_hit_count("cleanup", "groq", "llama-3.3-70b-versatile"),
        1
    );
    reset();
}

#[tokio::test(flavor = "current_thread")]
async fn pipeline_evicts_a_poisoned_cleanup_cache_entry_instead_of_serving_it() {
    // A cache entry written before the artifact-leak guard existed (or by
    // any future bug) must not be served forever just because it's a cache
    // hit — cache hits skip generation entirely, so they also skip the
    // guard that would otherwise have caught it.
    let _guard = harness_test_lock().lock().expect("harness lock");
    reset();
    set_enabled(true);
    register_fixture(FixtureSpec {
        task: "transcription".into(),
        provider: "groq".into(),
        model: "whisper-large-v3-turbo".into(),
        response: Some("cache me please".into()),
        error_kind: None,
        error_message: None,
    });
    register_fixture(FixtureSpec {
        task: "cleanup".into(),
        provider: "groq".into(),
        model: "llama-3.3-70b-versatile".into(),
        response: Some("Cache me please.".into()),
        error_kind: None,
        error_message: None,
    });

    let mut request = base_request(base_config());
    request.db = Some(crate::data::db::open(":memory:").expect("shared test db"));
    let first = run_pipeline_fixture(request.clone())
        .await
        .expect("first run should succeed");
    assert!(!first.cleanup_cache_key.is_empty());

    // Simulate a stale poisoned entry sitting at that same cache key.
    crate::data::db::cleanup_cache_insert_new(
        request.db.as_ref().expect("shared test db"),
        &first.cleanup_cache_key,
        "Thinking Process: 1. Analyze the request...",
        "2999-01-01 00:00:00",
        false,
    )
    .expect("poison cache entry");

    let second = run_pipeline_fixture(request)
        .await
        .expect("second run should succeed");

    assert!(!second.injected_text.contains("Thinking Process"));
    assert_eq!(second.injected_text, "Cache me please.");
    // Regenerated rather than served from the poisoned entry.
    assert_eq!(
        fixture_hit_count("cleanup", "groq", "llama-3.3-70b-versatile"),
        2
    );
    reset();
}
