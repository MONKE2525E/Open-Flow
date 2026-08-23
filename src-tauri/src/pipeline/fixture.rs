use super::*;

#[cfg(any(test, debug_assertions))]
#[allow(dead_code)]
#[derive(Clone, Debug)]
pub struct PipelineTestSnippet {
    pub trigger: String,
    pub expansion: String,
    pub instructions: String,
}

#[cfg(any(test, debug_assertions))]
#[allow(dead_code)]
#[derive(Clone, Debug)]
pub struct PipelineTestDictionaryEntry {
    pub term: String,
    pub mistake: Option<String>,
}

#[cfg(any(test, debug_assertions))]
#[allow(dead_code)]
#[derive(Clone, Debug)]
pub struct PipelineTestRequest {
    pub db: Option<DbHandle>,
    pub audio: CapturedAudio,
    pub rms: f32,
    pub config: store::PipelineConfig,
    pub profile: String,
    pub target_hwnd: usize,
    pub app_context: Option<String>,
    pub snippets: Vec<PipelineTestSnippet>,
    pub dictionary: Vec<PipelineTestDictionaryEntry>,
    pub caps_lock_on: bool,
}

#[cfg(any(test, debug_assertions))]
#[allow(dead_code)]
#[derive(Clone, Debug)]
pub struct PipelineTestResult {
    pub raw_text: String,
    pub final_text_before_dictionary: String,
    pub injected_text: String,
    pub api_used: String,
    pub cleanup_cache_key: String,
    pub history_entry: db::RecentEntry,
    pub recent: Vec<db::RecentEntry>,
    pub stats: db::Stats,
}

#[cfg(any(test, debug_assertions))]
async fn transcribe_fixture_provider(
    audio: &CapturedAudio,
    config: &store::PipelineConfig,
    provider_id: &str,
    model: &str,
) -> anyhow::Result<String> {
    if provider_id == store::LOCAL {
        #[cfg(any(test, debug_assertions))]
        if let Some(result) =
            crate::testing::resolve_provider_fixture("transcription", provider_id, model)
        {
            return result;
        }
        anyhow::bail!("Missing mock fixture for local transcription model '{model}'");
    }

    transcription::transcribe(
        audio.wav.clone(),
        ProviderId::from_str(provider_id),
        config.key_for(provider_id),
        &config.transcription_language,
        model,
        0,
    )
    .await
}

#[cfg(any(test, debug_assertions))]
#[allow(dead_code)]
pub async fn run_pipeline_fixture(
    request: PipelineTestRequest,
) -> anyhow::Result<PipelineTestResult> {
    if request.audio.duration_ms < MIN_RECORDING_MS {
        anyhow::bail!("Recording too short");
    }
    if request.rms < MIN_RECORDING_RMS {
        anyhow::bail!("Audio too quiet - check your mic");
    }
    if let Err(message) = validate_transcription_chain(&request.config, None) {
        anyhow::bail!(message);
    }

    let db_handle = match request.db {
        Some(d) => d,
        None => db::open(":memory:")?,
    };
    let context_id = db::everywhere_context_id(&db_handle)?;
    for snippet in &request.snippets {
        db::insert_snippet_returning(
            &db_handle,
            &snippet.trigger,
            &snippet.expansion,
            &snippet.instructions,
            None,
        )?;
    }
    for entry in &request.dictionary {
        db::insert_dictionary_entry_returning(&db_handle, &entry.term, entry.mistake.as_deref(), None)?;
    }

    let mut transcribed: Option<(String, String)> = None;
    let mut last_err: Option<anyhow::Error> = None;
    for (provider_id, model) in transcription_model_chain(&request.config) {
        match transcribe_fixture_provider(&request.audio, &request.config, &provider_id, &model)
            .await
        {
            Ok(raw) if !raw.is_empty() => {
                transcribed = Some((
                    normalize_transcription_math_artifacts(&raw),
                    format!("{provider_id}/{model}/transcription"),
                ));
                break;
            }
            Ok(_) => {}
            Err(e) => {
                // Mirrors run_transcription in stages.rs: always try the
                // next candidate regardless of retryable status — a
                // non-retryable error on this provider (e.g. missing key)
                // says nothing about whether a different fallback
                // provider/model would succeed.
                last_err = Some(e);
            }
        }
    }

    let (raw_text, api_used) = transcribed.ok_or_else(|| {
        last_err.unwrap_or_else(|| {
            anyhow::anyhow!("Transcription failed: no model in chain produced output")
        })
    })?;

    let (final_text_before_dictionary, dict_entries, cleanup_cache_key, _cleanup_api_used) =
        run_cleanup_and_snippets_for_db(
            &db_handle,
            &raw_text,
            None,
            &request.config,
            &request.profile,
            request.app_context.as_deref(),
            context_id,
            None,
            None,
            0,
        )
        .await?;
    let apply_caps_lock_upper = request.config.caps_lock_uppercase_enabled && request.caps_lock_on;
    let (injected_text, _applied_dict_ids) =
        dictionary::apply_substitutions_from(&final_text_before_dictionary, &dict_entries);
    let injected_text = if apply_caps_lock_upper {
        injected_text.to_uppercase()
    } else {
        injected_text
    };
    let words = raw_text.split_whitespace().count() as i64;
    // Mirrors finalize.rs: History must save the same text that actually
    // gets injected (dictionary substitution included), not the
    // pre-dictionary value.
    let clean_for_insert = injected_text.clone();
    let history_entry = db::insert_transcription_returning(
        &db_handle,
        &raw_text,
        &clean_for_insert,
        words,
        request.audio.duration_ms as i64,
        &api_used,
        None,
        // Mirror finalize.rs: dictations are attributed to the resolved
        // context so fixture results exercise context attribution too.
        Some(context_id),
    )?;
    let injected = injection::inject_text(
        &injected_text,
        request.target_hwnd,
        request.config.contextual_caps_enabled,
        request.config.auto_spacing_enabled,
        &request.profile,
        request.config.macos_clipboard_sniff_enabled,
    )
    .await?;
    let recent = db::query_recent(&db_handle)?;
    let stats = db::query_stats(&db_handle)?;

    Ok(PipelineTestResult {
        raw_text,
        final_text_before_dictionary,
        injected_text: injected.text,
        api_used,
        cleanup_cache_key,
        history_entry,
        recent,
        stats,
    })
}
