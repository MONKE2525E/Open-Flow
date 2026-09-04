//! Transient, approval-gated post-dictation repair flow.
//!
//! The model only proposes a closed typed action. This module owns the safe
//! input snapshot, deterministic validation, and the final mutation.

use super::*;
use serde::Serialize;
use serde_json::Value;
use std::sync::atomic::Ordering;

const COMPLAINT_LIMIT: usize = 2_000;
const SHORT_TEXT_LIMIT: usize = 3_000;
const EXCERPT_LIMIT: usize = 3_000;
// The JSON action shape itself never needs more than ~60-80 tokens; the rest
// of the old 320-token budget just gave a misbehaving model room to ramble
// inside a JSON string field (term/mistake), which then overflowed the fixed-
// width proposal card with no way to scroll. Trimming the ceiling makes that
// structurally harder, on top of the display-side truncation in `summary()`.
const REPAIR_MAX_OUTPUT_TOKENS: u32 = 120;
const REPAIR_TIMEOUT_SECS: u64 = 45;
const NO_SAFE_REPAIR_MESSAGE: &str = "I couldn't map this to a safe Verenu setting. Try speaking a little closer to the microphone and a little slower. If you want a reusable phrase, add a vocabulary item or snippet manually in Verenu.";

#[derive(Clone, Debug, Serialize)]
pub struct RepairSettings {
    pub cleanup_enabled: bool,
    pub cleanup_intensity: String,
    pub default_tone: String,
    pub contextual_formatting_enabled: bool,
    pub caps_lock_uppercase_enabled: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct RepairSnapshot {
    pub id: u64,
    pub raw: String,
    pub cleaned: String,
    pub delivered_private: String,
    pub process_name: String,
    pub browser_domain: Option<String>,
    pub context_id: i64,
    pub context_name: String,
    pub dictionary: Vec<db::DictionaryEntry>,
    pub settings: RepairSettings,
}

#[derive(Clone, Debug)]
pub struct RepairSession {
    pub snapshot: RepairSnapshot,
    pub proposal: Option<ValidatedProposal>,
}

#[derive(Clone, Debug, Serialize)]
pub struct RepairProposalView {
    pub id: u64,
    pub summary: String,
    pub scope: String,
}

#[derive(Clone, Debug)]
pub struct ValidatedProposal {
    pub id: u64,
    pub(super) action: RepairAction,
    pub summary: String,
    pub scope: String,
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn begin_feedback(
    app: &AppHandle,
    state: &SharedState,
    raw: &str,
    cleaned: &str,
    delivered_private: &str,
    process_name: String,
    browser_domain: Option<String>,
    context: &db::Context,
    dictionary: &[db::DictionaryEntry],
    cfg: &store::PipelineConfig,
) {
    let snapshot = RepairSnapshot {
        id: REPAIR_ID.fetch_add(1, Ordering::Relaxed),
        raw: raw.to_string(),
        cleaned: cleaned.to_string(),
        delivered_private: delivered_private.to_string(),
        process_name,
        browser_domain,
        context_id: context.id,
        context_name: context.name.clone(),
        dictionary: dictionary.to_vec(),
        settings: RepairSettings {
            cleanup_enabled: cfg.cleanup_enabled,
            cleanup_intensity: cfg.cleanup_intensity.clone(),
            default_tone: cfg.default_tone.clone(),
            contextual_formatting_enabled: cfg.contextual_formatting_enabled,
            caps_lock_uppercase_enabled: cfg.caps_lock_uppercase_enabled,
        },
    };
    if let Ok(mut locked) = state.lock() {
        locked.repair = Some(RepairSession {
            snapshot,
            proposal: None,
        });
    }
    super::show_pill(app, "feedback_prompt");
}

pub(crate) fn clear(state: &SharedState) {
    if let Ok(mut locked) = state.lock() {
        locked.repair = None;
    }
}

pub(crate) fn enter_input(app: &AppHandle) {
    super::show_pill(app, "repair_input");
}

pub(crate) fn enter_repair_input(app: &AppHandle) {
    enter_input(app);
}

pub(crate) fn emit_repair_error(app: &AppHandle, message: &str) {
    emit_error(app, message);
}

pub(crate) async fn diagnose_repair(
    app: AppHandle,
    state: SharedState,
    complaint: String,
) -> anyhow::Result<()> {
    diagnose(app, state, complaint).await
}

/// Transcribes whatever was just recorded as the repair complaint and hands
/// control back to the input card. Shared by the explicit dictate flow and
/// the global hotkey path (pressing the normal record hotkey while the
/// repair-input pill is open dictates into it directly, same as any other
/// target) — both end a repair-complaint recording the same way.
pub(crate) async fn finish_complaint_recording(app: AppHandle, state: SharedState) {
    match super::transcribe_input_only(app.clone(), state).await {
        Ok(text) => {
            app.emit("repair-complaint-result", &text).ok();
            enter_repair_input(&app);
        }
        Err(error) => emit_repair_error(&app, &crate::api::user_facing_error(&error)),
    }
}

pub(crate) async fn apply_repair(
    app: AppHandle,
    state: SharedState,
    proposal_id: u64,
) -> anyhow::Result<String> {
    apply(app, state, proposal_id).await
}

pub(crate) fn clear_repair(state: &SharedState) {
    clear(state);
}

pub(crate) fn emit_error(app: &AppHandle, message: &str) {
    app.emit("repair-error", message).ok();
    super::show_pill(app, "repair_error");
}

fn bounded_excerpt(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= SHORT_TEXT_LIMIT {
        return text.to_string();
    }
    let side = EXCERPT_LIMIT / 2;
    chars[..side]
        .iter()
        .chain(chars[chars.len() - side..].iter())
        .collect()
}

fn safe_text(text: &str, limit: usize) -> String {
    text.chars()
        .filter(|ch| !ch.is_control() || matches!(ch, '\n' | '\t'))
        .take(limit)
        .collect()
}

/// Returns a bounded window around the first changed span in `text` compared
/// with `reference`. This keeps the model focused on the transformation that
/// the user is reporting instead of sending an entire long dictation.
fn diff_excerpt(text: &str, reference: &str) -> String {
    let text_chars: Vec<char> = text.chars().collect();
    if text_chars.len() <= SHORT_TEXT_LIMIT {
        return text.to_string();
    }
    let reference_chars: Vec<char> = reference.chars().collect();
    let prefix = text_chars
        .iter()
        .zip(reference_chars.iter())
        .take_while(|(left, right)| left == right)
        .count();
    let suffix_limit = text_chars.len().saturating_sub(prefix);
    let suffix = text_chars
        .iter()
        .rev()
        .zip(reference_chars.iter().rev())
        .take(suffix_limit)
        .take_while(|(left, right)| left == right)
        .count();
    let changed_end = text_chars.len().saturating_sub(suffix);
    if prefix == text_chars.len() {
        return bounded_excerpt(text);
    }
    let context = 1_000;
    let mut start = prefix.saturating_sub(context);
    let mut end = (changed_end + context).min(text_chars.len());
    if end.saturating_sub(start) > EXCERPT_LIMIT {
        end = (start + EXCERPT_LIMIT).min(text_chars.len());
        start = end.saturating_sub(EXCERPT_LIMIT);
    }
    let mut excerpt: String = text_chars[start..end].iter().collect();
    if start > 0 {
        excerpt.insert(0, '…');
    }
    if end < text_chars.len() {
        excerpt.push('…');
    }
    excerpt
}

fn model_input(snapshot: &RepairSnapshot, complaint: &str) -> String {
    let complaint = safe_text(complaint, COMPLAINT_LIMIT);
    let raw = safe_text(
        &diff_excerpt(&snapshot.raw, &snapshot.cleaned),
        SHORT_TEXT_LIMIT,
    );
    let cleaned = safe_text(
        &diff_excerpt(&snapshot.cleaned, &snapshot.raw),
        SHORT_TEXT_LIMIT,
    );
    let delivered = safe_text(
        &diff_excerpt(&snapshot.delivered_private, &snapshot.cleaned),
        SHORT_TEXT_LIMIT,
    );
    let dictionary = snapshot
        .dictionary
        .iter()
        .take(16)
        .map(|entry| {
            format!(
                "{}|{}|{}",
                entry.id,
                safe_text(&entry.term, 80),
                entry
                    .mistake
                    .as_deref()
                    .map(|value| safe_text(value, 80))
                    .unwrap_or_default()
            )
        })
        .collect::<Vec<_>>()
        .join(";");
    format!(
        "Complaint:{complaint}\nRaw:{raw}\nCleaned:{cleaned}\nDelivered:{delivered}\nTarget:{}|{}|{}\nDictionary(id|term|mistake):{dictionary}\nSettings:cleanup_enabled={}|cleanup_intensity={}|tone={}|contextual_formatting={}|caps_lock_uppercase={}",
        safe_text(&snapshot.process_name, 80),
        safe_text(snapshot.browser_domain.as_deref().unwrap_or("none"), 120),
        safe_text(&snapshot.context_name, 80),
        snapshot.settings.cleanup_enabled,
        safe_text(&snapshot.settings.cleanup_intensity, 24),
        safe_text(&snapshot.settings.default_tone, 24),
        snapshot.settings.contextual_formatting_enabled,
        snapshot.settings.caps_lock_uppercase_enabled,
    )
}

const REPAIR_SYSTEM_PROMPT: &str = r#"You read one user complaint about a Verenu dictation and decide whether it describes a specific, fixable mistake. Return JSON only, no markdown, exactly one of:
{"status":"unsupported","action":null}
{"status":"proposed","action":{"kind":"dictionary","operation":"add|update|remove","dictionary_id":null or number,"term":string or null,"mistake":string or null,"scope":"context"|"everywhere","expected_term":string or null,"expected_mistake":string or null}}
{"status":"proposed","action":{"kind":"setting","key":"cleanup_enabled|cleanup_intensity|default_tone|contextual_formatting_enabled|caps_lock_uppercase_enabled","value":boolean or string,"expected_value":boolean or string}}

The complaint is itself often dictated, so it can carry small transcription typos or odd spacing (e.g. "oout" instead of "out") — read past those to what the user actually means, the same way you'd read past a typo in any message. Do not reject a complaint just because a word in it is spelled oddly.

DICTIONARY fix: needs two things named anywhere in the complaint, in any order or phrasing — the word the user actually meant (term) and the word Verenu produced instead (mistake). Both may be informal, invented, or nonsense-sounding (names, slang, made-up words, nicknames) — that alone is not "noise," it's just an unusual word. "Noise" means the complaint never actually names what was meant, e.g. "that was totally wrong" with no words given. Do not require the user to say "dictionary," "vocabulary," or "reusable rule" — any phrasing that names both words is enough evidence.
Examples (term / mistake):
- "yaba became pooo poo" -> term "yaba", mistake "pooo poo"
- "I wanted to say yaba bop but it came oout as yaba boo" -> term "yaba bop", mistake "yaba boo" (typo "oout" ignored)
- "it heard pool request instead of pull request" -> term "pull request", mistake "pool request" ("heard X instead of Y" means X is the mistake)
- "pool request should have been pull request" -> term "pull request", mistake "pool request"
- "that was totally wrong" -> unsupported, no specific words named
- "stop capitalizing randomly" -> not a dictionary fix, see setting example below

SETTING fix: only for the five keys listed above, only when the complaint clearly describes that exact behavior (too aggressive cleanup, wrong tone, spacing before punctuation, random capitalization, Caps Lock typing uppercase). Never invent a setting, context, dictionary target, or snippet, and never propose a snippet. Never fix ordinary grammar or a no-op such as "the" -> "the".

Use only the supplied dictionary ids for update/remove. For a dictionary action, default scope to "context" (the app this dictation happened in) — only use "everywhere" when the complaint itself says the mistake happens in general, everywhere, or across apps. term and mistake are single words or short phrases (a few words at most) — never a sentence, never the full complaint restated. Do not include extra fields."#;

async fn diagnose_with_model(
    app: &AppHandle,
    cfg: &store::PipelineConfig,
    input: &str,
) -> anyhow::Result<String> {
    let mut chain = Vec::new();
    if let Some((provider, model)) = store::parse_model_id(&cfg.cleanup_default_model) {
        chain.push((provider, model));
    }
    for id in &cfg.cleanup_fallback_models {
        if let Some((provider, model)) = store::parse_model_id(id) {
            if !chain.iter().any(|(p, m)| p == &provider && m == &model) {
                chain.push((provider, model));
            }
        }
    }
    let mut last_error = None;
    for (provider, model) in chain {
        let is_local = provider == store::LOCAL;
        let key = cfg.key_for(&provider);
        if !is_local && key.is_empty() {
            continue;
        }
        let result = if is_local {
            let manager = app
                .state::<crate::local_llm::LocalLlmManager>()
                .inner()
                .clone();
            manager
                .cleanup_with_prompt(
                    app,
                    &model,
                    input,
                    REPAIR_SYSTEM_PROMPT,
                    REPAIR_MAX_OUTPUT_TOKENS,
                )
                .await
        } else {
            cleanup::structured_request(
                input,
                ProviderId::from_str(&provider),
                key,
                &model,
                REPAIR_SYSTEM_PROMPT,
                REPAIR_MAX_OUTPUT_TOKENS,
                0,
            )
            .await
        };
        match result {
            Ok(value) if !value.trim().is_empty() => return Ok(value),
            Ok(_) => last_error = Some(anyhow::anyhow!("repair provider returned empty output")),
            Err(error) => last_error = Some(error),
        }
    }
    Err(last_error
        .unwrap_or_else(|| anyhow::anyhow!("No configured repair-capable provider/model")))
}

pub(crate) async fn diagnose(
    app: AppHandle,
    state: SharedState,
    complaint: String,
) -> anyhow::Result<()> {
    let complaint = complaint
        .trim()
        .chars()
        .take(COMPLAINT_LIMIT)
        .collect::<String>();
    if complaint.is_empty() {
        anyhow::bail!("Tell Verenu what went wrong first")
    }
    let snapshot = state
        .lock()
        .map_err(|_| anyhow::anyhow!("Repair state lock was poisoned"))?
        .repair
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("Repair session expired"))?
        .snapshot
        .clone();
    super::show_pill(&app, "repair_processing");

