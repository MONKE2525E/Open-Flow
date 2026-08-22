//! Repair proposal types and deterministic validation.
//!
//! The model only proposes a closed typed action (ModelProposal ->
//! RepairAction); everything in this module is pure computation over the
//! RepairSnapshot -- no I/O, no async, no AppHandle. Session state, model
//! calls, and the final mutation live in repair.rs.

use super::*;
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::atomic::{AtomicU64, Ordering};

pub(super) const DISPLAY_TEXT_LIMIT: usize = 60;

/// Monotonic id source shared with the repair session in repair.rs.
pub(super) static REPAIR_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ModelProposal {
    pub(super) status: String,
    pub(super) action: Option<RepairAction>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "kind", deny_unknown_fields)]
pub(super) enum RepairAction {
    #[serde(rename = "dictionary")]
    Dictionary {
        operation: DictionaryOperation,
        dictionary_id: Option<i64>,
        term: Option<String>,
        mistake: Option<String>,
        scope: DictionaryScope,
        expected_term: Option<String>,
        expected_mistake: Option<String>,
    },
    #[serde(rename = "setting")]
    Setting {
        key: String,
        value: Value,
        expected_value: Value,
    },
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum DictionaryOperation {
    Add,
    Update,
    Remove,
}

/// Semantic scope choice instead of a raw context id: the model has no
/// reliable way to know the numeric id of "the app this dictation happened
/// in" (it's never shown one), so it used to have to guess — sometimes
/// landing on an arbitrary/invalid number, sometimes drifting to Everywhere
/// by default. Resolving the actual id deterministically from the snapshot
/// (see `resolve_scope_id`) makes "assign it to the app's context group"
/// reliable instead of a guess, and makes an out-of-range scope structurally
/// impossible rather than something validation has to catch.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub(super) enum DictionaryScope {
    Context,
    Everywhere,
}

pub(super) fn resolve_scope_id(snapshot: &RepairSnapshot, scope: DictionaryScope) -> i64 {
    match scope {
        DictionaryScope::Context => snapshot.context_id,
        DictionaryScope::Everywhere => db::EVERYWHERE_CONTEXT_ID,
    }
}

/// Caps a string to `DISPLAY_TEXT_LIMIT` chars (never mid-character-boundary)
/// for the proposal card, which has no scrollbar and a bounded native window
/// height — display-only, never applied to the value actually written to the
/// dictionary or a setting.
pub(super) fn truncate_for_display(text: &str) -> std::borrow::Cow<'_, str> {
    if text.chars().count() <= DISPLAY_TEXT_LIMIT {
        return std::borrow::Cow::Borrowed(text);
    }
    let mut truncated: String = text.chars().take(DISPLAY_TEXT_LIMIT).collect();
    truncated.push('…');
    std::borrow::Cow::Owned(truncated)
}

impl RepairAction {
    pub(super) fn summary(&self, snapshot: &RepairSnapshot) -> anyhow::Result<(String, String)> {
        match self {
            Self::Dictionary {
                operation,
                dictionary_id,
                term,
                mistake,
                scope,
                ..
            } => {
                let scope_name = scope_name(snapshot, resolve_scope_id(snapshot, *scope))?;
                let summary = match operation {
                    DictionaryOperation::Add => format!(
                        "Add vocabulary item: {} -> {}",
                        truncate_for_display(mistake.as_deref().unwrap_or("spoken form")),
                        truncate_for_display(term.as_deref().unwrap_or("new term"))
                    ),
                    DictionaryOperation::Update => {
                        let id = dictionary_id.ok_or_else(|| anyhow::anyhow!("missing vocabulary target"))?;
                        let current = snapshot.dictionary.iter().find(|e| e.id == id).ok_or_else(|| anyhow::anyhow!("unknown vocabulary target"))?;
                        format!(
                            "Vocabulary item: {} -> {}",
                            truncate_for_display(current.mistake.as_deref().unwrap_or(&current.term)),
                            truncate_for_display(term.as_deref().unwrap_or(&current.term))
                        )
                    }
                    DictionaryOperation::Remove => {
                        let id = dictionary_id.ok_or_else(|| anyhow::anyhow!("missing vocabulary target"))?;
                        let current = snapshot.dictionary.iter().find(|e| e.id == id).ok_or_else(|| anyhow::anyhow!("unknown vocabulary target"))?;
                        format!("Remove vocabulary item {}", truncate_for_display(&current.term))
                    }
                };
                Ok((summary, format!("Apply in: {scope_name}")))
            }
            Self::Setting { key, value, expected_value } => {
                let label = setting_label(key).ok_or_else(|| anyhow::anyhow!("unsupported setting"))?;
                Ok((
                    format!(
                        "{label}: {} -> {}",
                        truncate_for_display(&display_value(expected_value)),
                        truncate_for_display(&display_value(value))
                    ),
                    "Apply globally".into(),
                ))
            }
        }
    }
}

