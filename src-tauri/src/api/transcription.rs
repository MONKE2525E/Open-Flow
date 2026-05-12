use anyhow::{Context, Result};
use reqwest::multipart;
use serde::Deserialize;

#[derive(Clone, Debug)]
pub enum Provider {
    Groq,
    OpenAI,
    Google,
}

#[derive(Deserialize)]
struct WhisperResponse {
    text: String,
}

/// Single Gemini call that both transcribes the audio and applies the cleanup
/// profile. Used when Google is selected for both transcription and cleanup so
/// we only pay one API round trip instead of two.
pub async fn transcribe_and_cleanup_gemini(
    wav: Vec<u8>,
    api_key: &str,
    profile_prompt: &str,
) -> Result<String> {
    // Splice the profile rules into a single instruction so one model call does both jobs.
    let instruction = format!(
        "Transcribe this audio, then immediately apply the following formatting rules to your \
         transcription output.\n\n{profile_prompt}\n\nReturn ONLY the final cleaned text — \
         no commentary, no quotes, no explanation."
    );
    transcribe_gemini_with_prompt(wav, api_key, &instruction, true).await
}

pub async fn transcribe(wav: Vec<u8>, provider: Provider, api_key: &str) -> Result<String> {
    match provider {
        Provider::Groq => transcribe_whisper(
            wav,
            api_key,
            "https://api.groq.com/openai/v1/audio/transcriptions",
            "whisper-large-v3-turbo",
        )
        .await,

        Provider::OpenAI => transcribe_whisper(
            wav,
            api_key,
            "https://api.openai.com/v1/audio/transcriptions",
            "gpt-4o-transcribe",
        )
        .await,

        // Use Gemini multimodal audio — same key as Gemini cleanup (Google AI Studio).
        // Google Cloud Speech-to-Text requires a separate Cloud project key; Gemini does not.
        Provider::Google => transcribe_gemini(wav, api_key).await,
    }
}

async fn transcribe_whisper(
    wav: Vec<u8>,
    api_key: &str,
    url: &str,
    model: &str,
) -> Result<String> {
    let part = multipart::Part::bytes(wav)
        .file_name("audio.wav")
        .mime_str("audio/wav")?;
    let form = multipart::Form::new()
        .part("file", part)
        .text("model", model.to_owned())
        .text("response_format", "json");

    let resp = super::client::get()
        .post(url)
        .bearer_auth(api_key)
        .multipart(form)
        .send()
        .await?
        .error_for_status()
        .context("Transcription API error")?;

    let body: WhisperResponse = resp.json().await?;
    Ok(body.text.trim().to_owned())
}

async fn transcribe_gemini(wav: Vec<u8>, api_key: &str) -> Result<String> {
    let prompt = "Transcribe this audio exactly as spoken. \
                  Return only the spoken words — no commentary, no formatting, no explanation.";
    transcribe_gemini_with_prompt(wav, api_key, prompt, false).await
}

async fn transcribe_gemini_with_prompt(wav: Vec<u8>, api_key: &str, prompt: &str, disable_thinking: bool) -> Result<String> {
    #[derive(Deserialize, Debug)]
    struct Resp {
        candidates: Option<Vec<Candidate>>,
        #[serde(rename = "promptFeedback")]
        prompt_feedback: Option<serde_json::Value>,
    }

    #[derive(Deserialize, Debug)]
    struct Candidate {
        content: Option<RespContent>,
        #[serde(rename = "finishReason")]
        finish_reason: Option<String>,
    }

    #[derive(Deserialize, Debug)]
    struct RespContent { parts: Vec<RespPart> }

    #[derive(Deserialize, Debug)]
    struct RespPart { text: Option<String> }

    let encoded = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &wav);

    let mut body = serde_json::json!({
        "contents": [{
            "parts": [
                { "inlineData": { "mimeType": "audio/wav", "data": encoded } },
                { "text": prompt }
            ]
        }]
    });
    if disable_thinking {
        body["generationConfig"] = serde_json::json!({
            "thinkingConfig": { "thinkingBudget": 0 }
        });
    }

    let url = format!(
        "https://generativelanguage.googleapis.com/v1beta/models/gemini-2.5-flash:generateContent?key={api_key}"
    );

    let resp = super::client::get()
        .post(&url)
        .json(&body)
        .send()
        .await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        anyhow::bail!("Gemini {status}: {body}");
    }

    let raw_body = resp.text().await?;
    log::debug!("Gemini transcription response: {raw_body}");

    let data: Resp = serde_json::from_str(&raw_body)
        .with_context(|| format!("Gemini parse error. Response: {raw_body}"))?;

    // Surface blocked/filtered responses as an explicit error
    if let Some(fb) = &data.prompt_feedback {
        if let Some(reason) = fb.get("blockReason").and_then(|v| v.as_str()) {
            anyhow::bail!("Gemini blocked: {reason}");
        }
    }

    let candidates = data.candidates.unwrap_or_default();
    if candidates.is_empty() {
        anyhow::bail!("Gemini returned no candidates (check API key or quota)");
    }

    let candidate = &candidates[0];
    if let Some(reason) = &candidate.finish_reason {
        if reason != "STOP" && reason != "MAX_TOKENS" {
            anyhow::bail!("Gemini finish_reason: {reason}");
        }
    }

    let text = candidate
        .content
        .as_ref()
        .and_then(|c| c.parts.iter().find_map(|p| p.text.as_deref()))
        .unwrap_or("")
        .trim()
        .to_owned();

    Ok(text)
}