    let settings = store::settings_snapshot(&app).map_err(anyhow::Error::msg)?;
    let cfg = store::load_pipeline_config(&settings);
    let output = tokio::time::timeout(
        std::time::Duration::from_secs(REPAIR_TIMEOUT_SECS),
        diagnose_with_model(&app, &cfg, &model_input(&snapshot, &complaint)),
    )
    .await
    .map_err(|_| anyhow::anyhow!("Repair diagnosis timed out"))?
    .inspect_err(|e| log::warn!("repair: model call failed: {e}"))?;
    let parsed = parse_model_proposal(&output).map_err(|e| {
        log::warn!("repair: could not parse model output as JSON: {e}");
        anyhow::anyhow!(NO_SAFE_REPAIR_MESSAGE)
    })?;
    let proposal = if parsed.status != "proposed" {
        log::debug!("repair: model returned status={}", parsed.status);
        anyhow::bail!(NO_SAFE_REPAIR_MESSAGE)
    } else {
        validate_action(
            &snapshot,
            &complaint,
            parsed
                .action
                .ok_or_else(|| anyhow::anyhow!(NO_SAFE_REPAIR_MESSAGE))?,
        )
        .map_err(|e| {
            log::warn!("repair: proposal failed validation: {e}");
            anyhow::anyhow!(NO_SAFE_REPAIR_MESSAGE)
        })?
    };
    let view = RepairProposalView {
        id: proposal.id,
        summary: proposal.summary.clone(),
        scope: proposal.scope.clone(),
    };
    {
        let mut locked = state
            .lock()
            .map_err(|_| anyhow::anyhow!("Repair state lock was poisoned"))?;
        let session = locked
            .repair
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("Repair session expired"))?;
        if session.snapshot.id != snapshot.id {
            anyhow::bail!("Repair session was replaced by a newer dictation")
        }
        session.proposal = Some(proposal);
    }
    app.emit("repair-proposal", &view)
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    super::show_pill(&app, "repair_proposal");
    Ok(())
}

