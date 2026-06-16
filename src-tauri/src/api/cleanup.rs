use anyhow::Result;
use serde::{Deserialize, Serialize};

use super::prompts::{
    cleanup_max_output_tokens, gemini_generation_config, get_cleanup_prompt_with_extras,
};

#[derive(Clone, Debug)]
pub enum CleanupProvider {
    Groq,
    OpenAI,
    Google,
}

#[allow(clippy::too_many_arguments)]
pub async fn cleanup(
    text: &str,
    provider: CleanupProvider,
    api_key: &str,
    model: &str,
    profile: &str,
    intensity: &str,
    snippet_instructions: &str,
    app_context: Option<&str>,
    custom_template: Option<&str>,
) -> Result<String> {
    let provider_id = provider_id(&provider);
    #[cfg(any(test, debug_assertions))]
    if let Some(result) = crate::testing::resolve_provider_fixture("cleanup", provider_id, model) {
        return result;
    }

    let prompt = get_cleanup_prompt_with_extras(
        provider_id,
        model,
        profile,
        intensity,
        snippet_instructions,
        app_context,
        text,
        custom_template,
    );
    let max_output_tokens = cleanup_max_output_tokens(intensity, text);
    log::debug!(
        "cleanup: start provider={:?} model={} profile={} intensity={} input_chars={} prompt_chars={} max_output_tokens={} snippet_rule_lines={} app_context={} custom_template={}",
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
    if crate::system::logger::is_verbose() {
        log::debug!("cleanup: input_full=\"{}\"", text);
        log::debug!("cleanup: prompt_full=\"{}\"", prompt);
        if !snippet_instructions.is_empty() {
            log::debug!("cleanup: snippet_rules_full=\"{}\"", snippet_instructions);
        }
        if let Some(ctx) = app_context {
            log::debug!("cleanup: app_context_full=\"{}\"", ctx);
        }
    }
    match provider {
        CleanupProvider::Groq => {
            openai_compat(
                text,
                api_key,
                "https://api.groq.com/openai/v1/chat/completions",
                "Groq",
                model,
                &prompt,
                max_output_tokens,
            )
            .await
        }
        CleanupProvider::OpenAI => {
            openai_compat(
                text,
                api_key,
                "https://api.openai.com/v1/chat/completions",
                "OpenAI",
                model,
                &prompt,
                max_output_tokens,
            )
            .await
        }
        CleanupProvider::Google => {
            google_cleanup(text, api_key, &prompt, model, max_output_tokens).await
        }
    }
}

fn provider_id(provider: &CleanupProvider) -> &'static str {
    match provider {
        CleanupProvider::Groq => "groq",
        CleanupProvider::OpenAI => "openai",
        CleanupProvider::Google => "google",
    }
}

pub fn provider_from_str(s: &str) -> CleanupProvider {
    match s {
        "openai" => CleanupProvider::OpenAI,
        "google" => CleanupProvider::Google,
        _ => CleanupProvider::Groq,
    }
}

