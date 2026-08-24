use std::path::PathBuf;
use std::{collections::BTreeMap, path::Path};

use bytes::Bytes;
use serde::Deserialize;

use super::{cleanup, transcription, ProviderId};

#[derive(Deserialize)]
struct FixtureFile {
    live_cases: Vec<LiveCase>,
}

#[derive(Deserialize)]
struct LiveCase {
    id: String,
    input: String,
    profile: String,
    intensity: String,
    required_terms: Vec<String>,
    forbidden_terms: Vec<String>,
    max_output_chars: usize,
}

fn settings_path() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        return std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .map(|path| path.join("com.verenu.app").join("settings.json"));
    }
    #[cfg(target_os = "macos")]
    {
        return std::env::var_os("HOME").map(PathBuf::from).map(|path| {
            path.join("Library")
                .join("Application Support")
                .join("com.verenu.app")
                .join("settings.json")
        });
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        let base = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|path| PathBuf::from(path).join(".config")))?;
        Some(base.join("com.verenu.app").join("settings.json"))
    }
}

fn load_settings() -> Option<serde_json::Value> {
    let raw = std::fs::read_to_string(settings_path()?).ok()?;
    serde_json::from_str(&raw).ok()
}

fn first_environment_provider() -> Option<String> {
    [
        ("groq", "GROQ_API_KEY"),
        ("openai", "OPENAI_API_KEY"),
        ("google", "GOOGLE_API_KEY"),
    ]
    .into_iter()
    .find(|(_, variable)| std::env::var(variable).is_ok_and(|value| !value.trim().is_empty()))
    .map(|(provider, _)| provider.to_string())
}

