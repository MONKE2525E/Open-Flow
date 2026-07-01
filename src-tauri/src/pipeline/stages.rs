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
async fn run_local_cleanup_request(
    app: Option<&AppHandle>,
    model: &str,
    expanded: &str,
    profile: &str,
    intensity: &str,
    extra_rules: &str,
    app_context: Option<&str>,
    custom_template: Option<&str>,
) -> anyhow::Result<String> {
    #[cfg(any(test, debug_assertions))]
    if let Some(result) = crate::testing::resolve_provider_fixture("cleanup", "local", model) {
        return result;
    }

    let app = app.ok_or_else(|| anyhow::anyhow!("Local cleanup runtime unavailable"))?;
    let prompt = prompts::get_cleanup_prompt_with_extras(
        "local",
        model,
        profile,
        intensity,
        extra_rules,
        app_context,
        expanded,
        custom_template,
    );
    // Local builds can spend a roughly fixed amount of budget on hidden
    // reasoning before producing visible output — observed across many
    // requests: gemma-4-e2b consistently burns 430-460 tokens on internal
    // reasoning alone, regardless of how short the input is, leaving as
    // little as 50-80 tokens for the actual cleaned text under a flat
    // budget. That's tight enough that a longer dictation can get its
    // content cut off mid-sentence (finish_reason="length" with non-empty
    // content — accepted as a "success" since the artifact-leak guard only
    // inspects text, not whether generation was truncated). Add the
    // reasoning overhead on top of the normal content budget rather than
    // using it as a flat floor, so a longer dictation still gets adequate
    // room for its own (longer) cleaned output, not just the same total cap
    // a short one gets.
    const LOCAL_REASONING_OVERHEAD_TOKENS: u32 = 512;
    let max_output_tokens =
        prompts::cleanup_max_output_tokens(intensity, expanded) + LOCAL_REASONING_OVERHEAD_TOKENS;
    let manager = app
        .state::<crate::local_llm::LocalLlmManager>()
        .inner()
        .clone();
    manager
        .cleanup_with_prompt(app, model, expanded, &prompt, max_output_tokens)
        .await
}

