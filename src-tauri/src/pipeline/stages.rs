use super::*;

/// Runtime safety net for a cleanup result that looks like the model
/// answering/refusing instead of returning cleaned dictation. Differential:
/// only acts if `cleaned` looks like a refusal AND `raw` does not (a real
/// speaker can legitimately say "I cannot...").
///
/// Returns `Some(text)` for a usable cleaned result (safe to cache), or
/// `None` if the retry also looks like a refusal/failed and the caller
/// should skip cleanup entirely and use the pre-cleanup text.
#[allow(clippy::too_many_arguments)]
pub(super) async fn guard_cleanup_refusal(
    cleaned: String,
    raw: &str,
    expanded: &str,
    provider_id: &str,
    model: &str,
    key: &str,
    profile: &str,
    intensity: &str,
    extra_rules: &str,
    app_context: Option<&str>,
) -> Option<String> {
    if !prompts::looks_like_refusal(&cleaned) || prompts::looks_like_refusal(raw) {
        return Some(cleaned);
    }

    log::warn!(
        "pipeline: cleanup output looks like a model refusal, retrying once with hardened prompt provider={provider_id} model={model}"
    );

    let cp = ProviderId::from_str(provider_id);
    let retried = cleanup::cleanup(
        expanded,
        cp,
        key,
        model,
        profile,
        intensity,
        extra_rules,
        app_context,
        Some(prompts::hardened_retry_template()),
    )
    .await;

    match retried {
        Ok(retried)
            if !retried.is_empty()
                && (!prompts::looks_like_refusal(&retried) || prompts::looks_like_refusal(raw)) =>
        {
            log::debug!(
                "pipeline: cleanup refusal retry succeeded provider={provider_id} model={model}"
            );
            Some(retried)
        }
        Ok(_) => {
            log::warn!(
                "pipeline: cleanup refusal retry still looks like a refusal, falling back to pre-cleanup text provider={provider_id} model={model}"
            );
            None
        }
        Err(e) => {
            log::warn!(
                "pipeline: cleanup refusal retry failed, falling back to pre-cleanup text provider={provider_id} model={model} error={}",
                trim_err(&e.to_string())
            );
            None
        }
    }
}
pub(super) fn resolve_app_mapping(
    store: Option<&store::SettingsSnapshot>,
    process_name: &str,
) -> Option<AppMapping> {
    store.and_then(|s| {
        s.get(store::APP_MAPPINGS)
            .and_then(|v| serde_json::from_value::<Vec<AppMapping>>(v.clone()).ok())
            .and_then(|list| {
                list.into_iter()
                    .find(|m| m.exe.trim().eq_ignore_ascii_case(process_name))
            })
    })
}

/// Resolves the effective tone profile for `mapping`, falling back to the
/// global `default_tone` when the app has no override, and applies the
/// app's `cleanup_intensity` override (if any) onto `cfg` in place.
pub(super) fn apply_app_style_overrides(
    cfg: &mut store::PipelineConfig,
    mapping: Option<&AppMapping>,
) -> String {
    if let Some(intensity) = mapping
        .and_then(|m| m.cleanup_intensity.as_deref())
        .map(str::trim)
        .filter(|i| !i.is_empty())
    {
        cfg.cleanup_intensity = intensity.to_owned();
    }
    mapping
        .map(|m| m.profile.trim().to_owned())
        .filter(|p| !p.is_empty())
        .unwrap_or_else(|| cfg.default_tone.clone())
}

