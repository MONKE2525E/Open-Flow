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
    let provider_id = provider.as_str();
    #[cfg(any(test, debug_assertions))]
    if let Some(result) = crate::testing::resolve_provider_fixture("cleanup", provider_id, model) {
        return result;
    }

    let prompt = super::prompts::get_cleanup_prompt_with_alternate(
        provider_id,
        model,
        profile,
        intensity,
        snippet_instructions,
        app_context,
        text,
        custom_template,
        alternate_transcript,
    );
    let max_output_tokens = cleanup_max_output_tokens(intensity, text);
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
        snippet_instructions.lines().filter(|l| !l.trim().is_empty()).count(),
        app_context.is_some(),
        custom_template.is_some()
    );
    if crate::system::logger::is_verbose() && !snippet_instructions.is_empty() {
        log::debug!(
            "cleanup: snippet_rules_meta lines={} chars={}",
            snippet_instructions
                .lines()
                .filter(|line| !line.trim().is_empty())
                .count(),
            snippet_instructions.chars().count()
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

/// Structured request transport shared by repair diagnosis. It deliberately
/// bypasses cleanup prompt construction and the cleanup-enabled setting while
/// reusing the existing authenticated provider clients and redacted errors.
pub async fn structured_request(
    text: &str,
    provider: ProviderId,
    api_key: &str,
    model: &str,
    prompt: &str,
    max_output_tokens: u32,
    gen: u64,
) -> Result<String> {
    let _ = gen;
    let request = async {
        if let Some(url) = provider.cleanup_url() {
            openai_compat(text, api_key, url, provider.label(), model, prompt, max_output_tokens, None, gen).await
        } else {
            google_cleanup(text, api_key, prompt, model, max_output_tokens, None, gen).await
        }
    };
    tokio::time::timeout(std::time::Duration::from_secs(CLEANUP_REQUEST_TIMEOUT_SECS), request)
        .await
        .map_err(|_| anyhow::anyhow!("Repair provider request timed out"))?
}

#[derive(Serialize)]
struct ChatReq {
    model: String,
    messages: Vec<Msg>,
    max_tokens: u32,
    temperature: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    reasoning_effort: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    include_reasoning: Option<bool>,
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
    let body = build_openai_compat_request_with_alternate(
        text,
        model,
        prompt,
        max_tokens,
        alternate_transcript,
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
    build_openai_compat_request_with_alternate(text, model, prompt, max_tokens, None)
}

fn build_openai_compat_request_with_alternate(
    text: &str,
    model: &str,
    prompt: &str,
    max_tokens: u32,
    alternate_transcript: Option<&str>,
) -> ChatReq {
    let user_content = match alternate_transcript {
        Some(alternate) => format!(
            "<primary_transcript>\n{}\n</primary_transcript>\n<alternate_transcript>\n{}\n</alternate_transcript>",
            escape_transcript_xml(text),
            escape_transcript_xml(alternate),
        ),
        None => format!(
            "<raw_dictation>\n{}\n</raw_dictation>",
            escape_transcript_xml(text)
        ),
    };
    let is_gpt_oss = model.starts_with("openai/gpt-oss-");
    let is_qwen_3_6 = model == "qwen/qwen3.6-27b";
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
        reasoning_effort: if is_qwen_3_6 {
            Some("none")
        } else {
            is_gpt_oss.then_some("low")
        },
        include_reasoning: is_gpt_oss.then_some(false),
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
    let input = match alternate_transcript {
        Some(alternate) => format!(
            "<primary_transcript>\n{}\n</primary_transcript>\n<alternate_transcript>\n{}\n</alternate_transcript>",
            escape_transcript_xml(text),
            escape_transcript_xml(alternate),
        ),
        None => format!(
            "<raw_dictation>\n{}\n</raw_dictation>",
            escape_transcript_xml(text)
        ),
    };
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

#[cfg(test)]
mod tests {
    use super::{
        build_google_cleanup_request, build_google_cleanup_request_with_alternate,
        build_openai_compat_request, build_openai_compat_request_with_alternate,
    };

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
    fn gpt_oss_cleanup_disables_reasoning_output() {
        let body = build_openai_compat_request("hello", "openai/gpt-oss-20b", "prompt", 128);
        let json = serde_json::to_value(&body).unwrap();
        assert_eq!(json["reasoning_effort"], "low");
        assert_eq!(json["include_reasoning"], false);
    }

    #[test]
    fn qwen_cleanup_uses_non_thinking_mode() {
        let body = build_openai_compat_request("hello", "qwen/qwen3.6-27b", "prompt", 128);
        let json = serde_json::to_value(&body).unwrap();
        assert_eq!(json["reasoning_effort"], "none");
        assert!(json.get("include_reasoning").is_none());
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
        let body = build_google_cleanup_request("hello", "prompt", "gemini-3.7-flash", 256);
        let json = serde_json::to_value(&body).unwrap();
        assert_eq!(json["generationConfig"]["thinkingConfig"]["thinkingLevel"], "minimal");
        assert_eq!(json["generationConfig"]["maxOutputTokens"], 256);
        assert_eq!(json["generationConfig"]["temperature"], 0.0);
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

