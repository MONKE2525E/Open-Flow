use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use super::prompts::get_system_prompt_with_extras;

#[derive(Clone, Debug)]
pub enum CleanupProvider {
    Groq,
    OpenAI,
    Google,
}

pub async fn cleanup(
    text: &str,
    provider: CleanupProvider,
    api_key: &str,
    profile: &str,
    intensity: &str,
    snippet_instructions: &str,
    app_context: Option<&str>,
) -> Result<String> {
    let prompt =
        get_system_prompt_with_extras(profile, intensity, snippet_instructions, app_context, text);
    log::debug!(
        "cleanup: start provider={:?} profile={} intensity={} input_chars={} prompt_chars={} snippet_rule_lines={} app_context={}",
        provider,
        profile,
        intensity,
        text.chars().count(),
        prompt.chars().count(),
        snippet_instructions.lines().filter(|l| !l.trim().is_empty()).count(),
        app_context.is_some()
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
                "llama-3.3-70b-versatile",
                &prompt,
            )
            .await
        }
        CleanupProvider::OpenAI => {
            openai_compat(
                text,
                api_key,
                "https://api.openai.com/v1/chat/completions",
                "gpt-4o-mini",
                &prompt,
            )
            .await
        }
        CleanupProvider::Google => google_cleanup(text, api_key, &prompt).await,
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
    model: &str,
    prompt: &str,
) -> Result<String> {
    let body = ChatReq {
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
        max_tokens: 4096,
        temperature: 0.0,
    };

    log::debug!(
        "cleanup: openai_compat request model={} url={} input_chars={} prompt_chars={}",
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
        .unwrap_or("-");
    log::debug!(
        "cleanup: openai_compat response status={} request_id={} latency_ms={}",
        resp.status(),
        request_id,
        request_started.elapsed().as_millis()
    );

    if resp.status().as_u16() == 429 {
        return Err(crate::api::quota_bail(model));
    }
    let resp = resp.error_for_status().context("Cleanup API error")?;

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

async fn google_cleanup(text: &str, api_key: &str, prompt: &str) -> Result<String> {
    use super::gemini_types::GeminiResp;

    #[derive(Serialize)]
    struct Req {
        contents: Vec<GContent>,
        #[serde(rename = "systemInstruction")]
        system_instruction: GContent,
        #[serde(rename = "generationConfig")]
        generation_config: GenerationConfig,
    }

    #[derive(Serialize)]
    struct GenerationConfig {
        #[serde(rename = "thinkingConfig")]
        thinking_config: ThinkingConfig,
    }

    #[derive(Serialize)]
    struct ThinkingConfig {
        #[serde(rename = "thinkingBudget")]
        thinking_budget: u32,
    }

    #[derive(Serialize)]
    struct GContent {
        parts: Vec<GPart>,
    }

    #[derive(Serialize)]
    struct GPart {
        text: String,
    }

    log::debug!(
        "cleanup: google request input_chars={} prompt_chars={}",
        text.chars().count(),
        prompt.chars().count()
    );
    let req = Req {
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
        generation_config: GenerationConfig {
            thinking_config: ThinkingConfig { thinking_budget: 0 },
        },
    };

    let url = format!(
        "https://generativelanguage.googleapis.com/v1beta/models/gemini-3.5-flash:generateContent?key={api_key}"
    );

    let request_started = std::time::Instant::now();
    let resp = super::client::get().post(&url).json(&req).send().await?;
    let request_id = resp
        .headers()
        .get("x-request-id")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("-");
    log::debug!(
        "cleanup: google response status={} request_id={} latency_ms={}",
        resp.status(),
        request_id,
        request_started.elapsed().as_millis()
    );

    if resp.status().as_u16() == 429 {
        return Err(crate::api::quota_bail("Google"));
    }
    let resp = resp
        .error_for_status()
        .context("Google Cleanup API error")?;

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