pub(super) fn setting_label(key: &str) -> Option<&'static str> {
    Some(match key {
        store::CLEANUP_ENABLED => "Automatic cleanup",
        store::CLEANUP_INTENSITY => "Cleanup intensity",
        store::DEFAULT_TONE => "Default tone",
        store::CONTEXTUAL_FORMATTING => "Contextual formatting",
        store::CONTEXTUAL_CAPS => "Contextual capitalization",
        store::AUTO_SPACING => "Automatic spacing",
        store::CAPS_LOCK_UPPERCASE => "Caps Lock uppercase",
        _ => return None,
    })
}

pub(super) fn display_value(value: &Value) -> String {
    value.as_str().map(str::to_owned).unwrap_or_else(|| value.to_string())
}

pub(super) fn scope_name(snapshot: &RepairSnapshot, context_id: i64) -> anyhow::Result<String> {
    if context_id == db::EVERYWHERE_CONTEXT_ID {
        Ok("Everywhere".into())
    } else if context_id == snapshot.context_id {
        Ok(snapshot.context_name.clone())
    } else {
        anyhow::bail!("scope is not the current context or Everywhere")
    }
}

pub(super) fn current_setting(snapshot: &RepairSnapshot, key: &str) -> Option<Value> {
    Some(match key {
        store::CLEANUP_ENABLED => json!(snapshot.settings.cleanup_enabled),
        store::CLEANUP_INTENSITY => json!(snapshot.settings.cleanup_intensity),
        store::DEFAULT_TONE => json!(snapshot.settings.default_tone),
        store::CONTEXTUAL_FORMATTING => json!(snapshot.settings.contextual_formatting_enabled),
        store::CONTEXTUAL_CAPS => json!(snapshot.settings.contextual_caps_enabled),
        store::AUTO_SPACING => json!(snapshot.settings.auto_spacing_enabled),
        store::CAPS_LOCK_UPPERCASE => json!(snapshot.settings.caps_lock_uppercase_enabled),
        _ => return None,
    })
}

pub(super) fn config_setting(cfg: &store::PipelineConfig, key: &str) -> Option<Value> {
    Some(match key {
        store::CLEANUP_ENABLED => json!(cfg.cleanup_enabled),
        store::CLEANUP_INTENSITY => json!(cfg.cleanup_intensity),
        store::DEFAULT_TONE => json!(cfg.default_tone),
        store::CONTEXTUAL_FORMATTING => json!(cfg.contextual_formatting_enabled),
        store::CONTEXTUAL_CAPS => json!(cfg.contextual_caps_enabled),
        store::AUTO_SPACING => json!(cfg.auto_spacing_enabled),
        store::CAPS_LOCK_UPPERCASE => json!(cfg.caps_lock_uppercase_enabled),
        _ => return None,
    })
}

pub(super) fn supports_complaint(snapshot: &RepairSnapshot, complaint: &str, action: &RepairAction) -> bool {
    let complaint_lower = complaint.to_lowercase();
    let evidence = format!("{} {} {}", complaint_lower, snapshot.raw, snapshot.delivered_private).to_lowercase();
    match action {
        // The real anti-hallucination guard is the substring checks below: both
        // the proposed correct term and the proposed mistake must actually
        // appear in what the user said or what was transcribed. An earlier
        // keyword allowlist ("said"/"wrote"/"transcribed"/...) additionally
        // required one of those exact words in the complaint, which rejected
        // completely ordinary phrasing like "X became Y" or "X should be
        // Y" — including the app's own placeholder example — so it's gone.
        RepairAction::Dictionary { term, mistake, .. } => {
            term.as_deref().is_some_and(|v| !v.trim().is_empty() && evidence.contains(&v.to_lowercase()))
                && mistake.as_deref().is_some_and(|v| !v.trim().is_empty() && evidence.contains(&v.to_lowercase()))
        }
        RepairAction::Setting { key, .. } => match key.as_str() {
            store::AUTO_SPACING => complaint_lower.contains("spacing") || complaint_lower.contains("space before"),
            store::CAPS_LOCK_UPPERCASE | store::CONTEXTUAL_CAPS => complaint_lower.contains("caps") || complaint_lower.contains("capital") || complaint_lower.contains("uppercase"),
            store::DEFAULT_TONE => complaint_lower.contains("tone") || complaint_lower.contains("formal") || complaint_lower.contains("casual"),
            store::CLEANUP_INTENSITY => complaint_lower.contains("cleanup") || complaint_lower.contains("formatting") || complaint_lower.contains("too aggressive"),
            store::CLEANUP_ENABLED => complaint_lower.contains("cleanup") || complaint_lower.contains("automatic cleanup"),
            _ => false,
        },
    }
}