/// Casual/formal cleanup sometimes omits a closing period on short utterances.
/// That leaves a bare word before the caret, which the contextual-capitalization
/// probe then reads as a mid-sentence continuation — so the *next* dictation has
/// its first letter lowercased. Appending a period when the cleaned text ends on
/// a plain word makes consecutive dictations read as separate sentences and
/// capitalize naturally.
///
/// Deliberately conservative:
/// - `very_casual` is skipped (its style is intentionally near-punctuation-free).
/// - `none`/verbatim intensity is skipped (must echo speech without editorializing).
/// - Only acts when the last non-space character is alphanumeric. Text already
///   ending in terminal punctuation, a comma/colon/dash (intentional
///   continuation), or a closing bracket/quote is left untouched.
pub(super) fn ensure_terminal_punctuation(
    text: &str,
    profile: &str,
    cleanup_intensity: &str,
) -> String {
    if profile == "very_casual" || cleanup_intensity == "none" {
        return text.to_owned();
    }
    let trimmed = text.trim_end();
    match trimmed.chars().next_back() {
        Some(last) if last.is_alphanumeric() => {
            // Preserve any trailing whitespace the model emitted after the word.
            format!("{trimmed}.{}", &text[trimmed.len()..])
        }
        _ => text.to_owned(),
    }
}
// session.stop() blocks until the audio thread finishes (denoise + resample + WAV encode).
// spawn_blocking keeps the tokio worker free during that wait.
pub(super) async fn stop_and_validate_audio(
    app: &AppHandle,
    session: audio::RecordingSession,
    min_rms: f32,
) -> Option<(bytes::Bytes, u64)> {
    let stop_result = tokio::task::spawn_blocking(move || session.stop()).await;
    let audio::RecordingResult {
        wav,
        duration_ms,
        rms,
        truncated,
    } = match stop_result {
        Ok(Ok(v)) => v,
        Ok(Err(e)) => {
            log::error!("audio stop: {e}");
            hide_pill(app);
            return None;
        }
        Err(e) => {
            log::error!("audio stop task panicked: {e}");
            hide_pill(app);
            return None;
        }
    };
    if truncated {
        log::warn!(
            "pipeline: rejected recording that exceeded max duration limit max_seconds={}",
            audio::MAX_RECORDING_SECONDS
        );
        // show_error_pill already logs and emits "verenu:error" itself, so pass
        // the full descriptive message here rather than emitting a second event.
        show_error_pill(
            app,
            &format!(
                "Recording exceeded the {} minute limit. Please split it into shorter dictations.",
                audio::MAX_RECORDING_SECONDS / 60
            ),
        )
        .await;
        return None;
    }
    if duration_ms < MIN_RECORDING_MS || rms < min_rms {
        let msg = if duration_ms < MIN_RECORDING_MS {
            "Recording too short"
        } else {
            "Audio too quiet — check your mic"
        };
        log::debug!(
            "pipeline: rejected — duration={duration_ms}ms rms={rms:.4} min_rms={min_rms:.4}"
        );
        reject_with_pill(app, msg);
        return None;
    }
    Some((bytes::Bytes::from(wav), duration_ms))
}

pub(super) async fn open_config_and_context(
    app: &AppHandle,
    process_name: &str,
) -> Option<(store::PipelineConfig, String, Option<String>)> {
    let settings_store = match store::settings_snapshot(app) {
        Ok(s) => s,
        Err(e) => {
            log::error!("store: {e}");
            hide_pill(app);
            return None;
        }
    };
    let mut cfg = store::load_pipeline_config(&settings_store);
    if !has_transcription_key_in_chain(&cfg) {
        show_error_pill(
            app,
            "No API key saved for selected transcription model chain",
        )
        .await;
        return None;
    }
    let mapping = resolve_app_mapping(Some(&settings_store), process_name);
    let profile = apply_app_style_overrides(&mut cfg, mapping.as_ref());
    log::debug!(
        "pipeline: app mapping resolved process={process_name} matched={} profile={profile} cleanup_intensity={} default_tone={}",
        mapping.as_ref().map(|m| m.exe.as_str()).unwrap_or("none"),
        cfg.cleanup_intensity,
        cfg.default_tone,
    );
    let app_context = if cfg.app_context_hint {
        window_context::get_app_context_hint(process_name)
    } else {
        None
    };
    Some((cfg, profile, app_context))
}

