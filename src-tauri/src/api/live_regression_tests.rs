use std::path::PathBuf;
use std::{collections::BTreeMap, path::Path};

use bytes::Bytes;
use serde::Deserialize;
use std::collections::BTreeSet;
use std::time::Instant;

use super::{cleanup, transcription, ProviderId};

#[derive(Deserialize)]
struct FixtureFile {
    live_cases: Vec<LiveCase>,
}

#[derive(Deserialize)]
struct LiveCase {
    id: String,
    input: String,
    #[serde(default)]
    alternate: Option<String>,
    #[serde(default)]
    evidence: String,
    #[serde(default)]
    app_context: Option<String>,
    profile: String,
    intensity: String,
    required_terms: Vec<String>,
    forbidden_terms: Vec<String>,
    #[serde(default)]
    wrong_correction_terms: Vec<String>,
    #[serde(default)]
    format_expectation: Option<String>,
    #[serde(default)]
    min_reduction_ratio: Option<f64>,
    #[serde(default)]
    cleanup_level_group: Option<String>,
    max_output_chars: usize,
}

fn settings_path() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .map(|path| path.join("com.verenu.app").join("settings.json"))
    }
    #[cfg(target_os = "macos")]
    {
        std::env::var_os("HOME").map(PathBuf::from).map(|path| {
            path.join("Library")
                .join("Application Support")
                .join("com.verenu.app")
                .join("settings.json")
        })
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

fn environment_credentials_allowed() -> bool {
    std::env::var_os("CI").is_some()
        || std::env::var("VERENU_ALLOW_ENV_CREDENTIALS").is_ok_and(|value| value == "1")
}

fn first_environment_provider() -> Option<String> {
    if !environment_credentials_allowed() {
        return None;
    }
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
        "google" => "gemini-3.5-flash-lite",
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
        "google" => "gemini-3.5-transcribe",
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
    if !environment_credentials_allowed() {
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

#[derive(Default)]
struct QualityScores {
    semantic_preservation: f64,
    unwanted_additions: f64,
    missed_cleanup: f64,
    incorrect_corrections: f64,
    formatting: f64,
    cleanup_level_compliance: f64,
}

struct ModelReport {
    model: String,
    cases: usize,
    successful_requests: usize,
    scores: QualityScores,
    latencies_ms: Vec<u128>,
    prompt_tokens: Vec<usize>,
    input_tokens: usize,
    output_tokens: usize,
    estimated_cost_usd: f64,
    formatting_cases: usize,
    failures: Vec<String>,
}

fn contains_eval_term(text: &str, term: &str) -> bool {
    let text = fold_eval_diacritics(&text.to_lowercase());
    let term = fold_eval_diacritics(&term.trim().to_lowercase());
    if term.is_empty() {
        return false;
    }
    // The live score is semantic rather than a verbatim diff. These are
    // intentionally narrow software-dictation equivalences; technical terms
    // such as Kubernetes, Claude, paths, and identifiers remain exact.
    let semantic_aliases: &[&str] = match term.as_str() {
        "release" => &["release", "ship"],
        "send" => &["send", "deliver"],
        _ => &[],
    };
    if semantic_aliases
        .iter()
        .any(|alias| contains_eval_term_normalized(&text, alias))
    {
        return true;
    }
    contains_eval_term_normalized(&text, &term)
}

fn contains_eval_term_normalized(text: &str, term: &str) -> bool {
    if term.chars().all(char::is_alphanumeric) {
        let tokens = crate::system::text::tokenize_lower_alnum(&text);
        if tokens.iter().any(|token| token == &term) {
            return true;
        }
        if let Some(number) = spoken_number_alias(&term) {
            return tokens.iter().any(|token| token == number);
        }
        return false;
    }
    text.contains(&term)
}

fn fold_eval_diacritics(text: &str) -> String {
    text.chars()
        .map(|character| match character {
            'á' | 'à' | 'â' | 'ä' | 'ã' | 'å' => 'a',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'í' | 'ì' | 'î' | 'ï' => 'i',
            'ó' | 'ò' | 'ô' | 'ö' | 'õ' => 'o',
            'ú' | 'ù' | 'û' | 'ü' => 'u',
            'ñ' => 'n',
            'ç' => 'c',
            other => other,
        })
        .collect()
}

fn spoken_number_alias(term: &str) -> Option<&'static str> {
    match term {
        "zero" => Some("0"),
        "one" => Some("1"),
        "two" => Some("2"),
        "three" => Some("3"),
        "four" => Some("4"),
        "five" => Some("5"),
        "six" => Some("6"),
        "seven" => Some("7"),
        "eight" => Some("8"),
        "nine" => Some("9"),
        "ten" => Some("10"),
        _ => None,
    }
}

fn eval_token_set(text: &str) -> BTreeSet<String> {
    crate::system::text::tokenize_lower_alnum(&fold_eval_diacritics(text))
        .into_iter()
        .collect()
}

fn eval_word_count(text: &str) -> usize {
    crate::system::text::tokenize_lower_alnum(&fold_eval_diacritics(text)).len()
}

fn production_cleanup_output(case: &LiveCase, output: String) -> String {
    if case.intensity == "none" {
        output
    } else {
        let output = crate::system::text::strip_unspoken_em_dashes(&case.input, &output);
        crate::system::text::strip_filler_hesitations(&output)
    }
}

fn score_case(case: &LiveCase, output: &str) -> (QualityScores, Vec<String>, bool) {
    let output_tokens = eval_token_set(output);
    let source_tokens = eval_token_set(&case.input);
    let required_hits = case
        .required_terms
        .iter()
        .filter(|term| contains_eval_term(output, term))
        .count();
    let forbidden_hits = case
        .forbidden_terms
        .iter()
        .filter(|term| contains_eval_term(output, term))
        .count();
    let wrong_correction_hits = case
        .wrong_correction_terms
        .iter()
        .filter(|term| contains_eval_term(output, term))
        .count();
    let additions = output_tokens.difference(&source_tokens).count();
    let output_word_count = eval_word_count(output);
    let source_word_count = eval_word_count(&case.input);
    let semantic_preservation = if case.required_terms.is_empty() {
        1.0
    } else {
        required_hits as f64 / case.required_terms.len() as f64
    };
    let missed_cleanup = if case.forbidden_terms.is_empty() {
        1.0
    } else {
        1.0 - forbidden_hits as f64 / case.forbidden_terms.len() as f64
    };
    let incorrect_corrections = if case.wrong_correction_terms.is_empty() {
        1.0
    } else {
        1.0 - wrong_correction_hits as f64 / case.wrong_correction_terms.len() as f64
    };
    let unwanted_additions = if output_word_count == 0 {
        0.0
    } else {
        (1.0 - additions as f64 / output_word_count as f64).max(0.0)
    };

    let formatting_applicable = case.format_expectation.is_some();
    let formatting = case
        .format_expectation
        .as_deref()
        .map(|expected| f64::from(contains_eval_term(output, expected)))
        .unwrap_or(1.0);

    let output_chars = output.chars().count();
    let max_ratio = match case.intensity.as_str() {
        "none" => 1.35,
        "light" => 1.15,
        "medium" => 1.2,
        "high" => 1.0,
        _ => 1.2,
    };
    let min_ratio = match case.intensity.as_str() {
        "none" => 0.65,
        "light" => 0.5,
        "medium" => 0.4,
        "high" => 0.25,
        _ => 0.4,
    };
    let requested_max_words = case
        .min_reduction_ratio
        .map(|reduction| ((1.0 - reduction) * source_word_count as f64).ceil() as usize);
    let level_ok = output_chars > 0
        && output_chars <= case.max_output_chars
        && (source_word_count == 0
            || ((output_word_count as f64) <= source_word_count as f64 * max_ratio
                && (output_word_count as f64) >= source_word_count as f64 * min_ratio))
        && requested_max_words.is_none_or(|max_words| output_word_count <= max_words);
    let cleanup_level_compliance = f64::from(level_ok);

    let mut failures = Vec::new();
    for term in &case.required_terms {
        if !contains_eval_term(output, term) {
            failures.push(format!("{} missing {term:?}", case.id));
        }
    }
    for term in &case.forbidden_terms {
        if contains_eval_term(output, term) {
            failures.push(format!("{} retained removable {term:?}", case.id));
        }
    }
    for term in &case.wrong_correction_terms {
        if contains_eval_term(output, term) {
            failures.push(format!("{} made incorrect correction to {term:?}", case.id));
        }
    }
    if formatting == 0.0 {
        failures.push(format!("{} missed expected formatting", case.id));
    }
    if output_chars == 0 || output_chars > case.max_output_chars {
        failures.push(format!(
            "{} output length {output_chars} outside 1..={}",
            case.id, case.max_output_chars
        ));
    }
    if !level_ok {
        failures.push(format!(
            "{} exceeded its {} cleanup budget (source_words={source_word_count} output_words={output_word_count} max_chars={})",
            case.id, case.intensity, case.max_output_chars
        ));
    }

    (
        QualityScores {
            semantic_preservation,
            unwanted_additions,
            missed_cleanup,
            incorrect_corrections,
            formatting,
            cleanup_level_compliance,
        },
        failures,
        formatting_applicable,
    )
}

fn estimate_tokens(chars: usize) -> usize {
    chars.saturating_add(3) / 4
}

fn gemini_price_per_million(model: &str) -> (f64, f64) {
    if model.to_ascii_lowercase().contains("flash-lite") {
        (0.30, 2.50)
    } else {
        (2.70, 16.20)
    }
}

fn average(values: &[usize]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    values.iter().sum::<usize>() as f64 / values.len() as f64
}

fn average_millis(values: &[u128]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    values.iter().sum::<u128>() as f64 / values.len() as f64
}

fn p95_millis(values: &[u128]) -> u128 {
    if values.is_empty() {
        return 0;
    }
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    let index = ((sorted.len() * 95).saturating_add(99) / 100).saturating_sub(1);
    sorted[index]
}

fn report_json(report: &ModelReport) -> serde_json::Value {
    let count = report.successful_requests.max(1) as f64;
    serde_json::json!({
        "model": report.model,
        "cases": report.cases,
        "successful_requests": report.successful_requests,
        "scores": {
            "semantic_preservation": report.scores.semantic_preservation / count,
            "unwanted_additions": report.scores.unwanted_additions / count,
            "missed_cleanup": report.scores.missed_cleanup / count,
            "incorrect_corrections": report.scores.incorrect_corrections / count,
            "formatting": report.scores.formatting / count,
            "cleanup_level_compliance": report.scores.cleanup_level_compliance / count,
        },
        "formatting_cases": report.formatting_cases,
        "average_latency_ms": average_millis(&report.latencies_ms),
        "p95_latency_ms": p95_millis(&report.latencies_ms),
        "average_rendered_prompt_tokens": average(&report.prompt_tokens),
        "worst_rendered_prompt_tokens": report.prompt_tokens.iter().copied().max().unwrap_or(0),
        "estimated_input_tokens": report.input_tokens,
        "estimated_output_tokens": report.output_tokens,
        "estimated_cost_usd": report.estimated_cost_usd,
        "failures": report.failures,
    })
}

async fn run_gemini_eval(model: &str, cases: &[LiveCase], api_key: &str) -> ModelReport {
    let (input_price, output_price) = gemini_price_per_million(model);
    let mut report = ModelReport {
        model: model.to_string(),
        cases: cases.len(),
        successful_requests: 0,
        scores: QualityScores::default(),
        latencies_ms: Vec::new(),
        prompt_tokens: Vec::new(),
        input_tokens: 0,
        output_tokens: 0,
        estimated_cost_usd: 0.0,
        formatting_cases: 0,
        failures: Vec::new(),
    };
    let mut cleanup_level_observations: BTreeMap<String, BTreeMap<String, usize>> = BTreeMap::new();

    for (index, case) in cases.iter().enumerate() {
        let prompt = super::prompts::get_cleanup_prompt_with_alternate_and_evidence(
            "google",
            model,
            &case.profile,
            &case.intensity,
            "",
            &case.evidence,
            case.app_context.as_deref(),
            &case.input,
            None,
            case.alternate.as_deref(),
        );
        let transcript_input =
            cleanup::format_transcript_input(&case.input, case.alternate.as_deref());
        let rendered_prompt_tokens = estimate_tokens(prompt.chars().count());
        let input_tokens = estimate_tokens(
            prompt
                .chars()
                .count()
                .saturating_add(transcript_input.chars().count()),
        );
        report.prompt_tokens.push(rendered_prompt_tokens);
        report.input_tokens = report.input_tokens.saturating_add(input_tokens);

        let started = Instant::now();
        let output = cleanup::cleanup_with_alternate_and_evidence(
            &case.input,
            ProviderId::Google,
            api_key,
            model,
            &case.profile,
            &case.intensity,
            "",
            &case.evidence,
            case.app_context.as_deref(),
            None,
            case.alternate.as_deref(),
            index as u64 + 1,
        )
        .await;
        let latency_ms = started.elapsed().as_millis();
        match output {
            Ok(output) => {
                let output = production_cleanup_output(case, output);
                let output_tokens = estimate_tokens(output.chars().count());
                let (scores, failures, formatting_applicable) = score_case(case, &output);
                report.successful_requests += 1;
                report.latencies_ms.push(latency_ms);
                report.output_tokens = report.output_tokens.saturating_add(output_tokens);
                report.estimated_cost_usd += input_tokens as f64 * input_price / 1_000_000.0
                    + output_tokens as f64 * output_price / 1_000_000.0;
                report.scores.semantic_preservation += scores.semantic_preservation;
                report.scores.unwanted_additions += scores.unwanted_additions;
                report.scores.missed_cleanup += scores.missed_cleanup;
                report.scores.incorrect_corrections += scores.incorrect_corrections;
                report.scores.formatting += scores.formatting;
                report.scores.cleanup_level_compliance += scores.cleanup_level_compliance;
                report.formatting_cases += usize::from(formatting_applicable);
                report.failures.extend(failures);
                if let Some(group) = case.cleanup_level_group.as_deref() {
                    cleanup_level_observations
                        .entry(group.to_string())
                        .or_default()
                        .insert(case.intensity.clone(), eval_word_count(&output));
                }
                println!(
                    "VERENU_GEMINI_EVAL_CASE: model={} id={} semantic={:.2} additions={:.2} missed_cleanup={:.2} incorrect_corrections={:.2} formatting={:.2} level={:.2} output_chars={} output_words={} latency_ms={} prompt_tokens={}",
                    model,
                    case.id,
                    scores.semantic_preservation,
                    scores.unwanted_additions,
                    scores.missed_cleanup,
                    scores.incorrect_corrections,
                    scores.formatting,
                    scores.cleanup_level_compliance,
                    output.chars().count(),
                    eval_word_count(&output),
                    latency_ms,
                    rendered_prompt_tokens
                );
            }
            Err(error) => {
                report
                    .failures
                    .push(format!("{} request failed: {error}", case.id));
                println!(
                    "VERENU_GEMINI_EVAL_CASE: model={} id={} request_error latency_ms={}",
                    model, case.id, latency_ms
                );
            }
        }
    }
    for (group, levels) in cleanup_level_observations {
        let Some(light) = levels.get("light").copied() else {
            continue;
        };
        let Some(medium) = levels.get("medium").copied() else {
            continue;
        };
        let Some(strong) = levels.get("high").copied() else {
            continue;
        };
        if light < medium.saturating_add(3) {
            report.failures.push(format!(
                "{group} did not materially separate Light from Medium (light_words={light} medium_words={medium})"
            ));
        }
        if medium < strong.saturating_add(3) {
            report.failures.push(format!(
                "{group} did not materially separate Medium from Strong (medium_words={medium} strong_words={strong})"
            ));
        }
    }
    report
}

#[tokio::test]
#[ignore = "uses Gemini models and may incur API cost"]
async fn live_gemini_cleanup_comparison() {
    let api_key = credential_for("google");
    if api_key.is_empty() {
        println!("VERENU_LIVE_SKIP: Google credential is unavailable");
        return;
    }
    let fixtures: FixtureFile = serde_json::from_str(include_str!(
        "../../../tests/fixtures/prompt-regressions.json"
    ))
    .expect("prompt regression fixtures must be valid JSON");
    let target_model = "gemini-3.5-flash-lite";
    let stronger_eligible_model = "gemini-3.5-flash";
    let previous_stronger_model = "gemini-3.7-flash";
    assert!(
        super::prompts::gemini_generation_reasoning_supported(target_model),
        "target model must have an explicit Gemini reasoning policy"
    );
    assert!(
        super::prompts::gemini_generation_reasoning_supported(stronger_eligible_model),
        "comparison model must have an explicit Gemini reasoning policy"
    );
    println!(
        "VERENU_GEMINI_EVAL_MODELS: target={} target_thinkingLevel=minimal comparison={} comparison_thinkingLevel=minimal previous_model={} previous_model_status=ineligible_minimal_unsupported",
        target_model, stronger_eligible_model, previous_stronger_model
    );

    let selected_cases: Vec<LiveCase> = if let Some(filter) = std::env::var_os("VERENU_LIVE_CASE") {
        let filter = filter.to_string_lossy();
        fixtures
            .live_cases
            .into_iter()
            .filter(|case| case.id == filter)
            .collect()
    } else {
        fixtures.live_cases
    };
    if selected_cases.is_empty() {
        println!("VERENU_LIVE_SKIP: VERENU_LIVE_CASE did not match the corpus");
        return;
    }
    let target = run_gemini_eval(target_model, &selected_cases, &api_key).await;
    let stronger = run_gemini_eval(stronger_eligible_model, &selected_cases, &api_key).await;
    let target_passed = target.successful_requests == target.cases && target.failures.is_empty();
    println!(
        "VERENU_TEST_RESULT={}",
        serde_json::json!({
            "status": if target_passed { "passed" } else { "failed" },
            "expected": "Gemini 3.5 Flash-Lite cleans the difficult dictation corpus without semantic, safety, formatting, or budget regressions",
            "observed": {
                "target": report_json(&target),
                "stronger_eligible_comparison": report_json(&stronger),
                "previous_stronger_model": {
                    "model": previous_stronger_model,
                    "status": "not_called",
                    "reason": "Gemini 3.7 Flash supports low/medium/high, not the required minimal level"
                }
            },
            "regression_area": "live cheap-model cleanup quality",
            "failure_kind": if target_passed { serde_json::Value::Null } else { serde_json::Value::String("product".to_string()) }
        })
    );
    assert!(
        target_passed,
        "Gemini 3.5 Flash-Lite live eval failed: {}",
        target.failures.join("; ")
    );
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
        let output = match cleanup::cleanup_with_alternate_and_evidence(
            &case.input,
            provider_id,
            &api_key,
            &model,
            &case.profile,
            &case.intensity,
            "",
            &case.evidence,
            case.app_context.as_deref(),
            None,
            case.alternate.as_deref(),
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
