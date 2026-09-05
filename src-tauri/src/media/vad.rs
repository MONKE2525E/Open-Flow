//! Local voice-activity detection, used to decide whether a recording
//! actually contains speech instead of relying on raw RMS loudness alone.
//!
//! Runs Silero VAD (via `transcribe_rs::vad::SileroVad`, already bundled
//! through the `onnx`/`vad-silero` crate features shared with local
//! transcription) over the recording's 16kHz samples in fixed 30ms frames.
//! This is CPU-only, local, and fast — Silero's own benchmarks put a single
//! frame at well under 1ms — so a full recording's worth of frames is cheap
//! enough to run synchronously inside a blocking task without adding
//! perceptible latency, especially since the caller runs it concurrently
//! with the network transcription call rather than gating on it first.

use crate::data::store;

/// Aggregate result of running VAD across an entire recording.
///
/// `contains_speech` drives `pipeline::passes_speech_gate`; the remaining
/// fields are surfaced to the setup wizard's mic calibration (via
/// `commands::recording::CalibrationResult`) and logged at both call sites, so
/// a rejection is diagnosable without carrying any dictated content.
#[derive(Debug, Clone, Copy)]
pub struct SpeechDetectionResult {
    pub contains_speech: bool,
    pub speech_ms: u64,
    pub speech_ratio: f32,
    pub peak_probability: f32,
    pub longest_segment_ms: u64,
}

/// Silero's fixed frame size for its v4 ONNX graph: 30ms at 16kHz.
const FRAME_SAMPLES: usize = 480;
const FRAME_MS: u64 = 30;

/// Per-frame speech/non-speech cutoff — transcribe-rs's own documented
/// recommended default for this model.
const SPEECH_PROBABILITY_THRESHOLD: f32 = 0.3;

// Acceptance thresholds at the app's default mic gain. Scaled down for
// higher gain via `gain_leniency_scale` below — starting points, not final
// tuned values (per the design this was built against).
const MIN_SPEECH_MS_BASE: u64 = 300;
const MIN_SPEECH_RATIO_BASE: f32 = 0.12;
const MIN_LONGEST_RUN_MS_BASE: u64 = 250;

/// The Silero v4 ONNX model, bundled directly into the binary. At ~1.8MB
/// this is small enough that shipping it as a Tauri bundle resource (with
/// its own resource-path resolution at runtime) isn't worth the extra
/// moving part — `include_bytes!` keeps dev and packaged builds identical.
static MODEL_BYTES: &[u8] = include_bytes!("../../assets/silero_vad_v4.onnx");

/// `SileroVad::new` only accepts a file path (it calls onnxruntime's
/// `commit_from_file`), so the embedded bytes are staged to a stable path
/// once per process and reused — writing 1.8MB to disk on every dictation
/// would defeat the point of keeping this cheap.
fn staged_model_path() -> anyhow::Result<std::path::PathBuf> {
    static PATH: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();
    static STAGE_LOCK: std::sync::OnceLock<std::sync::Mutex<()>> = std::sync::OnceLock::new();

    if let Some(path) = PATH.get() {
        return Ok(path.clone());
    }

    let _stage_guard = STAGE_LOCK
        .get_or_init(|| std::sync::Mutex::new(()))
        .lock()
        .map_err(|_| anyhow::anyhow!("Silero VAD model staging lock was poisoned"))?;

    if let Some(path) = PATH.get() {
        return Ok(path.clone());
    }

    let stage_dir = crate::app_data_dir().join("runtime");
    std::fs::create_dir_all(&stage_dir)
        .map_err(|e| anyhow::anyhow!("failed to create Silero VAD runtime directory: {e}"))?;
    let path = stage_dir.join("verenu_silero_vad_v4.onnx");
    if !staged_model_matches(&path) {
        let unique_suffix = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default();
        let temp_path = stage_dir.join(format!(
            "verenu_silero_vad_v4-{}-{unique_suffix}.onnx.tmp",
            std::process::id()
        ));
        if let Err(error) = std::fs::write(&temp_path, MODEL_BYTES) {
            let _ = std::fs::remove_file(&temp_path);
            return Err(anyhow::anyhow!("failed to stage Silero VAD model: {error}"));
        }
        if !staged_model_matches(&temp_path) {
            let _ = std::fs::remove_file(&temp_path);
            return Err(anyhow::anyhow!(
                "staged Silero VAD model failed integrity verification"
            ));
        }
        if staged_model_matches(&path) {
            let _ = std::fs::remove_file(&temp_path);
        } else {
            if path.exists() {
                if let Err(error) = std::fs::remove_file(&path) {
                    let _ = std::fs::remove_file(&temp_path);
                    return Err(anyhow::anyhow!(
                        "failed to replace staged Silero VAD model: {error}"
                    ));
                }
            }
            if let Err(error) = std::fs::rename(&temp_path, &path) {
                let _ = std::fs::remove_file(&temp_path);
                return Err(anyhow::anyhow!(
                    "failed to publish staged Silero VAD model: {error}"
                ));
            }
        }
        if !staged_model_matches(&path) {
            return Err(anyhow::anyhow!(
                "published Silero VAD model failed integrity verification"
            ));
        }
    }
    let _ = PATH.set(path.clone());
    Ok(path)
}

