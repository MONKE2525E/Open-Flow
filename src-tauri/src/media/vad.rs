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


/// Three-way verdict, replacing the old pass/fail boolean.
///
/// VAD is not a perfect oracle, and missing legitimate speech is substantially
/// worse than occasionally transcribing an empty recording — so the uncertain
/// middle is preserved rather than discarded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpeechClass {
    /// Confident speech — process normally.
    Speech,
    /// Possible quiet speech or a whisper. Transcribed anyway; only the
    /// classification and the diagnostics differ from `Speech`.
    Borderline,
    /// Confident silence — discard without paying for transcription.
    Silence,
}

impl SpeechClass {
    pub fn as_str(self) -> &'static str {
        match self {
            SpeechClass::Speech => "speech",
            SpeechClass::Borderline => "borderline",
            SpeechClass::Silence => "silence",
        }
    }
}

/// Aggregate result of running VAD across an entire recording.
///
/// `contains_speech` drives `pipeline::passes_speech_gate` (true for both
/// `Speech` and `Borderline`); the remaining fields exist so a rejection is
/// diagnosable from the logs without carrying any dictated content.
#[derive(Debug, Clone, Copy)]
pub struct SpeechDetectionResult {
    pub contains_speech: bool,
    pub class: SpeechClass,
    pub speech_ms: u64,
    pub speech_ratio: f32,
    pub peak_probability: f32,
    pub longest_segment_ms: u64,
    /// Speech time counted at the lower `WEAK_PROBABILITY_THRESHOLD`. A
    /// whisper often sits between the two thresholds for its whole duration.
    pub weak_speech_ms: u64,
    /// Estimated per-device/per-session noise floor: the 20th-percentile frame
    /// RMS of this recording. Used instead of any absolute dB cutoff, which
    /// would break across microphones.
    pub noise_floor_rms: f32,
    pub peak_frame_rms: f32,
    /// Loudest frame relative to that noise floor. A quiet whisper slightly
    /// above a very quiet floor still scores well here.
    pub snr_db: f32,
    /// The learned per-device sensitivity this verdict was reached with.
    pub sensitivity: f32,
}

/// Silero's fixed frame size for its v4 ONNX graph: 30ms at 16kHz.
const FRAME_SAMPLES: usize = 480;
const FRAME_MS: u64 = 30;

/// Per-frame speech/non-speech cutoff — transcribe-rs's own documented
/// recommended default for this model.
const SPEECH_PROBABILITY_THRESHOLD: f32 = 0.3;

/// Second, deliberately permissive per-frame cutoff used only to detect the
/// borderline zone. Silero scores a whisper well below its recommended 0.3
/// while still scoring it clearly above steady noise.
const WEAK_PROBABILITY_THRESHOLD: f32 = 0.15;

// Acceptance thresholds at the app's default mic gain. Scaled down for
// higher gain via `gain_leniency_scale` below — starting points, not final
// tuned values (per the design this was built against).
const MIN_SPEECH_MS_BASE: u64 = 300;
const MIN_SPEECH_RATIO_BASE: f32 = 0.12;
const MIN_LONGEST_RUN_MS_BASE: u64 = 250;

// Borderline-zone thresholds. Either weak-probability evidence *or* a peak
// probability that got somewhere at all qualifies, but both still require the
// waveform to sit meaningfully above the recording's own noise floor —
// otherwise a fan or a hiss would keep every accidental hotkey press alive.
const BORDERLINE_MIN_WEAK_SPEECH_MS_BASE: u64 = 150;
const BORDERLINE_MIN_PEAK_PROBABILITY: f32 = 0.2;
const BORDERLINE_MIN_SNR_DB: f32 = 3.0;

/// Above this SNR the clip clearly contained *something* loud relative to its
/// own floor. Used by the pipeline to refuse to train the adaptive system from
/// an empty transcription: plenty of signal plus no text is far more likely to
/// be an STT problem than a VAD one.
pub const CONFIDENT_SIGNAL_SNR_DB: f32 = 12.0;

