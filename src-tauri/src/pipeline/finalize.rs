use super::*;

pub(super) struct PipelineCompletionContext<'a> {
    pub(super) raw: &'a str,
    pub(super) final_text_before_dict: &'a str,
    pub(super) dict_entries: &'a [db::DictionaryEntry],
    pub(super) duration_ms: u64,
    pub(super) api_used: &'a str,
    pub(super) target_hwnd: usize,
    pub(super) cfg: &'a store::PipelineConfig,
    pub(super) profile: &'a str,
    pub(super) process_name: String,
    pub(super) cleanup_cache_key: String,
    pub(super) captured_at: std::time::Instant,
    pub(super) event_only: bool,
    pub(super) caps_lock_on: bool,
}

pub(super) async fn finalize_pipeline_completion(
    app: &AppHandle,
    state: &SharedState,
    ctx: PipelineCompletionContext<'_>,
) -> anyhow::Result<db::RecentEntry> {
    let dict_stage = std::time::Instant::now();
    let (final_text_substituted, applied_dict_ids) =
        dictionary::apply_substitutions_from(ctx.final_text_before_dict, ctx.dict_entries);
    let dict_changed = !applied_dict_ids.is_empty();
    let apply_caps_lock_upper = ctx.cfg.caps_lock_uppercase_enabled && ctx.caps_lock_on;
    // Logged unconditionally (not just when applied) so a report of
    // unexpected ALL CAPS output is actually diagnosable: did the setting
    // fire on a stale/incorrect caps_lock_on read, or is the uppercasing
    // coming from somewhere else entirely (e.g. the cleanup model itself)?
    log::debug!(
        "pipeline: caps lock uppercase setting_enabled={} detected_caps_lock_on={} applied={}",
        ctx.cfg.caps_lock_uppercase_enabled,
        ctx.caps_lock_on,
        apply_caps_lock_upper
    );
    let final_text_substituted = if apply_caps_lock_upper {
        final_text_substituted.to_uppercase()
    } else {
        final_text_substituted
    };
    log::debug!(
        "pipeline: dictionary apply changed={} before_chars={} after_chars={} stage_ms={}",
        dict_changed,
        ctx.final_text_before_dict.chars().count(),
        final_text_substituted.chars().count(),
        dict_stage.elapsed().as_millis()
    );
    if dict_changed && crate::system::logger::is_verbose() {
        log::debug!(
            "pipeline: dictionary before_full=\"{}\"",
            ctx.final_text_before_dict
        );
        log::debug!(
            "pipeline: dictionary after_full=\"{}\"",
            final_text_substituted
        );
    }

    let db_handle = app.state::<DbHandle>();
    let words = ctx.raw.split_whitespace().count() as i64;
    let db_for_insert = db_handle.inner().clone();
    let raw_for_insert = ctx.raw.to_string();
    // History's per-entry app metadata is the lowercase executable name. The
    // "unknown" fallback from window_context carries no signal (failed read /
    // lost foreground), so it is stored as NULL and simply has no app.
    let app_name_for_insert = (!ctx.process_name.is_empty() && ctx.process_name != "unknown")
        .then(|| ctx.process_name.clone());
    // `final_text_substituted` already has caps-lock uppercasing AND
    // dictionary substitution applied (see above) — it's the same text that
    // gets injected below. History must save exactly that, not
    // `final_text_before_dict`: saving the pre-dictionary text here meant a
    // dictionary correction could be applied correctly to what actually got
    // pasted while History silently kept showing the uncorrected version
    // forever, which is also what misled diagnosis of this exact bug.
    let clean_for_insert = final_text_substituted.clone();
    let api_used_for_insert = ctx.api_used.to_string();
    let duration_for_insert = ctx.duration_ms as i64;
    let dictionary_fixes_applied = applied_dict_ids.len() as i64;
    let entry = match tokio::task::spawn_blocking(move || -> anyhow::Result<db::RecentEntry> {
        let entry = db::insert_transcription_returning(
            &db_for_insert,
            &raw_for_insert,
            &clean_for_insert,
            words,
            duration_for_insert,
            &api_used_for_insert,
            app_name_for_insert.as_deref(),
        )?;
        if dictionary_fixes_applied > 0 {
            // Lifetime counter for the Insights "fixes made by Verenu" card;
            // same never-recomputed pattern as total_words. Best-effort —
            // a failed counter must not fail the dictation itself.
            if let Err(e) = db::increment_lifetime_dictionary_fixes(
                &db_for_insert,
                dictionary_fixes_applied,
            ) {
                log::warn!("pipeline: failed to record dictionary fixes: {e}");
            }
        }
        Ok(entry)
    })
    .await
    {
        Ok(Ok(entry)) => entry,
        Ok(Err(e)) => {
            show_error_pill(
                app,
                &format!("Failed to save transcription: {}", trim_err(&e.to_string())),
            )
            .await;
            return Err(e);
        }
        Err(e) => {
            show_error_pill(app, "Failed to save transcription: background task crashed").await;
            return Err(anyhow::anyhow!("insert_transcription task panicked: {e}"));
        }
    };

    // Per-call API usage records for the Insights cost card. Best-effort:
    // a failed audit row must never fail the dictation that already
    // inserted fine above. Only counts and model ids are ever logged.
    let calls = db::build_api_calls(
        entry.id,
        &entry.created_at,
        ctx.api_used,
        ctx.duration_ms as i64,
        ctx.raw,
        ctx.final_text_before_dict,
    );
    if !calls.is_empty() {
        let db_for_calls = db_handle.inner().clone();
        match tokio::task::spawn_blocking(move || db::insert_api_calls(&db_for_calls, &calls)).await
        {
            Ok(Ok(())) => {}
            Ok(Err(e)) => {
                log::warn!("pipeline: failed to record api usage rows: {}", trim_err(&e.to_string()))
            }
            Err(e) => log::warn!("pipeline: api usage recording task panicked: {e}"),
        }
    }

    if let Ok(mut st) = lock_state(state) {
        if st.retry_capture.as_ref().map(|v| v.captured_at) == Some(ctx.captured_at) {
            st.retry_capture = None;
        }
    }

    let inject_stage = std::time::Instant::now();

    // If Verenu itself has foreground focus, a Ctrl+V / Cmd+V paste would
    // land in our own WebView with no active text field and silently disappear.
    // Detect this by PID and fall back to clipboard-only so the user can paste manually.
    let self_inject = foreground_is_own_process() || hwnd_is_own_process(ctx.target_hwnd);
    log::debug!(
        "pipeline: injection decision target_hwnd={} event_only={} self_inject={}",
        ctx.target_hwnd,
        ctx.event_only,
        self_inject
    );

    let injected = if ctx.event_only {
        injection::InjectionOutcome {
            text: final_text_substituted.clone(),
            context_state: "event_only",
            case_decision: "setup_try_event",
            probe_source: "unavailable",
            selection_state: "unknown",
        }
    } else if self_inject {
        log::info!("pipeline: self-inject detected — clipboard fallback");
        if let Err(e) = injection::copy_to_clipboard(&final_text_substituted).await {
            log::warn!("pipeline: clipboard fallback write failed: {e}");
        }
        app.emit("verenu:error", "Text copied — press Ctrl+V to paste")
            .ok();
        injection::InjectionOutcome {
            text: final_text_substituted.clone(),
            context_state: "self_inject",
            case_decision: "clipboard_fallback",
            probe_source: "unavailable",
            selection_state: "unknown",
        }
    } else {
        match injection::inject_text(
            &final_text_substituted,
            ctx.target_hwnd,
            // Caps lock must be the final word on casing: contextual capitalization
            // (e.g. lowercasing a continuation's first letter) would otherwise run
            // on top of the all-caps text and undo part of it.
            ctx.cfg.contextual_caps_enabled && !apply_caps_lock_upper,
            ctx.cfg.auto_spacing_enabled,
            ctx.profile,
            ctx.cfg.macos_clipboard_sniff_enabled,
        )
        .await
        {
            Ok(outcome) => outcome,
            Err(e) => {
                log::error!("inject: {e}");
                if let Ok(mut st) = lock_state(state) {
                    st.paste_failure = Some(PasteFailure {
                        text: final_text_substituted.clone(),
                        captured_at: std::time::Instant::now(),
                    });
                }
                show_paste_failed_pill(app);
                injection::InjectionOutcome {
                    text: final_text_substituted.clone(),
                    context_state: "unknown",
                    case_decision: "inject_failed",
                    probe_source: "unavailable",
                    selection_state: "unknown",
                }
            }
        }
    };
    let injected_text = injected.text;
    log::debug!(
        "pipeline: delivery done contextual_caps={} auto_spacing={} context_state={} case_decision={} probe_source={} selection_state={} output_chars={} stage_ms={}",
        ctx.cfg.contextual_caps_enabled,
        ctx.cfg.auto_spacing_enabled,
        injected.context_state,
        injected.case_decision,
        injected.probe_source,
        injected.selection_state,
        injected_text.chars().count(),
        inject_stage.elapsed().as_millis()
    );
    app.emit("verenu:transcribed", &injected_text).ok();

    if ctx.event_only {
        hide_pill(app);
        return Ok(entry);
    }

    // Don't stomp the "Paste failed" pill (with its Copy button) that
    // show_paste_failed_pill just showed a few lines up — hide_pill would
    // instantly revert it to idle before the button even has a chance to
    // fade in, let alone be clicked.
    if injected.case_decision != "inject_failed" {
        hide_pill(app);
    } else {
        log::debug!("pipeline: skipping hide_pill — paste_failed pill stays up");
    }

    if !ctx.cleanup_cache_key.is_empty() {
        auto_learn::start_cache_rejection_monitor(
            injected_text.clone(),
            ctx.cleanup_cache_key,
            ctx.target_hwnd,
            db_handle.inner().clone(),
            app.clone(),
        );
    }
    if ctx.cfg.auto_learn_enabled {
        if !applied_dict_ids.is_empty() {
            auto_learn::start_rejection_monitor(
                injected_text.clone(),
                applied_dict_ids,
                ctx.target_hwnd,
                db_handle.inner().clone(),
                app.clone(),
            );
        }
        auto_learn::start_monitor(
            injected_text,
            ctx.process_name,
            db_handle.inner().clone(),
            app.clone(),
        );
    }

    Ok(entry)
}
