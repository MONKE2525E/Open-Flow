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
        // AssemblyAI has no models endpoint, so its key check is the odd one
        // out and can't share the models-list builder the other three use.
        store::ASSEMBLYAI => client
            .get("https://api.assemblyai.com/v2/transcript?limit=1")
            .header("authorization", trimmed),
        _ => match models_list_request(client, &provider, trimmed, None) {
            Some(request) => request,
            None => return Err(format!("Unknown provider: {provider}")),
        },
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

// ── Live provider model lists ──────────────────────────────────────────────
//
// The key check above and the catalog refresh below hit the same three
// endpoints with the same auth, so they share one builder and can't drift.
// AssemblyAI is deliberately not in here: it has no models endpoint.

/// AssemblyAI publishes no models endpoint, so its list is static. Kept here
/// rather than in the frontend catalog so `list_provider_models` answers for
/// every provider with the same shape.
const ASSEMBLYAI_MODELS: &[&str] = &["universal-3-5-pro", "universal-2"];

/// Google returns at most `pageSize` models per response; ask for the max.
const GOOGLE_PAGE_SIZE: usize = 1000;
/// Runaway guard. Hitting it is treated as a failure, not a short list.
const GOOGLE_MAX_PAGES: usize = 20;

fn models_list_request(
    client: &reqwest::Client,
    provider: &str,
    key: &str,
    page_token: Option<&str>,
) -> Option<reqwest::RequestBuilder> {
    Some(match provider {
        store::GROQ => client
            .get("https://api.groq.com/openai/v1/models")
            .bearer_auth(key),
        store::OPENAI => client.get("https://api.openai.com/v1/models").bearer_auth(key),
        store::GOOGLE => {
            let mut request = client
                .get("https://generativelanguage.googleapis.com/v1beta/models")
                .header("x-goog-api-key", key)
                .query(&[("pageSize", GOOGLE_PAGE_SIZE.to_string())]);
            if let Some(token) = page_token {
                request = request.query(&[("pageToken", token)]);
            }
            request
        }
        _ => return None,
    })
}

/// `{"data":[{"id":"…"}]}` — Groq and OpenAI both speak the OpenAI shape.
pub fn parse_openai_models(body: &str) -> Result<Vec<String>, String> {
    let value: serde_json::Value =
        serde_json::from_str(body).map_err(|e| format!("Malformed model list: {e}"))?;
    let data = value
        .get("data")
        .and_then(|d| d.as_array())
        .ok_or_else(|| "Model list had no `data` array.".to_string())?;
    Ok(data
        .iter()
        .filter_map(|entry| entry.get("id").and_then(|id| id.as_str()))
        .map(|id| id.trim().to_string())
        .filter(|id| !id.is_empty())
        .collect())
}

/// One page of `{"models":[{"name":"models/…","supportedGenerationMethods":[…]}],
/// "nextPageToken":"…"}`. Returns the page's ids and the token for the next one.
pub fn parse_google_models_page(body: &str) -> Result<(Vec<String>, Option<String>), String> {
    let value: serde_json::Value =
        serde_json::from_str(body).map_err(|e| format!("Malformed model list: {e}"))?;
    let models = value
        .get("models")
        .and_then(|m| m.as_array())
        .ok_or_else(|| "Model list had no `models` array.".to_string())?;

    let ids = models
        .iter()
        .filter(|entry| {
            // No `supportedGenerationMethods` at all means we can't tell, so
            // keep it — dropping it would read downstream as a deprecation.
            entry
                .get("supportedGenerationMethods")
                .and_then(|m| m.as_array())
                .is_none_or(|methods| {
                    methods
                        .iter()
                        .any(|m| m.as_str() == Some("generateContent"))
                })
        })
        .filter_map(|entry| entry.get("name").and_then(|n| n.as_str()))
        .map(|name| name.strip_prefix("models/").unwrap_or(name).trim().to_string())
        .filter(|id| !id.is_empty())
        .collect();

    let next = value
        .get("nextPageToken")
        .and_then(|t| t.as_str())
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .map(str::to_string);

    Ok((ids, next))
}

fn dedupe(ids: Vec<String>) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    ids.into_iter()
        .filter(|id| seen.insert(id.clone()))
        .collect()
}