pub(super) async fn run_transcription(
    app: &AppHandle,
    wav: &bytes::Bytes,
    cfg: &store::PipelineConfig,
) -> Option<(String, String)> {
    let wav = wav.clone();
    log::debug!(
        "pipeline: transcription stage start provider={} model={} language={} bytes={}",
        cfg.transcription_provider,
        cfg.transcription_default_model,
        cfg.transcription_language,
        wav.len()
    );

    let mut last_err: Option<anyhow::Error> = None;
    for (provider_id, model) in transcription_model_chain(cfg) {
        let key = cfg.key_for(&provider_id).to_owned();
        if key.is_empty() {
            continue;
        }
        let provider = ProviderId::from_str(&provider_id);
        let language = cfg.transcription_language.clone();
        match transcription::transcribe(wav.clone(), provider, &key, &language, &model).await {
            Ok(raw) if !raw.is_empty() => {
                log::debug!(
                    "pipeline: transcription provider success={} model={} chars={}",
                    provider_id,
                    model,
                    raw.chars().count()
                );
                return Some((raw, format!("{provider_id}/{model}/transcription")));
            }
            Ok(_) => {}
            Err(e) => {
                let retryable = crate::api::is_retryable_provider_error(&e);
                log::warn!(
                    "pipeline: transcription provider failed provider={} model={} retryable={} error={}",
                    provider_id,
                    model,
                    retryable,
                    trim_err(&e.to_string())
                );
                if retryable {
                    last_err = Some(e);
                    continue;
                }
                last_err = Some(e);
                break;
            }
        }
    }

    if let Some(e) = last_err {
        let mut user_msg = trim_err(&e.to_string());
        if let Some(parsed) = crate::api::parse_auth_401_error(&e.to_string()) {
            user_msg = crate::api::auth_401_display_message(&parsed);
        }
        log::error!(
            "pipeline: transcription failed error={}",
            trim_err(&e.to_string())
        );
        show_error_pill(app, &user_msg).await;
    } else {
        show_error_pill(
            app,
            "Nothing transcribed - please try speaking more clearly",
        )
        .await;
    }
    None
}

// Handles snippet fast-path, snippet instruction collection, LLM cleanup, and
// instruction override application. Returns (final_text_before_dict, dict_entries)
// so the caller can apply dictionary substitutions after saving to DB.
pub(super) async fn run_cleanup_and_snippets(
    app: &AppHandle,
    raw: &str,
    cfg: &store::PipelineConfig,
    profile: &str,
    app_context: Option<&str>,
) -> Option<(String, Vec<db::DictionaryEntry>, String)> {
    let db_handle = app.state::<DbHandle>();
    match run_cleanup_and_snippets_for_db(db_handle.inner(), raw, cfg, profile, app_context).await {
        Ok(result) => Some(result),
        Err(e) => {
            let mut user_msg = format!("Cleanup failed: {}", trim_err(&e.to_string()));
            if let Some(parsed) = crate::api::parse_auth_401_error(&e.to_string()) {
                user_msg = format!(
                    "Cleanup failed: {}",
                    crate::api::auth_401_display_message(&parsed)
                );
            }
            log::error!(
                "pipeline: cleanup failed error={}",
                trim_err(&e.to_string())
            );
            show_error_pill(app, &user_msg).await;
            None
        }
    }
}

