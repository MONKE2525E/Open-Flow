//! Pure recording-quality gates and transcription text-normalization helpers,
//! extracted from the pipeline orchestration to keep `pipeline.rs` focused on
//! flow. These are behavior-preserving moves; coverage lives in the `pipeline`
//! test module (see `super::tests`).

use crate::data::store;

/// Minimum recording length before any API is called. Shorter clips make
/// Whisper hallucinate, so they're rejected outright.
pub(super) const MIN_RECORDING_MS: u64 = 700;

/// Minimum RMS (at default gain) below which audio is treated as near-silence
/// and rejected as a likely accidental activation.
pub(super) const MIN_RECORDING_RMS: f32 = 0.008;

/// Scale the near-silence RMS threshold by the active mic gain so the gate
/// stays consistent whether the user is amplifying a quiet mic or attenuating
/// a hot one.
pub(super) fn recording_gate_rms(active_gain: f32) -> f32 {
    let gain = active_gain.clamp(store::MIN_MIC_GAIN, store::MAX_MIC_GAIN);
    if gain <= store::DEFAULT_MIC_GAIN {
        MIN_RECORDING_RMS * gain / store::DEFAULT_MIC_GAIN
    } else {
        MIN_RECORDING_RMS * store::DEFAULT_MIC_GAIN / gain
    }
}

/// Returns true when the transcription looks like a Whisper prompt-echo or
/// a well-known silent-audio hallucination rather than actual speech.
///
/// Whisper sometimes outputs literal phrases from its own system prompt
/// (e.g. "Return only spoken words.") when the audio contains no
/// recognisable speech.  We catch these before the cleanup step so they
/// are never injected and never populate the cleanup cache.
pub(super) fn is_transcription_hallucination(text: &str) -> bool {
    let t = text.trim().to_lowercase();
    // Phrases from our transcription system prompts (prompts.rs).  Whisper
    // echoes these verbatim when it receives near-silent audio.
    const PATTERNS: &[&str] = &[
        "return only spoken words",
        "return only the words spoken",
        "verenu dictation in ",
        "transcribe the audio in ",
        "preserve pronouns exactly",
        // Well-known generic Whisper hallucinations for silent/noisy audio
        "thank you for watching",
        "thanks for watching",
        "please subscribe",
        "subscribe to my channel",
        "subtitles by ",
        "transcribed by ",
        "[silence]",
        "[music]",
        "[music playing]",
        "[applause]",
        "[laughter]",
        "[no audio]",
        "[blank audio]",
    ];
    PATTERNS.iter().any(|p| t.starts_with(p))
}

/// Build a single-line, length-capped preview of dictation text for logs.
///
/// Privacy: dictation content must not land in default logs (the in-memory ring
/// buffer, the `verenu:log` event stream, or exported log files). The actual
/// text is therefore only included when developer verbose logging is enabled;
/// otherwise a content-free `<N chars redacted>` marker is returned so the log
/// still records that text existed and roughly how long it was. The companion
/// `*_full` logs (also verbose-gated) carry the untruncated text.
pub(super) fn preview_text(s: &str, limit: usize) -> String {
    if !crate::system::logger::is_verbose() {
        return format!("<{} chars redacted>", s.chars().count());
    }
    let compact = s.replace(['\n', '\r'], " ");
    let compact = compact.trim();
    if compact.chars().count() > limit {
        format!("{}...", compact.chars().take(limit).collect::<String>())
    } else {
        compact.to_string()
    }
}

/// Collapse spaced-out multiplication artifacts (e.g. "6 x 7" → "6x7") that the
/// transcription model occasionally inserts, so downstream number handling and
/// cleanup see a consistent form.
pub(super) fn normalize_transcription_math_artifacts(raw: &str) -> String {
    let chars: Vec<char> = raw.chars().collect();
    let mut out = String::with_capacity(chars.len());
    let mut i = 0usize;

    while i < chars.len() {
        if chars[i].is_ascii_digit() {
            let mut j = i + 1;
            while j < chars.len() && chars[j].is_whitespace() {
                j += 1;
            }
            if j < chars.len() && matches!(chars[j], 'x' | 'X') {
                let mut k = j + 1;
                while k < chars.len() && chars[k].is_whitespace() {
                    k += 1;
                }
                if k < chars.len() && chars[k].is_ascii_digit() {
                    let had_spacing = j > i + 1 || k > j + 1;
                    if had_spacing {
                        out.push(chars[i]);
                        out.push('x');
                        out.push(chars[k]);
                        i = k + 1;
                        continue;
                    }
                }
            }
        }
        out.push(chars[i]);
        i += 1;
    }

    out
}
