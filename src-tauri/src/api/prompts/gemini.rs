use super::{
    GeminiGenConfig, GeminiThinkingConfig, is_gemini_25_model, model_supports_gemini_thinking,
};

pub fn gemini_generation_config(model: &str, max_output_tokens: u32) -> GeminiGenConfig {
    let thinking_config = if is_gemini_25_model(model) {
        Some(GeminiThinkingConfig {
            thinking_budget: Some(0),
            thinking_level: None,
        })
    } else if model_supports_gemini_thinking(model) {
        Some(GeminiThinkingConfig {
            thinking_budget: None,
            thinking_level: Some("minimal".to_string()),
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
