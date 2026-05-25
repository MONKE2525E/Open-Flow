use std::collections::HashMap;
use std::ptr;
use std::sync::{Mutex, OnceLock};
use tauri::Wry;
use tauri_plugin_store::Store;
use windows::core::{PCWSTR, PWSTR};
use windows::Win32::Security::Credentials::{
    CredDeleteW, CredFree, CredReadW, CredWriteW, CREDENTIALW, CRED_PERSIST_LOCAL_MACHINE,
    CRED_TYPE_GENERIC,
};

const SERVICE: &str = "open-flow";

// HRESULT_FROM_WIN32(ERROR_NOT_FOUND) - credential entry absent, not an error
const HRESULT_NOT_FOUND: i32 = 0x80070490_u32 as i32;
static LAST_KEY_FINGERPRINTS: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();

fn user_for(provider: &str) -> Option<&'static str> {
    use crate::data::store;
    match provider {
        store::GROQ => Some(store::KEY_GROQ),
        store::OPENAI => Some(store::KEY_OPENAI),
        store::GOOGLE => Some(store::KEY_GOOGLE),
        _ => None,
    }
}

fn wide_null(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn normalize_api_key(raw: &str) -> String {
    raw.trim()
        .trim_end_matches(|c: char| c == '\0' || c.is_control())
        .trim()
        .to_string()
}

fn decode_utf16le_blob(blob: &[u8]) -> (String, bool) {
    if blob.is_empty() {
        return (String::new(), false);
    }

    let has_odd_length = !blob.len().is_multiple_of(2);
    let mut utf16 = Vec::with_capacity(blob.len() / 2);
    for i in 0..(blob.len() / 2) {
        let lo = blob[i * 2];
        let hi = blob[i * 2 + 1];
        utf16.push(u16::from_le_bytes([lo, hi]));
    }

    (String::from_utf16_lossy(&utf16), has_odd_length)
}

fn fnv1a64(input: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for b in input {
        hash ^= u64::from(*b);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

fn key_fingerprint(key: &str) -> String {
    format!("{:012x}", fnv1a64(key.as_bytes()) & 0xFFFFFFFFFFFF)
}

fn track_fingerprint(provider: &str, fingerprint: &str) {
    let cache = LAST_KEY_FINGERPRINTS.get_or_init(|| Mutex::new(HashMap::new()));
    let Ok(mut map) = cache.lock() else {
        return;
    };

    if let Some(prev) = map.get(provider) {
        if prev != fingerprint {
            log::warn!(
                "credentials: key fingerprint changed provider={} previous={} current={}",
                provider,
                prev,
                fingerprint
            );
        }
    }

    map.insert(provider.to_string(), fingerprint.to_string());
}

pub fn set(provider: &str, key: &str) -> Result<(), String> {
    let user = user_for(provider).ok_or_else(|| format!("Unknown provider: {provider}"))?;
    let target = format!("{user}.{SERVICE}");
    let mut target_wide = wide_null(&target);

    if key.is_empty() {
        unsafe {
            match CredDeleteW(PCWSTR(target_wide.as_ptr()), CRED_TYPE_GENERIC, Some(0)) {
                Ok(_) => Ok(()),
                Err(e) if e.code().0 == HRESULT_NOT_FOUND => Ok(()),
                Err(e) => Err(format!(
                    "Credential Manager delete failed for {provider}: {e}"
                )),
            }
        }
    } else {
        let normalized = normalize_api_key(key);
        if normalized.is_empty() {
            return Err(format!(
                "Credential Manager write failed for {provider}: key is empty after normalization"
            ));
        }

        let mut user_wide = wide_null(user);
        // Encode as UTF-16-LE. This matches Windows generic credential blob expectations.
        let utf16: Vec<u16> = normalized.encode_utf16().collect();
        let mut blob: Vec<u8> = utf16.iter().flat_map(|c| c.to_le_bytes()).collect();
        let key_fp = key_fingerprint(&normalized);

        let mut cred: CREDENTIALW = unsafe { std::mem::zeroed() };
        cred.Type = CRED_TYPE_GENERIC;
        cred.TargetName = PWSTR(target_wide.as_mut_ptr());
        cred.CredentialBlobSize = blob.len() as u32;
        cred.CredentialBlob = blob.as_mut_ptr();
        cred.Persist = CRED_PERSIST_LOCAL_MACHINE;
        cred.UserName = PWSTR(user_wide.as_mut_ptr());

        unsafe {
            CredWriteW(&cred, 0)
                .map_err(|e| format!("Credential Manager write failed for {provider}: {e}"))?;
        }

        log::debug!(
            "credentials: write ok provider={} key_len={} key_fp={} blob_bytes={}",
            provider,
            normalized.len(),
            key_fp,
            blob.len()
        );
        track_fingerprint(provider, &key_fp);
        Ok(())
    }
}

pub fn get(provider: &str) -> String {
    let user = match user_for(provider) {
        Some(u) => u,
        None => {
            log::warn!("credentials::get called with unknown provider: {provider}");
            return String::new();
        }
    };
    let target = format!("{user}.{SERVICE}");
    let target_wide = wide_null(&target);

    unsafe {
        let mut p_cred: *mut CREDENTIALW = ptr::null_mut();
        match CredReadW(
            PCWSTR(target_wide.as_ptr()),
            CRED_TYPE_GENERIC,
            Some(0),
            &mut p_cred,
        ) {
            Ok(()) => {
                let cred = match p_cred.as_ref() {
                    Some(c) => c,
                    None => {
                        log::error!(
                            "Credential Manager read returned success with null pointer for {provider}"
                        );
                        return String::new();
                    }
                };

                let blob_size = cred.CredentialBlobSize as usize;
                let blob_ptr = cred.CredentialBlob;
                let decoded = if blob_size == 0 {
                    String::new()
                } else if blob_ptr.is_null() {
                    log::error!(
                        "Credential Manager read returned non-zero blob size with null blob pointer for {provider}"
                    );
                    String::new()
                } else {
                    let blob = std::slice::from_raw_parts(blob_ptr, blob_size);
                    let (key, odd_blob_bytes) = decode_utf16le_blob(blob);
                    if odd_blob_bytes {
                        log::warn!(
                            "credentials: read decode anomaly provider={} reason=odd_blob_length blob_bytes={}",
                            provider,
                            blob_size
                        );
                    }
                    key
                };
                CredFree(p_cred as *const _);

                let normalized = normalize_api_key(&decoded);
                if !decoded.is_empty() && normalized != decoded {
                    log::warn!(
                        "credentials: normalized read key provider={} before_len={} after_len={}",
                        provider,
                        decoded.len(),
                        normalized.len()
                    );
                }

                let key_fp = if normalized.is_empty() {
                    "-".to_string()
                } else {
                    key_fingerprint(&normalized)
                };
                if crate::system::logger::is_verbose() {
                    log::debug!(
                        "credentials: read metadata provider={} blob_bytes={} key_len={} key_fp={}",
                        provider,
                        blob_size,
                        normalized.len(),
                        key_fp
                    );
                }
                if !normalized.is_empty() {
                    track_fingerprint(provider, &key_fp);
                }

                log::debug!(
                    "credentials: read ok provider={provider} key_len={}",
                    normalized.len()
                );
                normalized
            }
            Err(e) => {
                if e.code().0 != HRESULT_NOT_FOUND {
                    log::error!("Credential Manager read failed for {provider}: {e}");
                } else {
                    log::debug!("credentials: read miss provider={provider} (not found)");
                }
                String::new()
            }
        }
    }
}

pub fn has(provider: &str) -> bool {
    let user = match user_for(provider) {
        Some(u) => u,
        None => return false,
    };
    let target = format!("{user}.{SERVICE}");
    let target_wide = wide_null(&target);
    unsafe {
        let mut p_cred: *mut CREDENTIALW = ptr::null_mut();
        let ok = CredReadW(
            PCWSTR(target_wide.as_ptr()),
            CRED_TYPE_GENERIC,
            Some(0),
            &mut p_cred,
        )
        .is_ok();
        if ok && !p_cred.is_null() {
            CredFree(p_cred as *const _);
        }
        ok
    }
}

pub fn delete(provider: &str) -> Result<(), String> {
    set(provider, "")
}

/// One-shot migration: moves any plaintext API keys from settings.json into
/// Windows Credential Manager, then sets a flag so it never runs again.
pub fn migrate_from_store(store: &Store<Wry>) {
    let migrated = store
        .get(crate::data::store::CREDENTIALS_MIGRATED)
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let mut any_failed = false;
    let mut should_save = false;
    let mut lingering_plaintext = Vec::<(&str, usize)>::new();

    for (provider, store_key) in [
        (crate::data::store::GROQ, crate::data::store::KEY_GROQ),
        (crate::data::store::OPENAI, crate::data::store::KEY_OPENAI),
        (crate::data::store::GOOGLE, crate::data::store::KEY_GOOGLE),
    ] {
        let plaintext = store
            .get(store_key)
            .and_then(|v| v.as_str().map(String::from))
            .unwrap_or_default();

        if plaintext.is_empty() {
            continue;
        }

        let normalized_plaintext = normalize_api_key(&plaintext);
        if normalized_plaintext.is_empty() {
            log::warn!(
                "Migration: removed non-empty but invalid plaintext key provider={} raw_len={}",
                provider,
                plaintext.len()
            );
            let _ = store.delete(store_key);
            should_save = true;
            continue;
        }

        lingering_plaintext.push((provider, normalized_plaintext.len()));

        if !migrated {
            // Only write if not already present to avoid clobbering a manually-set credential.
            if get(provider).is_empty() {
                if let Err(e) = set(provider, &normalized_plaintext) {
                    log::error!(
                        "Migration: could not write {provider} key to Credential Manager: {e}"
                    );
                    any_failed = true;
                } else {
                    log::info!("Migration: moved {provider} API key to Credential Manager");
                }
            }
        }

        let _ = store.delete(store_key);
        should_save = true;
    }

    if !any_failed {
        store.set(
            crate::data::store::CREDENTIALS_MIGRATED,
            serde_json::json!(true),
        );
        should_save = true;
    }

    if migrated && !lingering_plaintext.is_empty() {
        for (provider, key_len) in &lingering_plaintext {
            log::warn!(
                "Migration: plaintext key field persisted after migration provider={} key_len={}. Field was scrubbed.",
                provider,
                key_len
            );
        }
    }

    if should_save {
        if let Err(e) = store.save() {
            log::warn!("Migration: could not persist settings.json after key removal: {e}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{decode_utf16le_blob, normalize_api_key};

    #[test]
    fn normalize_removes_whitespace_and_trailing_control_chars() {
        let normalized = normalize_api_key(" \r\n gsk_test_123 \0\r\n ");
        assert_eq!(normalized, "gsk_test_123");
    }

    #[test]
    fn normalize_can_result_in_empty_string() {
        let normalized = normalize_api_key("\0\r\n\t ");
        assert!(normalized.is_empty());
    }

    #[test]
    fn decode_utf16_blob_reports_odd_length_anomaly() {
        let bytes = vec![0x41, 0x00, 0x42];
        let (decoded, odd) = decode_utf16le_blob(&bytes);
        assert!(odd);
        assert_eq!(decoded, "A");
    }

    #[test]
    fn decode_utf16_blob_round_trips_ascii() {
        let bytes = vec![0x67, 0x00, 0x73, 0x00, 0x6b, 0x00];
        let (decoded, odd) = decode_utf16le_blob(&bytes);
        assert!(!odd);
        assert_eq!(decoded, "gsk");
    }
}