fn parse_model_proposal(output: &str) -> anyhow::Result<ModelProposal> {
    if let Ok(value) = serde_json::from_str::<ModelProposal>(output.trim()) {
        return Ok(value);
    }
    let fenced = output
        .split("```")
        .nth(1)
        .unwrap_or("")
        .trim_start_matches("json")
        .trim();
    serde_json::from_str::<ModelProposal>(fenced)
        .map_err(|_| anyhow::anyhow!("Repair response was not valid structured output"))
}

pub(crate) async fn apply(
    app: AppHandle,
    state: SharedState,
    proposal_id: u64,
) -> anyhow::Result<String> {
    let session = state
        .lock()
        .map_err(|_| anyhow::anyhow!("Repair state lock was poisoned"))?
        .repair
        .clone()
        .ok_or_else(|| anyhow::anyhow!("Repair session expired"))?;
    let proposal = session
        .proposal
        .ok_or_else(|| anyhow::anyhow!("No repair proposal is awaiting approval"))?;
    if proposal.id != proposal_id {
        anyhow::bail!("Repair proposal is stale")
    }
    let result = match proposal.action {
        RepairAction::Dictionary {
            operation,
            dictionary_id,
            term,
            mistake,
            scope,
            expected_term,
            expected_mistake,
        } => {
            let scope_context_id = resolve_scope_id(&session.snapshot, scope);
            let db = app.state::<crate::DbHandle>().inner().clone();
            let operation_name = match operation {
                DictionaryOperation::Add => "add",
                DictionaryOperation::Update => "update",
                DictionaryOperation::Remove => "remove",
            };
            let id = tokio::task::spawn_blocking(move || {
                db::apply_dictionary_repair(
                    &db,
                    operation_name,
                    dictionary_id,
                    term.as_deref(),
                    mistake.as_deref(),
                    scope_context_id,
                    expected_term.as_deref(),
                    expected_mistake.as_deref(),
                )
            })
            .await
            .map_err(|e| anyhow::anyhow!(e.to_string()))??;
            format!("Vocabulary item applied (entry {id})")
        }
        RepairAction::Setting {
            key,
            value,
            expected_value,
        } => {
            let settings = store::settings_handle(&app).map_err(anyhow::Error::msg)?;
            let key_for_thread = key.clone();
            let value_for_thread = value.clone();
            let expected_for_thread = expected_value.clone();
            tokio::task::spawn_blocking(move || {
                let snapshot = settings.snapshot().map_err(anyhow::Error::msg)?;
                let cfg = store::load_pipeline_config(&snapshot);
                let current = config_setting(&cfg, &key_for_thread).unwrap_or(Value::Null);
                if current != expected_for_thread {
                    anyhow::bail!("Setting changed while you were reviewing it")
                }
                crate::commands::validate_setting(&key_for_thread, &value_for_thread)
                    .map_err(anyhow::Error::msg)?;
                if key_for_thread == store::CONTEXTUAL_FORMATTING {
                    settings
                        .set(store::CONTEXTUAL_FORMATTING, value_for_thread.clone())
                        .map_err(anyhow::Error::msg)?;
                    settings
                        .set(store::CONTEXTUAL_CAPS, value_for_thread.clone())
                        .map_err(anyhow::Error::msg)?;
                    settings
                        .set(store::AUTO_SPACING, value_for_thread)
                        .map_err(anyhow::Error::msg)?;
                    settings.save().map_err(anyhow::Error::msg)
                } else {
                    settings
                        .save_value(key_for_thread, value_for_thread)
                        .map_err(anyhow::Error::msg)
                }
            })
            .await
            .map_err(|e| anyhow::anyhow!(e.to_string()))??;
            format!("{} updated", setting_label(&key).unwrap_or("Setting"))
        }
    };
    clear(&state);
    super::show_pill(&app, "repair_done");
    app.emit("repair-applied", &result).ok();
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::super::repair_proposal::DISPLAY_TEXT_LIMIT;
    use super::super::repair_proposal::{truncate_for_display, validate_action, RepairAction};
    use super::*;

    fn snapshot() -> RepairSnapshot {
        RepairSnapshot {
            id: 1,
            raw: "I said pull request and it wrote pool request".into(),
            cleaned: "I said pull request and it wrote pool request".into(),
            delivered_private: "I said pull request and it wrote pool request".into(),
            process_name: "code.exe".into(),
            browser_domain: None,
            context_id: 7,
            context_name: "Development".into(),
            dictionary: vec![db::DictionaryEntry {
                id: 3,
                term: "pull request".into(),
                mistake: Some("pool request".into()),
                auto_learned: false,
                correction_count: 0,
                confidence_tier: "manual".into(),
                last_seen_at: None,
                created_at: "".into(),
            }],
            settings: RepairSettings {
                cleanup_enabled: true,
                cleanup_intensity: "balanced".into(),
                default_tone: "casual".into(),
                contextual_formatting_enabled: true,
                caps_lock_uppercase_enabled: false,
            },
        }
    }

    #[test]
    fn diff_excerpt_keeps_changed_region_bounded() {
        let reference = "a".repeat(8_000);
        let mut text = reference.clone();
        text.replace_range(4_000..4_001, "b");
        let excerpt = diff_excerpt(&text, &reference);
        assert!(excerpt.chars().count() <= EXCERPT_LIMIT + 2);
        assert!(excerpt.contains('b'));
        assert!(excerpt.starts_with('…'));
        assert!(excerpt.ends_with('…'));
    }

    #[test]
    fn truncate_for_display_caps_long_text_but_leaves_short_text_alone() {
        assert_eq!(truncate_for_display("pull request"), "pull request");
        let long = "a".repeat(DISPLAY_TEXT_LIMIT + 20);
        let truncated = truncate_for_display(&long);
        assert_eq!(truncated.chars().count(), DISPLAY_TEXT_LIMIT + 1); // +1 for the ellipsis
        assert!(truncated.ends_with('…'));
    }

    #[test]
    fn strict_parser_rejects_unknown_fields_and_unauthorized_settings() {
        assert!(
            parse_model_proposal(r#"{"status":"proposed","extra":true,"action":null}"#,).is_err()
        );

        let action: RepairAction = serde_json::from_str(
            r#"{"kind":"setting","key":"api_key","value":true,"expected_value":false}"#,
        )
        .unwrap();
        assert!(validate_action(&snapshot(), "turn off cleanup", action).is_err());
    }

    #[test]
    fn dictionary_scope_is_a_closed_enum_not_an_arbitrary_id() {
        // The model is never shown a numeric context id (it has no reliable
        // way to know one), so scope is a semantic choice resolved
        // deterministically from the snapshot instead of a raw id the model
        // could guess wrong or hallucinate — any value outside
        // context/everywhere fails to parse at all, rather than needing a
        // runtime validity check.
        assert!(serde_json::from_str::<RepairAction>(
            r#"{"kind":"dictionary","operation":"add","dictionary_id":null,"term":"pull request","mistake":"pool request","scope":"some_other_app","expected_term":null,"expected_mistake":null}"#,
        )
        .is_err());

        let everywhere: RepairAction = serde_json::from_str(
            r#"{"kind":"dictionary","operation":"add","dictionary_id":null,"term":"pull request","mistake":"pool request","scope":"everywhere","expected_term":null,"expected_mistake":null}"#,
        )
        .unwrap();
        assert!(validate_action(
            &snapshot(),
            "I said pull request and it wrote pool request",
            everywhere
        )
        .is_ok());
    }

    #[test]
    fn valid_dictionary_add_requires_observed_evidence() {
        let action: RepairAction = serde_json::from_str(
            r#"{"kind":"dictionary","operation":"add","dictionary_id":null,"term":"pull request","mistake":"pool request","scope":"context","expected_term":null,"expected_mistake":null}"#,
        )
        .unwrap();
        assert!(validate_action(
            &snapshot(),
            "I said pull request and it wrote pool request",
            action
        )
        .is_ok());
    }

    #[test]
    fn natural_phrasing_without_a_keyword_is_accepted() {
        // Regression test: an earlier keyword allowlist ("said"/"wrote"/
        // "transcribed"/...) rejected ordinary phrasing like "X became Y" —
        // including the repair input's own placeholder example — even
        // though both terms were genuinely present in what was transcribed.
        let mut snap = snapshot();
        snap.raw = "please open the pool request".into();
        snap.cleaned = snap.raw.clone();
        snap.delivered_private = snap.raw.clone();
        let action: RepairAction = serde_json::from_str(
            r#"{"kind":"dictionary","operation":"add","dictionary_id":null,"term":"pull request","mistake":"pool request","scope":"context","expected_term":null,"expected_mistake":null}"#,
        )
        .unwrap();
        assert!(
            validate_action(&snap, "pool request should have been pull request", action).is_ok()
        );
    }

    #[test]
    fn no_op_dictionary_repairs_are_rejected() {
        let action: RepairAction = serde_json::from_str(
            r#"{"kind":"dictionary","operation":"update","dictionary_id":3,"term":"pull request","mistake":"pool request","scope":"context","expected_term":"pull request","expected_mistake":"pool request"}"#,
        )
        .unwrap();
        assert!(validate_action(
            &snapshot(),
            "I said pull request and it wrote pool request",
            action
        )
        .is_err());
    }

    #[test]
    fn scope_only_dictionary_update_is_not_a_no_op() {
        // Same term/mistake as the existing entry, but a different target
        // scope (everywhere vs. the snapshot's context 7) is a real change —
        // apply_dictionary_repair reassigns the entry's context on every
        // update, so this must not be rejected as a no-op.
        let action: RepairAction = serde_json::from_str(
            r#"{"kind":"dictionary","operation":"update","dictionary_id":3,"term":"pull request","mistake":"pool request","scope":"everywhere","expected_term":"pull request","expected_mistake":"pool request"}"#,
        )
        .unwrap();
        assert!(validate_action(
            &snapshot(),
            "I said pull request and it wrote pool request",
            action
        )
        .is_ok());
    }
}
