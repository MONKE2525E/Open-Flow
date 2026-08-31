use super::{
    is_gemini_25_model, model_supports_gemini_thinking, GeminiGenConfig, GeminiThinkingConfig,
};

pub fn gemini_generation_config(model: &str, max_output_tokens: u32) -> GeminiGenConfig {
    let thinking_config = if is_gemini_25_model(model) {
        Some(GeminiThinkingConfig {
            thinking_budget: Some(0),
            thinking_level: None,
        })
    } else if model_supports_gemini_thinking(model) {
        // "low", not "minimal": Gemini 3 models accept "low" and "high"
        // universally, while "minimal" is per-model and newer flashes reject it
        // outright with a 400 ("Thinking level MINIMAL is not supported").
        Some(GeminiThinkingConfig {
            thinking_budget: None,
            thinking_level: Some("low".to_string()),
        })
    } else {
        None
    };

    GeminiGenConfig {
        thinking_config,
        max_output_tokens: Some(max_output_tokens),
        temperature: Some(0.0),
    }
}
