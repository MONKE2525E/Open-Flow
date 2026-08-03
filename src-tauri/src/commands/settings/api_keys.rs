use super::*;

#[tauri::command]
pub async fn save_api_key(app: AppHandle, provider: String, key: String) -> Result<(), String> {
    run_blocking("save_api_key", move || {
        crate::data::credentials::save(&app, &provider, &key)
    })
    .await
}

#[tauri::command]
pub async fn delete_api_key(app: AppHandle, provider: String) -> Result<(), String> {
    run_blocking("delete_api_key", move || {
        crate::data::credentials::delete_saved(&app, &provider)
    })
    .await
}

#[tauri::command]
pub async fn get_api_key_status(_app: AppHandle) -> Result<serde_json::Value, String> {
    use crate::data::{credentials, store};
    run_blocking("get_api_key_status", move || {
        Ok(serde_json::json!({
            "groq":       credentials::has(store::GROQ),
            "openai":     credentials::has(store::OPENAI),
            "google":     credentials::has(store::GOOGLE),
            "assemblyai": credentials::has(store::ASSEMBLYAI),
        }))
    })
    .await
}

#[derive(serde::Serialize)]
pub struct KeyValidationResult {
    pub ok: bool,
    /// "valid" | "invalid" | "unknown" lets the frontend tell a definitive
    /// auth failure (401/403) apart from an inconclusive network/timeout/5xx
    /// result, which "ok: false" alone can't distinguish.
    pub status: String,
    pub message: String,
}

/// Pure status+body -> result mapping, kept separate from the network call so it's
/// unit-testable without mocking HTTP.
pub fn classify_validation_response(status: u16, body: &str) -> KeyValidationResult {
    if (200..300).contains(&status) {
        return KeyValidationResult {
            ok: true,
            status: "valid".to_string(),
            message: "Key verified.".to_string(),
        };
    }
    if status == 401 || status == 403 {
        let message = match crate::api::classify_unauthorized_body(body) {
            crate::api::AuthErrorCategory::InvalidOrRevokedKey => {
                "This key looks invalid or revoked.".to_string()
            }
            crate::api::AuthErrorCategory::ScopeOrAccountRestriction => {
                "This key was rejected for account or model-access reasons.".to_string()
            }
            crate::api::AuthErrorCategory::UnknownUnauthorized => {
                "The provider rejected this key.".to_string()
            }
        };
        return KeyValidationResult {
            ok: false,
            status: "invalid".to_string(),
            message,
        };
    }
    KeyValidationResult {
        ok: false,
        status: "unknown".to_string(),
        message: format!("Couldn't verify the key right now (provider returned status {status})."),
    }
}

/// Live, non-blocking key check: a cheap models-list GET, no audio and no token spend.
/// Anything short of a clean 2xx/401/403 is treated as inconclusive rather than a hard fail.
#[tauri::command]
pub async fn validate_api_key(
    provider: String,
    key: String,
) -> Result<KeyValidationResult, String> {
    let trimmed = key.trim();
    if trimmed.is_empty() {
        return Ok(KeyValidationResult {
            ok: false,
            status: "invalid".to_string(),
            message: "Key is empty.".to_string(),
        });
    }

    let client = crate::api::client::get();
    let request = match provider.as_str() {
        store::GROQ => client
            .get("https://api.groq.com/openai/v1/models")
            .bearer_auth(trimmed),
        store::OPENAI => client
            .get("https://api.openai.com/v1/models")
            .bearer_auth(trimmed),
        store::GOOGLE => client
            .get("https://generativelanguage.googleapis.com/v1beta/models")
            .header("x-goog-api-key", trimmed),
        store::ASSEMBLYAI => client
            .get("https://api.assemblyai.com/v2/transcript?limit=1")
            .header("authorization", trimmed),
        _ => return Err(format!("Unknown provider: {provider}")),
    };

    let response = match request
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await
    {
        Ok(r) => r,
        Err(_) => {
            return Ok(KeyValidationResult {
                ok: false,
                status: "unknown".to_string(),
                message: "Couldn't reach the provider to verify the key.".to_string(),
            })
        }
    };

    let status = response.status().as_u16();
    let body = response.text().await.unwrap_or_default();
    Ok(classify_validation_response(status, &body))
}
