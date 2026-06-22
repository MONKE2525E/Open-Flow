use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex, MutexGuard};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_store::StoreExt;

use crate::api::{auto_learn, cleanup, prompts, transcription};
use crate::core::{injection, window_context};
use crate::data::{db, dictionary, snippets, store};
use crate::media::audio;
use crate::system::apps::AppMapping;
use crate::system::number_parser;
use crate::system::text::is_number_word_token;
use crate::DbHandle;
use chrono::{DateTime, Duration, NaiveDateTime, SecondsFormat, Utc};

mod gates;
mod pill;
mod finalize;
#[cfg(any(test, debug_assertions))]
mod fixture;
mod cache;
mod chains;
mod session;
mod stages;
mod state;
use gates::{
    is_transcription_hallucination, normalize_transcription_math_artifacts, preview_text,
    recording_gate_rms, MIN_RECORDING_MS, MIN_RECORDING_RMS,
};
pub(crate) use pill::{hide_pill, show_pill};
use pill::{reject_with_pill, show_error_pill};
use finalize::{finalize_pipeline_completion, PipelineCompletionContext};
use cache::*;
use chains::*;
use stages::*;
pub use session::*;
pub use state::*;
#[cfg(any(test, debug_assertions))]
#[allow(unused_imports)]
pub use fixture::{
    run_pipeline_fixture, PipelineTestDictionaryEntry, PipelineTestRequest,
    PipelineTestResult, PipelineTestSnippet,
};






// ---------- pipeline ----------