/// Percentile of frame RMS treated as the noise floor. Low enough that a
/// mostly-speech recording still resolves a floor from its gaps, high enough
/// not to latch onto one anomalously dead frame.
const NOISE_FLOOR_PERCENTILE: f32 = 0.2;

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

/// Estimated noise floor of a recording: the `NOISE_FLOOR_PERCENTILE`-th
/// frame RMS. Relative, never absolute — a Blue Yeti's floor and a laptop's
/// floor are orders of magnitude apart, so a fixed dB cutoff would break on
/// one of them.
fn noise_floor(mut frame_rms: Vec<f32>) -> f32 {
    if frame_rms.is_empty() {
        return 0.0;
    }
    frame_rms.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let index = ((frame_rms.len() as f32 - 1.0) * NOISE_FLOOR_PERCENTILE).round() as usize;
    frame_rms[index.min(frame_rms.len() - 1)]
}

fn rms_of(frame: &[f32]) -> f32 {
    if frame.is_empty() {
        return 0.0;
    }
    (frame.iter().map(|sample| sample * sample).sum::<f32>() / frame.len() as f32).sqrt()
}

/// Loudest frame relative to the noise floor, in dB. Zero when there is no
/// audible signal at all (digital silence), which keeps silence out of the
/// borderline zone rather than letting a divide-by-almost-zero manufacture an
/// SNR out of nothing.
fn snr_db(peak_frame_rms: f32, noise_floor_rms: f32) -> f32 {
    const AUDIBLE_FLOOR: f32 = 1e-6;
    if peak_frame_rms <= AUDIBLE_FLOOR {
        return 0.0;
    }
    let floor = noise_floor_rms.max(AUDIBLE_FLOOR);
    (20.0 * (peak_frame_rms / floor).log10()).max(0.0)
}

/// Runs VAD over an entire recording and classifies it as speech, borderline,
/// or silence.
///
/// `sensitivity` is the learned per-device value from `media::vad_profile`.
/// **Higher means more sensitive**: every acceptance threshold below is
/// divided by it, so 2.0 halves the bar for "this looks like speech" and 0.5
/// doubles it.
///
/// Blocking (ONNX inference) — call from `spawn_blocking`, ideally started
/// concurrently with the transcription API call so it adds no wall-clock
/// latency of its own.
pub fn analyze_speech(
    samples_16k: &[f32],
    sensitivity: f32,
) -> anyhow::Result<SpeechDetectionResult> {
    let model_path = staged_model_path()?;
    // Constructed at the *weak* cutoff so the model's own internal gating does
    // not discard the sub-0.3 frames the borderline zone is built on; the
    // strong cutoff is applied per frame below.
    let mut vad = transcribe_rs::vad::SileroVad::new(&model_path, WEAK_PROBABILITY_THRESHOLD)
        .map_err(|e| anyhow::anyhow!("failed to load Silero VAD model: {e}"))?;

    let mut speech_ms: u64 = 0;
    let mut weak_speech_ms: u64 = 0;
    let mut longest_run_ms: u64 = 0;
    let mut current_run_ms: u64 = 0;
    let mut peak_probability: f32 = 0.0;
    let mut peak_frame_rms: f32 = 0.0;
    let mut frame_count: u64 = 0;
    let mut frame_rms = Vec::new();

    for frame in samples_16k.chunks_exact(FRAME_SAMPLES) {
        frame_count += 1;
        let probability = vad
            .speech_probability(frame)
            .map_err(|e| anyhow::anyhow!("Silero VAD inference failed: {e}"))?;
        peak_probability = peak_probability.max(probability);
        let level = rms_of(frame);
        peak_frame_rms = peak_frame_rms.max(level);
        frame_rms.push(level);
        if probability >= SPEECH_PROBABILITY_THRESHOLD {
            speech_ms += FRAME_MS;
            current_run_ms += FRAME_MS;
            longest_run_ms = longest_run_ms.max(current_run_ms);
        } else {
            current_run_ms = 0;
        }
        if probability >= WEAK_PROBABILITY_THRESHOLD {
            weak_speech_ms += FRAME_MS;
        }
    }

    let total_ms = frame_count * FRAME_MS;
    let speech_ratio = if total_ms > 0 {
        speech_ms as f32 / total_ms as f32
    } else {
        0.0
    };
    let noise_floor_rms = noise_floor(frame_rms);
    let snr_db = snr_db(peak_frame_rms, noise_floor_rms);
    let sensitivity = crate::media::vad_profile::clamp_sensitivity(sensitivity);

    let class = classify(
        speech_ms,
        speech_ratio,
        longest_run_ms,
        weak_speech_ms,
        peak_probability,
        snr_db,
        sensitivity,
    );

    Ok(SpeechDetectionResult {
        contains_speech: class != SpeechClass::Silence,
        class,
        speech_ms,
        speech_ratio,
        peak_probability,
        longest_segment_ms: longest_run_ms,
        weak_speech_ms,
        noise_floor_rms,
        peak_frame_rms,
        snr_db,
        sensitivity,
    })
}

