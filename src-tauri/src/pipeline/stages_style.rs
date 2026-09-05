//! Style resolution: App Mappings, Context overrides, and the tone/intensity
//! priority chain that decides the cleanup profile for a dictation.

use super::*;
pub(super) fn resolve_app_mapping(
    store: Option<&store::SettingsSnapshot>,
    process_name: &str,
) -> Option<AppMapping> {
    store.and_then(|s| {
        s.get(store::APP_MAPPINGS)
            .and_then(|v| serde_json::from_value::<Vec<AppMapping>>(v.clone()).ok())
            .and_then(|list| {
                list.into_iter()
                    .find(|m| m.exe.trim().eq_ignore_ascii_case(process_name))
            })
    })
}

/// Resolves the effective tone profile, in priority order: the active
/// Context's tone override, then the app-mapping's profile, then the global
/// `default_tone`. Cleanup intensity follows the same priority and is
/// applied onto `cfg` in place. The Context override is the most specific
/// signal (it can be a per-website match), so it wins over the exe-keyed
/// AppMapping when both are set.
pub(super) fn apply_app_style_overrides(
    cfg: &mut store::PipelineConfig,
    mapping: Option<&AppMapping>,
    context: Option<&db::Context>,
) -> String {
    let context_intensity = context
        .and_then(|c| c.cleanup_intensity.as_deref())
        .map(str::trim)
        .filter(|i| !i.is_empty());
    let mapping_intensity = mapping
        .and_then(|m| m.cleanup_intensity.as_deref())
        .map(str::trim)
        .filter(|i| !i.is_empty());
    if let Some(intensity) = context_intensity.or(mapping_intensity) {
        cfg.cleanup_intensity = intensity.to_owned();
    }
    if context.is_some_and(|context| context.contextual_formatting_disabled) {
        cfg.contextual_formatting_enabled = false;
    }

    let context_tone = context
        .and_then(|c| c.tone.as_deref())
        .map(str::trim)
        .filter(|t| !t.is_empty());
    let mapping_profile = mapping.map(|m| m.profile.trim()).filter(|p| !p.is_empty());
    context_tone
        .or(mapping_profile)
        .map(str::to_owned)
        .unwrap_or_else(|| cfg.default_tone.clone())
}

/// Resolves the context that would apply to a foreground window without
/// running the full pipeline, and emits its name to the pill so the recording
/// state can show where the dictation is headed. Used at recording start,
/// where the full `open_config_and_context` (chain validation + error pills)
/// is too heavy and would double-resolve; the pipeline re-emits the
/// domain-refined context at processing time.
///
/// The hwnd→process-name read can come up empty (elevated target processes,
/// race between capture and start), so the process name falls back to the
/// live foreground window. Browser domains are read from the captured target
/// too, making website-only groups available before recording begins; an
/// unresolved context remains hidden until processing resolves one.
pub(super) fn emit_context_for_window(app: &AppHandle, hwnd: usize) {
    let process_name = if hwnd != 0 {
        window_context::get_process_name_for_hwnd(hwnd)
    } else {
        None
    }
    .or_else(window_context::get_active_process_name)
    .unwrap_or_default();
    // Read the domain from the captured browser window as well as the exe.
    // This keeps website-only context groups accurate on the recording pill;
    // the bounded UIA probe remains best-effort and falls back to exe lookup.
    let browser_domain = if window_context::is_browser_exe(&process_name) {
        crate::core::browser_probe::read_browser_domain_for_window(hwnd)
    } else {
        None
    };
    let db_handle = app.state::<crate::DbHandle>().inner().clone();
    if let Ok(context) =
        crate::core::context::resolve_context(&db_handle, &process_name, browser_domain.as_deref())
    {
        crate::pipeline::pill::queue_pill_context(&context.name);
    }
}

/// Casual/formal cleanup sometimes omits a closing period on short utterances.
/// That leaves a bare word before the caret, which the contextual-capitalization
/// probe then reads as a mid-sentence continuation — so the *next* dictation has
/// its first letter lowercased. Appending a period when the cleaned text ends on
/// a plain word makes consecutive dictations read as separate sentences and
/// capitalize naturally.
///
/// Deliberately conservative:
/// - `very_casual` is skipped (its style is intentionally near-punctuation-free).
/// - `none`/verbatim intensity is skipped (must echo speech without editorializing).
/// - Only acts when the last non-space character is alphanumeric. Text already
///   ending in terminal punctuation, a comma/colon/dash (intentional
///   continuation), or a closing bracket/quote is left untouched.
pub(super) fn ensure_terminal_punctuation(
    text: &str,
    profile: &str,
    cleanup_intensity: &str,
) -> String {
    if profile == "very_casual" || cleanup_intensity == "none" {
        return text.to_owned();
    }
    let trimmed = text.trim_end();
    match trimmed.chars().next_back() {
        Some(last) if last.is_alphanumeric() => {
            // Chinese/Japanese text uses the full-width period; a Western "."
            // reads as out of place after CJK ideographs or kana.
            let punct = if is_cjk(last) { "。" } else { "." };
            // Preserve any trailing whitespace the model emitted after the word.
            format!("{trimmed}{punct}{}", &text[trimmed.len()..])
        }
        _ => text.to_owned(),
    }
}

fn is_cjk(c: char) -> bool {
    matches!(c,
        '\u{4e00}'..='\u{9fff}'   // CJK Unified Ideographs
        | '\u{3400}'..='\u{4dbf}' // CJK Unified Ideographs Extension A
        | '\u{3040}'..='\u{309f}' // Hiragana
        | '\u{30a0}'..='\u{30ff}' // Katakana
    )
}