fn staged_model_matches(path: &std::path::Path) -> bool {
    let Ok(metadata) = std::fs::metadata(path) else {
        return false;
    };
    if metadata.len() != MODEL_BYTES.len() as u64 {
        return false;
    }
    let Ok(bytes) = std::fs::read(path) else {
        return false;
    };
    bytes.as_slice() == MODEL_BYTES
}

/// Scales how lenient the speech thresholds are with the active mic gain,
/// mirroring `pipeline::gates::recording_gate_rms`'s normalization: a user
/// who raised gain for a quiet voice or a distant mic already told the app
/// their raw signal is faint, so the bar for "this looks like speech" comes
/// down proportionally instead of penalizing them twice for the same thing.
/// Floored at 0.4 rather than scaling to zero — VAD still needs *some*
/// signal to tell speech from a fan.
fn gain_leniency_scale(active_gain: f32) -> f32 {
    let gain = active_gain.clamp(store::MIN_MIC_GAIN, store::MAX_MIC_GAIN);
    if gain <= store::DEFAULT_MIC_GAIN {
        1.0
    } else {
        (store::DEFAULT_MIC_GAIN / gain).max(0.4)
    }
}

/// Runs VAD over an entire recording and judges whether it contains speech.
///
/// Blocking (ONNX inference) — call from `spawn_blocking`, ideally started
/// concurrently with the transcription API call so it adds no wall-clock
/// latency of its own.
#[allow(unknown_lints, clippy::chunks_exact_to_as_chunks)]
pub fn analyze_speech(
    samples_16k: &[f32],
    active_gain: f32,
) -> anyhow::Result<SpeechDetectionResult> {
    let model_path = staged_model_path()?;
    let mut vad = transcribe_rs::vad::SileroVad::new(&model_path, SPEECH_PROBABILITY_THRESHOLD)
        .map_err(|e| anyhow::anyhow!("failed to load Silero VAD model: {e}"))?;

    let mut speech_ms: u64 = 0;
    let mut longest_run_ms: u64 = 0;
    let mut current_run_ms: u64 = 0;
    let mut peak_probability: f32 = 0.0;
    let mut frame_count: u64 = 0;

    for frame in samples_16k.chunks_exact(FRAME_SAMPLES) {
        frame_count += 1;
        let probability = vad
            .speech_probability(frame)
            .map_err(|e| anyhow::anyhow!("Silero VAD inference failed: {e}"))?;
        peak_probability = peak_probability.max(probability);
        if probability >= SPEECH_PROBABILITY_THRESHOLD {
            speech_ms += FRAME_MS;
            current_run_ms += FRAME_MS;
            longest_run_ms = longest_run_ms.max(current_run_ms);
        } else {
            current_run_ms = 0;
        }
    }

    let total_ms = frame_count * FRAME_MS;
    let speech_ratio = if total_ms > 0 {
        speech_ms as f32 / total_ms as f32
    } else {
        0.0
    };

    let scale = gain_leniency_scale(active_gain);
    let min_speech_ms = (MIN_SPEECH_MS_BASE as f32 * scale) as u64;
    let min_ratio = MIN_SPEECH_RATIO_BASE * scale;
    let min_longest_run_ms = (MIN_LONGEST_RUN_MS_BASE as f32 * scale) as u64;

    let contains_speech = speech_ms >= min_speech_ms
        && (speech_ratio >= min_ratio || longest_run_ms >= min_longest_run_ms);

    Ok(SpeechDetectionResult {
        contains_speech,
        speech_ms,
        speech_ratio,
        peak_probability,
        longest_segment_ms: longest_run_ms,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gain_leniency_scale_is_neutral_at_default_gain() {
        assert_eq!(gain_leniency_scale(store::DEFAULT_MIC_GAIN), 1.0);
    }

    #[test]
    fn gain_leniency_scale_relaxes_thresholds_for_boosted_gain() {
        let scale = gain_leniency_scale(store::MAX_MIC_GAIN);
        assert!(scale < 1.0);
        assert!(scale >= 0.4);
    }

    #[test]
    fn gain_leniency_scale_never_goes_below_floor() {
        // active_gain is clamped to MAX_MIC_GAIN before scaling, so the floor
        // is only reachable if the ratio itself would go under 0.4 within
        // the valid gain range — assert the invariant instead of a specific
        // value baked in from an out-of-range input.
        assert!(gain_leniency_scale(store::MAX_MIC_GAIN) >= 0.4);
    }

    #[test]
    fn calibration_gain_of_one_uses_unscaled_thresholds() {
        // `stop_calibration_monitoring` passes 1.0 because calibration forces
        // gain 1.0 at capture, so VAD must judge the raw signal on the full
        // thresholds rather than the relaxed ones meant for boosted mics.
        assert_eq!(gain_leniency_scale(1.0), 1.0);
    }

    #[test]
    fn analyze_speech_on_digital_silence_finds_no_speech() {
        let silence = vec![0.0f32; 16_000]; // 1s of exact silence
        let result = analyze_speech(&silence, store::DEFAULT_MIC_GAIN)
            .expect("model should load and run on staged path");
        assert!(!result.contains_speech);
        assert_eq!(result.speech_ms, 0);
    }
}
