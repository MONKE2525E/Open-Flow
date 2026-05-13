use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

pub use super::prompts::get_system_prompt;

#[derive(Clone, Debug)]
pub enum CleanupProvider {
    Groq,
    OpenAI,
    Google,
}

pub async fn cleanup(text: &str, provider: CleanupProvider, api_key: &str, profile: &str, intensity: &str) -> Result<String> {
    let prompt = get_system_prompt(profile, intensity);
    match provider {
        CleanupProvider::Groq => {
            openai_compat(
                text,
                api_key,
                "https://api.groq.com/openai/v1/chat/completions",
                "llama-3.3-70b-versatile",
                &prompt
            )
            .await
        }
        CleanupProvider::OpenAI => {
            openai_compat(
                text,
                api_key,
                "https://api.openai.com/v1/chat/completions",
                "gpt-4o-mini",
                &prompt
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

async fn openai_compat(text: &str, api_key: &str, url: &str, model: &str, prompt: &str) -> Result<String> {
    let body = ChatReq {
        model: model.to_owned(),
        messages: vec![
            Msg { role: "system".into(), content: prompt.to_owned() },
            Msg { role: "user".into(),   content: format!("<transcription>\n{}\n</transcription>", text) },
        ],
        max_tokens: 4096,
        temperature: 0.2,
    };

    let resp = super::client::get()
        .post(url)
        .bearer_auth(api_key)
        .json(&body)
        .send()
        .await?
        .error_for_status()
        .context("Cleanup API error")?;

    let data: ChatResp = resp.json().await?;
    data.choices.first()
        .map(|c| c.message.content.trim().to_owned())
        .ok_or_else(|| anyhow::anyhow!("No choices in OpenAI response"))
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

    let req = Req {
        contents: vec![GContent { parts: vec![GPart { text: format!("<transcription>\n{}\n</transcription>", text) }] }],
        system_instruction: GContent { parts: vec![GPart { text: prompt.to_owned() }] },
        generation_config: GenerationConfig {
            thinking_config: ThinkingConfig { thinking_budget: 0 },
        },
    };

    let url = format!("https://generativelanguage.googleapis.com/v1beta/models/gemini-2.5-flash:generateContent?key={}", api_key);

    let resp = super::client::get()
        .post(&url)
        .json(&req)
        .send()
        .await?
        .error_for_status()
        .context("Google Cleanup API error")?;

    let data: GeminiResp = resp.json().await?;
    data.candidates
        .unwrap_or_default()
        .into_iter()
        .next()
        .and_then(|c| c.content)
        .and_then(|c| c.parts.into_iter().next())
        .and_then(|p| p.text)
        .map(|t| t.trim().to_owned())
        .ok_or_else(|| anyhow::anyhow!("No candidates or parts in Google response"))
}
