//! Audio capture handoff, quality-gate evaluation, and every transcription
//! path (single-chain, primary-chain, and dual-model candidates).

use super::stages_style::{apply_app_style_overrides, resolve_app_mapping};
use super::*;
// session.stop() blocks until the audio thread finishes (denoise + resample + WAV encode).
// spawn_blocking keeps the tokio worker free during that wait. Split from the
// quality gate (below) so a resumed/prepended recording can be merged first
// and validated once as a whole, instead of gating the (possibly very short)
// new fragment on its own before it's had a chance to be merged.
struct ExclusiveMicReleaseGuard(Option<u64>);

impl Drop for ExclusiveMicReleaseGuard {
    fn drop(&mut self) {
        if let Some(session_id) = self.0.take() {
            crate::system::volume::release_mic(session_id);
        }
    }
}

pub(super) async fn stop_and_capture_audio(
    app: &AppHandle,
    session: audio::RecordingSession,
    exclusive_mic_session_id: Option<u64>,
) -> Option<(CapturedAudio, f32, f32)> {
    let stop_result = tokio::task::spawn_blocking(move || {
        let _mic_release = ExclusiveMicReleaseGuard(exclusive_mic_session_id);
        session.stop()
    })
    .await;
    if let Some(manager) = app.try_state::<crate::local_stt::LocalTranscriptionManager>() {
        manager.set_recording_active(false);
    }
    let audio::RecordingResult {
        wav,
        samples_16k,
        sample_rate,
        duration_ms,
        rms,
        raw_rms,
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
    Some((
        CapturedAudio {
            wav: bytes::Bytes::from(wav),
            samples_16k: Arc::new(samples_16k),
            sample_rate,
            duration_ms,
        },
        rms,
        raw_rms,
    ))
}

/// Quality gate (min duration / near-silence RMS) against already-captured
/// (and possibly prepend-merged) audio. Shows the rejection pill itself.
pub(super) fn validate_captured_audio(
    app: &AppHandle,
    audio: &CapturedAudio,
    rms: f32,
    raw_rms: f32,
    min_rms: f32,
    active_gain: f32,
) -> bool {
    let gate_rms = effective_recording_rms(rms, raw_rms, active_gain);
    if audio.duration_ms < MIN_RECORDING_MS || gate_rms < min_rms {
        let msg = if audio.duration_ms < MIN_RECORDING_MS {
            "Recording too short"
        } else {
            "Audio too quiet — check your mic"
        };
        log::debug!(
            "pipeline: rejected — duration={}ms rms={rms:.4} gate_rms={gate_rms:.4} min_rms={min_rms:.4}",
            audio.duration_ms
        );
        reject_with_pill(app, msg);
        return false;
    }
    true
}

/// Speech-presence decision made before any transcription API call. VAD is
/// authoritative when available. RMS is only a fallback when VAD itself
/// failed to load or run (`vad_result: None`), so loud non-speech cannot bypass
/// a successful VAD rejection.
/// Shows the rejection pill itself, matching `validate_captured_audio`.
pub(super) fn passes_speech_gate(
    app: &AppHandle,
    rms: f32,
    raw_rms: f32,
    min_rms: f32,
    active_gain: f32,
    vad_result: Option<&crate::media::vad::SpeechDetectionResult>,
) -> bool {
    let gate_rms = effective_recording_rms(rms, raw_rms, active_gain);
    if speech_gate_accepts(vad_result, gate_rms, min_rms) {
        log::debug!(
            "pipeline: speech gate accepted gate_rms={gate_rms:.4} min_rms={min_rms:.4} vad={:?}",
            vad_result
        );
        return true;
    }
    log::debug!(
        "pipeline: speech gate rejected — no speech detected rms={rms:.4} min_rms={min_rms:.4} vad={:?}",
        vad_result
    );
    reject_with_pill(app, "No speech detected");
    false
}

/// Pure part of the speech gate, kept separate so the important VAD-versus-
/// RMS precedence is testable without constructing a Tauri app handle.
pub(super) fn speech_gate_accepts(
    vad_result: Option<&crate::media::vad::SpeechDetectionResult>,
    gate_rms: f32,
    min_rms: f32,
) -> bool {
    vad_result.map_or(gate_rms >= min_rms, |result| result.contains_speech)
}

pub(super) async fn open_config_and_context(
    app: &AppHandle,
    process_name: &str,
    target_id: usize,
    browser_domain: Option<&str>,
    context: Option<&db::Context>,
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
    let profile = apply_app_style_overrides(&mut cfg, mapping.as_ref(), context);
    log::debug!(
        "pipeline: app mapping resolved process={process_name} matched={} profile={profile} cleanup_intensity={} default_tone={}",
        mapping.as_ref().map(|m| m.exe.as_str()).unwrap_or("none"),
        cfg.cleanup_intensity,
        cfg.default_tone,
    );
    let app_context = if cfg.app_context_hint {
        window_context::get_app_context_hint(process_name, target_id, browser_domain)
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
    gen: u64,
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
        gen,
    )
    .await
}

/// Active connectivity disambiguation: returns true when the user's own
/// connection is down (not just one provider). Reads the Verenu service-check
/// preference so the probe honors "don't contact Verenu" — falling back to
/// google.com when Verenu checks are disabled, or when Verenu itself is the
/// thing that's unreachable.
async fn confirm_offline(app: &AppHandle) -> bool {
    let checks_enabled = store::settings_snapshot(app)
        .map(|s| {
            s.get(store::VERENU_SERVICE_CHECKS_ENABLED)
                .and_then(|v| v.as_bool())
                .unwrap_or(true)
        })
        .unwrap_or(true);
    crate::system::connectivity::confirm_offline(checks_enabled).await
}

pub(super) async fn run_transcription(
    app: &AppHandle,
    audio: &CapturedAudio,
    cfg: &store::PipelineConfig,
    gen: u64,
) -> Option<(String, String, Option<TranscriptCandidate>)> {
    log::debug!(
        "pipeline: transcription stage start gen={} provider={} model={} language={} wav_bytes={} pcm_samples={}",
        gen,
        cfg.transcription_provider,
        cfg.transcription_default_model,
        cfg.transcription_language,
        audio.wav.len(),
        audio.samples_16k.len()
    );

    let dual_enabled = cfg.dual_transcription_enabled && transcription_model_chain(cfg).len() > 1;
    let (raw, provider_id, model, alternate_result) = match if dual_enabled {
        run_dual_transcription_candidates(app, audio, cfg, gen).await
    } else {
        run_primary_transcription_chain(app, audio, cfg, gen)
            .await
            .map(|(raw, provider, model)| (raw, provider, model, None))
    } {
        Ok((raw, provider_id, model, alternate)) => (raw, provider_id, model, alternate),
        Err(error) => {
            // Never surface the raw provider context string (body previews,
            // request ids) — user_facing_error() strips it and keeps the
            // detail in the log line below.
            let user_msg = crate::api::user_facing_error(&error);
            log::error!(
                "pipeline: transcription failed gen={} error={}",
                gen,
                trim_err(&error.to_string())
            );
            // Every provider in the chain just failed to be reached (a
            // connect error, not an HTTP rejection). Before blaming the
            // provider or showing a generic "nothing transcribed", actively
            // probe whether the user's own connection is down so a dropped
            // network reads as exactly that instead of a confusing error.
            if crate::api::is_connectivity_error(&error) && confirm_offline(app).await {
                emit_connectivity_recheck(app);
                show_error_pill(
                    app,
                    "No internet connection — check your network and try again.",
                )
                .await;
                return None;
            }
            if crate::api::is_retryable_provider_error(&error) {
                emit_provider_recheck(app);
            }
            show_error_pill(app, &user_msg).await;
            return None;
        }
    };

    let corroborated_candidate = alternate_result.as_ref().is_some_and(|(alternate, _, _)| {
        let primary = prepare_transcript_text(alternate, false, true);
        let alternate = prepare_transcript_text(&raw, false, true);
        !primary.trim().is_empty() && primary.trim().eq_ignore_ascii_case(alternate.trim())
    });
    let primary_text =
        prepare_transcript_text(&raw, alternate_result.is_some(), corroborated_candidate);
    if primary_text.is_empty() {
        show_error_pill(
            app,
            "Nothing transcribed - please try speaking more clearly",
        )
        .await;
        return None;
    }
    let alternate = alternate_result.and_then(|(text, provider, model)| {
        let text = prepare_transcript_text(&text, true, corroborated_candidate);
        (!text.is_empty()).then_some(TranscriptCandidate {
            text,
            provider,
            model,
        })
    });
    let api_used = match &alternate {
        Some(candidate) => format!(
            "primary={}/{};secondary={}/{}",
            provider_id, model, candidate.provider, candidate.model
        ),
        None => format!("{provider_id}/{model}/transcription"),
    };
    Some((primary_text, api_used, alternate))
}

const DUAL_TRANSCRIPTION_TIMEOUT_SECS: u64 = 30;

type CandidateOutcome = (usize, String, String, anyhow::Result<String>);

async fn run_dual_transcription_candidates(
    app: &AppHandle,
    audio: &CapturedAudio,
    cfg: &store::PipelineConfig,
    gen: u64,
) -> anyhow::Result<(String, String, String, Option<(String, String, String)>)> {
    let chain = transcription_model_chain(cfg);
    let mut next_index = 0usize;
    let mut in_flight = tokio::task::JoinSet::<CandidateOutcome>::new();
    let mut successes = Vec::<(usize, String, String, String)>::new();
    let mut last_err: Option<anyhow::Error> = None;

    while next_index < chain.len() && in_flight.len() < 2 {
        spawn_transcription_candidate(
            &mut in_flight,
            app,
            audio,
            cfg,
            next_index,
            chain[next_index].clone(),
            next_index > 0,
            gen,
        );
        next_index += 1;
    }

    while let Some(joined) = in_flight.join_next().await {
        match joined {
            Ok((index, provider, model, Ok(text))) if !text.trim().is_empty() => {
                log::debug!(
                    "pipeline: dual transcription candidate success index={} provider={} model={} chars={}",
                    index,
                    provider,
                    model,
                    text.chars().count()
                );
                successes.push((index, text, provider, model));
                if successes.len() >= 2 {
                    in_flight.abort_all();
                    break;
                }
            }
            Ok((index, provider, model, Ok(_))) => {
                log::warn!(
                    "pipeline: dual transcription candidate empty index={} provider={} model={}",
                    index,
                    provider,
                    model
                );
            }
            Ok((index, provider, model, Err(error))) => {
                log::warn!(
                    "pipeline: dual transcription candidate failed index={} provider={} model={} error={}",
                    index,
                    provider,
                    model,
                    trim_err(&error.to_string())
                );
                last_err = Some(error);
            }
            Err(error) => {
                log::warn!("pipeline: dual transcription task failed error={error}");
            }
        }

        if successes.len() + in_flight.len() < 2 && next_index < chain.len() {
            spawn_transcription_candidate(
                &mut in_flight,
                app,
                audio,
                cfg,
                next_index,
                chain[next_index].clone(),
                true,
                gen,
            );
            next_index += 1;
        }
    }

    successes.sort_by_key(|(index, _, _, _)| *index);
    let Some((_, primary_text, primary_provider, primary_model)) = successes.first().cloned()
    else {
        // Preserve the underlying provider error when one exists, so a
        // connectivity outage can be detected upstream instead of being
        // flattened into a generic "nothing transcribed".
        if let Some(error) = last_err {
            return Err(error);
        }
        anyhow::bail!("Nothing transcribed - please try speaking more clearly");
    };
    let alternate = successes
        .get(1)
        .map(|(_, text, provider, model)| (text.clone(), provider.clone(), model.clone()));
    Ok((primary_text, primary_provider, primary_model, alternate))
}

#[allow(clippy::too_many_arguments)]
fn spawn_transcription_candidate(
    in_flight: &mut tokio::task::JoinSet<CandidateOutcome>,
    app: &AppHandle,
    audio: &CapturedAudio,
    cfg: &store::PipelineConfig,
    index: usize,
    candidate: (String, String),
    bounded: bool,
    gen: u64,
) {
    let app = app.clone();
    let audio = audio.clone();
    let cfg = cfg.clone();
    in_flight.spawn(async move {
        let (provider, model) = candidate;
        let key = cfg.key_for(&provider).to_owned();
        let language = cfg.transcription_language.clone();
        let request = transcribe_any(
            &app,
            &audio,
            &provider,
            if key.is_empty() {
                None
            } else {
                Some(key.as_str())
            },
            &language,
            &model,
            gen,
        );
        let result = if bounded {
            match tokio::time::timeout(
                std::time::Duration::from_secs(DUAL_TRANSCRIPTION_TIMEOUT_SECS),
                request,
            )
            .await
            {
                Ok(result) => result,
                Err(_) => Err(anyhow::anyhow!(
                    "secondary transcription timed out after {} seconds",
                    DUAL_TRANSCRIPTION_TIMEOUT_SECS
                )),
            }
        } else {
            request.await
        };
        (index, provider, model, result)
    });
}

fn prepare_transcript_text(
    raw: &str,
    strip_provider_artifacts: bool,
    preserve_corroborated_artifacts: bool,
) -> String {
    let normalized = normalize_transcription_math_artifacts(raw);
    let normalized = if preserve_corroborated_artifacts {
        normalized
    } else {
        strip_trailing_hallucination(&strip_hallucinated_suffix(&normalized))
    };
    let normalized = crate::system::text::collapse_degenerate_word_runs(&normalized);
    if strip_provider_artifacts && !preserve_corroborated_artifacts {
        crate::pipeline::gates::strip_provider_artifacts(&normalized)
    } else {
        normalized
    }
}

async fn run_primary_transcription_chain(
    app: &AppHandle,
    audio: &CapturedAudio,
    cfg: &store::PipelineConfig,
    gen: u64,
) -> anyhow::Result<(String, String, String)> {
    let mut last_err: Option<anyhow::Error> = None;
    for (provider_id, model) in transcription_model_chain(cfg) {
        let key = cfg.key_for(&provider_id).to_owned();
        let language = cfg.transcription_language.clone();
        match transcribe_any(
            app,
            audio,
            &provider_id,
            if key.is_empty() {
                None
            } else {
                Some(key.as_str())
            },
            &language,
            &model,
            gen,
        )
        .await
        {
            Ok(raw) if !raw.is_empty() => {
                log::debug!(
                    "pipeline: transcription provider success gen={} provider={} model={} chars={}",
                    gen,
                    provider_id,
                    model,
                    raw.chars().count()
                );
                return Ok((raw, provider_id, model));
            }
            Ok(_) => {}
            Err(e) => {
                // Always move on to the next candidate, retryable or not.
                // "Retryable" only answers "would retrying this same
                // provider help" (no for a missing/invalid key, bad
                // request, etc.) — it says nothing about whether a
                // different fallback provider/model would succeed, so it
                // must never gate whether the rest of the chain gets tried.
                // Previously this `break`d on non-retryable errors, which
                // meant a primary provider with no API key saved (a very
                // common config: cloud primary + local fallback) skipped
                // every configured fallback and failed outright.
                let retryable = crate::api::is_retryable_provider_error(&e);
                log::warn!(
                    "pipeline: transcription provider failed gen={} provider={} model={} retryable={} error={}",
                    gen,
                    provider_id,
                    model,
                    retryable,
                    trim_err(&e.to_string())
                );
                last_err = Some(e);
            }
        }
    }

    Err(last_err.unwrap_or_else(|| {
        anyhow::anyhow!("Nothing transcribed - please try speaking more clearly")
    }))
}
