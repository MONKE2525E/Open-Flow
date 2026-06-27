use anyhow::{Context, Result};
use bytes::Bytes;
use reqwest::multipart;

use super::gemini_types::GeminiResp;
use super::prompts::{gemini_generation_config, get_transcription_prompt};
use super::ProviderId;

#[derive(Clone, Debug, Eq, PartialEq)]
struct WhisperFormFields {
    model: String,
    response_format: String,
    language: String,
    prompt: String,
}

pub async fn transcribe(
    wav: Bytes,
    provider: ProviderId,
    api_key: &str,
    language: &str,
    model: &str,
) -> Result<String> {
    #[cfg(any(test, debug_assertions))]
    if let Some(result) =
        crate::testing::resolve_provider_fixture("transcription", provider.as_str(), model)
    {
        return result;
    }

    log::debug!(
        "transcription: start provider={:?} language={} wav_bytes={}",
        provider,
        language,
        wav.len()
    );
    if let Some(url) = provider.whisper_url() {
        transcribe_whisper(
            wav,
            api_key,
            url,
            provider.label(),
            provider.as_str(),
            model,
            language,
        )
        .await
    } else {
        transcribe_gemini(wav, api_key, language, model).await
    }
}

async fn transcribe_whisper(
    wav: Bytes,
    api_key: &str,
    url: &str,
    provider_label: &str,
    provider_id: &str,
    model: &str,
    language: &str,
) -> Result<String> {
    #[derive(serde::Deserialize)]
    struct WhisperResponse {
        text: String,
    }

    let language_label = crate::data::store::transcription_language_label(language);
    let prompt = get_transcription_prompt(provider_id, model, language_label);
    let fields = build_whisper_form_fields(model, language, &prompt);
    log::debug!(
        "transcription: whisper request provider={} model={} url={} language={} wav_bytes={} prompt_chars={}",
        provider_label,
        model,
        url,
        language,
        wav.len(),
        fields.prompt.chars().count()
    );
    let form = build_whisper_form(wav, &fields)?;

    let request_started = std::time::Instant::now();
    let resp = super::client::get()
        .post(url)
        .bearer_auth(api_key)
        .multipart(form)
        .send()
        .await?;
    let status = resp.status();
    let request_id = super::response_request_id(&resp);
    log::debug!(
        "transcription: whisper response provider={} status={} request_id={} latency_ms={}",
        provider_label,
        status,
        request_id,
        request_started.elapsed().as_millis()
    );

    let resp = match super::ensure_provider_success(resp, model, Some((provider_label, model)))
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
                "transcription: whisper unauthorized provider={} model={} status={} request_id={} body_preview=\"{}\"",
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
                "transcription: whisper non_success provider={} model={} status={} request_id={} body_preview=\"{}\"",
                provider_label,
                model,
                status,
                request_id,
                preview
            );
            return Err(anyhow::Error::new(source).context(format!(
                "Transcription API error provider={} model={} status={} request_id={} body_preview={}",
                provider_label, model, status, request_id, preview
            )));
        }
    };

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
    let prompt = get_transcription_prompt("google", model, language_label);
    transcribe_gemini_with_prompt(wav, api_key, &prompt, model).await
}

async fn transcribe_gemini_with_prompt(
    wav: Bytes,
    api_key: &str,
    prompt: &str,
    model: &str,
) -> Result<String> {
    log::debug!(
        "transcription: gemini request wav_bytes={} prompt_chars={}",
        wav.len(),
        prompt.chars().count()
    );
    let encoded = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &wav);

    let body = build_gemini_transcription_request(encoded, prompt, model);

    super::validate_model_for_url(model)?;
    // Key goes in the `x-goog-api-key` header, never the URL query string — see the
    // matching note in api/cleanup.rs. A leaked URL must not carry the secret.
    let url =
        format!("https://generativelanguage.googleapis.com/v1beta/models/{model}:generateContent");

    let request_started = std::time::Instant::now();
    let resp = super::client::get()
        .post(&url)
        .header("x-goog-api-key", api_key)
        .json(&body)
        .send()
        .await?;

    let status = resp.status();
    let request_id = super::response_request_id(&resp);
    log::debug!(
        "transcription: gemini response status={} request_id={} latency_ms={}",
        status,
        request_id,
        request_started.elapsed().as_millis()
    );
    let resp = match super::ensure_provider_success(resp, "Google", None).await {
        Ok(resp) => resp,
        Err(super::ProviderHttpError::Quota(e)) => return Err(e),
        Err(super::ProviderHttpError::Auth { error, .. }) => return Err(error),
        Err(super::ProviderHttpError::NonSuccess {
            source,
            status,
            request_id,
            preview,
        }) => {
            log::warn!(
                "transcription: gemini non_success model={} status={} request_id={} body_preview=\"{}\"",
                model,
                status,
                request_id,
                preview
            );
            return Err(anyhow::Error::new(source).context(format!(
                "Gemini error status={} request_id={} body_preview={}",
                status, request_id, preview
            )));
        }
    };

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

fn build_whisper_form_fields(model: &str, language: &str, prompt: &str) -> WhisperFormFields {
    WhisperFormFields {
        model: model.to_owned(),
        response_format: "json".to_string(),
        language: language.to_owned(),
        prompt: prompt.to_owned(),
    }
}

fn build_whisper_form(wav: Bytes, fields: &WhisperFormFields) -> Result<multipart::Form> {
    let part =
        multipart::Part::stream_with_length(reqwest::Body::from(wav.clone()), wav.len() as u64)
            .file_name("audio.wav")
            .mime_str("audio/wav")?;
    Ok(multipart::Form::new()
        .part("file", part)
        .text("model", fields.model.clone())
        .text("response_format", fields.response_format.clone())
        .text("language", fields.language.clone())
        .text("prompt", fields.prompt.clone()))
}

fn build_gemini_transcription_request(
    encoded_audio: String,
    prompt: &str,
    model: &str,
) -> super::gemini_types::GeminiTranscribeReq {
    super::gemini_types::GeminiTranscribeReq {
        contents: vec![super::gemini_types::GeminiReqContent {
            parts: vec![
                super::gemini_types::GeminiReqPart {
                    inline_data: Some(super::gemini_types::GeminiInlineData {
                        mime_type: "audio/wav".to_string(),
                        data: encoded_audio,
                    }),
                    text: None,
                },
                super::gemini_types::GeminiReqPart {
                    inline_data: None,
                    text: Some(prompt.to_string()),
                },
            ],
        }],
        generation_config: Some(gemini_generation_config(model, 2048)),
    }
}

#[cfg(test)]
mod tests {
    use super::{build_gemini_transcription_request, build_whisper_form_fields};

    #[test]
    fn whisper_form_fields_include_prompt() {
        let fields = build_whisper_form_fields("gpt-4o-transcribe", "en", "prompt text");
        assert_eq!(fields.prompt, "prompt text");
        assert_eq!(fields.response_format, "json");
    }

    #[test]
    fn gemini_transcription_request_includes_generation_config() {
        let body = build_gemini_transcription_request(
            "ZmFrZQ==".to_string(),
            "prompt text",
            "gemini-3.5-flash",
        );
        let json = serde_json::to_value(&body).unwrap();
        assert_eq!(
            json["generationConfig"]["thinkingConfig"]["thinkingLevel"],
            "minimal"
        );
        assert_eq!(json["generationConfig"]["maxOutputTokens"], 2048);
        assert_eq!(json["generationConfig"]["temperature"], 0.0);
    }
}
