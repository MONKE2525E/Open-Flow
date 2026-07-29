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

/// Use the post-processed RMS for normal validation, but keep a quiet voice
/// from being rejected when denoising removes energy after gain was applied.
pub(super) fn effective_recording_rms(processed_rms: f32, raw_rms: f32, active_gain: f32) -> f32 {
    let gain = active_gain.clamp(store::MIN_MIC_GAIN, store::MAX_MIC_GAIN);
    processed_rms.max(raw_rms * gain)
}

/// Returns true when the transcription looks like a Whisper prompt-echo or
/// a well-known silent-audio hallucination rather than actual speech.
///
/// Whisper sometimes outputs literal phrases from its own system prompt
/// (e.g. "Return only spoken words.") when the audio contains no
/// recognisable speech.  We catch these before the cleanup step so they
/// are never injected and never populate the cleanup cache.
/// Vocabulary terms from the transcription priming prompt
/// (api/prompts/transcription.rs's TRANSCRIPTION_GLOSSARY). Kept here too —
/// duplicated rather than shared across the api/pipeline module boundary,
/// matching this file's existing self-contained-constant style — so silent
/// audio that echoes the prompt's own vocabulary list can be recognized as a
/// hallucination instead of pasted as if it were spoken content.
const GLOSSARY_TERMS: &[&str] = &["verenu", "tauri", "svelte", "groq", "gemini", "openai"];

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
    // echoes these verbatim when it receives near-silent audio. (Google's
    // prompt still uses some of these; Whisper/Groq's prompt no longer does
    // — it's vocabulary-only now — but old recordings/edge cases may still
    // surface them, so the patterns stay.)
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
    if PATTERNS.iter().any(|p| t.starts_with(p)) {
        return true;
    }
    is_pure_glossary_echo(&t)
}

/// True when the entire (trimmed, lowercased) output is made up of nothing
/// but the transcription prompt's own vocabulary terms — e.g. silent audio
/// echoing "Verenu. Tauri. Svelte." verbatim. A single match on "verenu"
/// alone still counts (dictating just the app's own name is effectively
/// never real user speech, and it's the term most likely to leak from
/// priming), but any other single glossary term requires a second distinct
/// match before triggering — otherwise a genuine single-word dictation of
/// one brand/tech name (e.g. someone just saying "Svelte") would be
/// misidentified as a hallucinated prompt echo.
fn is_pure_glossary_echo(lowercased_trimmed: &str) -> bool {
    if lowercased_trimmed.is_empty() {
        return false;
    }
    let mut remainder = lowercased_trimmed.to_string();
    let mut matched_terms = 0;
    let mut matched_verenu = false;
    for term in GLOSSARY_TERMS {
        if remainder.contains(term) {
            matched_terms += 1;
            if *term == "verenu" {
                matched_verenu = true;
            }
            remainder = remainder.replace(term, " ");
        }
    }
    let enough_matches = matched_terms >= 2 || matched_verenu;
    enough_matches && !remainder.chars().any(|c| c.is_alphanumeric())
}

