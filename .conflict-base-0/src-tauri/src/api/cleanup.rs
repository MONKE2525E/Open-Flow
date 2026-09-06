use anyhow::Result;
use serde::{Deserialize, Serialize};

use super::gemini_types::{GeminiGenerateReq, GeminiReqContent, GeminiReqPart};
use super::prompts::{cleanup_max_output_tokens, gemini_generation_config};
use super::ProviderId;

// Cleanup should be fast enough to run inline with dictation delivery. Keep
// this shorter than the shared client timeout so a stalled provider can fall
// through to the configured cleanup fallback instead of leaving the pill in
// processing for two minutes.
const CLEANUP_REQUEST_TIMEOUT_SECS: u64 = 45;

#[allow(clippy::too_many_arguments)]
pub async fn cleanup(
    text: &str,
    provider: ProviderId,
    api_key: &str,
    model: &str,
    profile: &str,
    intensity: &str,
    snippet_instructions: &str,
    app_context: Option<&str>,
    custom_template: Option<&str>,
    gen: u64,
) -> Result<String> {
    cleanup_with_alternate(
        text,
        provider,
        api_key,
        model,
        profile,
        intensity,
        snippet_instructions,
        app_context,
        custom_template,
        None,
        gen,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
pub async fn cleanup_with_alternate(
    text: &str,
    provider: ProviderId,
    api_key: &str,
    model: &str,
    profile: &str,
    intensity: &str,
    snippet_instructions: &str,
    app_context: Option<&str>,
    custom_template: Option<&str>,
    alternate_transcript: Option<&str>,
    gen: u64,
) -> Result<String> {
    cleanup_with_alternate_and_evidence(
        text,
        provider,
        api_key,
        model,
        profile,
        intensity,
        snippet_instructions,
        "",
        app_context,
        custom_template,
        alternate_transcript,
        gen,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
pub async fn cleanup_with_alternate_and_evidence(
    text: &str,
    provider: ProviderId,
    api_key: &str,
    model: &str,
    profile: &str,
    intensity: &str,
    user_overrides: &str,
    evidence: &str,
    app_context: Option<&str>,
    custom_template: Option<&str>,
    alternate_transcript: Option<&str>,
    gen: u64,
) -> Result<String> {
    if provider == ProviderId::AssemblyAi {
        anyhow::bail!("AssemblyAI provides transcription only; choose a cleanup provider")
    }
    if intensity == "none" && alternate_transcript.is_none() {
        // The pipeline normally bypasses this function for Off. Keep the API
        // boundary safe too: only dual-transcript reconciliation is a valid
        // model operation at this intensity.
        return Ok(text.to_owned());
    }
    let provider_id = provider.as_str();
    #[cfg(any(test, debug_assertions))]
    if let Some(result) = crate::testing::resolve_provider_fixture("cleanup", provider_id, model) {
        return result;
    }

    let prompt = super::prompts::get_cleanup_prompt_with_alternate_and_evidence(
        provider_id,
        model,
        profile,
        intensity,
        user_overrides,
        evidence,
        app_context,
        text,
        custom_template,
        alternate_transcript,
    );
    let max_output_tokens = if intensity == "none" {
        alternate_transcript
            .map(|alternate| super::prompts::fusion_max_output_tokens(text, alternate))
            .unwrap_or_else(|| cleanup_max_output_tokens(intensity, text))
    } else {
        cleanup_max_output_tokens(intensity, text)
    };
    log::debug!(
        "cleanup: start gen={} provider={:?} model={} profile={} intensity={} input_chars={} prompt_chars={} max_output_tokens={} snippet_rule_lines={} app_context={} custom_template={}",
        gen,
        provider,
        model,
        profile,
        intensity,
        text.chars().count(),
        prompt.chars().count(),
        max_output_tokens,
        user_overrides.lines().filter(|l| !l.trim().is_empty()).count(),
        app_context.is_some(),
        custom_template.is_some()
    );
    if crate::system::logger::is_verbose() && !user_overrides.is_empty() {
        log::debug!(
            "cleanup: snippet_rules_meta lines={} chars={}",
            user_overrides
                .lines()
                .filter(|line| !line.trim().is_empty())
                .count(),
            user_overrides.chars().count()
        );
    }
    let request = async {
        if let Some(url) = provider.cleanup_url() {
            openai_compat(
                text,
                api_key,
                url,
                provider.label(),
                model,
                &prompt,
                max_output_tokens,
                alternate_transcript,
                gen,
            )
            .await
        } else {
            google_cleanup(
                text,
                api_key,
                &prompt,
                model,
                max_output_tokens,
                alternate_transcript,
                gen,
            )
            .await
        }
    };

    match tokio::time::timeout(
        std::time::Duration::from_secs(CLEANUP_REQUEST_TIMEOUT_SECS),
        request,
    )
    .await
    {
        Ok(result) => result,
        Err(_) => {
            log::warn!(
                "cleanup: request timeout gen={} provider={} model={} timeout_secs={}",
                gen,
                provider.label(),
                model,
                CLEANUP_REQUEST_TIMEOUT_SECS
            );
            Err(anyhow::anyhow!(
                "Cleanup API timeout provider={} model={} timeout_secs={}",
                provider.label(),
                model,
                CLEANUP_REQUEST_TIMEOUT_SECS
            ))
        }
    }
}

#[derive(Serialize)]
struct ChatReq {
    model: String,
    messages: Vec<Msg>,
    max_tokens: u32,
    temperature: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    reasoning_effort: Option<&'static str>,
}

#[derive(Serialize)]
struct Msg {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct ChatResp {
    choices: Vec<Choice>,
}

#[derive(Deserialize)]
struct Choice {
    message: MsgResp,
}

#[derive(Deserialize)]
struct MsgResp {
    content: String,
}

#[allow(clippy::too_many_arguments)]
async fn openai_compat(
    text: &str,
    api_key: &str,
    url: &str,
    provider_label: &str,
    model: &str,
    prompt: &str,
    max_tokens: u32,
    alternate_transcript: Option<&str>,
    gen: u64,
) -> Result<String> {
    ensure_openai_compat_reasoning_policy(provider_label, model)?;
    let body = build_openai_compat_request_with_alternate(
        text,
        model,
        prompt,
        max_tokens,
        alternate_transcript,
        provider_label,
    );

    log::debug!(
        "cleanup: openai_compat request gen={} provider={} model={} url={} input_chars={} prompt_chars={}",
        gen,
        provider_label,
        model,
        url,
        text.chars().count(),
        prompt.chars().count()
    );
    let request_started = std::time::Instant::now();
    let resp = super::client::get()
        .post(url)
        .bearer_auth(api_key)
        .json(&body)
        .send()
        .await?;
    let status = resp.status();
    let request_id = super::response_request_id(&resp);
    log::debug!(
        "cleanup: openai_compat response gen={} provider={} status={} request_id={} latency_ms={}",
        gen,
        provider_label,
        status,
        request_id,
        request_started.elapsed().as_millis()
    );

    let resp = match super::ensure_provider_success(
        resp,
        provider_label,
        Some((provider_label, model)),
    )
    .await
    {
        Ok(resp) => resp,
        Err(super::ProviderHttpError::Quota(e)) => return Err(e),
        Err(super::ProviderHttpError::Auth {
            error,
            status,
            request_id,
            preview,
        }) => {
            log::warn!(
                "cleanup: openai_compat unauthorized gen={} provider={} model={} status={} request_id={} body_preview=\"{}\"",
                gen,
                provider_label,
                model,
                status,
                request_id,
                preview
            );
            return Err(error);
        }
        Err(super::ProviderHttpError::NonSuccess {
            source,
            status,
            request_id,
            preview,
        }) => {
            log::warn!(
                "cleanup: openai_compat non_success gen={} provider={} model={} status={} request_id={} body_preview=\"{}\"",
                gen,
                provider_label,
                model,
                status,
                request_id,
                preview
            );
            return Err(anyhow::Error::new(source).context(format!(
                "Cleanup API error provider={} model={} status={} request_id={} body_preview={}",
                provider_label, model, status, request_id, preview
            )));
        }
    };

    let data: ChatResp = resp.json().await?;
    let output = data
        .choices
        .first()
        .map(|c| c.message.content.trim().to_owned())
        .ok_or_else(|| anyhow::anyhow!("No choices in OpenAI response"))?;
    log::debug!(
        "cleanup: openai_compat parsed gen={} chars={}",
        gen,
        output.chars().count()
    );
    Ok(output)
}

fn ensure_openai_compat_reasoning_policy(provider_label: &str, model: &str) -> Result<()> {
    if !openai_compat_model_supports_no_reasoning(provider_label, model) {
        anyhow::bail!(
            "{} model '{model}' cannot satisfy Verenu's dictation reasoning policy; choose Qwen 3.6/3.8, GPT-5.1, or an ordinary non-reasoning model.",
            provider_label
        )
    }
    Ok(())
}

fn is_groq_qwen_no_reasoning_model(model: &str) -> bool {
    let model = model.trim().to_ascii_lowercase();
    model.starts_with("qwen/qwen3.6-") || model.starts_with("qwen/qwen3.8-")
}

fn is_openai_gpt_51_no_reasoning_model(model: &str) -> bool {
    model.trim().to_ascii_lowercase().starts_with("gpt-5.1")
}

fn openai_compat_model_supports_no_reasoning(provider_label: &str, model: &str) -> bool {
    let provider = provider_label.trim().to_ascii_lowercase();
    let model = model.trim().to_ascii_lowercase();
    if model.contains("gpt-oss") {
        return false;
    }
    if provider == "groq" {
        // GPT-OSS and other Qwen 3 variants expose reasoning but not the
        // no-thinking mode used here. Known ordinary Groq models need no
        // reasoning field at all.
        return !model.starts_with("qwen/qwen3") || is_groq_qwen_no_reasoning_model(&model);
    }
    if provider == "openai" {
        // OpenAI's o-series and GPT-5 before 5.1 do not support none. GPT-5.1
        // does; all ordinary GPT-4.x chat models are non-reasoning.
        if model.starts_with('o')
            || (model.starts_with("gpt-5") && !is_openai_gpt_51_no_reasoning_model(&model))
        {
            return false;
        }
    }
    true
}

/// Whether a selected cleanup backend satisfies the dictation reasoning
/// policy. Google Gemini 3.x is the deliberate exception to "off": it is
/// accepted only when the request can carry the supported minimum level.
/// Provider-chain selection uses this so unsupported models are skipped before
/// a request is made.
pub fn model_supports_cleanup_reasoning_policy(provider: ProviderId, model: &str) -> bool {
    match provider {
        ProviderId::Groq => openai_compat_model_supports_no_reasoning("Groq", model),
        ProviderId::Google => super::prompts::gemini_generation_reasoning_supported(model),
        ProviderId::OpenAI => openai_compat_model_supports_no_reasoning("OpenAI", model),
        ProviderId::Local => true,
        // AssemblyAI is transcription-only and has no cleanup endpoint.
        ProviderId::AssemblyAi => false,
    }
}

async fn google_cleanup(
    text: &str,
    api_key: &str,
    prompt: &str,
    model: &str,
    max_output_tokens: u32,
    alternate_transcript: Option<&str>,
    gen: u64,
) -> Result<String> {
    use super::gemini_types::GeminiResp;

    super::prompts::ensure_gemini_generation_model(model)?;

    log::debug!(
        "cleanup: google request gen={} input_chars={} prompt_chars={} max_output_tokens={}",
        gen,
        text.chars().count(),
        prompt.chars().count(),
        max_output_tokens
    );
    let req = build_google_cleanup_request_with_alternate(
        text,
        prompt,
        model,
        max_output_tokens,
        alternate_transcript,
    );

    super::validate_model_for_url(model)?;
    // Pass the key in the `x-goog-api-key` header, never in the URL query string:
    // URLs leak into error messages, proxies, and logs, and the bare `?key=` form
    // would expose the secret (Verenu's top rule is "API keys never hit logs").
    let url =
        format!("https://generativelanguage.googleapis.com/v1beta/models/{model}:generateContent");

    let request_started = std::time::Instant::now();
    let resp = super::client::get()
        .post(&url)
        .header("x-goog-api-key", api_key)
        .json(&req)
        .send()
        .await?;
    let status = resp.status();
    let request_id = super::response_request_id(&resp);
    log::debug!(
        "cleanup: google response gen={} status={} request_id={} latency_ms={}",
        gen,
        status,
        request_id,
        request_started.elapsed().as_millis()
    );

    let resp = match super::ensure_provider_success(resp, "Google", Some(("Google", model))).await {
        Ok(resp) => resp,
        Err(super::ProviderHttpError::Quota(e)) => return Err(e),
        Err(super::ProviderHttpError::Auth {
            error,
            status,
            request_id,
            preview,
        }) => {
            log::warn!(
                "cleanup: google unauthorized gen={} model={} status={} request_id={} body_preview=\"{}\"",
                gen,
                model,
                status,
                request_id,
                preview
            );
            return Err(error);
        }
        Err(super::ProviderHttpError::NonSuccess {
            source,
            status,
            request_id,
            preview,
        }) => {
            log::warn!(
                "cleanup: google non_success gen={} model={} status={} request_id={} body_preview=\"{}\"",
                gen,
                model,
                status,
                request_id,
                preview
            );
            return Err(anyhow::Error::new(source).context(format!(
                "Google Cleanup API error status={} request_id={} body_preview={}",
                status, request_id, preview
            )));
        }
    };

    let data: GeminiResp = resp.json().await?;
    if let Some(candidate) = data.candidates.as_ref().and_then(|c| c.first()) {
        if let Some(reason) = candidate.finish_reason.as_deref() {
            if reason != "STOP" && reason != "MAX_TOKENS" {
                anyhow::bail!("Gemini cleanup finish_reason: {reason}");
            }
            if reason == "MAX_TOKENS" {
                anyhow::bail!(
                    "Gemini cleanup output reached max_output_tokens={max_output_tokens}"
                );
            }
        }
    }
    let output = data
        .candidates
        .unwrap_or_default()
        .into_iter()
        .next()
        .and_then(|c| c.content)
        .and_then(|c| c.parts.into_iter().next())
        .and_then(|p| p.text)
        .map(|t| t.trim().to_owned())
        .ok_or_else(|| anyhow::anyhow!("No candidates or parts in Google response"))?;
    log::debug!(
        "cleanup: google parsed gen={} chars={}",
        gen,
        output.chars().count()
    );
    Ok(output)
}

#[cfg(test)]
fn build_openai_compat_request(text: &str, model: &str, prompt: &str, max_tokens: u32) -> ChatReq {
    build_openai_compat_request_with_alternate(text, model, prompt, max_tokens, None, "OpenAI")
}

fn build_openai_compat_request_with_alternate(
    text: &str,
    model: &str,
    prompt: &str,
    max_tokens: u32,
    alternate_transcript: Option<&str>,
    provider_label: &str,
) -> ChatReq {
    let user_content = format_transcript_input(text, alternate_transcript);
    let lower_model = model.to_ascii_lowercase();
    let is_no_thinking = (provider_label.eq_ignore_ascii_case("Groq")
        && is_groq_qwen_no_reasoning_model(&lower_model))
        || (provider_label.eq_ignore_ascii_case("OpenAI")
            && is_openai_gpt_51_no_reasoning_model(&lower_model));
    ChatReq {
        model: model.to_owned(),
        messages: vec![
            Msg {
                role: "system".into(),
                content: prompt.to_owned(),
            },
            Msg {
                role: "user".into(),
                content: user_content,
            },
        ],
        max_tokens,
        temperature: 0.0,
        reasoning_effort: is_no_thinking.then_some("none"),
    }
}

#[cfg(test)]
fn build_google_cleanup_request(
    text: &str,
    prompt: &str,
    model: &str,
    max_output_tokens: u32,
) -> GeminiGenerateReq {
    build_google_cleanup_request_with_alternate(text, prompt, model, max_output_tokens, None)
}

fn build_google_cleanup_request_with_alternate(
    text: &str,
    prompt: &str,
    model: &str,
    max_output_tokens: u32,
    alternate_transcript: Option<&str>,
) -> GeminiGenerateReq {
    let input = format_transcript_input(text, alternate_transcript);
    GeminiGenerateReq {
        contents: vec![GeminiReqContent {
            parts: vec![GeminiReqPart {
                inline_data: None,
                text: Some(input),
            }],
        }],
        system_instruction: GeminiReqContent {
            parts: vec![GeminiReqPart {
                inline_data: None,
                text: Some(prompt.to_owned()),
            }],
        },
        generation_config: gemini_generation_config(model, max_output_tokens),
    }
}

/// Escapes dictation text for embedding inside the `<raw_dictation>` XML tag
/// of prompts. Shared with the pipeline's local-cleanup path.
pub fn escape_transcript_xml(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Formats transcript candidates identically for every cleanup provider. The
/// tags are labels for data; the system prompt defines how candidates are
/// reconciled.
pub fn format_transcript_input(primary: &str, alternate: Option<&str>) -> String {
    match alternate {
        Some(alternate) => format!(
            "<primary_transcript>\n{}\n</primary_transcript>\n<alternate_transcript>\n{}\n</alternate_transcript>",
            escape_transcript_xml(primary),
            escape_transcript_xml(alternate),
        ),
        None => format!(
            "<raw_dictation>\n{}\n</raw_dictation>",
            escape_transcript_xml(primary)
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        build_google_cleanup_request, build_google_cleanup_request_with_alternate,
        build_openai_compat_request, build_openai_compat_request_with_alternate,
        ensure_openai_compat_reasoning_policy, model_supports_cleanup_reasoning_policy,
    };

    #[tokio::test]
    async fn off_without_alternate_bypasses_the_cleanup_provider() {
        let result = super::cleanup(
            "um keep this raw",
            crate::api::ProviderId::Groq,
            "",
            "openai/gpt-oss-20b",
            "formal",
            "none",
            "",
            None,
            None,
            0,
        )
        .await
        .unwrap();
        assert_eq!(result, "um keep this raw");
    }

    #[test]
    fn openai_compat_request_uses_dynamic_max_tokens() {
        let body = build_openai_compat_request("hello", "gpt-4o-mini", "prompt", 128);
        let json = serde_json::to_value(&body).unwrap();
        assert_eq!(json["max_tokens"], 128);
        assert_eq!(json["temperature"], 0.0);
        assert!(json.get("reasoning_effort").is_none());
        assert_eq!(json["messages"][0]["content"], "prompt");
    }

    #[test]
    fn gpt_oss_cleanup_is_rejected_instead_of_using_hidden_reasoning() {
        let body = build_openai_compat_request("hello", "openai/gpt-oss-20b", "prompt", 128);
        let json = serde_json::to_value(&body).unwrap();
        assert!(json.get("reasoning_effort").is_none());
        assert!(json.get("include_reasoning").is_none());
        assert!(ensure_openai_compat_reasoning_policy("Groq", "openai/gpt-oss-20b").is_err());
        assert!(!model_supports_cleanup_reasoning_policy(
            crate::api::ProviderId::Groq,
            "openai/gpt-oss-20b"
        ));
    }

    #[test]
    fn qwen_cleanup_uses_non_thinking_mode() {
        let body = build_openai_compat_request_with_alternate(
            "hello",
            "qwen/qwen3.6-27b",
            "prompt",
            128,
            None,
            "Groq",
        );
        let json = serde_json::to_value(&body).unwrap();
        assert_eq!(json["reasoning_effort"], "none");
        assert!(json.get("include_reasoning").is_none());
    }

    #[test]
    fn unsupported_reasoning_families_are_not_advertised_for_cleanup() {
        assert!(!model_supports_cleanup_reasoning_policy(
            crate::api::ProviderId::Groq,
            "qwen/qwen3-32b"
        ));
        assert!(!model_supports_cleanup_reasoning_policy(
            crate::api::ProviderId::OpenAI,
            "o3-mini"
        ));
        assert!(!model_supports_cleanup_reasoning_policy(
            crate::api::ProviderId::OpenAI,
            "gpt-5"
        ));
        assert!(!model_supports_cleanup_reasoning_policy(
            crate::api::ProviderId::OpenAI,
            "gpt-oss-20b"
        ));
        assert!(model_supports_cleanup_reasoning_policy(
            crate::api::ProviderId::OpenAI,
            "gpt-5.1"
        ));
        assert!(!model_supports_cleanup_reasoning_policy(
            crate::api::ProviderId::AssemblyAi,
            "universal-2"
        ));
    }

    #[test]
    fn openai_gpt_51_uses_its_no_reasoning_mode() {
        let body = build_openai_compat_request_with_alternate(
            "hello", "gpt-5.1", "prompt", 128, None, "OpenAI",
        );
        let json = serde_json::to_value(body).unwrap();
        assert_eq!(json["reasoning_effort"], "none");
    }

    #[test]
    fn openai_compat_request_escapes_raw_transcript_xml() {
        let body = build_openai_compat_request("<tag> & text", "gpt-4o-mini", "prompt", 128);
        let json = serde_json::to_value(&body).unwrap();
        assert_eq!(
            json["messages"][1]["content"],
            "<raw_dictation>\n&lt;tag&gt; &amp; text\n</raw_dictation>"
        );
    }

    #[test]
    fn google_cleanup_request_includes_gemini_config() {
        let body = build_google_cleanup_request("hello", "prompt", "gemini-2.5-flash-lite", 256);
        let json = serde_json::to_value(&body).unwrap();
        assert_eq!(
            json["generationConfig"]["thinkingConfig"]["thinkingBudget"],
            0
        );
        assert_eq!(json["generationConfig"]["maxOutputTokens"], 256);
        assert_eq!(json["generationConfig"]["temperature"], 0.0);
    }

    #[test]
    fn gemini_3_5_cleanup_request_uses_minimal_thinking_level() {
        let body = build_google_cleanup_request("hello", "prompt", "gemini-3.5-flash-lite", 256);
        let json = serde_json::to_value(&body).unwrap();
        assert_eq!(
            json["generationConfig"]["thinkingConfig"]["thinkingLevel"],
            "minimal"
        );
        assert!(json["generationConfig"]["thinkingConfig"]
            .get("thinkingBudget")
            .is_none());
        assert!(json["generationConfig"].get("temperature").is_none());
    }

    #[test]
    fn google_cleanup_request_escapes_raw_transcript_xml() {
        let body = build_google_cleanup_request("<tag> & text", "prompt", "gemini-2.5-flash", 128);
        let json = serde_json::to_value(&body).unwrap();
        assert_eq!(
            json["contents"][0]["parts"][0]["text"],
            "<raw_dictation>\n&lt;tag&gt; &amp; text\n</raw_dictation>"
        );
    }

    #[test]
    fn enhanced_cleanup_requests_keep_candidates_in_user_data() {
        let body = build_openai_compat_request_with_alternate(
            "the issue was clawed",
            "gpt-4o-mini",
            "reconcile",
            128,
            Some("the issue was called"),
            "Groq",
        );
        let json = serde_json::to_value(body).unwrap();
        assert_eq!(json["messages"][0]["content"], "reconcile");
        let user = json["messages"][1]["content"].as_str().unwrap();
        assert!(user.contains("<primary_transcript>"));
        assert!(user.contains("<alternate_transcript>"));

        let google = build_google_cleanup_request_with_alternate(
            "primary",
            "reconcile",
            "gemini-2.5-flash",
            128,
            Some("alternate"),
        );
        let google_json = serde_json::to_value(google).unwrap();
        let input = google_json["contents"][0]["parts"][0]["text"]
            .as_str()
            .unwrap();
        assert!(input.contains("<primary_transcript>"));
        assert!(input.contains("<alternate_transcript>"));
    }
}
