//! Client for the Verenu website's status API (`api.verenu.com`) — provider
//! outage alerts filtered to the models the user actually has selected, plus
//! a plain up/down health check of that same API.

use serde::{Deserialize, Serialize};

const PROVIDER_STATUS_URL: &str = "https://api.verenu.com/v1/provider-status";
const HEALTH_URL: &str = "https://api.verenu.com/v1/health";

#[derive(Debug, Deserialize)]
struct ProviderStatusEntry {
    id: String,
    name: String,
    status: String,
    severity: String,
    #[serde(rename = "showToUsers")]
    show_to_users: bool,
    #[serde(rename = "userMessage")]
    user_message: String,
    #[serde(rename = "detailsUrl")]
    details_url: String,
}

#[derive(Debug, Deserialize)]
struct ProviderStatusResponse {
    providers: Vec<ProviderStatusEntry>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderStatusAlert {
    pub provider_id: String,
    pub provider_name: String,
    pub status: String,
    pub severity: String,
    pub message: String,
    pub details_url: String,
}

/// Keeps only the entries worth surfacing to the user: the backend must have
/// flagged them (`showToUsers`), the status must not be `operational` or
/// `unknown` ("unknown" means the provider doesn't publish a
/// machine-readable feed we can check — it is not a signal that something is
/// actually broken), and the provider must be one the user has selected.
fn filter_alerts(
    providers: Vec<ProviderStatusEntry>,
    selected_providers: &[String],
) -> Vec<ProviderStatusAlert> {
    providers
        .into_iter()
        .filter(|p| {
            p.show_to_users
                && p.status != "operational"
                && p.status != "unknown"
                && selected_providers.iter().any(|id| id == &p.id)
        })
        .map(|p| ProviderStatusAlert {
            provider_id: p.id,
            provider_name: p.name,
            status: p.status,
            severity: p.severity,
            message: p.user_message,
            details_url: p.details_url,
        })
        .collect()
}

/// Fetches provider status and returns only the alerts relevant to
/// `selected_providers` (the user's configured transcription/cleanup
/// providers) that the backend has flagged as worth surfacing
/// (`showToUsers`). Providers the user hasn't selected are dropped
/// silently, even if they're having issues.
pub async fn fetch_relevant_alerts(
    selected_providers: &[String],
) -> anyhow::Result<Vec<ProviderStatusAlert>> {
    let response: ProviderStatusResponse = super::client::get()
        .get(PROVIDER_STATUS_URL)
        .header("User-Agent", "verenu")
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    Ok(filter_alerts(response.providers, selected_providers))
}

/// Fetches provider status and returns the response body untouched, for the
/// Developer panel's manual check button — lets us see exactly what the API
/// returned (including fields our typed model doesn't parse) rather than the
/// filtered/reshaped alerts the pipeline actually acts on.
pub async fn fetch_raw() -> anyhow::Result<serde_json::Value> {
    Ok(super::client::get()
        .get(PROVIDER_STATUS_URL)
        .header("User-Agent", "verenu")
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?)
}

/// Plain reachability check for `api.verenu.com` — no parsing beyond the
/// HTTP status, since callers only need up/down.
pub async fn check_health() -> bool {
    matches!(
        super::client::get()
            .get(HEALTH_URL)
            .header("User-Agent", "verenu")
            .send()
            .await,
        Ok(resp) if resp.status().is_success()
    )
}

#[cfg(test)]
mod tests {
    use super::{filter_alerts, ProviderStatusEntry};

    fn entry(id: &str, status: &str, show_to_users: bool) -> ProviderStatusEntry {
        ProviderStatusEntry {
            id: id.to_string(),
            name: format!("{id} name"),
            status: status.to_string(),
            severity: "low".to_string(),
            show_to_users,
            user_message: format!("{id} user message"),
            details_url: format!("https://example.invalid/{id}"),
        }
    }

    #[test]
    fn operational_never_alerts_even_when_flagged() {
        let providers = vec![entry("groq", "operational", true)];
        assert!(filter_alerts(providers, &["groq".to_string()]).is_empty());
    }

    #[test]
    fn unknown_never_alerts_even_when_flagged() {
        // "unknown" means Google doesn't publish a machine-readable feed —
        // it must not be treated as a real problem.
        let providers = vec![entry("google", "unknown", true)];
        assert!(filter_alerts(providers, &["google".to_string()]).is_empty());
    }

    #[test]
    fn non_operational_does_not_alert_unless_backend_flagged_it() {
        let providers = vec![entry("groq", "degraded", false)];
        assert!(filter_alerts(providers, &["groq".to_string()]).is_empty());
    }

    #[test]
    fn unselected_provider_never_alerts_even_when_broken() {
        let providers = vec![entry("openai", "degraded", true)];
        assert!(filter_alerts(providers, &["groq".to_string()]).is_empty());
    }

    #[test]
    fn selected_flagged_non_operational_provider_alerts() {
        let providers = vec![entry("groq", "degraded", true)];
        let alerts = filter_alerts(providers, &["groq".to_string(), "google".to_string()]);
        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].provider_id, "groq");
        assert_eq!(alerts[0].status, "degraded");
    }

    #[test]
    fn only_matching_providers_are_kept_from_a_mixed_batch() {
        let providers = vec![
            entry("groq", "operational", true),
            entry("google", "unknown", true),
            entry("openai", "degraded", true),
        ];
        let alerts = filter_alerts(
            providers,
            &["groq".to_string(), "google".to_string(), "openai".to_string()],
        );
        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].provider_id, "openai");
    }
}
