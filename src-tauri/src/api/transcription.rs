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
    gen: u64,
) -> Result<String> {
    #[cfg(any(test, debug_assertions))]
    if let Some(result) =
        crate::testing::resolve_provider_fixture("transcription", provider.as_str(), model)
    {
        return result;
    }

    log::debug!(
        "transcription: start gen={} provider={:?} language={} wav_bytes={}",
        gen,
        provider,
        language,
        wav.len()
    );
    match provider {
        ProviderId::Google => transcribe_gemini(wav, api_key, language, model, gen).await,
        ProviderId::AssemblyAi => transcribe_assemblyai(wav, api_key, language, model, gen).await,
        ProviderId::Local => {
            anyhow::bail!("Local provider must not reach api::transcription::transcribe")
        }
        ProviderId::Groq | ProviderId::OpenAI => {
            let url = provider
                .whisper_url()
                .expect("Groq/OpenAI always have a whisper_url");
            transcribe_whisper(
                wav,
                api_key,
                url,
                provider.label(),
                provider.as_str(),
                model,
                language,
                gen,
            )
            .await
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn transcribe_whisper(
    wav: Bytes,
    api_key: &str,
    url: &str,
    provider_label: &str,
    provider_id: &str,
    model: &str,
    language: &str,
    gen: u64,
) -> Result<String> {
    #[derive(serde::Deserialize)]
    struct WhisperResponse {
        text: String,
    }

    let language_label = crate::data::store::transcription_language_label(language);
    let prompt = get_transcription_prompt(provider_id, model, language_label);
    let fields = build_whisper_form_fields(model, language, &prompt);
    log::debug!(
        "transcription: whisper request gen={} provider={} model={} url={} language={} wav_bytes={} prompt_chars={}",
        gen,
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
        "transcription: whisper response gen={} provider={} status={} request_id={} latency_ms={}",
        gen,
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
                "transcription: whisper unauthorized gen={} provider={} model={} status={} request_id={} body_preview=\"{}\"",
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
                "transcription: whisper non_success gen={} provider={} model={} status={} request_id={} body_preview=\"{}\"",
                gen,
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
        "transcription: whisper parsed gen={} chars={}",
        gen,
        body.text.trim().chars().count()
    );
    Ok(body.text.trim().to_owned())
}

async fn transcribe_gemini(
    wav: Bytes,
    api_key: &str,
    language: &str,
    model: &str,
    gen: u64,
) -> Result<String> {
    let language_label = crate::data::store::transcription_language_label(language);
    let prompt = get_transcription_prompt("google", model, language_label);
    if model == "gemini-3.5-transcribe" {
        return transcribe_gemini_dedicated(wav, api_key, &prompt, model, gen).await;
    }
    transcribe_gemini_with_prompt(wav, api_key, &prompt, model, gen).await
}

async fn transcribe_gemini_dedicated(
    wav: Bytes,
    api_key: &str,
    prompt: &str,
    model: &str,
    gen: u64,
) -> Result<String> {
    let encoded = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &wav);
    let body = super::gemini_types::GeminiInteractionTranscribeReq {
        model: model.to_owned(),
        input: vec![
            super::gemini_types::GeminiInteractionInput::Audio {
                data: encoded,
                mime_type: "audio/wav".to_owned(),
            },
            super::gemini_types::GeminiInteractionInput::Text {
                text: prompt.to_owned(),
            },
        ],
    };
    log::debug!(
        "transcription: gemini dedicated request gen={} model={} wav_bytes={} prompt_chars={}",
        gen,
        model,
        wav.len(),
        prompt.chars().count()
    );

    let request_started = std::time::Instant::now();
    let resp = super::client::get()
        .post("https://generativelanguage.googleapis.com/v1beta/interactions")
        .header("x-goog-api-key", api_key)
        .json(&body)
        .send()
        .await?;
    let status = resp.status();
    let request_id = super::response_request_id(&resp);
    log::debug!(
        "transcription: gemini dedicated response gen={} status={} request_id={} latency_ms={}",
        gen,
        status,
        request_id,
        request_started.elapsed().as_millis()
    );
    let resp = match super::ensure_provider_success(resp, "Google", Some(("Google", model))).await {
        Ok(resp) => resp,
        Err(super::ProviderHttpError::Quota(e)) => return Err(e),
        Err(super::ProviderHttpError::Auth { error, .. }) => return Err(error),
        Err(super::ProviderHttpError::NonSuccess {
            source,
            status,
            request_id,
            preview,
        }) => {
            return Err(anyhow::Error::new(source).context(format!(
                "Gemini Transcribe error status={} request_id={} body_preview={}",
                status, request_id, preview
            )))
        }
    };
    let body: serde_json::Value = resp.json().await?;
    parse_gemini_interaction_text(&body)
        .ok_or_else(|| anyhow::anyhow!("Gemini Transcribe returned no transcript"))
}

fn parse_gemini_interaction_text(body: &serde_json::Value) -> Option<String> {
    if let Some(text) = body.get("output_text").and_then(|v| v.as_str()) {
        if !text.trim().is_empty() {
            return Some(text.trim().to_owned());
        }
    }
    let text = body
        .get("steps")
        .and_then(|v| v.as_array())
        .into_iter()
        .flatten()
        .filter(|step| step.get("type").and_then(|v| v.as_str()) == Some("model_output"))
        .flat_map(|step| {
            step.get("content")
                .and_then(|v| v.as_array())
                .into_iter()
                .flatten()
        })
        .filter_map(|content| content.get("text").and_then(|v| v.as_str()))
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    (!text.is_empty()).then_some(text)
}

async fn transcribe_gemini_with_prompt(
    wav: Bytes,
    api_key: &str,
    prompt: &str,
    model: &str,
    gen: u64,
) -> Result<String> {
    log::debug!(
        "transcription: gemini request gen={} wav_bytes={} prompt_chars={}",
        gen,
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
        "transcription: gemini response gen={} status={} request_id={} latency_ms={}",
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
                "transcription: gemini unauthorized gen={} model={} status={} request_id={} body_preview=\"{}\"",
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
                "transcription: gemini non_success gen={} model={} status={} request_id={} body_preview=\"{}\"",
                gen,
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
        "transcription: gemini parsed gen={} chars={}",
        gen,
        text.chars().count()
    );

    Ok(text)
}

const ASSEMBLYAI_POLL_INTERVAL_MS: u64 = 1000;
const ASSEMBLYAI_POLL_TIMEOUT_SECS: u64 = 120;

#[derive(serde::Serialize, Clone, Debug, PartialEq)]
struct AssemblyAiSubmitRequest {
    audio_url: String,
    speech_models: Vec<String>,
    language_code: String,
    prompt: String,
}

fn build_assemblyai_submit_request(
    audio_url: String,
    model: &str,
    language: &str,
    prompt: &str,
) -> AssemblyAiSubmitRequest {
    AssemblyAiSubmitRequest {
        audio_url,
        speech_models: vec![model.to_owned()],
        language_code: language.to_owned(),
        prompt: prompt.to_owned(),
    }
}

/// AssemblyAI's pre-recorded API has no synchronous endpoint: unlike the
/// whisper-compatible and Gemini paths (single request in, text out), this
/// requires uploading the audio, submitting a transcription job, then
/// polling until the job finishes. This adds real latency (upload + submit +
/// several poll round trips) that the other providers don't have — inherent
/// to the API, not a bug.
async fn transcribe_assemblyai(
    wav: Bytes,
    api_key: &str,
    language: &str,
    model: &str,
    gen: u64,
) -> Result<String> {
    #[derive(serde::Deserialize)]
    struct UploadResponse {
        upload_url: String,
    }
    #[derive(serde::Deserialize)]
    struct SubmitResponse {
        id: String,
    }
    #[derive(serde::Deserialize)]
    struct PollResponse {
        status: String,
        text: Option<String>,
        error: Option<String>,
    }

    async fn handle_provider_error(
        gen: u64,
        result: Result<reqwest::Response, super::ProviderHttpError>,
        stage: &str,
    ) -> Result<reqwest::Response> {
        match result {
            Ok(resp) => Ok(resp),
            Err(super::ProviderHttpError::Quota(e)) => Err(e),
            Err(super::ProviderHttpError::Auth {
                error,
                status,
                request_id,
                preview,
            }) => {
                log::warn!(
                    "transcription: assemblyai {stage} unauthorized gen={} status={} request_id={} body_preview=\"{}\"",
                    gen,
                    status,
                    request_id,
                    preview
                );
                Err(error)
            }
            Err(super::ProviderHttpError::NonSuccess {
                source,
                status,
                request_id,
                preview,
            }) => {
                log::warn!(
                    "transcription: assemblyai {stage} non_success gen={} status={} request_id={} body_preview=\"{}\"",
                    gen,
                    status,
                    request_id,
                    preview
                );
                Err(anyhow::Error::new(source).context(format!(
                    "AssemblyAI {stage} error status={} request_id={} body_preview={}",
                    status, request_id, preview
                )))
            }
        }
    }

    let language_label = crate::data::store::transcription_language_label(language);
    let prompt = get_transcription_prompt("assemblyai", model, language_label);
    log::debug!(
        "transcription: assemblyai upload gen={} wav_bytes={}",
        gen,
        wav.len()
    );

    let request_started = std::time::Instant::now();
    let upload_resp = super::client::get()
        .post("https://api.assemblyai.com/v2/upload")
        .header("authorization", api_key)
        .header("content-type", "application/octet-stream")
        .body(wav)
        .send()
        .await?;
    log::debug!(
        "transcription: assemblyai upload response gen={} status={} latency_ms={}",
        gen,
        upload_resp.status(),
        request_started.elapsed().as_millis()
    );
    let upload_resp = handle_provider_error(
        gen,
        super::ensure_provider_success(upload_resp, model, Some(("AssemblyAI", model))).await,
        "upload",
    )
    .await?;
    let upload: UploadResponse = upload_resp.json().await?;

    let submit_started = std::time::Instant::now();
    let submit_body = build_assemblyai_submit_request(upload.upload_url, model, language, &prompt);
    let submit_resp = super::client::get()
        .post("https://api.assemblyai.com/v2/transcript")
        .header("authorization", api_key)
        .json(&submit_body)
        .send()
        .await?;
    log::debug!(
        "transcription: assemblyai submit response gen={} status={} latency_ms={}",
        gen,
        submit_resp.status(),
        submit_started.elapsed().as_millis()
    );
    let submit_resp = handle_provider_error(
        gen,
        super::ensure_provider_success(submit_resp, model, Some(("AssemblyAI", model))).await,
        "submit",
    )
    .await?;
    let submitted: SubmitResponse = submit_resp.json().await?;
    log::debug!(
        "transcription: assemblyai submitted gen={} job_id={}",
        gen,
        submitted.id
    );

    let poll_url = format!("https://api.assemblyai.com/v2/transcript/{}", submitted.id);
    let deadline = request_started + std::time::Duration::from_secs(ASSEMBLYAI_POLL_TIMEOUT_SECS);
    let mut poll_count: u32 = 0;
    loop {
        if std::time::Instant::now() >= deadline {
            log::warn!(
                "transcription: assemblyai poll timed out gen={} job_id={} poll_count={} elapsed_ms={}",
                gen,
                submitted.id,
                poll_count,
                request_started.elapsed().as_millis()
            );
            anyhow::bail!("AssemblyAI transcription timed out waiting for a result");
        }
        let poll_resp = super::client::get()
            .get(&poll_url)
            .header("authorization", api_key)
            .send()
            .await?;
        let poll_resp = handle_provider_error(
            gen,
            super::ensure_provider_success(poll_resp, model, Some(("AssemblyAI", model))).await,
            "poll",
        )
        .await?;
        let poll: PollResponse = poll_resp.json().await?;
        poll_count += 1;
        log::debug!(
            "transcription: assemblyai poll gen={} job_id={} poll_count={} status={} elapsed_ms={}",
            gen,
            submitted.id,
            poll_count,
            poll.status,
            request_started.elapsed().as_millis()
        );

        match poll.status.as_str() {
            "completed" => {
                let text = poll.text.unwrap_or_default().trim().to_owned();
                log::debug!(
                    "transcription: assemblyai parsed chars={}",
                    text.chars().count()
                );
                return Ok(text);
            }
            "error" => {
                anyhow::bail!(
                    "AssemblyAI transcription error: {}",
                    poll.error.unwrap_or_else(|| "unknown error".to_string())
                );
            }
            _ => {}
        }

        tokio::time::sleep(std::time::Duration::from_millis(
            ASSEMBLYAI_POLL_INTERVAL_MS,
        ))
        .await;
    }
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
    use super::{
        build_assemblyai_submit_request, build_gemini_transcription_request,
        build_whisper_form_fields,
    };

    #[test]
    fn assemblyai_submit_request_includes_model_and_prompt() {
        let body = build_assemblyai_submit_request(
            "https://cdn.assemblyai.com/upload/fake".to_string(),
            "universal-3-5-pro",
            "en",
            "prompt text",
        );
        let json = serde_json::to_value(&body).unwrap();
        assert_eq!(json["audio_url"], "https://cdn.assemblyai.com/upload/fake");
        assert_eq!(json["speech_models"][0], "universal-3-5-pro");
        assert_eq!(json["language_code"], "en");
        assert_eq!(json["prompt"], "prompt text");
    }

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
            "gemini-3.7-flash",
        );
        let json = serde_json::to_value(&body).unwrap();
        assert_eq!(
            json["generationConfig"]["thinkingConfig"]["thinkingLevel"],
            "minimal"
        );
        assert_eq!(json["generationConfig"]["maxOutputTokens"], 2048);
        assert_eq!(json["generationConfig"]["temperature"], 0.0);
    }

    #[test]
    fn dedicated_gemini_interaction_request_uses_audio_input() {
        let body = super::super::gemini_types::GeminiInteractionTranscribeReq {
            model: "gemini-3.5-transcribe".to_string(),
            input: vec![
                super::super::gemini_types::GeminiInteractionInput::Audio {
                    data: "ZmFrZQ==".to_string(),
                    mime_type: "audio/wav".to_string(),
                },
                super::super::gemini_types::GeminiInteractionInput::Text {
                    text: "transcribe this".to_string(),
                },
            ],
        };
        let json = serde_json::to_value(body).unwrap();
        assert_eq!(json["model"], "gemini-3.5-transcribe");
        assert_eq!(json["input"][0]["type"], "audio");
        assert_eq!(json["input"][0]["mime_type"], "audio/wav");
        assert_eq!(json["input"][1]["type"], "text");
    }

    #[test]
    fn dedicated_gemini_interaction_parser_reads_output_text_and_steps() {
        assert_eq!(
            super::parse_gemini_interaction_text(&serde_json::json!({"output_text":" hello "})),
            Some("hello".to_string())
        );
        assert_eq!(
            super::parse_gemini_interaction_text(&serde_json::json!({
                "steps": [{"type":"model_output", "content":[{"type":"text", "text":"hello"}, {"type":"text", "text":"world"}]}]
            })),
            Some("hello world".to_string())
        );
    }
}
