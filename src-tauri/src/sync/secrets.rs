//! Secure storage for the sync identity private key.
//!
//! Mirrors `data/credentials.rs` (Windows Credential Manager via CredWriteW,
//! macOS Keychain via security-framework) but with its own target names, since
//! that module is deliberately closed over the four API-provider keys. If the
//! platform store fails, the key falls back to a 0600 file in the app data
//! directory with a loud warning - pairing still works, but users should know.

#[cfg(target_os = "windows")]
const WIN_TARGET: &str = "verenu.sync.identity";
#[cfg(target_os = "macos")]
const MAC_SERVICE: &str = "com.verenu.app";
#[cfg(target_os = "macos")]
const MAC_ACCOUNT: &str = "sync.identity";
const FALLBACK_FILE: &str = "sync-identity.key";

pub fn store_identity_key(key_der: &[u8]) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        win_store::store(WIN_TARGET, key_der)
    }
    #[cfg(target_os = "macos")]
    {
        mac_store::store(MAC_SERVICE, MAC_ACCOUNT, key_der)
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        store_fallback(key_der)
    }
}

pub fn load_identity_key() -> Option<Vec<u8>> {
    #[cfg(target_os = "windows")]
    {
        win_store::load(WIN_TARGET)
    }
    #[cfg(target_os = "macos")]
    {
        mac_store::load(MAC_SERVICE, MAC_ACCOUNT)
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        load_fallback()
    }
}

#[allow(dead_code)] // used by future "forget this device" flows
pub fn delete_identity_key() {
    #[cfg(target_os = "windows")]
    {
        let _ = win_store::delete(WIN_TARGET);
    }
    #[cfg(target_os = "macos")]
    {
        let _ = mac_store::delete(MAC_SERVICE, MAC_ACCOUNT);
    }
    if let Some(path) = fallback_path() {
        let _ = std::fs::remove_file(path);
    }
}

/// Last-resort storage when the OS credential store is unavailable. The key
/// file is only readable by the current user where the OS enforces modes.
fn fallback_path() -> Option<std::path::PathBuf> {
    let dir = crate::app_data_dir();
    std::fs::create_dir_all(&dir).ok()?;
    Some(dir.join(FALLBACK_FILE))
}

fn store_fallback(key_der: &[u8]) -> Result<(), String> {
    let path = fallback_path().ok_or("no app data dir")?;
    #[cfg(unix)]
    {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;
        use std::os::unix::fs::PermissionsExt;

        // Set the mode at creation time as well as after opening an existing
        // file, so a newly generated identity is never briefly world-readable.
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create(true).truncate(true).mode(0o600);
        let mut file = options
            .open(&path)
            .map_err(|e| format!("fallback key open failed: {e}"))?;
        file.write_all(key_der)
            .map_err(|e| format!("fallback key write failed: {e}"))?;
        file.set_permissions(std::fs::Permissions::from_mode(0o600))
            .map_err(|e| format!("fallback key permission update failed: {e}"))?;
    }
    #[cfg(not(unix))]
    std::fs::write(&path, key_der).map_err(|e| format!("fallback key write failed: {e}"))?;
    Ok(())
}

fn load_fallback() -> Option<Vec<u8>> {
    std::fs::read(fallback_path()?).ok()
}

#[cfg(target_os = "windows")]
mod win_store {
    //! Windows Credential Manager storage, same mechanism as
    //! `data::credentials` but for the sync identity key.

    use super::store_fallback;
    use windows::core::{PCWSTR, PWSTR};
    use windows::Win32::Security::Credentials::{
        CredDeleteW, CredFree, CredReadW, CredWriteW, CREDENTIALW, CRED_PERSIST_LOCAL_MACHINE,
        CRED_TYPE_GENERIC,
    };

    // Windows stores the blob as UTF-16; encode hex so any byte value survives.
    fn encode(bytes: &[u8]) -> Vec<u16> {
        let mut out = Vec::with_capacity(bytes.len() * 2);
        for b in bytes {
            let hex = format!("{b:02x}");
            out.push(hex.as_bytes()[0] as u16);
            out.push(hex.as_bytes()[1] as u16);
        }
        out
    }

