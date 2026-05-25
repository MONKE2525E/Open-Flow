use anyhow::{Context, Result};
use bytes::Bytes;
use reqwest::multipart;

use super::gemini_types::GeminiResp;

#[derive(Clone, Debug)]
pub enum Provider {
    Groq,
    OpenAI,
    Google,
}

/// Single Gemini call that both transcribes the audio and applies the cleanup
/// profile. Used when Google is selected for both transcription and cleanup so
/// we only pay one API round trip instead of two.
#[expect(
    dead_code,
    reason = "Planned Google fast path, but pipeline still needs raw text for history"
)]
pub async fn transcribe_and_cleanup_gemini(
    wav: Bytes,
    api_key: &str,
    profile_prompt: &str,
) -> Result<String> {
    let instruction = format!(
        "Transcribe this audio, then immediately apply the following formatting rules to your \
         transcription output.\n\n{profile_prompt}\n\nReturn ONLY the final cleaned text, \
         no commentary, no quotes, no explanation."
    );
    transcribe_gemini_with_prompt(wav, api_key, &instruction, true, "gemini-3.5-flash").await
}

pub async fn transcribe(
    wav: Bytes,
    provider: Provider,
    api_key: &str,
    language: &str,
    model: &str,
) -> Result<String> {
    log::debug!(
        "transcription: start provider={:?} language={} wav_bytes={}",
        provider,
        language,
        wav.len()
    );
    match provider {
        Provider::Groq => {
            transcribe_whisper(
                wav,
                api_key,
                "https://api.groq.com/openai/v1/audio/transcriptions",
                "Groq",
                model,
                language,
            )
            .await
        }

        Provider::OpenAI => {
            transcribe_whisper(
                wav,
                api_key,
                "https://api.openai.com/v1/audio/transcriptions",
                "OpenAI",
                model,
                language,
            )
            .await
        }

        Provider::Google => transcribe_gemini(wav, api_key, language, model).await,
    }
}

async fn transcribe_whisper(
    wav: Bytes,
    api_key: &str,
    url: &str,
    provider_label: &str,
    model: &str,
    language: &str,
) -> Result<String> {
    #[derive(serde::Deserialize)]
    struct WhisperResponse {
        text: String,
    }

    log::debug!(
        "transcription: whisper request provider={} model={} url={} language={} wav_bytes={}",
        provider_label,
        model,
        url,
        language,
        wav.len()
    );
    let part =
        multipart::Part::stream_with_length(reqwest::Body::from(wav.clone()), wav.len() as u64)
            .file_name("audio.wav")
            .mime_str("audio/wav")?;
    let form = multipart::Form::new()
        .part("file", part)
        .text("model", model.to_owned())
        .text("response_format", "json")
        .text("language", language.to_owned());

    let request_started = std::time::Instant::now();
    let resp = super::client::get()
        .post(url)
        .bearer_auth(api_key)
        .multipart(form)
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
        "transcription: whisper response provider={} status={} request_id={} latency_ms={}",
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
            "transcription: whisper unauthorized provider={} model={} status={} request_id={} body_preview=\"{}\"",
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
            "transcription: whisper non_success provider={} model={} status={} request_id={} body_preview=\"{}\"",
            provider_label,
            model,
            status,
            request_id,
            preview
        );
        return Err(anyhow::Error::new(e).context(format!(
            "Transcription API error provider={} model={} status={} request_id={} body_preview={}",
            provider_label, model, status, request_id, preview
        )));
    }

    let body: WhisperResponse = resp.json().await?;
    log::debug!(
        "transcription: whisper parsed chars={}",
        body.text.trim().chars().count()
    );
    Ok(body.text.trim().to_owned())
}

async fn transcribe_gemini(
    wav: Bytes,
    api_key: &str,
    language: &str,
    model: &str,
) -> Result<String> {
    let language_label = crate::data::store::transcription_language_label(language);
    let prompt = format!(
        "Transcribe this audio exactly as spoken in {language_label}. \
         Return only the spoken words, no commentary, no formatting, no explanation."
    );
    transcribe_gemini_with_prompt(wav, api_key, &prompt, true, model).await
}

async fn transcribe_gemini_with_prompt(
    wav: Bytes,
    api_key: &str,
    prompt: &str,
    disable_thinking: bool,
    model: &str,
) -> Result<String> {
    log::debug!(
        "transcription: gemini request disable_thinking={} wav_bytes={} prompt_chars={}",
        disable_thinking,
        wav.len(),
        prompt.chars().count()
    );
    let encoded = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &wav);

    let body = super::gemini_types::GeminiTranscribeReq {
        contents: vec![super::gemini_types::GeminiReqContent {
            parts: vec![
                super::gemini_types::GeminiReqPart {
                    inline_data: Some(super::gemini_types::GeminiInlineData {
                        mime_type: "audio/wav".to_string(),
                        data: encoded,
                    }),
                    text: None,
                },
                super::gemini_types::GeminiReqPart {
                    inline_data: None,
                    text: Some(prompt.to_string()),
                },
            ],
        }],
        generation_config: if disable_thinking {
            Some(super::gemini_types::GeminiGenConfig {
                thinking_config: super::gemini_types::GeminiThinkingConfig { thinking_budget: 0 },
            })
        } else {
            None
        },
    };

    super::validate_model_for_url(model)?;
    let url =
        format!("https://generativelanguage.googleapis.com/v1beta/models/{model}:generateContent?key={api_key}");

    let request_started = std::time::Instant::now();
    let resp = super::client::get().post(&url).json(&body).send().await?;

    let status = resp.status();
    let request_id = resp
        .headers()
        .get("x-request-id")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("-")
        .to_string();
    log::debug!(
        "transcription: gemini response status={} request_id={} latency_ms={}",
        status,
        request_id,
        request_started.elapsed().as_millis()
    );
    if status.as_u16() == 429 {
        return Err(crate::api::quota_bail("Google"));
    }
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        let preview = crate::api::sanitize_error_body_preview(&body);
        log::warn!(
            "transcription: gemini non_success model={} status={} request_id={} body_preview=\"{}\"",
            model,
            status,
            request_id,
            preview
        );
        anyhow::bail!(
            "Gemini error status={} request_id={} body_preview={}",
            status,
            request_id,
            preview
        );
    }

    let raw_body = resp.text().await?;
    let data: GeminiResp =
        serde_json::from_str(&raw_body).context("Gemini transcription response parse error")?;

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
    log::debug!(
        "transcription: gemini parsed chars={}",
        text.chars().count()
    );

    Ok(text)
}