#[derive(Serialize)]
struct ChatReq {
    model: String,
    messages: Vec<Msg>,
    max_tokens: u32,
    temperature: f32,
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

async fn openai_compat(
    text: &str,
    api_key: &str,
    url: &str,
    provider_label: &str,
    model: &str,
    prompt: &str,
    max_tokens: u32,
) -> Result<String> {
    let body = build_openai_compat_request(text, model, prompt, max_tokens);

    log::debug!(
        "cleanup: openai_compat request provider={} model={} url={} input_chars={} prompt_chars={}",
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
    let request_id = resp
        .headers()
        .get("x-request-id")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("-")
        .to_string();
    let status = resp.status();
    log::debug!(
        "cleanup: openai_compat response provider={} status={} request_id={} latency_ms={}",
        provider_label,
        status,
        request_id,
        request_started.elapsed().as_millis()
    );

    if status.as_u16() == 429 {
        return Err(crate::api::quota_bail(model));
    }

    if status.as_u16() == 401 {
        let body = resp.text().await.unwrap_or_default();
        let preview = crate::api::sanitize_error_body_preview(&body);
        let category = crate::api::classify_unauthorized_body(&body);
        log::warn!(
            "cleanup: openai_compat unauthorized provider={} model={} status={} request_id={} body_preview=\"{}\"",
            provider_label,
            model,
            status,
            request_id,
            preview
        );
        return Err(crate::api::auth_401_error(
            provider_label,
            model,
            &request_id,
            category,
        ));
    }

    if let Err(e) = resp.error_for_status_ref() {
        let body = resp.text().await.unwrap_or_default();
        let preview = crate::api::sanitize_error_body_preview(&body);
        log::warn!(
            "cleanup: openai_compat non_success provider={} model={} status={} request_id={} body_preview=\"{}\"",
            provider_label,
            model,
            status,
            request_id,
            preview
        );
        return Err(anyhow::Error::new(e).context(format!(
            "Cleanup API error provider={} model={} status={} request_id={} body_preview={}",
            provider_label, model, status, request_id, preview
        )));
    }

    let data: ChatResp = resp.json().await?;
    let output = data
        .choices
        .first()
        .map(|c| c.message.content.trim().to_owned())
        .ok_or_else(|| anyhow::anyhow!("No choices in OpenAI response"))?;
    log::debug!(
        "cleanup: openai_compat parsed chars={}",
        output.chars().count()
    );
    if crate::system::logger::is_verbose() {
        log::debug!("cleanup: openai_compat output_full=\"{}\"", output);
    }
    Ok(output)
}

#[derive(Serialize)]
struct GoogleCleanupReq {
    contents: Vec<GContent>,
    #[serde(rename = "systemInstruction")]
    system_instruction: GContent,
    #[serde(rename = "generationConfig")]
    generation_config: super::gemini_types::GeminiGenConfig,
}

#[derive(Serialize)]
struct GContent {
    parts: Vec<GPart>,
}

#[derive(Serialize)]
struct GPart {
    text: String,
}

async fn google_cleanup(
    text: &str,
    api_key: &str,
    prompt: &str,
    model: &str,
    max_output_tokens: u32,
) -> Result<String> {
    use super::gemini_types::GeminiResp;

    log::debug!(
        "cleanup: google request input_chars={} prompt_chars={} max_output_tokens={}",
        text.chars().count(),
        prompt.chars().count(),
        max_output_tokens
    );
    let req = build_google_cleanup_request(text, prompt, model, max_output_tokens);

    super::validate_model_for_url(model)?;
    let url =
        format!("https://generativelanguage.googleapis.com/v1beta/models/{model}:generateContent?key={api_key}");

    let request_started = std::time::Instant::now();
    let resp = super::client::get().post(&url).json(&req).send().await?;
    let request_id = resp
        .headers()
        .get("x-request-id")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("-")
        .to_string();
    let status = resp.status();
    log::debug!(
        "cleanup: google response status={} request_id={} latency_ms={}",
        status,
        request_id,
        request_started.elapsed().as_millis()
    );

    if status.as_u16() == 429 {
        return Err(crate::api::quota_bail("Google"));
    }

    if let Err(e) = resp.error_for_status_ref() {
        let body = resp.text().await.unwrap_or_default();
        let preview = crate::api::sanitize_error_body_preview(&body);
        log::warn!(
            "cleanup: google non_success model={} status={} request_id={} body_preview=\"{}\"",
            model,
            status,
            request_id,
            preview
        );
        return Err(anyhow::Error::new(e).context(format!(
            "Google Cleanup API error status={} request_id={} body_preview={}",
            status, request_id, preview
        )));
    }

    let data: GeminiResp = resp.json().await?;
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
    log::debug!("cleanup: google parsed chars={}", output.chars().count());
    if crate::system::logger::is_verbose() {
        log::debug!("cleanup: google output_full=\"{}\"", output);
    }
    Ok(output)
}

fn build_openai_compat_request(text: &str, model: &str, prompt: &str, max_tokens: u32) -> ChatReq {
    ChatReq {
        model: model.to_owned(),
        messages: vec![
            Msg {
                role: "system".into(),
                content: prompt.to_owned(),
            },
            Msg {
                role: "user".into(),
                content: format!("<raw_dictation>\n{text}\n</raw_dictation>"),
            },
        ],
        max_tokens,
        temperature: 0.0,
    }
}

fn build_google_cleanup_request(
    text: &str,
    prompt: &str,
    model: &str,
    max_output_tokens: u32,
) -> GoogleCleanupReq {
    GoogleCleanupReq {
        contents: vec![GContent {
            parts: vec![GPart {
                text: format!("<raw_dictation>\n{text}\n</raw_dictation>"),
            }],
        }],
        system_instruction: GContent {
            parts: vec![GPart {
                text: prompt.to_owned(),
            }],
        },
        generation_config: gemini_generation_config(model, max_output_tokens),
    }
}

#[cfg(test)]
mod tests {
    use super::{build_google_cleanup_request, build_openai_compat_request};

    #[test]
    fn openai_compat_request_uses_dynamic_max_tokens() {
        let body = build_openai_compat_request("hello", "gpt-4o-mini", "prompt", 128);
        let json = serde_json::to_value(&body).unwrap();
        assert_eq!(json["max_tokens"], 128);
        assert_eq!(json["temperature"], 0.0);
        assert_eq!(json["messages"][0]["content"], "prompt");
    }

    #[test]
    fn google_cleanup_request_includes_gemini_config() {
        let body = build_google_cleanup_request("hello", "prompt", "gemini-2.5-flash", 256);
        let json = serde_json::to_value(&body).unwrap();
        assert_eq!(
            json["generationConfig"]["thinkingConfig"]["thinkingBudget"],
            0
        );
        assert_eq!(json["generationConfig"]["maxOutputTokens"], 256);
        assert_eq!(json["generationConfig"]["temperature"], 0.0);
    }
}