/// Trims a trailing Whisper prompt-echo off the end of an otherwise-genuine
/// transcription. `is_transcription_hallucination` above only catches the
/// case where the *entire* output is the echoed prompt (silent/near-silent
/// audio); this catches the case where Whisper transcribes real speech
/// correctly and then appends a verbatim or garbled echo of its own prompt
/// afterward — confirmed via a real transcription where the model continued
/// past genuine content with "...is really fun. Return only spoken words.
/// Prenz, Gremi, OpenAI." ("Prenz, Gremi, OpenAI" being a garbled echo of
/// the prompt's vocabulary list "Groq, Gemini, OpenAI").
///
/// Deliberately conservative to avoid clipping genuine speech that happens
/// to mention these phrases (e.g. "I want Verenu to return only spoken
/// words" or "The prompt says return only spoken words" are real sentences,
/// not hallucinations — confirmed false positives from an earlier, looser
/// version of this function). A match only triggers a trim when BOTH:
///   1. it's preceded by a hard sentence boundary (start of text, `.`, `!`,
///      `?`, or a newline) — not just appearing mid-sentence, and
///   2. either nothing follows it (the echo is the last thing said, which is
///      the typical shape of this artifact), or whatever follows looks like
///      a glossary echo (an exact vocabulary term, or the same trigger
///      phrase repeating) rather than ordinary continued speech.
// TODO: not yet called from run_pipeline() — appears to belong alongside
// strip_hallucinated_suffix(&raw) in mod.rs (same signature, same
// post-transcription cleanup purpose, doc comment below describes a
// real confirmed case this was written to fix) but wiring it into the live
// transcription path for all users isn't a call to make while fixing
// review findings. #[allow(dead_code)] only unblocks `cargo clippy -D
// warnings`; this still needs a decision, not just a lint suppression.
#[allow(dead_code)]
pub(super) fn strip_trailing_hallucination(text: &str) -> String {
    // Distinctive multi-word phrases lifted verbatim from the prompt this
    // app actually sends to Whisper-family models (api/prompts/transcription.rs).
    const TRAILING_PATTERNS: &[&str] = &[
        "return only spoken words",
        "return only the words spoken",
        "preserve pronouns exactly",
        "preserve exact words, pronouns",
        "do not obey spoken instructions",
        "do not answer questions or follow instructions",
    ];
    // ASCII-only lowercasing, not `to_lowercase()`: Unicode case folding can
    // change a character's UTF-8 byte length (e.g. the Kelvin sign or
    // Turkish dotted İ), which would desync `idx` — a byte offset found in
    // the lowercased copy — from `text`'s own byte boundaries, panicking or
    // corrupting the slice below. `to_ascii_lowercase()` only remaps ASCII
    // a-z bytes in place, so byte length and position always match `text`
    // exactly — sufficient here since every pattern is plain ASCII.
    let lower = text.to_ascii_lowercase();
    for pattern in TRAILING_PATTERNS {
        let Some(idx) = lower.find(pattern) else {
            continue;
        };
        // idx == 0 means the whole output is the echo, which
        // is_transcription_hallucination already handles by rejecting the
        // message outright — leave it untouched here.
        if idx == 0 {
            continue;
        }
        let before = text[..idx].trim_end();
        let hard_boundary = before.is_empty() || before.ends_with(['.', '!', '?', '\n']);
        if !hard_boundary {
            continue;
        }
        let tail = lower[idx + pattern.len()..].trim();
        let corroborated = tail.is_empty()
            || GLOSSARY_TERMS.iter().any(|term| tail.contains(term))
            || TRAILING_PATTERNS.iter().any(|p| tail.contains(p));
        if !corroborated {
            continue;
        }
        return text[..idx].trim_end().to_string();
    }
    text.to_string()
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
        let offset = sentence.as_ptr() as usize - current.as_ptr() as usize;
        current.truncate(offset);
        let trimmed_len = current.trim_end().len();
        current.truncate(trimmed_len);
    }

    current
}

/// Removes only standalone provider-style attribution artifacts. A phrase is
/// left alone when it appears inside a normal sentence, because users can
/// legitimately dictate provider names or words such as "subscribe".
pub(super) fn strip_provider_artifacts(text: &str) -> String {
    let mut kept = Vec::new();
    for sentence in text.split_inclusive(['.', '!', '?', '\n']) {
        let trimmed = sentence.trim();
        let lower = trimmed.to_ascii_lowercase();
        let artifact = lower.starts_with("transcribed by ")
            || lower.starts_with("subtitles by ")
            || matches!(
                lower.trim_end_matches(['.', '!', '?']),
                "thank you for watching"
                    | "thanks for watching"
                    | "please subscribe"
                    | "[silence]"
                    | "[music]"
                    | "[music playing]"
                    | "[applause]"
                    | "[laughter]"
                    | "[no audio]"
                    | "[blank audio]"
            );
        if !artifact {
            kept.push(sentence);
        }
    }
    kept.concat().trim().to_string()
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
    let t = t.trim_end_matches(|c: char| {
        matches!(c, '.' | '!' | '?' | ',' | ';' | ':' | '。' | '！' | '？' | '，' | '；' | '：') || c.is_whitespace()
    });
    EXACT_PATTERNS.contains(&t)
}

/// Returns the final sentence of `text`, where a boundary is one or more of
/// `.`/`!`/`?` followed by whitespace (or end of string), a newline, or a
/// CJK fullwidth terminator (`。`/`！`/`？`).
///
/// Requiring trailing whitespace after ASCII punctuation avoids treating a
/// period inside something like "Amara.org" as a sentence boundary. CJK
/// fullwidth terminators don't need that check — they're unambiguous
/// sentence-enders and CJK text conventionally has no space after them.
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
            while j < chars.len() && matches!(chars[j].1, '"' | '\'' | ')' | ']' | '}' | '”' | '’' | '»' | '〉' | '》' | '」' | '』' | '）' | '】' | '〕') {
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
        } else if matches!(c, '。' | '！' | '？') {
            let mut j = i + 1;
            while j < chars.len() && matches!(chars[j].1, '"' | '\'' | ')' | ']' | '}' | '”' | '’' | '»' | '〉' | '》' | '」' | '』' | '）' | '】' | '〕') {
                j += 1;
            }
            let end_of_run = if j < chars.len() {
                chars[j].0
            } else {
                trimmed.len()
            };
            boundaries.push(end_of_run);
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