/// Refusal text ("I am an AI..."), leaked model internals (chat-template
/// control tokens, chain-of-thought preamble), degenerate repetition,
/// fabricated content (output sharing almost no words with what was
/// actually dictated), excessive content loss (a "light"/"none" intensity
/// result missing a large chunk of what was actually dictated), unwanted
/// expansion (a "light"/"none" intensity result padded with extra words
/// built mostly from vocabulary that genuinely appears in the input, so
/// fabrication's word-overlap check doesn't catch it), and perspective flip
/// (the model answers dictation that sounds like it's addressed to someone,
/// swapping every "you" for "I" or vice versa — pronouns are too small a
/// fraction of total words to move the fabrication/length checks) are all
/// "the model didn't return usable cleaned dictation" — none of these are
/// ever safe to inject as if they were the user's speech. `reference` is the
/// text `text` is judged against for the fabrication/length/perspective
/// checks (the actual LLM input); pass the same string for both when there's
/// no meaningful baseline to compare against (e.g. judging the raw dictation
/// on its own, where these checks relative to itself are moot).
fn cleanup_output_is_unusable(intensity: &str, reference: &str, text: &str) -> bool {
    prompts::looks_like_refusal(text)
        || prompts::looks_like_model_artifact_leak(text)
        || prompts::looks_like_degenerate_repetition(text)
        || prompts::looks_like_fabricated_content(reference, text)
        || prompts::looks_like_excessive_content_loss(intensity, reference, text)
        || prompts::looks_like_unwanted_expansion(intensity, reference, text)
        || prompts::looks_like_perspective_flip(reference, text)
}

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
    app: Option<&AppHandle>,
) -> Option<String> {
    if !cleanup_output_is_unusable(intensity, expanded, &cleaned)
        || cleanup_output_is_unusable(intensity, raw, raw)
    {
        return Some(cleaned);
    }

    log::warn!(
        "pipeline: cleanup output looks like a refusal, leaked model internals, or fabricated content, retrying once with hardened prompt provider={provider_id} model={model}"
    );

    let retried = if provider_id == store::LOCAL {
        run_local_cleanup_request(
            app,
            model,
            expanded,
            profile,
            intensity,
            extra_rules,
            app_context,
            Some(prompts::hardened_retry_template()),
        )
        .await
    } else {
        let cp = ProviderId::from_str(provider_id);
        cleanup::cleanup(
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
        .await
    };

    match retried {
        Ok(retried)
            if !retried.is_empty()
                && (!cleanup_output_is_unusable(intensity, expanded, &retried)
                    || cleanup_output_is_unusable(intensity, raw, raw)) =>
        {
            log::debug!(
                "pipeline: cleanup refusal retry succeeded provider={provider_id} model={model}"
            );
            Some(retried)
        }
        Ok(_) => {
            log::warn!(
                "pipeline: cleanup refusal retry still unusable, falling back to pre-cleanup text provider={provider_id} model={model}"
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
            // Chinese/Japanese text uses the full-width period; a Western "."
            // reads as out of place after CJK ideographs or kana.
            let punct = if is_cjk(last) { "。" } else { "." };
            // Preserve any trailing whitespace the model emitted after the word.
            format!("{trimmed}{punct}{}", &text[trimmed.len()..])
        }
        _ => text.to_owned(),
    }
}

fn is_cjk(c: char) -> bool {
    matches!(c,
        '\u{4e00}'..='\u{9fff}'   // CJK Unified Ideographs
        | '\u{3400}'..='\u{4dbf}' // CJK Unified Ideographs Extension A
        | '\u{3040}'..='\u{309f}' // Hiragana
        | '\u{30a0}'..='\u{30ff}' // Katakana
    )
}
// session.stop() blocks until the audio thread finishes (denoise + resample + WAV encode).
// spawn_blocking keeps the tokio worker free during that wait.
pub(super) async fn stop_and_validate_audio(
    app: &AppHandle,
    session: audio::RecordingSession,
    min_rms: f32,
) -> Option<CapturedAudio> {
    let stop_result = tokio::task::spawn_blocking(move || session.stop()).await;
    if let Some(manager) = app.try_state::<crate::local_stt::LocalTranscriptionManager>() {
        manager.set_recording_active(false);
    }
    let audio::RecordingResult {
        wav,
        samples_16k,
        sample_rate,
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
    Some(CapturedAudio {
        wav: bytes::Bytes::from(wav),
        samples_16k: Arc::new(samples_16k),
        sample_rate,
        duration_ms,
    })
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
    if let Err(message) = validate_transcription_chain(&cfg, None) {
        show_error_pill(app, &message).await;
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

pub(super) async fn transcribe_any(
    app: &AppHandle,
    audio: &CapturedAudio,
    provider_id: &str,
    api_key: Option<&str>,
    language: &str,
    model: &str,
) -> anyhow::Result<String> {
    if provider_id == store::LOCAL {
        let manager = app
            .state::<crate::local_stt::LocalTranscriptionManager>()
            .inner()
            .clone();
        // Only show the "loading model" pill when this specific model
        // actually needs to load — previously shown unconditionally for
        // every local transcription, so it flashed even when the model was
        // already warm in memory (e.g. dictating twice in a row).
        let state = manager.state();
        let already_warm = state.is_loaded && state.current_model_id.as_deref() == Some(model);
        if !already_warm {
            show_pill(app, "loading_local_model");
        }
        let result = crate::local_stt::transcribe::transcribe(
            manager,
            app.clone(),
            model.to_string(),
            Arc::clone(&audio.samples_16k),
            audio.sample_rate,
            language.to_string(),
        )
        .await;
        // Only switch back to "processing" if we actually left it for
        // "loading_local_model" above — re-emitting the same state the pill
        // is already in is a no-op for the frontend, but it's still a wasted
        // Rust -> IPC -> WebView2 round trip and window re-show call on the
        // (common) warm-model path.
        if !already_warm {
            show_pill(app, "processing");
        }
        return result;
    }

    let key = api_key.ok_or_else(|| anyhow::anyhow!("No API key saved for {provider_id}"))?;
    transcription::transcribe(
        audio.wav.clone(),
        ProviderId::from_str(provider_id),
        key,
        language,
        model,
    )
    .await
}

pub(super) async fn run_transcription(
    app: &AppHandle,
    audio: &CapturedAudio,
    cfg: &store::PipelineConfig,
) -> Option<(String, String)> {
    log::debug!(
        "pipeline: transcription stage start provider={} model={} language={} wav_bytes={} pcm_samples={}",
        cfg.transcription_provider,
        cfg.transcription_default_model,
        cfg.transcription_language,
        audio.wav.len(),
        audio.samples_16k.len()
    );

    let mut last_err: Option<anyhow::Error> = None;
    for (provider_id, model) in transcription_model_chain(cfg) {
        let key = cfg.key_for(&provider_id).to_owned();
        let language = cfg.transcription_language.clone();
        match transcribe_any(
            app,
            audio,
            &provider_id,
            if key.is_empty() { None } else { Some(key.as_str()) },
            &language,
            &model,
        )
        .await
        {
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

struct CleanupCachePlan {
    key: String,
    allow_cache: bool,
    has_snippets: bool,
}

struct CleanupSuccess {
    cleaned: String,
    provider_id: String,
    model: String,
    key: String,
}

fn cleanup_cache_plan(
    expanded: &str,
    profile: &str,
    intensity: &str,
    snippet_instructions: &str,
) -> CleanupCachePlan {
    let has_snippets = !snippet_instructions.is_empty();
    let (cache_tokens, cache_separators) = number_parser::tokenize_cache_key_parts(expanded);
    let allow_cache = should_use_cleanup_cache_tokens(&cache_tokens)
        && (expanded.chars().count() <= 200 || has_snippets);
    let key = if allow_cache {
        let base_cache_key =
            number_parser::normalize_cleanup_cache_key_parts(&cache_tokens, &cache_separators);
        let mut key = style_scoped_cleanup_cache_key(&base_cache_key, profile, intensity);
        if !key.is_empty() && has_snippets {
            let fp = snippet_instructions_fingerprint(snippet_instructions);
            key = format!("{key}|snip:{fp:x}");
        }
        key
    } else {
        String::new()
    };

    CleanupCachePlan {
        key,
        allow_cache,
        has_snippets,
    }
}

fn touch_cleanup_cache_hit(db_handle: &DbHandle, cache_key: &str, entry: &db::CleanupCacheEntry) {
    let now = Utc::now();
    let new_hit_count = entry.hit_count + 1;
    let now_str = now.format("%Y-%m-%d %H:%M:%S").to_string();
    let new_expires_at =
        next_cache_expiry(new_hit_count, &entry.created_at, &entry.expires_at, now);
    match db::cleanup_cache_touch_hit(
        db_handle,
        cache_key,
        new_hit_count,
        &now_str,
        &new_expires_at,
    ) {
        Ok(_) => log::debug!(
            "pipeline: cleanup cache touch hit_count={} expires_at={}",
            new_hit_count,
            new_expires_at
        ),
        Err(err) => log::warn!("pipeline: cleanup cache touch failed: {err}"),
    }
}

fn cleanup_cache_hit_text(
    db_handle: &DbHandle,
    cache_key: &str,
    profile: &str,
    intensity: &str,
    snippet_instructions: &str,
) -> Option<String> {
    let entry = db::cleanup_cache_get_active(db_handle, cache_key)
        .ok()
        .flatten()?;
    // A cache hit skips generation entirely, so it also skips
    // guard_cleanup_refusal — an entry written before that guard existed (or
    // from any future bug) would otherwise be served forever until its
    // expiry, regardless of how good the guard gets. Validate on every read,
    // not just on write, and drop poisoned entries so the next miss
    // regenerates and overwrites them with a clean result. The cache only
    // stores the cleaned output, not the original dictation (by design —
    // raw dictation must not be persisted), so the fabrication and
    // content-loss checks have no baseline to compare against here and are
    // effectively a no-op; refusal, artifact-leak, and repetition checks
    // still apply.
    if cleanup_output_is_unusable(intensity, &entry.clean_text, &entry.clean_text) {
        log::warn!(
            "pipeline: cleanup cache entry looks unusable (model artifact leak/refusal), evicting and treating as miss key_len={}",
            cache_key.len()
        );
        let _ = db::cleanup_cache_delete_by_key(db_handle, cache_key);
        return None;
    }
    log::debug!(
        "pipeline: cleanup cache hit key_len={} hit_count={}",
        cache_key.len(),
        entry.hit_count
    );
    touch_cleanup_cache_hit(db_handle, cache_key, &entry);
    let punctuated = ensure_terminal_punctuation(&entry.clean_text, profile, intensity);
    Some(snippets::apply_cleanup_instruction_overrides(
        &punctuated,
        snippet_instructions,
    ))
}

#[allow(clippy::too_many_arguments)]
async fn run_cleanup_provider_chain(
    expanded: &str,
    cfg: &store::PipelineConfig,
    profile: &str,
    extra_rules: &str,
    app_context: Option<&str>,
    app: Option<&AppHandle>,
) -> (Option<CleanupSuccess>, Option<anyhow::Error>) {
    let mut last_cleanup_err: Option<anyhow::Error> = None;
    for (provider_id, model) in cleanup_model_chain(cfg) {
        let is_local = provider_id == store::LOCAL;
        let key = cfg.key_for(&provider_id).to_owned();
        if key.is_empty() && !is_local {
            continue;
        }
        let custom_template = cfg.cleanup_override_for(&provider_id, &model);
        let outcome = if is_local {
            run_local_cleanup_request(
                app,
                &model,
                expanded,
                profile,
                &cfg.cleanup_intensity,
                extra_rules,
                app_context,
                custom_template,
            )
            .await
        } else {
            let cp = ProviderId::from_str(&provider_id);
            cleanup::cleanup(
                expanded,
                cp,
                &key,
                &model,
                profile,
                &cfg.cleanup_intensity,
                extra_rules,
                app_context,
                custom_template,
            )
            .await
        };
        match outcome {
            Ok(cleaned) if !cleaned.is_empty() => {
                log::debug!(
                    "pipeline: cleanup provider success={} model={} cleaned_chars={}",
                    provider_id,
                    model,
                    cleaned.chars().count()
                );
                return (
                    Some(CleanupSuccess {
                        cleaned,
                        provider_id,
                        model,
                        key,
                    }),
                    None,
                );
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
                return (None, Some(e));
            }
        }
    }

    (None, last_cleanup_err)
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
    match run_cleanup_and_snippets_for_db(
        db_handle.inner(),
        raw,
        cfg,
        profile,
        app_context,
        Some(app),
    )
    .await
    {
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
    app: Option<&AppHandle>,
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
        let cache_plan = cleanup_cache_plan(
            &expanded,
            profile,
            &cfg.cleanup_intensity,
            &snippet_instructions,
        );
        let cache_key = cache_plan.key.clone();
        if !cache_key.is_empty() {
            used_cache_key = cache_key.clone();
            if let Some(overridden) = cleanup_cache_hit_text(
                db_handle,
                &cache_key,
                profile,
                &cfg.cleanup_intensity,
                &snippet_instructions,
            ) {
                return Ok((overridden, dict_entries, cache_key));
            }
        }
        log::debug!(
            "pipeline: cleanup cache {} key_len={}",
            if cache_plan.allow_cache {
                "miss"
            } else {
                "bypass"
            },
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

        let (cleanup_res, last_cleanup_err) = run_cleanup_provider_chain(
            &expanded,
            cfg,
            profile,
            &extra_rules,
            app_context,
            app,
        )
        .await;
        let provider_succeeded = cleanup_res.is_some();
        let guarded = match cleanup_res {
            Some(success) => {
                guard_cleanup_refusal(
                    success.cleaned,
                    raw,
                    &expanded,
                    &success.provider_id,
                    &success.model,
                    &success.key,
                    profile,
                    &cfg.cleanup_intensity,
                    &extra_rules,
                    app_context,
                    app,
                )
                .await
            }
            None => None,
        };

        match guarded {
            Some(cleaned) => {
                // Strip em dashes the model introduced (vs. ones the speaker
                // actually dictated) before caching, so a poisoned-by-style
                // result never gets baked into the cache.
                let cleaned = crate::system::text::strip_unspoken_em_dashes(&expanded, &cleaned);
                // Punctuate before caching + overrides so the cache stores the
                // normalized text and snippet "no period" instructions can still
                // override it afterward.
                let cleaned =
                    ensure_terminal_punctuation(&cleaned, profile, &cfg.cleanup_intensity);
                let overridden =
                    snippets::apply_cleanup_instruction_overrides(&cleaned, &snippet_instructions);
                if !cache_key.is_empty() {
                    let expires = sqlite_utc_plus(7);
                    match db::cleanup_cache_insert_new(
                        db_handle,
                        &cache_key,
                        &cleaned,
                        &expires,
                        cache_plan.has_snippets,
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