/// The decision itself, pure so all three zones are testable from fixture
/// numbers instead of a real microphone.
#[allow(clippy::too_many_arguments)]
pub fn classify(
    speech_ms: u64,
    speech_ratio: f32,
    longest_run_ms: u64,
    weak_speech_ms: u64,
    peak_probability: f32,
    snr_db: f32,
    sensitivity: f32,
) -> SpeechClass {
    // Lower scale == easier to accept. The learned per-device sensitivity is
    // the *only* thing that scales these thresholds now: capture gain is a
    // fixed constant, so nothing else can quietly move the bar underneath the
    // adaptive detector.
    let scale = 1.0 / crate::media::vad_profile::clamp_sensitivity(sensitivity);
    let min_speech_ms = (MIN_SPEECH_MS_BASE as f32 * scale) as u64;
    let min_ratio = MIN_SPEECH_RATIO_BASE * scale;
    let min_longest_run_ms = (MIN_LONGEST_RUN_MS_BASE as f32 * scale) as u64;

    if speech_ms >= min_speech_ms
        && (speech_ratio >= min_ratio || longest_run_ms >= min_longest_run_ms)
    {
        return SpeechClass::Speech;
    }

    let min_weak_ms = (BORDERLINE_MIN_WEAK_SPEECH_MS_BASE as f32 * scale) as u64;
    let has_weak_evidence =
        weak_speech_ms >= min_weak_ms || peak_probability >= BORDERLINE_MIN_PEAK_PROBABILITY;
    if has_weak_evidence && snr_db >= BORDERLINE_MIN_SNR_DB {
        return SpeechClass::Borderline;
    }

    SpeechClass::Silence
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::media::vad_profile::{DEFAULT_SENSITIVITY, MAX_SENSITIVITY, MIN_SENSITIVITY};

    #[test]
    fn analyze_speech_on_digital_silence_finds_no_speech() {
        let silence = vec![0.0f32; 16_000]; // 1s of exact silence
        let result = analyze_speech(&silence, DEFAULT_SENSITIVITY)
            .expect("model should load and run on staged path");
        assert!(!result.contains_speech);
        assert_eq!(result.class, SpeechClass::Silence);
        assert_eq!(result.speech_ms, 0);
    }

    /// Fixture: obvious, sustained speech.
    fn obvious_speech(sensitivity: f32) -> SpeechClass {
        classify(
            2_400,
            0.8,
            2_100,
            2_700,
            0.97,
            28.0,
            sensitivity,
        )
    }

    /// Fixture: a whisper. Nothing clears the strong per-frame cutoff, but the
    /// weak cutoff fires for a while and the signal sits above the room floor.
    fn quiet_whisper(sensitivity: f32) -> SpeechClass {
        classify(
            0,
            0.0,
            0,
            900,
            0.24,
            7.5,
            sensitivity,
        )
    }

    /// Fixture: an accidental hotkey press in a quiet room.
    fn obvious_silence(sensitivity: f32) -> SpeechClass {
        classify(
            0,
            0.0,
            0,
            0,
            0.02,
            0.4,
            sensitivity,
        )
    }

    #[test]
    fn obvious_speech_is_accepted() {
        assert_eq!(obvious_speech(DEFAULT_SENSITIVITY), SpeechClass::Speech);
        // ...at every sensitivity in range, including the least sensitive.
        assert_eq!(obvious_speech(MIN_SENSITIVITY), SpeechClass::Speech);
    }

    #[test]
    fn obvious_silence_is_rejected() {
        assert_eq!(obvious_silence(DEFAULT_SENSITIVITY), SpeechClass::Silence);
        // Even at maximum learned sensitivity, no signal means no dictation.
        assert_eq!(obvious_silence(MAX_SENSITIVITY), SpeechClass::Silence);
    }

    #[test]
    fn borderline_whisper_reaches_the_fallback_path_instead_of_being_discarded() {
        assert_eq!(quiet_whisper(DEFAULT_SENSITIVITY), SpeechClass::Borderline);
    }

    #[test]
    fn loud_non_speech_still_needs_some_vad_evidence() {
        // A door slam: huge SNR, but Silero never scored it anywhere.
        assert_eq!(
            classify(
                0,
                0.0,
                0,
                0,
                0.03,
                30.0,
                MAX_SENSITIVITY
            ),
            SpeechClass::Silence
        );
    }

    #[test]
    fn weak_evidence_at_the_noise_floor_is_not_borderline() {
        // The same weak frames as a whisper, but nothing rises above the room.
        assert_eq!(
            classify(
                0,
                0.0,
                0,
                900,
                0.24,
                1.0,
                DEFAULT_SENSITIVITY
            ),
            SpeechClass::Silence
        );
    }

    #[test]
    fn higher_sensitivity_never_makes_a_clip_harder_to_accept() {
        // Monotonicity is what keeps the adaptive loop reasoning-friendly: a
        // Skip VAD recovery must never make the next clip *more* likely to be
        // rejected.
        let marginal = |sensitivity: f32| {
            classify(
                240,
                0.10,
                210,
                600,
                0.55,
                9.0,
                sensitivity,
            )
        };
        let rank = |class: SpeechClass| match class {
            SpeechClass::Silence => 0,
            SpeechClass::Borderline => 1,
            SpeechClass::Speech => 2,
        };
        let low = rank(marginal(MIN_SENSITIVITY));
        let default = rank(marginal(DEFAULT_SENSITIVITY));
        let high = rank(marginal(MAX_SENSITIVITY));
        assert!(low <= default && default <= high);
        assert!(high > low, "sensitivity must actually change the verdict");
    }

    #[test]
    fn noise_floor_uses_a_low_percentile_not_the_mean() {
        // Mostly loud speech with a few quiet gaps: the floor is the gap, not
        // the average, so a whisper is still measured against the room.
        let floor = noise_floor(vec![0.001, 0.002, 0.3, 0.4, 0.5]);
        assert!(floor <= 0.002, "floor was {floor}");
    }

    #[test]
    fn snr_of_digital_silence_is_zero() {
        assert_eq!(snr_db(0.0, 0.0), 0.0);
    }

    #[test]
    fn out_of_range_sensitivity_is_clamped_not_trusted() {
        assert_eq!(
            classify(0, 0.0, 0, 0, 0.0, 0.0, f32::NAN),
            SpeechClass::Silence
        );
        assert_eq!(obvious_speech(1e9), SpeechClass::Speech);
    }
}
