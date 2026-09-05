pub mod auto_learn;
pub mod cleanup;
pub mod client;
pub mod gemini_types;
pub mod prompts;
pub mod service_status;
pub mod transcription;
pub mod updater;

#[cfg(test)]
mod live_regression_tests;

const AUTH_401_PREFIX: &str = "AUTH_401";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProviderId {
    Groq,
    OpenAI,
    Google,
    AssemblyAi,
    Local,
}

impl ProviderId {
    pub fn from_str(value: &str) -> Self {
        match value {
            "openai" => Self::OpenAI,
            "google" => Self::Google,
            "assemblyai" => Self::AssemblyAi,
            "local" => Self::Local,
            _ => Self::Groq,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Groq => "groq",
            Self::OpenAI => "openai",
            Self::Google => "google",
            Self::AssemblyAi => "assemblyai",
            Self::Local => "local",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Groq => "Groq",
            Self::OpenAI => "OpenAI",
            Self::Google => "Google",
            Self::AssemblyAi => "AssemblyAI",
            Self::Local => "Local",
        }
    }

    pub fn whisper_url(self) -> Option<&'static str> {
        match self {
            Self::Groq => Some("https://api.groq.com/openai/v1/audio/transcriptions"),
            Self::OpenAI => Some("https://api.openai.com/v1/audio/transcriptions"),
            Self::Google => None,
            Self::AssemblyAi => None,
            Self::Local => None,
        }
    }

    pub fn cleanup_url(self) -> Option<&'static str> {
        match self {
            Self::Groq => Some("https://api.groq.com/openai/v1/chat/completions"),
            Self::OpenAI => Some("https://api.openai.com/v1/chat/completions"),
            Self::Google => None,
            Self::AssemblyAi => None,
            Self::Local => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AuthErrorCategory {
    InvalidOrRevokedKey,
    ScopeOrAccountRestriction,
    UnknownUnauthorized,
}

impl AuthErrorCategory {
    fn as_wire_value(self) -> &'static str {
        match self {
            Self::InvalidOrRevokedKey => "invalid_or_revoked_key",
            Self::ScopeOrAccountRestriction => "scope_or_account_restriction",
            Self::UnknownUnauthorized => "unknown_unauthorized",
        }
    }

    fn from_wire_value(v: &str) -> Option<Self> {
        match v {
            "invalid_or_revoked_key" => Some(Self::InvalidOrRevokedKey),
            "scope_or_account_restriction" => Some(Self::ScopeOrAccountRestriction),
            "unknown_unauthorized" => Some(Self::UnknownUnauthorized),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ParsedAuth401Error {
    pub provider: String,
    pub category: AuthErrorCategory,
    pub model: Option<String>,
    pub request_id: Option<String>,
}

pub fn quota_bail(provider: &str) -> anyhow::Error {
    anyhow::anyhow!("QUOTA_EXCEEDED: {provider} quota reached")
}

pub fn is_quota_error(e: &anyhow::Error) -> bool {
    e.to_string().starts_with("QUOTA_EXCEEDED:")
}

pub fn validate_model_for_url(model: &str) -> anyhow::Result<()> {
    if model.is_empty() {
        anyhow::bail!("Invalid model identifier (empty)");
    }
    if model.contains("..") {
        anyhow::bail!("Invalid model identifier (path traversal): {model}");
    }
    if model
        .chars()
        .all(|c| c.is_alphanumeric() || matches!(c, '-' | '.' | '_' | '/'))
    {
        Ok(())
    } else {
        anyhow::bail!("Invalid model identifier for API URL: {model}")
    }
}

pub fn sanitize_error_body_preview(body: &str) -> String {
    let compact = body.split_whitespace().collect::<Vec<_>>().join(" ");
    if compact.chars().count() > 180 {
        format!("{}...", compact.chars().take(177).collect::<String>())
    } else {
        compact
    }
}

enum ProviderHttpError {
    Quota(anyhow::Error),
    Auth {
        error: anyhow::Error,
        status: reqwest::StatusCode,
        request_id: String,
        preview: String,
    },
    NonSuccess {
        source: reqwest::Error,
        status: reqwest::StatusCode,
        request_id: String,
        preview: String,
    },
}

fn response_request_id(resp: &reqwest::Response) -> String {
    resp.headers()
        .get("x-request-id")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("-")
        .to_string()
}

async fn ensure_provider_success(
    resp: reqwest::Response,
    quota_label: &str,
    auth: Option<(&str, &str)>,
) -> Result<reqwest::Response, ProviderHttpError> {
    let status = resp.status();
    let request_id = response_request_id(&resp);

    if status.as_u16() == 429 {
        return Err(ProviderHttpError::Quota(quota_bail(quota_label)));
    }

    if matches!(status.as_u16(), 401 | 403) {
        if let Some((provider, model)) = auth {
            let body = resp.text().await.unwrap_or_default();
            let preview = sanitize_error_body_preview(&body);
            let category = classify_unauthorized_body(&body);
            let error = auth_status_error(provider, model, &request_id, status.as_u16(), category);
            return Err(ProviderHttpError::Auth {
                error,
                status,
                request_id,
                preview,
            });
        }
    }

    if let Err(source) = resp.error_for_status_ref() {
        let body = resp.text().await.unwrap_or_default();
        let preview = sanitize_error_body_preview(&body);
        return Err(ProviderHttpError::NonSuccess {
            source,
            status,
            request_id,
            preview,
        });
    }

    Ok(resp)
}

pub fn classify_unauthorized_body(body: &str) -> AuthErrorCategory {
    let lower = body.to_lowercase();

    if [
        "forbidden",
        "permission",
        "not allowed",
        "access denied",
        "scope",
        "organization",
        "role",
        "project",
        "team owner",
        "developer role",
        "insufficient",
    ]
    .iter()
    .any(|needle| lower.contains(needle))
    {
        return AuthErrorCategory::ScopeOrAccountRestriction;
    }

    if [
        "invalid api key",
        "incorrect api key",
        "invalid key",
        "revoked",
        "authentication failed",
        "unauthorized",
        "api key is not valid",
        "bad api key",
        "key has expired",
        "expired key",
    ]
    .iter()
    .any(|needle| lower.contains(needle))
    {
        return AuthErrorCategory::InvalidOrRevokedKey;
    }

    AuthErrorCategory::UnknownUnauthorized
}

fn auth_401_user_message(provider: &str, category: AuthErrorCategory) -> String {
    match category {
        AuthErrorCategory::InvalidOrRevokedKey => {
            format!("{provider} API key looks invalid or revoked. Re-enter it in Settings.")
        }
        AuthErrorCategory::ScopeOrAccountRestriction => format!(
            "{provider} rejected this key for account or model access. Check key role, team, and model permissions."
        ),
        AuthErrorCategory::UnknownUnauthorized => {
            format!("{provider} rejected authentication. Re-enter the key and verify account access.")
        }
    }
}

pub fn auth_401_error(
    provider: &str,
    model: &str,
    request_id: &str,
    category: AuthErrorCategory,
) -> anyhow::Error {
    auth_status_error(provider, model, request_id, 401, category)
}

fn auth_status_error(
    provider: &str,
    model: &str,
    request_id: &str,
    status: u16,
    category: AuthErrorCategory,
) -> anyhow::Error {
    let user_msg = auth_401_user_message(provider, category);
    anyhow::anyhow!(
        "{AUTH_401_PREFIX}|provider={provider}|category={}|model={model}|request_id={request_id}|status={status}: {user_msg}",
        category.as_wire_value()
    )
}

pub fn parse_auth_401_error(message: &str) -> Option<ParsedAuth401Error> {
    if !message.starts_with(AUTH_401_PREFIX) {
        return None;
    }
    let meta = message.split(": ").next().unwrap_or(message);
    let mut provider: Option<String> = None;
    let mut category: Option<AuthErrorCategory> = None;
    let mut model: Option<String> = None;
    let mut request_id: Option<String> = None;

    for part in meta.split('|').skip(1) {
        let Some((key, value)) = part.split_once('=') else {
            continue;
        };
        match key {
            "provider" => provider = Some(value.to_string()),
            "category" => category = AuthErrorCategory::from_wire_value(value),
            "model" if !value.is_empty() && value != "-" => model = Some(value.to_string()),
            "request_id" if !value.is_empty() && value != "-" => {
                request_id = Some(value.to_string())
            }
            _ => {}
        }
    }

    Some(ParsedAuth401Error {
        provider: provider?,
        category: category?,
        model,
        request_id,
    })
}

pub fn auth_401_display_message(parsed: &ParsedAuth401Error) -> String {
    auth_401_user_message(&parsed.provider, parsed.category)
}

pub fn is_retryable_provider_error(e: &anyhow::Error) -> bool {
    if is_quota_error(e) {
        return true;
    }

    for cause in e.chain() {
        if let Some(reqwest_err) = cause.downcast_ref::<reqwest::Error>() {
            if reqwest_err.is_timeout() || reqwest_err.is_connect() || reqwest_err.is_request() {
                return true;
            }
            if let Some(status) = reqwest_err.status() {
                return status.as_u16() == 408
                    || status.as_u16() == 429
                    || status.is_server_error();
            }
        }
    }

    let msg = e.to_string().to_lowercase();
    if let Some(status) = extract_http_status_code(&msg) {
        if status == 408 || status == 429 || (500..=599).contains(&status) {
            return true;
        }
    }
    msg.contains("timeout")
        || msg.contains("timed out")
        || msg.contains("connection")
        || msg.contains("temporarily unavailable")
        || msg.contains("overloaded")
        || msg.contains("rate limit")
        || msg.contains(" 502")
        || msg.contains(" 503")
        || msg.contains(" 504")
}

/// Converts an error into a safe, actionable user-facing message. Provider
/// metadata and response bodies must never be shown directly in the UI.
pub fn user_facing_error(e: &anyhow::Error) -> String {
    user_facing_message(&e.to_string())
}

/// String-based sibling for call sites that already hold an error message.
pub fn user_facing_message(msg: &str) -> String {
    if let Some(parsed) = parse_auth_401_error(msg) {
        return auth_401_display_message(&parsed);
    }
    if msg.starts_with("QUOTA_EXCEEDED:") {
        let provider = msg
            .strip_prefix("QUOTA_EXCEEDED:")
            .unwrap_or("")
            .split_whitespace()
            .next()
            .unwrap_or("The provider");
        return format!(
            "{provider} quota reached. Wait for it to reset or add credits, then try again."
        );
    }
    if msg.contains("body_preview=") || msg.contains("request_id=") {
        return match extract_http_status_code(msg) {
            Some(status @ (408 | 429 | 500..=599)) => format!(
                "The provider is temporarily unavailable (HTTP {status}). Wait a moment, then try again."
            ),
            Some(status) => format!(
                "The provider rejected the request (HTTP {status}). Check your API key and model settings, then try again."
            ),
            None => "The provider rejected the request. Check your API key and model settings, then try again."
                .to_string(),
        };
    }
    truncate_display(msg)
}

fn truncate_display(s: &str) -> String {
    let s = s.trim();
    if s.chars().count() > 120 {
        format!("{}…", s.chars().take(117).collect::<String>())
    } else {
        s.to_string()
    }
}

/// Whether an error indicates the request never reached a reachable server.
pub fn is_connectivity_error(e: &anyhow::Error) -> bool {
    for cause in e.chain() {
        if let Some(reqwest_err) = cause.downcast_ref::<reqwest::Error>() {
            if reqwest_err.is_connect() || reqwest_err.is_request() {
                return true;
            }
        }
    }

    let msg = e.to_string().to_lowercase();
    msg.contains("error sending request")
        || msg.contains("dns error")
        || msg.contains("connection reset")
        || msg.contains("connection refused")
        || msg.contains("network is unreachable")
        || msg.contains("failed to resolve")
}

fn extract_http_status_code(msg: &str) -> Option<u16> {
    for marker in ["status=", "status:"] {
        if let Some(idx) = msg.find(marker) {
            let digits: String = msg[idx + marker.len()..]
                .chars()
                .skip_while(|c| c.is_whitespace())
                .take_while(|c| c.is_ascii_digit())
                .collect();
            if digits.len() == 3 {
                if let Ok(status) = digits.parse::<u16>() {
                    return Some(status);
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::{
        auth_401_display_message, classify_unauthorized_body, parse_auth_401_error,
        sanitize_error_body_preview, AuthErrorCategory, ParsedAuth401Error,
    };

    #[test]
    fn classifies_invalid_or_revoked_key_signals() {
        let c = classify_unauthorized_body(r#"{"error":{"message":"Invalid API Key"}}"#);
        assert_eq!(c, AuthErrorCategory::InvalidOrRevokedKey);
    }

    #[test]
    fn classifies_scope_and_role_signals() {
        let c = classify_unauthorized_body(
            r#"{"error":{"message":"Only team owners or users with the developer role may create or manage API keys."}}"#,
        );
        assert_eq!(c, AuthErrorCategory::ScopeOrAccountRestriction);
    }

    #[test]
    fn parses_auth_401_metadata() {
        let parsed = parse_auth_401_error(
            "AUTH_401|provider=Groq|category=invalid_or_revoked_key|model=whisper-large-v3-turbo|request_id=req_123|status=401: Groq API key looks invalid or revoked. Re-enter it in Settings.",
        )
        .expect("parse");
        assert_eq!(parsed.provider, "Groq");
        assert_eq!(parsed.category, AuthErrorCategory::InvalidOrRevokedKey);
        assert_eq!(parsed.model.as_deref(), Some("whisper-large-v3-turbo"));
        assert_eq!(parsed.request_id.as_deref(), Some("req_123"));
    }

    #[test]
    fn auth_display_message_is_specific() {
        let msg = auth_401_display_message(&ParsedAuth401Error {
            provider: "Groq".to_string(),
            category: AuthErrorCategory::InvalidOrRevokedKey,
            model: None,
            request_id: None,
        });
        assert!(msg.to_lowercase().contains("invalid or revoked"));
    }

    #[test]
    fn auth_error_can_encode_forbidden_status() {
        let err = super::auth_status_error(
            "Google",
            "gemini-3.5-flash",
            "req_403",
            403,
            AuthErrorCategory::ScopeOrAccountRestriction,
        );
        let msg = err.to_string();
        assert!(msg.starts_with("AUTH_401|provider=Google"));
        assert!(msg.contains("status=403"));
    }

    #[test]
    fn error_preview_is_single_line_and_truncated() {
        let source = "line one\nline two\tline three";
        let preview = sanitize_error_body_preview(source);
        assert!(!preview.contains('\n'));
        assert!(!preview.contains('\t'));
        assert!(preview.contains("line one line two line three"));
    }
}