pub(super) fn same_word(left: &str, right: &str) -> bool {
    left.trim().eq_ignore_ascii_case(right.trim())
}

pub(super) fn validate_action(snapshot: &RepairSnapshot, complaint: &str, action: RepairAction) -> anyhow::Result<ValidatedProposal> {
    if !supports_complaint(snapshot, complaint, &action) {
        anyhow::bail!("No safe repair could be mapped to the observed text")
    }

    match &action {
        RepairAction::Dictionary {
            operation,
            dictionary_id,
            term,
            mistake,
            scope,
            expected_term,
            expected_mistake,
        } => {
            scope_name(snapshot, resolve_scope_id(snapshot, *scope))?;
            match operation {
                DictionaryOperation::Add => {
                    if dictionary_id.is_some() || term.as_deref().unwrap_or("").trim().is_empty() || mistake.as_deref().unwrap_or("").trim().is_empty() {
                        anyhow::bail!("invalid dictionary add")
                    }
                    if same_word(term.as_deref().unwrap_or(""), mistake.as_deref().unwrap_or("")) {
                        anyhow::bail!("dictionary add is a no-op")
                    }
                }
                DictionaryOperation::Update => {
                    let id = dictionary_id.ok_or_else(|| anyhow::anyhow!("missing dictionary target"))?;
                    let current = snapshot.dictionary.iter().find(|e| e.id == id).ok_or_else(|| anyhow::anyhow!("unknown dictionary target"))?;
                    if expected_term.as_deref() != Some(current.term.as_str()) || expected_mistake.as_deref() != current.mistake.as_deref() {
                        anyhow::bail!("dictionary entry changed")
                    }
                    if term.as_deref().unwrap_or("").trim().is_empty() {
                        anyhow::bail!("invalid dictionary update")
                    }
                    // snapshot.dictionary is always the current context's entries
                    // (see query_dictionary_for_context in run_cleanup_and_snippets_for_db),
                    // so resolve_scope_id(scope) == snapshot.context_id means the
                    // proposed scope matches where this entry already lives — a
                    // real no-op only when term, mistake, AND scope are unchanged,
                    // since an update can also exist purely to move an entry's scope.
                    if same_word(term.as_deref().unwrap_or(""), current.term.as_str())
                        && mistake.as_deref().is_none_or(|value| current.mistake.as_deref().is_some_and(|old| same_word(value, old)))
                        && resolve_scope_id(snapshot, *scope) == snapshot.context_id
                    {
                        anyhow::bail!("dictionary update is a no-op")
                    }
                }
                DictionaryOperation::Remove => {
                    let id = dictionary_id.ok_or_else(|| anyhow::anyhow!("missing dictionary target"))?;
                    let current = snapshot.dictionary.iter().find(|e| e.id == id).ok_or_else(|| anyhow::anyhow!("unknown dictionary target"))?;
                    if expected_term.as_deref() != Some(current.term.as_str()) || expected_mistake.as_deref() != current.mistake.as_deref() {
                        anyhow::bail!("dictionary entry changed")
                    }
                }
            }
        }
        RepairAction::Setting { key, value, expected_value } => {
            if setting_label(key).is_none() || current_setting(snapshot, key).as_ref() != Some(expected_value) {
                anyhow::bail!("setting is not allowlisted or changed")
            }
            if matches!(key.as_str(), store::CLEANUP_INTENSITY) && !value.as_str().is_some_and(store::is_supported_cleanup_intensity) {
                anyhow::bail!("invalid cleanup intensity")
            }
            if matches!(key.as_str(), store::DEFAULT_TONE) && !value.as_str().is_some_and(store::is_supported_default_tone) {
                anyhow::bail!("invalid tone")
            }
            if matches!(key.as_str(), store::CLEANUP_ENABLED | store::CONTEXTUAL_CAPS | store::AUTO_SPACING | store::CAPS_LOCK_UPPERCASE) && !value.is_boolean() {
                anyhow::bail!("setting requires boolean")
            }
        }
    }
    let (summary, scope) = action.summary(snapshot)?;
    Ok(ValidatedProposal { id: REPAIR_ID.fetch_add(1, Ordering::Relaxed), action, summary, scope })
}
