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

fn transcription_chain_root(
    local_manager: Option<&crate::local_stt::LocalTranscriptionManager>,
) -> std::path::PathBuf {
    local_manager
        .and_then(|manager| manager.prepare_models_dir().ok())
        .unwrap_or_else(crate::local_stt::LocalTranscriptionManager::models_root)
}

pub(super) fn validate_transcription_chain(
    cfg: &store::PipelineConfig,
    local_manager: Option<&crate::local_stt::LocalTranscriptionManager>,
) -> Result<(), String> {
    let root = transcription_chain_root(local_manager);
    let mut selected_local_missing = false;

    let has_usable_candidate = transcription_model_chain(cfg)
        .iter()
        .any(|(provider, model)| {
            if provider == store::LOCAL {
                let is_downloaded = crate::local_stt::model::manifest_by_id(model)
                    .map(|manifest| manifest.is_downloaded(&root))
                    .unwrap_or(false);
                if !is_downloaded
                    && cfg.transcription_provider == store::LOCAL
                    && store::parse_model_id(&cfg.transcription_default_model).is_some_and(
                        |(provider, selected)| provider == store::LOCAL && selected == *model,
                    )
                {
                    selected_local_missing = true;
                }
                is_downloaded
            } else {
                !cfg.key_for(provider).is_empty()
            }
        });

    if has_usable_candidate {
        Ok(())
    } else if selected_local_missing {
        Err("Download the selected local model.".to_string())
    } else {
        Err("No configured transcription backend is available".to_string())
    }
}

pub(super) fn has_cleanup_key_in_chain(cfg: &store::PipelineConfig) -> bool {
    cleanup_model_chain(cfg).iter().any(|(provider, model)| {
        if provider == store::LOCAL {
            crate::local_llm::model::manifest_by_id(model)
                .map(|manifest| {
                    manifest.is_downloaded(&crate::local_llm::LocalLlmManager::models_root())
                })
                .unwrap_or(false)
        } else {
            !cfg.key_for(provider).is_empty()
        }
    })
}

pub(super) fn trim_err(s: &str) -> String {
    let s = s.trim();
    if s.chars().count() > 120 {
        format!("{}…", s.chars().take(117).collect::<String>())
    } else {
        s.to_string()
    }
}
