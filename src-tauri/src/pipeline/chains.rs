use super::*;

pub(super) fn transcription_provider_from_str(s: &str) -> transcription::Provider {
    match s {
        "openai" => transcription::Provider::OpenAI,
        "google" => transcription::Provider::Google,
        _ => transcription::Provider::Groq,
    }
}
pub(super) fn transcription_model_chain(cfg: &store::PipelineConfig) -> Vec<(String, String)> {
    let mut chain = Vec::<(String, String)>::new();
    if let Some((provider, model)) = store::parse_model_id(&cfg.transcription_default_model) {
        chain.push((provider, model));
    }
    for id in &cfg.transcription_fallback_models {
        if let Some((provider, model)) = store::parse_model_id(id) {
            if !chain.iter().any(|(p, m)| p == &provider && m == &model) {
                chain.push((provider, model));
            }
        }
    }
    chain
}

pub(super) fn cleanup_model_chain(cfg: &store::PipelineConfig) -> Vec<(String, String)> {
    let mut chain = Vec::<(String, String)>::new();
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
    chain
}

pub(super) fn has_transcription_key_in_chain(cfg: &store::PipelineConfig) -> bool {
    transcription_model_chain(cfg)
        .iter()
        .any(|(provider, _)| !cfg.key_for(provider).is_empty())
}

pub(super) fn has_cleanup_key_in_chain(cfg: &store::PipelineConfig) -> bool {
    cleanup_model_chain(cfg)
        .iter()
        .any(|(provider, _)| !cfg.key_for(provider).is_empty())
}

pub(super) fn trim_err(s: &str) -> String {
    let s = s.trim();
    if s.chars().count() > 120 {
        format!("{}…", s.chars().take(117).collect::<String>())
    } else {
        s.to_string()
    }
}
