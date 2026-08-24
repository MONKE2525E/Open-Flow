//! LAN device sync: pair Verenu devices on the same network and keep selected
//! user data in sync - no account, no cloud, no server.
//!
//! Layout:
//! - [`identity`] - per-device uuid + self-signed certificate.
//! - [`secrets`] - OS credential storage for the identity private key.
//! - [`transport`] - rustls TLS with per-device certs (pinning at protocol layer).
//! - [`pairing`] - SPAKE2 code-verified pairing handshake.
//! - [`protocol`] - framed JSON messages.
//! - [`engine`] - change-log delta sync with last-writer-wins merging.
//! - [`store`] - SQLite access for sync bookkeeping.
//! - [`manager`] - orchestration, discovery, status, frontend events.
//!
//! See docs/lan-sync.md for the full architecture and data classification.

pub(crate) mod engine;
pub(crate) mod identity;
pub(crate) mod manager;
pub(crate) mod pairing;
pub(crate) mod protocol;
pub(crate) mod secrets;
pub(crate) mod store;
pub(crate) mod transport;

pub use manager::SyncManager;

#[cfg(test)]
mod tests;
