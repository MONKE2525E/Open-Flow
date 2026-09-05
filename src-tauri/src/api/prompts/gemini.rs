use crate::api::gemini_types::{GeminiGenConfig, GeminiThinkingConfig};

use super::{is_gemini_25_model, is_gemini_3_model};

/// Models for which the generateContent request below has an explicit,
/// supported dictation policy. Gemini 3.x has no full thinking-off switch:
/// eligible models must receive `thinkingLevel: "minimal"`. Gemini 2.5 Flash
/// and Flash-Lite accept `thinkingBudget: 0`.
pub fn gemini_generation_reasoning_supported(model: &str) -> bool {
    let normalized = model.trim().to_ascii_lowercase();
    if is_gemini_3_model(&normalized) {
        return gemini_3_minimal_supported(&normalized);
    }
    is_gemini_25_model(&normalized)
        && normalized.contains("gemini-2.5-flash")
        && !normalized.contains("gemini-2.5-pro")
}

pub fn ensure_gemini_generation_model(model: &str) -> anyhow::Result<()> {
    if gemini_generation_reasoning_supported(model) {
        Ok(())
    } else {
        anyhow::bail!(
            "Google model '{model}' cannot satisfy Verenu's dictation reasoning policy; Gemini 3.x cleanup requires thinkingLevel=minimal, while Gemini 2.5 Flash/Flash-Lite require thinkingBudget=0."
        )
    }
}

pub fn gemini_generation_config(model: &str, max_output_tokens: u32) -> GeminiGenConfig {
    let normalized = model.trim().to_ascii_lowercase();
    let thinking_config = if is_gemini_3_model(&normalized) {
        Some(GeminiThinkingConfig {
            thinking_budget: None,
            thinking_level: Some("minimal".to_string()),
        })
    } else if is_gemini_25_model(&normalized) {
        Some(GeminiThinkingConfig {
            thinking_budget: Some(0),
            thinking_level: None,
        })
    } else {
        None
    };

    GeminiGenConfig {
        thinking_config,
        max_output_tokens: Some(max_output_tokens),
        // Gemini 3.x rejects deprecated sampling controls. Gemini 2.5 keeps
        // deterministic temperature=0 alongside its zero thinking budget.
        temperature: (!is_gemini_3_model(&normalized)).then_some(0.0),
    }
}

/// Keep the allowlist narrow: an unknown Gemini 3.x model must not receive a
/// level it may not support and must be skipped before a request is made.
fn gemini_3_minimal_supported(model: &str) -> bool {
    model.contains("gemini-3.5-flash-lite") || model.contains("gemini-3.5-flash")
}