pub(super) async fn run_cleanup_and_snippets_for_db(
    db_handle: &DbHandle,
    raw: &str,
    cfg: &store::PipelineConfig,
    profile: &str,
    app_context: Option<&str>,
) -> anyhow::Result<(String, Vec<db::DictionaryEntry>, String)> {
    let mut db_snippets = db::query_snippets(db_handle).unwrap_or_default();
    let dict_entries = db::query_dictionary(db_handle).unwrap_or_default();
    log::debug!(
        "pipeline: cleanup inputs snippets={} dict_entries={}",
        db_snippets.len(),
        dict_entries.len()
    );

    let snippet_instructions = snippets::collect_snippet_instructions_from(raw, &db_snippets);
    log::debug!(
        "pipeline: cleanup stage start raw_chars={} snippet_override_lines={} cleanup_enabled={}",
        raw.chars().count(),
        snippet_instructions
            .lines()
            .filter(|l| !l.trim().is_empty())
            .count(),
        cfg.cleanup_enabled
    );
    if crate::system::logger::is_verbose() && !snippet_instructions.is_empty() {
        log::debug!(
            "pipeline: cleanup snippet_instructions_meta lines={} chars={} fingerprint={}",
            snippet_instructions
                .lines()
                .filter(|line| !line.trim().is_empty())
                .count(),
            snippet_instructions.chars().count(),
            snippet_instructions_fingerprint(&snippet_instructions)
        );
    }

    // Fast path: entire transcription was a single snippet trigger — skip the LLM.
    let pure_expansion = if snippet_instructions.is_empty() {
        snippets::try_pure_snippet_expand_from(raw, &db_snippets, db_handle)
    } else {
        None
    };
    let expanded = pure_expansion
        .clone()
        .unwrap_or_else(|| snippets::expand_snippets_from(raw, &mut db_snippets, db_handle));
    log::debug!(
        "pipeline: snippets expanded pure_fast_path={} expanded_chars={}",
        pure_expansion.is_some(),
        expanded.chars().count()
    );

    let mut used_cache_key = String::new();
    let final_text = if should_run_cleanup_llm(
        cfg.cleanup_enabled,
        has_cleanup_key_in_chain(cfg),
        pure_expansion.is_none(),
        &cfg.cleanup_intensity,
        profile,
    ) {
        let has_snippets = !snippet_instructions.is_empty();
        let (cache_tokens, cache_separators) = number_parser::tokenize_cache_key_parts(&expanded);
        let allow_cache = should_use_cleanup_cache_tokens(&cache_tokens)
            && (expanded.chars().count() <= 200 || has_snippets);
        let cache_key = if allow_cache {
            let base_cache_key =
                number_parser::normalize_cleanup_cache_key_parts(&cache_tokens, &cache_separators);
            let mut key =
                style_scoped_cleanup_cache_key(&base_cache_key, profile, &cfg.cleanup_intensity);
            if !key.is_empty() && has_snippets {
                let fp = snippet_instructions_fingerprint(&snippet_instructions);
                key = format!("{key}|snip:{fp:x}");
            }
            key
        } else {
            String::new()
        };
        if !cache_key.is_empty() {
            used_cache_key = cache_key.clone();
            if let Ok(Some(entry)) = db::cleanup_cache_get_active(db_handle, &cache_key) {
                log::debug!(
                    "pipeline: cleanup cache hit key_len={} hit_count={}",
                    cache_key.len(),
                    entry.hit_count
                );
                let now = Utc::now();
                let new_hit_count = entry.hit_count + 1;
                let now_str = now.format("%Y-%m-%d %H:%M:%S").to_string();
                let new_expires_at =
                    next_cache_expiry(new_hit_count, &entry.created_at, &entry.expires_at, now);
                let _ = db::cleanup_cache_touch_hit(
                    db_handle,
                    &cache_key,
                    new_hit_count,
                    &now_str,
                    &new_expires_at,
                );
                log::debug!(
                    "pipeline: cleanup cache touch hit_count={} expires_at={}",
                    new_hit_count,
                    new_expires_at
                );
                let punctuated = ensure_terminal_punctuation(
                    &entry.clean_text,
                    profile,
                    &cfg.cleanup_intensity,
                );
                let overridden = snippets::apply_cleanup_instruction_overrides(
                    &punctuated,
                    &snippet_instructions,
                );
                return Ok((overridden, dict_entries, cache_key));
            }
        }
        log::debug!(
            "pipeline: cleanup cache {} key_len={}",
            if allow_cache { "miss" } else { "bypass" },
            cache_key.len()
        );
        let dict_instructions =
            dictionary::build_relevant_dictionary_prompt_from(&dict_entries, raw);
        let extra_rules = [snippet_instructions.as_str(), dict_instructions.as_str()]
            .iter()
            .filter(|s| !s.is_empty())
            .copied()
            .collect::<Vec<_>>()
            .join("\n\n");
        log::debug!(
            "pipeline: cleanup extra_rules chars={} lines={}",
            extra_rules.chars().count(),
            extra_rules.lines().filter(|l| !l.trim().is_empty()).count()
        );

        let mut cleaned_res: Option<(String, String, String, String)> = None;
        let mut last_cleanup_err: Option<anyhow::Error> = None;
        for (provider_id, model) in cleanup_model_chain(cfg) {
            let key = cfg.key_for(&provider_id).to_owned();
            if key.is_empty() {
                continue;
            }
            let cp = ProviderId::from_str(&provider_id);
            let custom_template = cfg.cleanup_override_for(&provider_id, &model);
            match cleanup::cleanup(
                &expanded,
                cp,
                &key,
                &model,
                profile,
                &cfg.cleanup_intensity,
                &extra_rules,
                app_context,
                custom_template,
            )
            .await
            {
                Ok(cleaned) if !cleaned.is_empty() => {
                    log::debug!(
                        "pipeline: cleanup provider success={} model={} cleaned_chars={}",
                        provider_id,
                        model,
                        cleaned.chars().count()
                    );
                    cleaned_res = Some((cleaned, provider_id.clone(), model.clone(), key.clone()));
                    break;
                }
                Ok(_) => {
                    last_cleanup_err = None;
                }
                Err(e) => {
                    let retryable = crate::api::is_retryable_provider_error(&e);
                    log::warn!(
                        "pipeline: cleanup provider failed provider={} model={} retryable={} error={}",
                        provider_id,
                        model,
                        retryable,
                        trim_err(&e.to_string())
                    );
                    if retryable {
                        last_cleanup_err = Some(e);
                        continue;
                    }
                    last_cleanup_err = Some(e);
                    break;
                }
            }
        }

        let provider_succeeded = cleaned_res.is_some();
        let guarded = match cleaned_res {
            Some((cleaned, provider_id, model, key)) => {
                guard_cleanup_refusal(
                    cleaned,
                    raw,
                    &expanded,
                    &provider_id,
                    &model,
                    &key,
                    profile,
                    &cfg.cleanup_intensity,
                    &extra_rules,
                    app_context,
                )
                .await
            }
            None => None,
        };

        match guarded {
            Some(cleaned) => {
                // Punctuate before caching + overrides so the cache stores the
                // normalized text and snippet "no period" instructions can still
                // override it afterward.
                let cleaned = ensure_terminal_punctuation(&cleaned, profile, &cfg.cleanup_intensity);
                let overridden =
                    snippets::apply_cleanup_instruction_overrides(&cleaned, &snippet_instructions);
                if !cache_key.is_empty() {
                    let expires = sqlite_utc_plus(7);
                    match db::cleanup_cache_insert_new(
                        db_handle,
                        &cache_key,
                        &cleaned,
                        &expires,
                        has_snippets,
                    ) {
                        Ok(_) => {
                            log::debug!("pipeline: cleanup cache insert ok expires_at={expires}")
                        }
                        Err(err) => log::warn!("pipeline: cleanup cache insert failed: {err}"),
                    }
                }
                overridden
            }
            None if !provider_succeeded && last_cleanup_err.is_some() => {
                return Err(last_cleanup_err.expect("checked"))
            }
            None => snippets::apply_cleanup_instruction_overrides(&expanded, &snippet_instructions),
        }
    } else {
        snippets::apply_cleanup_instruction_overrides(&expanded, &snippet_instructions)
    };

    Ok((final_text, dict_entries, used_cache_key))
}
