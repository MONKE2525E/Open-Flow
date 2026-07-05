use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex, MutexGuard};
use tauri::{AppHandle, Emitter, Manager};

use crate::DbHandle;
use crate::api::{ProviderId, auto_learn, cleanup, prompts, transcription};
use crate::core::{injection, window_context};
use crate::data::{db, dictionary, snippets, store};
use crate::media::audio;
use crate::system::apps::AppMapping;
use crate::system::number_parser;
use crate::system::text::is_number_word_token;
use chrono::{DateTime, Duration, NaiveDateTime, SecondsFormat, Utc};

mod cache;
mod chains;
mod finalize;
#[cfg(any(test, debug_assertions))]
mod fixture;
mod gates;
mod pill;
mod pill_animation;
mod pill_position;
mod session;
mod stages;
mod state;
use cache::*;
use chains::*;
use finalize::{PipelineCompletionContext, finalize_pipeline_completion};
#[cfg(any(test, debug_assertions))]
#[allow(unused_imports)]
pub use fixture::{
    PipelineTestDictionaryEntry, PipelineTestRequest, PipelineTestResult, PipelineTestSnippet,
    run_pipeline_fixture,
};
use gates::{
    MIN_RECORDING_MS, MIN_RECORDING_RMS, is_transcription_hallucination,
    normalize_transcription_math_artifacts, preview_text, recording_gate_rms,
    strip_hallucinated_suffix,
};
pub(crate) use pill::{hide_pill, show_pill};
use pill::{reject_with_pill, show_error_pill};
pub use session::*;
use stages::*;
pub use state::*;

#[derive(Clone, Debug)]
pub struct CapturedAudio {
    pub wav: bytes::Bytes,
    pub samples_16k: Arc<Vec<f32>>,
    pub sample_rate: u32,
    pub duration_ms: u64,
}

// ---------- pipeline ----------

