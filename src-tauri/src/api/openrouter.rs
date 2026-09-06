//! OpenRouter's public model catalog, used as the source for estimate rates.

use std::collections::BTreeMap;

use anyhow::{Context, Result};
use serde::Deserialize;

use super::client;

const MODELS_URL: &str = "https://openrouter.ai/api/v1/models?output_modalities=all";

#[derive(Debug, Clone, PartialEq)]
pub struct ModelPricing {
    pub model_id: String,
    pub prompt_usd_per_token: f64,
    pub completion_usd_per_token: f64,
}

#[derive(Debug, Deserialize)]
struct ModelsResponse {
    #[serde(default)]
    data: Vec<ModelRecord>,
}

#[derive(Debug, Deserialize)]
struct ModelRecord {
    id: String,
    pricing: Option<ModelPricingFields>,
}

#[derive(Debug, Deserialize)]
struct ModelPricingFields {
    prompt: String,
    completion: String,
}

/// Fetches the public OpenRouter catalog. The endpoint does not need one of
/// the user's provider keys, so pricing refreshes never send private data.
pub async fn fetch_model_pricing() -> Result<Vec<ModelPricing>> {
    let response = tokio::time::timeout(
        std::time::Duration::from_secs(15),
        client::get()
            .get(MODELS_URL)
            .header(
                reqwest::header::USER_AGENT,
                format!("Verenu/{}", env!("CARGO_PKG_VERSION")),
            )
            .send(),
    )
    .await
    .context("OpenRouter pricing request timed out")??
    .error_for_status()
    .context("OpenRouter pricing request returned an error")?
    .json::<ModelsResponse>()
    .await
    .context("OpenRouter pricing response was invalid")?;

    let rates = parse_model_pricing(response.data);
    if rates.is_empty() {
        anyhow::bail!("OpenRouter returned no usable model pricing");
    }
    Ok(rates)
}

/// Turns the catalog into one canonical row per model. OpenRouter can return
/// paid/free/batch variants with the same base id; the plain model wins, and
/// a lexical tie-breaker keeps the result deterministic.
fn parse_model_pricing(records: Vec<ModelRecord>) -> Vec<ModelPricing> {
    let mut by_base_id = BTreeMap::<String, ModelPricing>::new();
    for record in records {
        let Some(pricing) = record.pricing else {
            continue;
        };
        let model_id = record.id.trim().to_ascii_lowercase();
        let Some(base_id) = model_id.split(':').next().filter(|id| !id.is_empty()) else {
            continue;
        };
        let Ok(prompt) = pricing.prompt.parse::<f64>() else {
            continue;
        };
        let Ok(completion) = pricing.completion.parse::<f64>() else {
            continue;
        };
        if !prompt.is_finite() || prompt < 0.0 || !completion.is_finite() || completion < 0.0 {
            continue;
        }

        let candidate = ModelPricing {
            model_id: model_id.clone(),
            prompt_usd_per_token: prompt,
            completion_usd_per_token: completion,
        };
        let replace = by_base_id.get(base_id).is_none_or(|existing| {
            // Prefer the unsuffixed paid model over a variant. If both are
            // variants, use the lexical order so API ordering cannot change
            // which duplicate survives.
            let existing_plain = existing.model_id.eq_ignore_ascii_case(base_id);
            let candidate_plain = model_id.eq_ignore_ascii_case(base_id);
            (candidate_plain && !existing_plain)
                || (candidate_plain == existing_plain
                    && model_id.as_str() < existing.model_id.as_str())
        });
        if replace {
            by_base_id.insert(base_id.to_string(), candidate);
        }
    }
    by_base_id.into_values().collect()
}

#[cfg(test)]
mod tests {
    use super::{parse_model_pricing, ModelPricingFields, ModelRecord};

    fn record(id: &str, prompt: &str, completion: &str) -> ModelRecord {
        ModelRecord {
            id: id.to_string(),
            pricing: Some(ModelPricingFields {
                prompt: prompt.to_string(),
                completion: completion.to_string(),
            }),
        }
    }

    #[test]
    fn deduplicates_variants_and_prefers_the_plain_model() {
        let rates = parse_model_pricing(vec![
            record("google/gemini-3.7-flash:free", "0", "0"),
            record("google/gemini-3.7-flash", "0.00000075", "0.00000375"),
            record("google/gemini-3.7-flash:batch", "0.0000003", "0.0000015"),
        ]);

        assert_eq!(rates.len(), 1);
        assert_eq!(rates[0].model_id, "google/gemini-3.7-flash");
        assert_eq!(rates[0].prompt_usd_per_token, 0.00000075);
    }

    #[test]
    fn ignores_invalid_prices() {
        let rates = parse_model_pricing(vec![
            record("good/model", "0.1", "0.2"),
            record("bad/model", "not-a-number", "0.2"),
            record("negative/model", "-1", "0.2"),
        ]);

        assert_eq!(rates.len(), 1);
        assert_eq!(rates[0].model_id, "good/model");
    }
}
