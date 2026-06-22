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
    pub wav: bytes::Bytes,
    pub duration_ms: u64,
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
#[allow(dead_code)]
pub async fn run_pipeline_fixture(
    request: PipelineTestRequest,
) -> anyhow::Result<PipelineTestResult> {
    if request.duration_ms < MIN_RECORDING_MS {
        anyhow::bail!("Recording too short");
    }
    if request.rms < MIN_RECORDING_RMS {
        anyhow::bail!("Audio too quiet - check your mic");
    }
    if !has_transcription_key_in_chain(&request.config) {
        anyhow::bail!("No API key configured for any model in the transcription chain");
    }

    let db_handle = match request.db {
        Some(d) => d,
        None => db::open(":memory:")?,
    };
    for snippet in &request.snippets {
        db::insert_snippet_returning(
            &db_handle,
            &snippet.trigger,
            &snippet.expansion,
            &snippet.instructions,
        )?;
    }
    for entry in &request.dictionary {
        db::insert_dictionary_entry_returning(&db_handle, &entry.term, entry.mistake.as_deref())?;
    }

    let mut transcribed: Option<(String, String)> = None;
    let mut last_err: Option<anyhow::Error> = None;
    for (provider_id, model) in transcription_model_chain(&request.config) {
        let key = request.config.key_for(&provider_id).to_owned();
        if key.is_empty() {
            continue;
        }
        let provider = transcription_provider_from_str(&provider_id);
        match transcription::transcribe(
            request.wav.clone(),
            provider,
            &key,
            &request.config.transcription_language,
            &model,
        )
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
                if crate::api::is_retryable_provider_error(&e) {
                    last_err = Some(e);
                    continue;
                }
                last_err = Some(e);
                break;
            }
        }
    }

    let (raw_text, api_used) = transcribed.ok_or_else(|| {
        last_err.unwrap_or_else(|| {
            anyhow::anyhow!("Transcription failed: no model in chain produced output")
        })
    })?;

    let (final_text_before_dictionary, dict_entries, cleanup_cache_key) =
        run_cleanup_and_snippets_for_db(
            &db_handle,
            &raw_text,
            &request.config,
            &request.profile,
            request.app_context.as_deref(),
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
    let clean_for_insert = if apply_caps_lock_upper {
        final_text_before_dictionary.to_uppercase()
    } else {
        final_text_before_dictionary.clone()
    };
    let history_entry = db::insert_transcription_returning(
        &db_handle,
        &raw_text,
        &clean_for_insert,
        words,
        request.duration_ms as i64,
        &api_used,
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