pub async fn transcribe_input_only(app: AppHandle, state: SharedState) -> anyhow::Result<String> {
    let session = {
        let mut st = lock_state(&state)?;
        let session = st.session.take();
        let exclusive_mic_session_id = st.exclusive_mic_session_id.take();
        (session, exclusive_mic_session_id)
    };
    let (session, exclusive_mic_session_id) = session;
    let Some(session) = session else {
        if let Some(session_id) = exclusive_mic_session_id {
            tokio::task::spawn_blocking(move || crate::system::volume::release_mic(session_id));
        }
        anyhow::bail!("No active recording");
    };

    let _media_pause_guard = crate::system::media_control::DictationMediaPauseGuard::new();
    crate::media::sound::coordinated_unmute();
    show_pill(&app, "processing");

    let settings_store = match store::settings_snapshot(&app) {
        Ok(s) => s,
        Err(e) => {
            hide_pill(&app);
            return Err(anyhow::anyhow!(e));
        }
    };
    let active_gain = store::load_audio_config(&settings_store).mic_gain;
    let min_rms = recording_gate_rms(active_gain);
    log::debug!("pipeline: input gate active_gain={active_gain:.2} min_rms={min_rms:.6}");

    let stop_result = tokio::task::spawn_blocking(move || {
        let stop_result = session.stop();
        if let Some(session_id) = exclusive_mic_session_id {
            crate::system::volume::release_mic(session_id);
        }
        stop_result
    })
    .await?;
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
        anyhow::bail!("Audio too quiet — check your mic");
    }
    let captured_audio = CapturedAudio {
        wav: bytes::Bytes::from(wav),
        samples_16k: Arc::new(samples_16k),
        sample_rate,
        duration_ms,
    };

    let cfg = store::load_pipeline_config(&settings_store);

    if let Err(message) = validate_transcription_chain(&cfg, None) {
        hide_pill(&app);
        anyhow::bail!(message);
    }

    let mut transcribed: Option<String> = None;
    let mut last_err: Option<anyhow::Error> = None;
    for (provider_id, model) in transcription_model_chain(&cfg) {
        let language = cfg.transcription_language.clone();
        let key = cfg.key_for(&provider_id).to_owned();
        match transcribe_any(
            &app,
            &captured_audio,
            &provider_id,
            if key.is_empty() { None } else { Some(key.as_str()) },
            &language,
            &model,
        )
        .await
        {
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
        Some(text) => Ok(crate::system::text::collapse_degenerate_word_runs(&text)),
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
    let Some((session, target, exclusive_mic_session_id)) = take_pipeline_session(&state) else {
        log::debug!("pipeline: no session - recording never started or was already consumed");
        return;
    };
    let Some(session) = session else {
        if let Some(session_id) = exclusive_mic_session_id {
            tokio::task::spawn_blocking(move || crate::system::volume::release_mic(session_id));
        }
        log::debug!("pipeline: no session - recording never started or was already consumed");
        return;
    };
    let _media_pause_guard = crate::system::media_control::DictationMediaPauseGuard::new();

    // Read once, synchronously, as close to the hotkey-release moment as
    // possible — the rest of the pipeline is async and the user may keep
    // typing (toggling Caps Lock) while it runs.
    let caps_lock_on = crate::core::hotkey::caps_lock_is_on();

    // Resolve the app identity from the window text will actually be injected
    // into (captured at record-start), not the live foreground at release — the
    // two can diverge (handsfree, focus shifts) and that divergence let one
    // app's mapping style leak into another app. Issue #144. Falls back to the
    // live foreground only when the captured target id is unavailable (null/0).
    let process_name = window_context::get_process_name_for_hwnd(target.id)
        .or_else(window_context::get_active_process_name)
        .unwrap_or_else(|| "unknown".into())
        .to_lowercase();
    log::info!("pipeline: start target_id={}", target.id);

    // Mark the session inactive before unmuting or waiting on stop() so the
    // delayed mute helper cannot wake up and re-mute the system mid-shutdown.
    session.active.store(false, Ordering::Relaxed);
    crate::media::sound::cancel_pending_start();
    crate::media::sound::coordinated_unmute();
    show_pill(&app, "processing");

    // Keep the quiet-audio gate permissive at high gain. Whisper recordings can
    // still have low post-denoise RMS, even after amplification.
    let audio_cfg = match store::settings_snapshot(&app) {
        Ok(s) => store::load_audio_config(&s),
        Err(e) => {
            log::warn!("pipeline: failed to load audio config, using defaults: {e}");
            store::AudioConfig::default()
        }
    };
    let active_gain = audio_cfg.mic_gain;
    let min_rms = recording_gate_rms(active_gain);
    log::debug!("pipeline: audio gate active_gain={active_gain:.2} min_rms={min_rms:.6}");

    let stage_audio = std::time::Instant::now();
    // Quiet/short recordings are rejected inside stop_and_validate_audio, which
    // plays the error cue via reject_with_pill — so only play the stop cue once
    // the audio is *accepted*, otherwise a rejection would sound stop + error.
    let Some(captured_audio) =
        stop_and_validate_audio(&app, session, exclusive_mic_session_id, min_rms).await
    else {
        return;
    };
    if audio_cfg.play_start_stop_sounds {
        crate::media::sound::play(crate::media::sound::SoundCue::Stop);
    }
    log::debug!(
        "pipeline: audio accepted duration_ms={} wav_bytes={} stage_ms={}",
        captured_audio.duration_ms,
        captured_audio.wav.len(),
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
        "pipeline: context resolved app_context_present={} stage_ms={}",
        app_context.is_some(),
        stage_config.elapsed().as_millis()
    );

    let retry_captured_at = std::time::Instant::now();
    if let Ok(mut st) = lock_state(&state) {
        st.retry_capture = Some(RetryCapture {
            audio: captured_audio.clone(),
            captured_at: retry_captured_at,
            target,
            process_name: process_name.clone(),
            profile: profile.clone(),
            app_context: app_context.clone(),
            caps_lock_on,
        });
    }

    let stage_transcribe = std::time::Instant::now();
    let Some((raw_unorm, api_used)) = run_transcription(&app, &captured_audio, &cfg).await else {
        emit_pipeline_failed(&app);
        return;
    };
    let raw = normalize_transcription_math_artifacts(&raw_unorm);
    let raw_chars_before_strip = raw.chars().count();
    let raw = strip_hallucinated_suffix(&raw);
    if raw.chars().count() != raw_chars_before_strip {
        log::warn!(
            "pipeline: trimmed trailing hallucination provider={} chars_before={} chars_after={}",
            api_used,
            raw_chars_before_strip,
            raw.chars().count()
        );
    }
    let raw_chars_before_collapse = raw.chars().count();
    let raw = crate::system::text::collapse_degenerate_word_runs(&raw);
    if raw.chars().count() != raw_chars_before_collapse {
        log::warn!(
            "pipeline: collapsed degenerate word run provider={} chars_before={} chars_after={}",
            api_used,
            raw_chars_before_collapse,
            raw.chars().count()
        );
    }
    log::debug!(
        "pipeline: transcription ok provider={} raw_chars={} raw_preview=\"{}\"",
        api_used,
        raw.chars().count(),
        preview_text(&raw, 140)
    );
    // Diagnostic only, no behavioral effect — counts and a ratio, never the
    // text. Average conversational speech runs ~2-3 words/sec; a ratio well
    // under that on a recording long enough to judge reliably (rules out
    // pauses/silence at the start dominating a short clip) is a signal worth
    // having on hand if a future report of "words missing" turns out to be
    // the transcription itself dropping content rather than cleanup, which
    // today has no equivalent completeness check of its own.
    let raw_words = raw.split_whitespace().count();
    if captured_audio.duration_ms >= 3000 {
        let words_per_sec = raw_words as f64 / (captured_audio.duration_ms as f64 / 1000.0);
        log::debug!(
            "pipeline: transcription completeness words={} duration_ms={} words_per_sec={:.2}",
            raw_words,
            captured_audio.duration_ms,
            words_per_sec
        );
    }
    log::debug!(
        "pipeline: transcription stage_ms={}",
        stage_transcribe.elapsed().as_millis()
    );

    // Post-transcription hallucination gate — silently drop prompt-echoes and
    // known silent-audio artifacts before they reach cleanup or the cache.
    // (A trailing hallucinated sentence has already been trimmed above; this
    // catches the case where the whole transcription is still one.)
    if raw.is_empty() || is_transcription_hallucination(&raw) {
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
            duration_ms: captured_audio.duration_ms,
            api_used: &api_used,
            target_hwnd: target.id,
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
        captured_audio.duration_ms,
        started_at.elapsed().as_millis()
    );
}

#[cfg(test)]
mod tests;

pub async fn retry_transcription_impl(
    app: &AppHandle,
    state: &SharedState,
) -> anyhow::Result<db::RecentEntry> {
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

    capture.target = capture.target.refreshed();
    if let Ok(mut st) = lock_state(state) {
        st.target = capture.target;
        st.pill_placement_stale = true;
    }
    show_pill(app, "processing");

    let settings_store = store::settings_snapshot(app).map_err(anyhow::Error::msg)?;
    let mut cfg = store::load_pipeline_config(&settings_store);

    if let Err(message) = validate_transcription_chain(&cfg, None) {
        show_error_pill(app, &message).await;
        anyhow::bail!(message);
    }

    let mapping = resolve_app_mapping(Some(&settings_store), &capture.process_name);
    capture.profile = apply_app_style_overrides(&mut cfg, mapping.as_ref());

    let Some((raw_unorm, api_used)) = run_transcription(app, &capture.audio, &cfg).await else {
        anyhow::bail!("Retry transcription failed");
    };
    let raw = normalize_transcription_math_artifacts(&raw_unorm);
    let raw = strip_hallucinated_suffix(&raw);
    let raw = crate::system::text::collapse_degenerate_word_runs(&raw);
    if raw.is_empty() || is_transcription_hallucination(&raw) {
        log::warn!(
            "pipeline: retry transcription matched hallucination pattern, dropping raw=\"{}\"",
            preview_text(&raw, 60)
        );
        anyhow::bail!("Recording was too quiet — nothing was transcribed");
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
            duration_ms: capture.audio.duration_ms,
            api_used: &api_used,
            target_hwnd: capture.target.id,
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