    fn decode(blob: &[u8]) -> Option<Vec<u8>> {
        let text: Vec<u16> = blob
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        String::from_utf16(&text)
            .ok()?
            .as_bytes()
            .chunks(2)
            .map(|c| u8::from_str_radix(std::str::from_utf8(c).ok()?, 16).ok())
            .collect()
    }

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    pub fn store(target: &str, key_der: &[u8]) -> Result<(), String> {
        let mut blob = encode(key_der);
        let mut target_wide = wide(target);
        let mut cred: CREDENTIALW = unsafe { std::mem::zeroed() };
        cred.Type = CRED_TYPE_GENERIC;
        cred.TargetName = PWSTR(target_wide.as_mut_ptr());
        cred.CredentialBlobSize = (blob.len() * 2) as u32;
        cred.CredentialBlob = blob.as_mut_ptr().cast();
        cred.Persist = CRED_PERSIST_LOCAL_MACHINE;
        // SAFETY: cred's buffers live for the duration of the call; the API
        // copies the credential and does not retain the pointers.
        let result = unsafe { CredWriteW(&cred, 0) };
        if result.is_err() {
            // Fall back to a protected file rather than losing the identity.
            log::warn!("sync: CredWriteW failed; storing identity key in app data fallback");
            return store_fallback(key_der);
        }
        Ok(())
    }

    pub fn load(target: &str) -> Option<Vec<u8>> {
        let target_wide = wide(target);
        let mut p_cred: *mut CREDENTIALW = std::ptr::null_mut();
        // SAFETY: target_wide outlives the call; p_cred receives an allocation
        // we own until CredFree.
        let result = unsafe {
            CredReadW(
                PCWSTR(target_wide.as_ptr()),
                CRED_TYPE_GENERIC,
                Some(0),
                &mut p_cred,
            )
        };
        if let Err(_err) = result {
            if let Some(key) = super::load_fallback() {
                return Some(key);
            }
            return None;
        }
        unsafe {
            let credential = p_cred.as_ref()?;
            let size = credential.CredentialBlobSize as usize;
            let blob = if size == 0 || credential.CredentialBlob.is_null() {
                Vec::new()
            } else {
                std::slice::from_raw_parts(credential.CredentialBlob, size).to_vec()
            };
            CredFree(p_cred.cast());
            decode(&blob)
        }
    }

    #[allow(dead_code)]
    pub fn delete(target: &str) -> Result<(), String> {
        let target_wide = wide(target);
        // SAFETY: target_wide outlives the call.
        unsafe { CredDeleteW(PCWSTR(target_wide.as_ptr()), CRED_TYPE_GENERIC, Some(0)) }
            .map_err(|e| format!("CredDeleteW failed: {e}"))
    }
}

#[cfg(target_os = "macos")]
mod mac_store {
    use security_framework::passwords::{
        delete_generic_password, get_generic_password, set_generic_password,
    };

    const NOT_FOUND: i32 = -25300;

    pub fn store(service: &str, account: &str, key_der: &[u8]) -> Result<(), String> {
        match set_generic_password(service, account, key_der) {
            Ok(()) => Ok(()),
            Err(_) => {
                // Duplicate item: rewrite in place.
                let _ = delete_generic_password(service, account);
                match set_generic_password(service, account, key_der) {
                    Ok(()) => Ok(()),
                    Err(err) => {
                        log::warn!("sync: Keychain write failed; using app data fallback: {err}");
                        super::store_fallback(key_der)
                    }
                }
            }
        }
    }

    pub fn load(service: &str, account: &str) -> Option<Vec<u8>> {
        match get_generic_password(service, account) {
            Ok(bytes) => Some(bytes),
            Err(_) => super::load_fallback(),
        }
    }

    pub fn delete(service: &str, account: &str) -> Result<(), String> {
        delete_generic_password(service, account).map_err(|e| {
            if e.code() == NOT_FOUND {
                String::new()
            } else {
                e.to_string()
            }
        })
    }
}