/// Every model id the provider currently offers, read with the *saved*
/// credential — the frontend never holds a key, only a presence boolean.
///
/// Failure and emptiness must stay distinguishable: the catalog store treats
/// `Err` as "no trustworthy list" and leaves its cache alone, while `Ok(vec![])`
/// is a real (if odd) answer. That is why a partial Google pagination is an
/// error rather than the pages that did arrive — a truncated list would read
/// downstream as mass deprecation.
#[tauri::command]
pub async fn list_provider_models(
    _app: AppHandle,
    provider: String,
) -> Result<Vec<String>, String> {
    if provider == store::ASSEMBLYAI {
        return Ok(ASSEMBLYAI_MODELS.iter().map(|m| m.to_string()).collect());
    }
    if provider == store::LOCAL {
        return Err("Local models are not listed by a provider.".to_string());
    }

    let key = crate::data::credentials::get(&provider);
    let key = key.trim().to_string();
    if key.is_empty() {
        return Err(format!("No saved API key for {provider}."));
    }

    let client = crate::api::client::get();
    let mut ids: Vec<String> = Vec::new();
    let mut page_token: Option<String> = None;

    for page in 0..GOOGLE_MAX_PAGES {
        let request = models_list_request(client, &provider, &key, page_token.as_deref())
            .ok_or_else(|| format!("Unknown provider: {provider}"))?;

        let response = request
            .timeout(std::time::Duration::from_secs(10))
            .send()
            .await
            .map_err(|_| format!("Couldn't reach {provider} to list models."))?;

        let status = response.status().as_u16();
        let body = response.text().await.unwrap_or_default();
        if !(200..300).contains(&status) {
            return Err(format!(
                "{provider} returned status {status} listing models."
            ));
        }

        if provider != store::GOOGLE {
            ids.extend(parse_openai_models(&body)?);
            return Ok(dedupe(ids));
        }

        let (page_ids, next) = parse_google_models_page(&body)?;
        ids.extend(page_ids);
        match next {
            None => return Ok(dedupe(ids)),
            Some(token) => page_token = Some(token),
        }
        if page + 1 == GOOGLE_MAX_PAGES {
            return Err("Google model list did not finish paginating.".to_string());
        }
    }

    Err(format!("Could not finish listing {provider} models."))
}

#[cfg(test)]
mod model_list_tests {
    use super::*;

    #[test]
    fn parses_openai_shape() {
        let body =
            r#"{"object":"list","data":[{"id":"whisper-large-v3"},{"id":" gpt-4o "},{"id":""}]}"#;
        assert_eq!(
            parse_openai_models(body).unwrap(),
            vec!["whisper-large-v3".to_string(), "gpt-4o".to_string()]
        );
    }

    #[test]
    fn rejects_openai_body_without_data() {
        assert!(parse_openai_models(r#"{"error":"nope"}"#).is_err());
        assert!(parse_openai_models("not json").is_err());
    }

    #[test]
    fn strips_google_prefix_and_filters_methods() {
        let body = r#"{"models":[
            {"name":"models/gemini-3.7-flash","supportedGenerationMethods":["generateContent"]},
            {"name":"models/embedding-001","supportedGenerationMethods":["embedContent"]},
            {"name":"models/unknown-methods"}
        ]}"#;
        let (ids, next) = parse_google_models_page(body).unwrap();
        assert_eq!(
            ids,
            vec!["gemini-3.7-flash".to_string(), "unknown-methods".to_string()]
        );
        assert!(next.is_none());
    }

    #[test]
    fn surfaces_google_next_page_token() {
        let body = r#"{"models":[{"name":"models/a","supportedGenerationMethods":["generateContent"]}],"nextPageToken":"tok"}"#;
        let (ids, next) = parse_google_models_page(body).unwrap();
        assert_eq!(ids, vec!["a".to_string()]);
        assert_eq!(next.as_deref(), Some("tok"));
    }

    #[test]
    fn treats_blank_page_token_as_last_page() {
        let body = r#"{"models":[{"name":"models/a"}],"nextPageToken":"  "}"#;
        assert!(parse_google_models_page(body).unwrap().1.is_none());
    }

    #[test]
    fn rejects_google_body_without_models() {
        assert!(parse_google_models_page(r#"{"error":"nope"}"#).is_err());
    }

    #[test]
    fn dedupes_accumulated_pages() {
        let ids = dedupe(vec!["a".into(), "b".into(), "a".into()]);
        assert_eq!(ids, vec!["a".to_string(), "b".to_string()]);
    }
}
