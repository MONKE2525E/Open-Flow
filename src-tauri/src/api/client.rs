use std::sync::OnceLock;

static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();

/// Returns the process-wide shared reqwest client.
/// Reusing one client enables TCP connection pooling and TLS session reuse,
/// saving ~200–400ms per request compared to Client::new() each time.
pub fn get() -> &'static reqwest::Client {
    CLIENT.get_or_init(reqwest::Client::new)
}
