use super::*;

pub(super) fn transcription_model_chain(cfg: &store::PipelineConfig) -> Vec<(String, String)> {
    model_chain(
        &cfg.transcription_default_model,
        &cfg.transcription_fallback_models,
    )
}

pub(super) fn cleanup_model_chain(cfg: &store::PipelineConfig) -> Vec<(String, String)> {
    model_chain(&cfg.cleanup_default_model, &cfg.cleanup_fallback_models)
}

fn model_chain(default_model: &str, fallback_models: &[String]) -> Vec<(String, String)> {
    let mut chain = Vec::<(String, String)>::new();
    if let Some((provider, model)) = store::parse_model_id(default_model) {
        chain.push((provider, model));
    }
    for id in fallback_models {
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
