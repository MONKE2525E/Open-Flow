//! Cleanup stage: refusal/artifact guards, local cleanup requests, the
//! cleanup-result cache, provider fallback chains, and the
//! cleanup+snippet orchestration entrypoints.

use super::*;
use super::stages_style::ensure_terminal_punctuation;

// Cleanup is an enhancement, not a reason to leave a completed dictation
// blocked behind a provider that has accepted a request but stopped replying.
// Normal cleanup completes in about a second, so retry once quickly and then
// deliver the transcription without cleanup if both attempts stall.
const CLEANUP_FAST_ATTEMPT_TIMEOUT_SECS: u64 = 3;
const CLEANUP_FAST_ATTEMPTS: u8 = 2;

fn cleanup_soft_timeout_error(provider: &str, model: &str) -> anyhow::Error {
    anyhow::anyhow!(
        "CLEANUP_SOFT_TIMEOUT provider={provider} model={model} timeout_secs={CLEANUP_FAST_ATTEMPT_TIMEOUT_SECS}"
    )
}

fn is_cleanup_soft_timeout(error: &anyhow::Error) -> bool {
    error.to_string().starts_with("CLEANUP_SOFT_TIMEOUT ")
}

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
    alternate_transcript: Option<&str>,
) -> anyhow::Result<String> {
    #[cfg(any(test, debug_assertions))]
    if let Some(result) = crate::testing::resolve_provider_fixture("cleanup", "local", model) {
        return result;
    }

    let app = app.ok_or_else(|| anyhow::anyhow!("Local cleanup runtime unavailable"))?;
    let prompt = prompts::get_cleanup_prompt_with_alternate(
        "local",
        model,
        profile,
        intensity,
        extra_rules,
        app_context,
        expanded,
        custom_template,
        alternate_transcript,
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
    let input = alternate_transcript
        .map(|alternate| {
            format!(
                "<primary_transcript>\n{}\n</primary_transcript>\n<alternate_transcript>\n{}\n</alternate_transcript>",
                crate::api::cleanup::escape_transcript_xml(expanded),
                crate::api::cleanup::escape_transcript_xml(alternate),
            )
        })
        .unwrap_or_else(|| expanded.to_owned());
    manager
        .cleanup_with_prompt(app, model, &input, &prompt, max_output_tokens)
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

fn cleanup_output_is_unusable_against_candidates(
    intensity: &str,
    primary: &str,
    alternate: Option<&str>,
    text: &str,
) -> bool {
    let intrinsic_failure = prompts::looks_like_refusal(text)
        || prompts::looks_like_model_artifact_leak(text)
        || prompts::looks_like_degenerate_repetition(text);
    if intrinsic_failure {
        return true;
    }

    let primary_failure = cleanup_output_is_unusable(intensity, primary, text);
    match alternate {
        Some(alternate) => {
            // A reconciler is allowed to choose wording that only the
            // alternate candidate supports. Reject it only when it fails
            // against both untrusted candidates.
            primary_failure && cleanup_output_is_unusable(intensity, alternate, text)
        }
        None => primary_failure,
    }
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
    alternate_transcript: Option<&str>,
    gen: u64,
) -> Option<String> {
    if !cleanup_output_is_unusable_against_candidates(
        intensity,
        expanded,
        alternate_transcript,
        &cleaned,
    ) || prompts::looks_like_refusal(raw)
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
            alternate_transcript,
        )
        .await
    } else {
        let cp = ProviderId::from_str(provider_id);
        cleanup::cleanup_with_alternate(
            expanded,
            cp,
            key,
            model,
            profile,
            intensity,
            extra_rules,
            app_context,
            Some(prompts::hardened_retry_template()),
            alternate_transcript,
            gen,
        )
        .await
    };

    match retried {
        Ok(retried)
            if !retried.is_empty()
                && (!cleanup_output_is_unusable_against_candidates(
                    intensity,
                    expanded,
                    alternate_transcript,
                    &retried,
                ) || cleanup_output_is_unusable(intensity, raw, raw)) =>
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


pub(super) struct CleanupCachePlan {
    pub(super) key: String,
    allow_cache: bool,
    has_snippets: bool,
}

struct CleanupSuccess {
    cleaned: String,
    provider_id: String,
    model: String,
    key: String,
}

#[cfg_attr(not(test), allow(dead_code))]
pub(super) fn cleanup_cache_plan(
    expanded: &str,
    profile: &str,
    intensity: &str,
    snippet_instructions: &str,
    alternate_transcript: Option<&str>,
    dual_context_fingerprint: Option<u64>,
) -> CleanupCachePlan {
    cleanup_cache_plan_for_context(
        expanded,
        profile,
        intensity,
        snippet_instructions,
        alternate_transcript,
        dual_context_fingerprint,
        None,
    )
}

pub(super) fn cleanup_cache_plan_for_context(
    expanded: &str,
    profile: &str,
    intensity: &str,
    snippet_instructions: &str,
    alternate_transcript: Option<&str>,
    dual_context_fingerprint: Option<u64>,
    context_id: Option<i64>,
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
        if !key.is_empty() {
            if let Some(alternate) = alternate_transcript {
                key = format!(
                    "{key}|dual:{:x}",
                    snippet_instructions_fingerprint(alternate)
                );
            }
        }
        if !key.is_empty() {
            if let Some(fingerprint) = dual_context_fingerprint {
                key = format!("{key}|dualctx:{fingerprint:x}");
            }
        }
        if let Some(context_id) = context_id.filter(|id| *id != db::EVERYWHERE_CONTEXT_ID) {
            if !key.is_empty() {
                key = format!("{key}|ctx:{context_id}");
            }
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

pub(super) fn dual_cleanup_context_fingerprint(
    cfg: &store::PipelineConfig,
    extra_rules: &str,
    app_context: Option<&str>,
) -> u64 {
    let mut context = String::new();
    context.push_str(&cfg.cleanup_default_model);
    context.push('\n');
    for fallback in &cfg.cleanup_fallback_models {
        context.push_str(fallback);
        context.push('\n');
    }
    context.push_str(extra_rules);
    context.push('\n');
    context.push_str(app_context.unwrap_or(""));
    context.push('\n');
    if let Some(template) = cfg.cleanup_override() {
        context.push_str(template);
        context.push('\n');
    }
    snippet_instructions_fingerprint(&context)
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
        &entry.created_at,
        entry.hit_count,
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
    alternate_transcript: Option<&str>,
    cfg: &store::PipelineConfig,
    profile: &str,
    extra_rules: &str,
    app_context: Option<&str>,
    app: Option<&AppHandle>,
    gen: u64,
) -> (Option<CleanupSuccess>, Option<anyhow::Error>, bool) {
    let mut last_cleanup_err: Option<anyhow::Error> = None;
    let mut saw_soft_timeout = false;
    for (provider_id, model) in cleanup_model_chain(cfg) {
        let is_local = provider_id == store::LOCAL;
        let key = cfg.key_for(&provider_id).to_owned();
        if key.is_empty() && !is_local {
            continue;
        }
        let attempts = if is_local { 1 } else { CLEANUP_FAST_ATTEMPTS };
        for attempt in 1..=attempts {
            let custom_template = cfg.cleanup_override();
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
                    alternate_transcript,
                )
                .await
            } else {
                let cp = ProviderId::from_str(&provider_id);
                match tokio::time::timeout(
                    std::time::Duration::from_secs(CLEANUP_FAST_ATTEMPT_TIMEOUT_SECS),
                    cleanup::cleanup_with_alternate(
                        expanded,
                        cp,
                        &key,
                        &model,
                        profile,
                        &cfg.cleanup_intensity,
                        extra_rules,
                        app_context,
                        custom_template,
                        alternate_transcript,
                        gen,
                    ),
                )
                .await
                {
                    Ok(result) => result,
                    Err(_) => Err(cleanup_soft_timeout_error(&provider_id, &model)),
                }
            };
            match outcome {
                Ok(cleaned) if !cleaned.is_empty() => {
                    log::debug!(
                        "pipeline: cleanup provider success gen={} provider={} model={} attempt={} cleaned_chars={}",
                        gen,
                        provider_id,
                        model,
                        attempt,
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
                        false,
                    );
                }
                Ok(_) => {
                    last_cleanup_err = None;
                    break;
                }
                Err(e) => {
                    let retryable =
                        crate::api::is_retryable_provider_error(&e) || is_cleanup_soft_timeout(&e);
                    log::warn!(
                        "pipeline: cleanup provider failed gen={} provider={} model={} attempt={}/{} retryable={} error={}",
                        gen,
                        provider_id,
                        model,
                        attempt,
                        attempts,
                        retryable,
                        trim_err(&e.to_string())
                    );
                    // A real provider error should move to the configured
                    // fallback immediately. Only a silent stall gets the
                    // same-provider retry, because the second connection is
                    // often healthy even though the first one wedged.
                    let should_retry = is_cleanup_soft_timeout(&e) && attempt < attempts;
                    saw_soft_timeout |= is_cleanup_soft_timeout(&e);
                    last_cleanup_err = Some(e);
                    if should_retry {
                        continue;
                    }
                    break;
                }
            }
        }
    }

    (None, last_cleanup_err, saw_soft_timeout)
}

// Handles snippet fast-path, snippet instruction collection, LLM cleanup, and
// instruction override application. Returns (final_text_before_dict,
// dictionary entries, cache key, cleanup provider/model metadata).
#[allow(clippy::too_many_arguments)]
pub(super) async fn run_cleanup_and_snippets(
    app: &AppHandle,
    raw: &str,
    alternate: Option<&TranscriptCandidate>,
    cfg: &store::PipelineConfig,
    profile: &str,
    app_context: Option<&str>,
    context_id: i64,
    protected_instruction: Option<&str>,
    gen: u64,
) -> Option<(String, Vec<db::DictionaryEntry>, String, String)> {
    let db_handle = app.state::<DbHandle>();
    match run_cleanup_and_snippets_for_db(
        db_handle.inner(),
        raw,
        alternate,
        cfg,
        profile,
        app_context,
        context_id,
        protected_instruction,
        Some(app),
        gen,
    )
    .await
    {
        Ok(result) => Some(result),
        Err(e) => {
            let user_msg = format!("Cleanup failed: {}", crate::api::user_facing_error(&e));
            log::error!(
                "pipeline: cleanup failed gen={} error={}",
                gen,
                trim_err(&e.to_string())
            );
            if crate::api::is_retryable_provider_error(&e) {
                emit_provider_recheck(app);
            }
            show_error_pill(app, &user_msg).await;
            None
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn run_cleanup_and_snippets_for_db(
    db_handle: &DbHandle,
    raw: &str,
    alternate: Option<&TranscriptCandidate>,
    cfg: &store::PipelineConfig,
    profile: &str,
    app_context: Option<&str>,
    context_id: i64,
    protected_instruction: Option<&str>,
    app: Option<&AppHandle>,
    gen: u64,
) -> anyhow::Result<(String, Vec<db::DictionaryEntry>, String, String)> {
    let mut db_snippets = db::query_snippets_for_context(db_handle, context_id).unwrap_or_default();
    let dict_entries = db::query_dictionary_for_context(db_handle, context_id).unwrap_or_default();
    log::debug!(
        "pipeline: cleanup inputs gen={} snippets={} dict_entries={}",
        gen,
        db_snippets.len(),
        dict_entries.len()
    );

    let snippet_instructions = snippets::collect_snippet_instructions_from(raw, &db_snippets);
    log::debug!(
        "pipeline: cleanup stage start gen={} raw_chars={} snippet_override_lines={} cleanup_enabled={}",
        gen,
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

    let dict_instructions = dictionary::build_relevant_dictionary_prompt_from(&dict_entries, raw);
    let context_custom_instructions = db::query_context(db_handle, context_id)
        .ok()
        .and_then(|c| c.custom_instructions);
    let extra_rules = [
        snippet_instructions.as_str(),
        dict_instructions.as_str(),
        context_custom_instructions.as_deref().unwrap_or(""),
        protected_instruction.unwrap_or(""),
    ]
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

    let mut used_cache_key = String::new();
    let mut cleanup_api_used = String::new();
    let final_text = if should_run_cleanup_llm(
        cfg.cleanup_enabled,
        has_cleanup_key_in_chain(cfg),
        pure_expansion.is_none(),
        &cfg.cleanup_intensity,
        profile,
    ) {
        let cache_plan = cleanup_cache_plan_for_context(
            &expanded,
            profile,
            &cfg.cleanup_intensity,
            &snippet_instructions,
            alternate.map(|candidate| candidate.text.as_str()),
            alternate
                .as_ref()
                .map(|_| dual_cleanup_context_fingerprint(cfg, &extra_rules, app_context)),
            Some(context_id),
        );
        // Protected clipboard payloads are unique per invocation and must not
        // reuse or populate the cleanup cache, even though the marker itself
        // is intentionally stable enough to be safe in the prompt.
        let cache_key = if protected_instruction.is_some() {
            String::new()
        } else {
            cache_plan.key.clone()
        };
        if protected_instruction.is_none() && !cache_key.is_empty() {
            used_cache_key = cache_key.clone();
            if let Some(overridden) = cleanup_cache_hit_text(
                db_handle,
                &cache_key,
                profile,
                &cfg.cleanup_intensity,
                &snippet_instructions,
            ) {
                return Ok((
                    overridden,
                    dict_entries,
                    cache_key,
                    configured_cleanup_api_used(cfg),
                ));
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
        let (cleanup_res, last_cleanup_err, saw_soft_timeout) = run_cleanup_provider_chain(
            &expanded,
            alternate.map(|candidate| candidate.text.as_str()),
            cfg,
            profile,
            &extra_rules,
            app_context,
            app,
            gen,
        )
        .await;
        let provider_succeeded = cleanup_res.is_some();
        if let Some(success) = cleanup_res.as_ref() {
            cleanup_api_used = format!("{}/{}", success.provider_id, success.model);
        }
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
                    alternate.map(|candidate| candidate.text.as_str()),
                    gen,
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
                // Mechanical backstop for "light" intensity: the prompt
                // already tells every model to remove filler/hesitation
                // words, but small local models apply that rule
                // unreliably — observed in practice: one "um" correctly
                // stripped while others survived untouched in the same
                // output. Deterministic removal guarantees these are gone
                // regardless of model behavior, unlike "like"/"you know"
                // which have legitimate non-filler meanings and stay
                // entirely up to the model.
                let cleaned = if cfg.cleanup_intensity == "light" {
                    crate::system::text::strip_filler_hesitations(&cleaned)
                } else {
                    cleaned
                };
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
            None if !provider_succeeded && saw_soft_timeout => {
                log::warn!("pipeline: cleanup stalled twice, delivering pre-cleanup transcription");
                cleanup_api_used.clear();
                let text =
                    snippets::apply_cleanup_instruction_overrides(&expanded, &snippet_instructions);
                if cfg.cleanup_intensity != "none" {
                    crate::system::text::strip_filler_hesitations(&text)
                } else {
                    text
                }
            }
            None if !provider_succeeded && last_cleanup_err.is_some() => {
                return Err(last_cleanup_err.expect("checked"))
            }
            None => {
                cleanup_api_used.clear();
                let text =
                    snippets::apply_cleanup_instruction_overrides(&expanded, &snippet_instructions);
                if cfg.cleanup_intensity != "none" {
                    crate::system::text::strip_filler_hesitations(&text)
                } else {
                    text
                }
            }
        }
    } else {
        let text = snippets::apply_cleanup_instruction_overrides(&expanded, &snippet_instructions);
        if cfg.cleanup_intensity != "none" {
            crate::system::text::strip_filler_hesitations(&text)
        } else {
            text
        }
    };

    Ok((final_text, dict_entries, used_cache_key, cleanup_api_used))
}

fn configured_cleanup_api_used(cfg: &store::PipelineConfig) -> String {
    cleanup_model_chain(cfg)
        .into_iter()
        .find(|(provider, _)| provider == store::LOCAL || !cfg.key_for(provider).is_empty())
        .map(|(provider, model)| format!("{provider}/{model}"))
        .unwrap_or_default()
}
