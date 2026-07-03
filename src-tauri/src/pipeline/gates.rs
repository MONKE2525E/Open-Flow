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

/// Removes a trailing sentence that matches a known Whisper hallucination
/// pattern, leaving any real speech before it intact.
///
/// Whisper sometimes bolts one of these phrases onto the *end* of an
/// otherwise-correct transcription when the recording has a tail of
/// near-silent or background audio after the user finishes talking (e.g. the
/// mic keeps capturing for a moment before the hotkey is released). That
/// differs from `is_transcription_hallucination`, which only catches the
/// case where the *entire* output is one of these phrases.
pub(super) fn strip_hallucinated_suffix(text: &str) -> String {
    let mut current = text.trim().to_string();

    // Bounded loop: guards against pathological repeated matches rather than
    // expecting more than one hallucinated sentence in practice.
    for _ in 0..4 {
        let sentence = last_sentence(&current);
        if sentence.is_empty() || !is_sentence_hallucination(sentence) {
            break;
        }
        let cut = current.len() - sentence.len();
        current.truncate(cut);
        current = current.trim().to_string();
    }

    current
}

/// Like `is_transcription_hallucination`, but scoped to a single sentence
/// pulled from the end of a transcription rather than the whole output.
///
/// A handful of the known hallucinations ("thank you for watching", "please
/// subscribe", ...) are short, generic phrases that legitimate dictation can
/// plausibly start a sentence with (e.g. "Please subscribe to our
/// newsletter."). Matching those as a prefix — as the whole-output gate does
/// — would silently delete real speech. So here they require an exact match
/// on the whole sentence (ignoring trailing punctuation) instead. Patterns
/// that are effectively unambiguous even as a prefix (credit lines, system
/// prompt echoes) keep prefix matching.
fn is_sentence_hallucination(sentence: &str) -> bool {
    let t = sentence.trim().to_lowercase();

    const PREFIX_PATTERNS: &[&str] = &[
        "return only spoken words",
        "return only the words spoken",
        "verenu dictation in ",
        "transcribe the audio in ",
        "preserve pronouns exactly",
        "subtitles by ",
        "transcribed by ",
    ];
    if PREFIX_PATTERNS.iter().any(|p| t.starts_with(p)) {
        return true;
    }

    const EXACT_PATTERNS: &[&str] = &[
        "thank you for watching",
        "thanks for watching",
        "please subscribe",
        "subscribe to my channel",
        "[silence]",
        "[music]",
        "[music playing]",
        "[applause]",
        "[laughter]",
        "[no audio]",
        "[blank audio]",
    ];
    let t = t.trim_end_matches(|c: char| matches!(c, '.' | '!' | '?' | ',' | ';' | ':') || c.is_whitespace());
    EXACT_PATTERNS.contains(&t)
}

/// Returns the final sentence of `text`, where a boundary is one or more of
/// `.`/`!`/`?` followed by whitespace (or end of string), or a newline.
///
/// Requiring trailing whitespace after the punctuation avoids treating a
/// period inside something like "Amara.org" as a sentence boundary.
fn last_sentence(text: &str) -> &str {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return trimmed;
    }

    let chars: Vec<(usize, char)> = trimmed.char_indices().collect();
    let mut boundaries: Vec<usize> = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let (_, c) = chars[i];
        if matches!(c, '.' | '!' | '?') {
            let mut j = i + 1;
            while j < chars.len() && matches!(chars[j].1, '.' | '!' | '?') {
                j += 1;
            }
            let end_of_run = if j < chars.len() {
                chars[j].0
            } else {
                trimmed.len()
            };
            if j >= chars.len() || chars[j].1.is_whitespace() {
                boundaries.push(end_of_run);
            }
            i = j;
            continue;
        } else if c == '\n' {
            boundaries.push(chars[i].0 + 1);
        }
        i += 1;
    }

    // Walk boundaries from the end, skipping the one that just closes off the
    // final sentence itself (nothing but whitespace follows it).
    let start = boundaries
        .into_iter()
        .rev()
        .find(|&b| !trimmed[b..].trim().is_empty())
        .unwrap_or(0);
    trimmed[start..].trim()
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