fn configured_cleanup() -> Option<(String, String)> {
    let settings = load_settings();
    let provider = settings
        .as_ref()
        .and_then(|value| value.get("cleanup_provider"))
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
        .or_else(first_environment_provider)?;
    let default_model = match provider.as_str() {
        "openai" => "gpt-4o-mini",
        "google" => "gemini-3.5-flash",
        _ => "qwen/qwen3.6-27b",
    };
    let configured_model = settings
        .as_ref()
        .and_then(|value| value.get("cleanup_model"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or(default_model);
    let model = configured_model
        .strip_prefix(&format!("{provider}/"))
        .unwrap_or(configured_model)
        .to_string();
    Some((provider, model))
}

fn configured_transcription() -> Option<(String, String, String)> {
    let settings = load_settings();
    let provider = settings
        .as_ref()
        .and_then(|value| value.get("transcription_provider"))
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
        .or_else(first_environment_provider)?;
    let default_model = match provider.as_str() {
        "openai" => "gpt-4o-transcribe",
        "google" => "gemini-3.5-flash",
        _ => "whisper-large-v3-turbo",
    };
    let configured_model = settings
        .as_ref()
        .and_then(|value| value.get("transcription_model"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or(default_model);
    let model = configured_model
        .strip_prefix(&format!("{provider}/"))
        .unwrap_or(configured_model)
        .to_string();
    let language = settings
        .as_ref()
        .and_then(|value| value.get("transcription_language"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or("en")
        .to_string();
    Some((provider, model, language))
}

fn credential_for(provider: &str) -> String {
    let saved = crate::data::credentials::get(provider);
    if !saved.is_empty() {
        return saved;
    }
    let allow_environment = std::env::var_os("CI").is_some()
        || std::env::var("VERENU_ALLOW_ENV_CREDENTIALS").is_ok_and(|value| value == "1");
    if !allow_environment {
        return String::new();
    }
    let variable = match provider {
        "groq" => "GROQ_API_KEY",
        "openai" => "OPENAI_API_KEY",
        "google" => "GOOGLE_API_KEY",
        "assemblyai" => "ASSEMBLYAI_API_KEY",
        _ => return String::new(),
    };
    std::env::var(variable)
        .unwrap_or_default()
        .trim()
        .to_string()
}

#[tokio::test]
#[ignore = "uses the configured provider and may incur API cost"]
async fn live_prompt_regression() {
    let Some((provider, model)) = configured_cleanup() else {
        println!("VERENU_LIVE_SKIP: no configured cleanup provider/model was found");
        return;
    };
    if provider == "local" || provider == "assemblyai" {
        println!(
            "VERENU_LIVE_SKIP: configured cleanup provider has no supported cloud cleanup path"
        );
        return;
    }
    let api_key = credential_for(&provider);
    if api_key.is_empty() {
        println!("VERENU_LIVE_SKIP: configured provider credential is unavailable");
        return;
    }

    let fixtures: FixtureFile = serde_json::from_str(include_str!(
        "../../../tests/fixtures/prompt-regressions.json"
    ))
    .expect("prompt regression fixtures must be valid JSON");
    let provider_id = ProviderId::from_str(&provider);
    let mut failures = Vec::new();
    let mut measurements = BTreeMap::new();

    for (index, case) in fixtures.live_cases.into_iter().enumerate() {
        let output = match cleanup::cleanup(
            &case.input,
            provider_id,
            &api_key,
            &model,
            &case.profile,
            &case.intensity,
            "",
            None,
            None,
            index as u64 + 1,
        )
        .await
        {
            Ok(output) => output,
            Err(error) => {
                failures.push(format!("{} provider request failed: {error}", case.id));
                continue;
            }
        };
        let normalized = output.to_lowercase();
        for required in case.required_terms {
            if !normalized.contains(&required.to_lowercase()) {
                failures.push(format!(
                    "{} missing required meaning token {required:?}",
                    case.id
                ));
            }
        }
        for forbidden in case.forbidden_terms {
            if normalized.contains(&forbidden.to_lowercase()) {
                failures.push(format!(
                    "{} included forbidden behavior token {forbidden:?}",
                    case.id
                ));
            }
        }
        let chars = output.chars().count();
        if chars == 0 || chars > case.max_output_chars {
            failures.push(format!(
                "{} output length {chars} was outside 1..={}",
                case.id, case.max_output_chars
            ));
        }
        measurements.insert(case.id.clone(), chars);
        println!("VERENU_LIVE_CASE: {} chars={chars}", case.id);
    }

    let status = if failures.is_empty() {
        "passed"
    } else {
        "failed"
    };
    println!(
        "VERENU_TEST_RESULT={}",
        serde_json::json!({
            "status": status,
            "expected": "Configured cleanup model obeys meaning, instruction-boundary, language, and length invariants",
            "observed": if failures.is_empty() { "All live prompt cases held".to_string() } else { failures.join("; ") },
            "measurements": measurements,
            "regression_area": "prompt and model behavior",
            "failure_kind": if failures.is_empty() { serde_json::Value::Null } else { serde_json::Value::String("product".to_string()) }
        })
    );
    assert!(failures.is_empty(), "{}", failures.join("; "));
}

#[tokio::test]
#[ignore = "uses the configured provider and may incur API cost"]
async fn live_transcription_regression() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("tests")
        .join("smoke")
        .join("smoke_test.wav");
    if !fixture.is_file() {
        println!("VERENU_LIVE_SKIP: optional smoke_test.wav fixture is unavailable");
        return;
    }
    let Some((provider, model, language)) = configured_transcription() else {
        println!("VERENU_LIVE_SKIP: no configured transcription provider/model was found");
        return;
    };
    if provider == "local" {
        println!("VERENU_LIVE_SKIP: local transcription requires the native runtime harness");
        return;
    }
    let api_key = credential_for(&provider);
    if api_key.is_empty() {
        println!("VERENU_LIVE_SKIP: configured provider credential is unavailable");
        return;
    }
    let wav = std::fs::read(&fixture).expect("read smoke_test.wav");
    let wav_bytes = wav.len();
    let output = match transcription::transcribe(
        Bytes::from(wav),
        ProviderId::from_str(&provider),
        &api_key,
        &language,
        &model,
        1,
    )
    .await
    {
        Ok(output) => output,
        Err(error) => {
            println!(
                "VERENU_TEST_RESULT={}",
                serde_json::json!({
                    "status": "failed",
                    "expected": "Configured provider returns a non-trivial transcription for the known WAV fixture",
                    "observed": format!("Configured transcription request failed: {error}"),
                    "measurements": { "wav_bytes": wav_bytes },
                    "regression_area": "provider transcription pipeline",
                    "failure_kind": "infrastructure"
                })
            );
            panic!("configured transcription request failed: {error}");
        }
    };
    let chars = output.trim().chars().count();
    let words = output.split_whitespace().count();
    let passed = chars >= 10 && words >= 3;
    println!(
        "VERENU_TEST_RESULT={}",
        serde_json::json!({
            "status": if passed { "passed" } else { "failed" },
            "expected": "Configured provider returns a non-trivial transcription for the known WAV fixture",
            "observed": if passed { format!("Transcription returned {words} words") } else { format!("Transcription was too short: {chars} characters, {words} words") },
            "measurements": { "wav_bytes": wav_bytes, "output_chars": chars, "output_words": words },
            "regression_area": "provider transcription pipeline",
            "failure_kind": if passed { serde_json::Value::Null } else { serde_json::Value::String("product".to_string()) }
        })
    );
    assert!(
        passed,
        "configured transcription output was empty or too short"
    );
}
