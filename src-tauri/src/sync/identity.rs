//! Per-device sync identity: a random uuid plus an ECDSA P-256 key pair with a
//! self-signed certificate (the same pattern Tailscale-style mesh tools use).
//! The private key lives in the OS credential store, the public certificate in
//! the app data directory, and the uuid in the database's `sync_identity` row.
//! Together they are the device's long-term identity; deleting all three is
//! the only way pairing "forgets" this device.

use anyhow::{anyhow, Context, Result};
use chrono::Datelike;
use rcgen::{CertificateParams, KeyPair, PKCS_ECDSA_P256_SHA256};
use rustls::pki_types::{CertificateDer, PrivatePkcs8KeyDer};
use sha2::{Digest, Sha256};

use super::secrets;

const CERT_FILE: &str = "sync-identity.der";
/// Certificate lifetime in years; re-issued lazily shortly before expiry.
const CERT_YEARS: i64 = 10;
const CERT_DAYS: i64 = 365 * CERT_YEARS;
/// Re-issue when the stored cert is older than this fraction of its lifetime.
const REISSUE_FRACTION: f64 = 0.9;

#[derive(Debug, Clone)]
pub struct DeviceIdentity {
    pub uuid: String,
    pub name: String,
    cert_der: CertificateDer<'static>,
    key_der: Vec<u8>,
}

impl DeviceIdentity {
    pub fn cert_der(&self) -> &CertificateDer<'static> {
        &self.cert_der
    }

    /// SHA-256 of the certificate DER, hex encoded - the fingerprint peers pin.
    #[allow(dead_code)] // exercised by the pairing tests
    pub fn cert_fingerprint(&self) -> String {
        fingerprint_of(self.cert_der.as_ref())
    }

    pub fn tls_key(&self) -> PrivatePkcs8KeyDer<'static> {
        PrivatePkcs8KeyDer::from(self.key_der.clone())
    }
}

pub fn fingerprint_of(der: &[u8]) -> String {
    let hash = Sha256::digest(der);
    let mut hex = String::with_capacity(hash.len() * 2);
    for byte in hash {
        use std::fmt::Write;
        let _ = write!(hex, "{byte:02x}");
    }
    hex
}

/// Loads the identity, re-using stored pieces and creating whatever is missing.
/// `known_uuid` is the uuid previously persisted in the database, if any - it
/// survives keychain/cert loss so a repaired device keeps its sync identity.
pub fn load_or_create(
    app_data_dir: &std::path::Path,
    known_uuid: Option<String>,
) -> Result<DeviceIdentity> {
    std::fs::create_dir_all(app_data_dir).ok();
    let cert_path = app_data_dir.join(CERT_FILE);

    let stored_cert = std::fs::read(&cert_path).ok();
    let cert_is_fresh = stored_cert.as_ref().is_some_and(|_| cert_file_fresh(&cert_path));
    let stored_key = secrets::load_identity_key();

    if let (Some(cert_der), Some(key_der), true) = (stored_cert, stored_key, cert_is_fresh) {
        if let Err(err) = validate_key(&key_der) {
            log::warn!("sync: stored identity key is invalid ({err}); re-issuing identity");
        } else {
            let uuid = known_uuid.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
            let mut identity = DeviceIdentity {
                uuid,
                name: String::new(),
                cert_der: CertificateDer::from(cert_der),
                key_der,
            };
            identity.name = default_device_name();
            return Ok(identity);
        }
    }

    // Missing, expired, or corrupt: re-issue the whole identity, keeping the
    // known uuid when there is one.
    let identity = create_identity(known_uuid)?;
    secrets::store_identity_key(&identity.key_der)
        .map_err(|e| anyhow!("failed to store sync identity key: {e}"))?;
    std::fs::write(&cert_path, identity.cert_der.as_ref())
        .with_context(|| "failed to write sync certificate")?;
    Ok(identity)
}

fn validate_key(key_der: &[u8]) -> Result<()> {
    KeyPair::from_pkcs8_der_and_sign_algo(
        &PrivatePkcs8KeyDer::from(key_der.to_vec()),
        &PKCS_ECDSA_P256_SHA256,
    )
    .map_err(|e| anyhow!("stored private key is invalid: {e}"))?;
    Ok(())
}

