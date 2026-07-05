//! Verenu status-API checks: provider outage alerts scoped to the user's
//! selected models, and a plain health check of `api.verenu.com`.

use super::*;
use crate::api::service_status::ProviderStatusAlert;

#[tauri::command]
pub async fn check_provider_status(app: AppHandle) -> Result<Vec<ProviderStatusAlert>, String> {
    let handle = store::settings_handle(&app)?;
    let provider_or_default = |key: &str| {
        handle
            .get(key)
            .and_then(|v| v.as_str().map(str::to_owned))
            .unwrap_or_else(|| "groq".to_string())
    };

    let mut selected = vec![provider_or_default(store::TRANSCRIPTION_PROVIDER)];
    let cleanup_provider = provider_or_default(store::CLEANUP_PROVIDER);
    if !selected.contains(&cleanup_provider) {
        selected.push(cleanup_provider);
    }

    match crate::api::service_status::fetch_relevant_alerts(&selected).await {
        Ok(alerts) => Ok(alerts),
        Err(e) => {
            log::warn!("Provider status check failed: {e}");
            Ok(Vec::new())
        }
    }
}

#[tauri::command]
pub async fn check_verenu_api_health() -> bool {
    crate::api::service_status::check_health().await
}

#[tauri::command]
pub async fn check_provider_status_raw() -> Result<serde_json::Value, String> {
    crate::api::service_status::fetch_raw()
        .await
        .map_err(|e| e.to_string())
}
