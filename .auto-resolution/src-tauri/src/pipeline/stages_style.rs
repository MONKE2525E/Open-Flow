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
    let mapping_profile = mapping
        .map(|m| m.profile.trim())
        .filter(|p| !p.is_empty());
    context_tone
        .or(mapping_profile)
        .map(str::to_owned)
        .unwrap_or_else(|| cfg.default_tone.clone())
}

/// Resolves the tone profile that would apply to a foreground window without
/// running the full pipeline, and emits it to the pill so the recording state
/// can show which style will apply. Used at recording start, where the full
/// `open_config_and_context` (chain validation + error pills) is too heavy and
/// would double-resolve; the pipeline re-emits the same value at processing
/// time so the shown profile always matches what actually runs.
///
/// Deliberately never fails silently: the hwnd→process-name read can come up
/// empty (elevated target processes, race between capture and start), so the
/// process name falls back to the live foreground window and then to no
/// mapping at all — `apply_app_style_overrides` falls back to the default
/// tone, so the recording pill always shows *some* mode label. Without this
/// fallback the label only ever appeared once processing began (where the
/// pipeline resolves the name through a different path), which made the
/// mode display useless right when it matters.
pub(super) fn emit_profile_for_window(app: &AppHandle, hwnd: usize) {
    let process_name = if hwnd != 0 {
        window_context::get_process_name_for_hwnd(hwnd)
    } else {
        None
    }
    .or_else(window_context::get_active_process_name)
    .unwrap_or_default();
    let Ok(settings) = store::settings_snapshot(app) else {
        return;
    };
    let mapping = resolve_app_mapping(Some(&settings), &process_name);
    let mut cfg = store::load_pipeline_config(&settings);
    // Exe-only context lookup (no address-bar probe here — this runs on the
    // recording-start path and must stay fast; the real pipeline resolves
    // the domain-refined context and re-emits the true profile).
    let db_handle = app.state::<crate::DbHandle>().inner().clone();
    let context = crate::core::context::resolve_context(&db_handle, &process_name, None).ok();
    let profile = apply_app_style_overrides(&mut cfg, mapping.as_ref(), context.as_ref());
    crate::pipeline::pill::queue_pill_profile(&profile);
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