/// The certificate carries no parseable expiry check without an X.509 parser;
/// the file's modification time is written once at creation, so its age tells
/// us when to rotate. A missing mtime forces re-issue (safe default).
fn cert_file_fresh(cert_path: &std::path::Path) -> bool {
    let Ok(meta) = std::fs::metadata(cert_path) else {
        return false;
    };
    let Ok(modified) = meta.modified() else {
        return false;
    };
    let age_days = std::time::SystemTime::now()
        .duration_since(modified)
        .map(|d| d.as_secs() as f64 / 86_400.0)
        // A future mtime is a clock-skew case, not evidence that the cert is
        // expired. Treat it as newly written so a clock correction cannot
        // rotate the pinned identity unexpectedly.
        .unwrap_or(0.0);
    age_days < CERT_DAYS as f64 * REISSUE_FRACTION
}

fn create_identity(known_uuid: Option<String>) -> Result<DeviceIdentity> {
    let key_pair = KeyPair::generate().context("failed to generate sync key pair")?;
    let uuid = known_uuid.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let mut params =
        CertificateParams::new(vec![uuid.clone()]).context("failed to build certificate params")?;
    params
        .distinguished_name
        .push(rcgen::DnType::CommonName, "Verenu Sync");
    params
        .distinguished_name
        .push(rcgen::DnType::OrganizationName, "Verenu");
    // Small back-dated start so a slightly skewed peer clock still accepts it.
    let now = chrono::Utc::now();
    let this_year = now.year() as i64;
    let month = now.month() as u8;
    // Start on the first of the current month so certificate creation is
    // stable across month lengths and remains slightly back-dated.
    params.not_before = rcgen::date_time_ymd((this_year - 1) as i32, month, 1);
    params.not_after = rcgen::date_time_ymd(
        (this_year - 1 + CERT_YEARS) as i32,
        month,
        1,
    );
    let cert = params
        .self_signed(&key_pair)
        .context("failed to self-sign sync certificate")?;
    Ok(DeviceIdentity {
        uuid,
        name: default_device_name(),
        cert_der: cert.der().clone(),
        key_der: key_pair.serialize_der(),
    })
}

/// Best-effort human name for this machine, shown on the peer during pairing.
pub fn default_device_name() -> String {
    hostname_raw()
        .unwrap_or_else(|| "This device".to_string())
        .trim()
        .chars()
        .take(60)
        .collect()
}

#[cfg(target_os = "windows")]
fn hostname_raw() -> Option<String> {
    std::env::var("COMPUTERNAME")
        .ok()
        .filter(|s| !s.trim().is_empty())
}

#[cfg(not(target_os = "windows"))]
fn hostname_raw() -> Option<String> {
    let mut buf = [0u8; 256];
    // libc is already a macOS dependency; gethostname needs no permissions.
    if unsafe { libc::gethostname(buf.as_mut_ptr() as *mut libc::c_char, buf.len()) } != 0 {
        return None;
    }
    // POSIX does not guarantee a terminator when the hostname fills the
    // supplied buffer.
    *buf.last_mut()? = 0;
    let end = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
    String::from_utf8(buf[..end].to_vec()).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Restart persistence: the same app data dir + known uuid must yield the
    /// same certificate (and thus the same fingerprint peers pin), and a lost
    /// keychain must re-issue without changing the uuid.
    #[test]
    fn identity_survives_restart() {
        let dir = std::env::temp_dir().join(format!(
            "verenu-sync-identity-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        let first = load_or_create(&dir, None).expect("create");
        let restarted = load_or_create(&dir, Some(first.uuid.clone())).expect("reload");
        assert_eq!(first.uuid, restarted.uuid, "uuid must survive restart");
        assert_eq!(
            first.cert_fingerprint(),
            restarted.cert_fingerprint(),
            "certificate (and pin) must survive restart"
        );

        // Keychain wiped: a new key + certificate, same uuid.
        secrets::delete_identity_key();
        let reissued = load_or_create(&dir, Some(first.uuid.clone())).expect("reissue");
        assert_eq!(reissued.uuid, first.uuid, "uuid survives key loss");

        drop(reissued);
        drop(first);
        let _ = std::fs::remove_dir_all(&dir);
        secrets::delete_identity_key();
    }
}
