pub mod auto_learn;
pub mod cleanup;
pub mod client;
pub mod gemini_types;
pub mod prompts;
pub mod transcription;
pub mod updater;

pub fn quota_bail(provider: &str) -> anyhow::Error {
    anyhow::anyhow!("QUOTA_EXCEEDED: {} quota reached", provider)
}

pub fn is_quota_error(e: &anyhow::Error) -> bool {
    e.to_string().starts_with("QUOTA_EXCEEDED:")
}