pub async fn transcribe_input_only(app: AppHandle, state: SharedState) -> anyhow::Result<String> {
    let session = {
        let mut st = lock_state(&state)?;
        st.session.take()
    };
    let Some(session) = session else {
        anyhow::bail!("No active recording");
    };

    std::thread::spawn(crate::system::volume::unmute);
    show_pill(&app, "processing");

    let settings_store = match app.store("settings.json") {
        Ok(s) => s,
        Err(e) => {
            hide_pill(&app);
            return Err(anyhow::anyhow!(e.to_string()));
        }
    };
    let active_gain = store::load_audio_config(&settings_store).mic_gain;
    let min_rms = recording_gate_rms(active_gain);
    log::debug!("pipeline: input gate active_gain={active_gain:.2} min_rms={min_rms:.6}");

    let stop_result = tokio::task::spawn_blocking(move || session.stop()).await?;
    let audio::RecordingResult {
        wav,
        duration_ms,
        rms,
        truncated,
    } = stop_result?;

    if truncated {
        hide_pill(&app);
        anyhow::bail!(
            "Recording exceeded the {} minute limit. Please split it into shorter dictations.",
            audio::MAX_RECORDING_SECONDS / 60
        );
    }

    if duration_ms < MIN_RECORDING_MS || rms < min_rms {
        hide_pill(&app);
        if duration_ms < MIN_RECORDING_MS {
            anyhow::bail!("Recording too short");
        }
        anyhow::bail!("Audio too quiet Ã¢â‚¬â€ check your mic");
    }
    let wav = bytes::Bytes::from(wav);

    let cfg = store::load_pipeline_config(&settings_store);

    if !has_transcription_key_in_chain(&cfg) {
        hide_pill(&app);
        anyhow::bail!("No API key configured for any model in the transcription chain");
    }

    let mut transcribed: Option<String> = None;
    let mut last_err: Option<anyhow::Error> = None;
    for (provider_id, model) in transcription_model_chain(&cfg) {
        let key = cfg.key_for(&provider_id).to_owned();
        if key.is_empty() {
            continue;
        }
        let provider = transcription_provider_from_str(&provider_id);
        let language = cfg.transcription_language.clone();
        match transcription::transcribe(wav.clone(), provider, &key, &language, &model).await {
            Ok(text) if !text.is_empty() => {
                transcribed = Some(text);
                break;
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

    hide_pill(&app);

    match transcribed {
        Some(text) => Ok(text),
        None => Err(last_err.unwrap_or_else(|| {
            anyhow::anyhow!("Transcription failed: no model in chain produced output")
        })),
    }
}

pub async fn run_pipeline(app: AppHandle, state: SharedState) {
    run_pipeline_with_delivery(app, state, false).await;
}

pub async fn run_pipeline_event_only(app: AppHandle, state: SharedState) {
    run_pipeline_with_delivery(app, state, true).await;
}

async fn run_pipeline_with_delivery(app: AppHandle, state: SharedState, event_only: bool) {
    let started_at = std::time::Instant::now();
    let Some((session, target_hwnd)) = take_pipeline_session(&state) else {
        log::debug!("pipeline: no session - recording never started or was already consumed");
        return;
    };

    // Read once, synchronously, as close to the hotkey-release moment as
    // possible Ã¢â‚¬â€ the rest of the pipeline is async and the user may keep
    // typing (toggling Caps Lock) while it runs.
    let caps_lock_on = crate::core::hotkey::caps_lock_is_on();

    let process_name = window_context::get_active_process_name()
        .unwrap_or_else(|| "unknown".into())
        .to_lowercase();
    log::info!("pipeline: start process={process_name} target_hwnd={target_hwnd}");

    std::thread::spawn(crate::system::volume::unmute);
    show_pill(&app, "processing");

    // Keep the quiet-audio gate permissive at high gain. Whisper recordings can
    // still have low post-denoise RMS, even after amplification.
    let active_gain = match app.store("settings.json") {
        Ok(s) => store::load_audio_config(&s).mic_gain,
        Err(e) => {
            log::warn!("pipeline: failed to load audio config, using default gain: {e}");
            store::DEFAULT_MIC_GAIN
        }
    };
    let min_rms = recording_gate_rms(active_gain);
    log::debug!("pipeline: audio gate active_gain={active_gain:.2} min_rms={min_rms:.6}");

    let stage_audio = std::time::Instant::now();
    let Some((wav, duration_ms)) = stop_and_validate_audio(&app, session, min_rms).await else {
        return;
    };
    log::debug!(
        "pipeline: audio accepted duration_ms={duration_ms} wav_bytes={} stage_ms={}",
        wav.len(),
        stage_audio.elapsed().as_millis()
    );

    let stage_config = std::time::Instant::now();
    let Some((cfg, profile, app_context)) = open_config_and_context(&app, &process_name).await
    else {
        return;
    };
    log::debug!(
        "pipeline: config t_provider={} c_provider={} t_model={} c_model={} cleanup_enabled={} intensity={} app_context_hint={} profile={}",
        cfg.transcription_provider,
        cfg.cleanup_provider,
        cfg.transcription_default_model,
        cfg.cleanup_default_model,
        cfg.cleanup_enabled,
        cfg.cleanup_intensity,
        cfg.app_context_hint,
        profile
    );
    log::debug!(
        "pipeline: context resolved app_context={} stage_ms={}",
        app_context.as_deref().unwrap_or("none"),
        stage_config.elapsed().as_millis()
    );

    let retry_captured_at = std::time::Instant::now();
    if let Ok(mut st) = lock_state(&state) {
        st.retry_capture = Some(RetryCapture {
            wav: wav.clone(),
            captured_at: retry_captured_at,
            duration_ms,
            target_hwnd,
            process_name: process_name.clone(),
            profile: profile.clone(),
            app_context: app_context.clone(),
            caps_lock_on,
        });
    }

    let stage_transcribe = std::time::Instant::now();
    let Some((raw_unorm, api_used)) = run_transcription(&app, &wav, &cfg).await else {
        emit_pipeline_failed(&app);
        return;
    };
    let raw = normalize_transcription_math_artifacts(&raw_unorm);
    log::debug!(
        "pipeline: transcription ok provider={} raw_chars={} raw_preview=\"{}\"",
        api_used,
        raw.chars().count(),
        preview_text(&raw, 140)
    );
    if crate::system::logger::is_verbose() {
        log::debug!("pipeline: transcription raw_full=\"{}\"", raw);
    }
    log::debug!(
        "pipeline: transcription stage_ms={}",
        stage_transcribe.elapsed().as_millis()
    );

    // Post-transcription hallucination gate Ã¢â‚¬â€ silently drop prompt-echoes and
    // known silent-audio artifacts before they reach cleanup or the cache.
    if is_transcription_hallucination(&raw) {
        log::warn!(
            "pipeline: transcription matched hallucination pattern, dropping silently raw=\"{}\"",
            preview_text(&raw, 60)
        );
        hide_pill(&app);
        return;
    }

    let stage_cleanup = std::time::Instant::now();
    let Some((final_text, dict_entries, cleanup_cache_key)) =
        run_cleanup_and_snippets(&app, &raw, &cfg, &profile, app_context.as_deref()).await
    else {
        emit_pipeline_failed(&app);
        return;
    };
    log::debug!(
        "pipeline: cleanup/snippets ok final_chars={} final_preview=\"{}\" dict_entries={}",
        final_text.chars().count(),
        preview_text(&final_text, 140),
        dict_entries.len()
    );
    if crate::system::logger::is_verbose() {
        log::debug!("pipeline: final_text_full=\"{}\"", final_text);
    }
    log::debug!(
        "pipeline: cleanup stage_ms={}",
        stage_cleanup.elapsed().as_millis()
    );

    let words = raw.split_whitespace().count() as i64;
    if let Err(e) = finalize_pipeline_completion(
        &app,
        &state,
        PipelineCompletionContext {
            raw: &raw,
            final_text_before_dict: &final_text,
            dict_entries: &dict_entries,
            duration_ms,
            api_used: &api_used,
            target_hwnd,
            cfg: &cfg,
            profile: &profile,
            process_name,
            cleanup_cache_key,
            captured_at: retry_captured_at,
            event_only,
            caps_lock_on,
        },
    )
    .await
    {
        log::error!("pipeline finalize failed: {e}");
        return;
    }

    log::info!(
        "pipeline: completed words={} duration_ms={} elapsed_ms={}",
        words,
        duration_ms,
        started_at.elapsed().as_millis()
    );
}





#[cfg(test)]
mod tests;

pub async fn retry_transcription_impl(
    app: &AppHandle,
    state: &SharedState,
) -> anyhow::Result<db::RecentEntry> {
    show_pill(app, "processing");
    let mut retry_expired = false;
    let capture = {
        let mut st = lock_state(state)?;
        match &st.retry_capture {
            Some(retry) => {
                if retry.captured_at.elapsed() > RETRY_WINDOW {
                    st.retry_capture = None;
                    retry_expired = true;
                    None
                } else {
                    Some(retry.clone())
                }
            }
            None => None,
        }
    };
    if retry_expired {
        show_error_pill(app, "Retry window expired").await;
        anyhow::bail!("Retry window expired");
    }
    let Some(mut capture) = capture else {
        hide_pill(app);
        anyhow::bail!("No retry available");
    };

    let settings_store = app.store("settings.json")?;
    let mut cfg = store::load_pipeline_config(&settings_store);

    if !has_transcription_key_in_chain(&cfg) {
        show_error_pill(app, "No API key configured").await;
        anyhow::bail!("No API key configured");
    }

    let mapping = resolve_app_mapping(Some(&settings_store), &capture.process_name);
    capture.profile = apply_app_style_overrides(&mut cfg, mapping.as_ref());

    let Some((raw_unorm, api_used)) = run_transcription(app, &capture.wav, &cfg).await else {
        anyhow::bail!("Retry transcription failed");
    };
    let raw = normalize_transcription_math_artifacts(&raw_unorm);
    if is_transcription_hallucination(&raw) {
        log::warn!(
            "pipeline: retry transcription matched hallucination pattern, dropping raw=\"{}\"",
            preview_text(&raw, 60)
        );
        anyhow::bail!("Recording was too quiet Ã¢â‚¬â€ nothing was transcribed");
    }
    let Some((final_text, dict_entries, cleanup_cache_key)) = run_cleanup_and_snippets(
        app,
        &raw,
        &cfg,
        &capture.profile,
        capture.app_context.as_deref(),
    )
    .await
    else {
        anyhow::bail!("Retry cleanup failed");
    };

    finalize_pipeline_completion(
        app,
        state,
        PipelineCompletionContext {
            raw: &raw,
            final_text_before_dict: &final_text,
            dict_entries: &dict_entries,
            duration_ms: capture.duration_ms,
            api_used: &api_used,
            target_hwnd: capture.target_hwnd,
            cfg: &cfg,
            profile: &capture.profile,
            process_name: capture.process_name,
            cleanup_cache_key,
            captured_at: capture.captured_at,
            event_only: false,
            caps_lock_on: capture.caps_lock_on,
        },
    )
    .await
}