pub mod auto_learn;
pub mod cleanup;
pub mod client;
pub mod gemini_types;
pub mod prompts;
pub mod transcription;
pub mod updater;

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
    if model.chars().all(|c| c.is_alphanumeric() || matches!(c, '-' | '.' | '_' | '/')) {
        Ok(())
    } else {
        anyhow::bail!("Invalid model identifier for API URL: {model}")
    }
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
