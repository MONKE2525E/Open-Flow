use tauri::{AppHandle, Manager, Wry};
use tauri_plugin_store::Store;

#[cfg(windows)]
use std::ptr;
#[cfg(windows)]
use windows::core::{PCWSTR, PWSTR};
#[cfg(windows)]
use windows::Win32::Security::Credentials::{
    CredDeleteW, CredFree, CredReadW, CredWriteW, CREDENTIALW, CRED_PERSIST_LOCAL_MACHINE,
    CRED_TYPE_GENERIC,
};

#[cfg(windows)]
const SERVICE: &str = "open-flow";

// HRESULT_FROM_WIN32(ERROR_NOT_FOUND) — credential entry absent, not an error
#[cfg(windows)]
const HRESULT_NOT_FOUND: i32 = 0x80070490_u32 as i32;

fn user_for(provider: &str) -> Option<&'static str> {
    use crate::data::store;
    match provider {
        store::GROQ => Some(store::KEY_GROQ),
        store::OPENAI => Some(store::KEY_OPENAI),
        store::GOOGLE => Some(store::KEY_GOOGLE),
        _ => None,
    }
}

fn normalize_key(key: &str) -> &str {
    key.trim()
}

#[cfg(windows)]
fn wide_null(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

// ============================ Windows: Credential Manager ============================

#[cfg(windows)]
pub fn set(provider: &str, key: &str) -> Result<(), String> {
    let key = normalize_key(key);
    let user = user_for(provider).ok_or_else(|| format!("Unknown provider: {provider}"))?;
    let target = format!("{user}.{SERVICE}");
    let mut target_wide = wide_null(&target);

    if key.is_empty() {
        unsafe {
            match CredDeleteW(PCWSTR(target_wide.as_ptr()), CRED_TYPE_GENERIC, None) {
                Ok(_) => Ok(()),
                Err(e) if e.code().0 == HRESULT_NOT_FOUND => Ok(()),
                Err(e) => Err(format!(
                    "Credential Manager delete failed for {provider}: {e}"
                )),
            }
        }
    } else {
        let mut user_wide = wide_null(user);
        // Encode as UTF-16-LE — Windows-native format for credential blobs
        let utf16: Vec<u16> = key.encode_utf16().collect();
        let mut blob: Vec<u8> = utf16.iter().flat_map(|c| c.to_le_bytes()).collect();

        let mut cred: CREDENTIALW = unsafe { std::mem::zeroed() };
        cred.Type = CRED_TYPE_GENERIC;
        cred.TargetName = PWSTR(target_wide.as_mut_ptr());
        cred.CredentialBlobSize = blob.len() as u32;
        cred.CredentialBlob = blob.as_mut_ptr();
        cred.Persist = CRED_PERSIST_LOCAL_MACHINE;
        cred.UserName = PWSTR(user_wide.as_mut_ptr());

        unsafe {
            CredWriteW(&cred, 0)
                .map_err(|e| format!("Credential Manager write failed for {provider}: {e}"))
        }
    }
}

#[cfg(windows)]
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
            None,
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
                let pw = if blob_size == 0 {
                    String::new()
                } else if blob_ptr.is_null() {
                    log::error!(
                        "Credential Manager read returned non-zero blob size with null blob pointer for {provider}"
                    );
                    String::new()
                } else {
                    let blob = std::slice::from_raw_parts(blob_ptr, blob_size);
                    // Decode UTF-16-LE back to a Rust String
                    let utf16: Vec<u16> = blob
                        .chunks_exact(2)
                        .map(|b| u16::from_le_bytes([b[0], b[1]]))
                        .collect();
                    String::from_utf16_lossy(&utf16)
                };
                CredFree(p_cred as *const _);
                log::debug!(
                    "credentials: read ok provider={provider} key_len={}",
                    pw.len()
                );
                pw
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

#[cfg(windows)]
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
            None,
            &mut p_cred,
        )
        .is_ok();
        if ok && !p_cred.is_null() {
            CredFree(p_cred as *const _);
        }
        ok
    }
}

// ================================ macOS: Keychain ================================

#[cfg(target_os = "macos")]
use security_framework::passwords::{
    delete_generic_password, get_generic_password, set_generic_password,
};

#[cfg(target_os = "macos")]
const KEYCHAIN_SERVICE: &str = "com.openflow.app";
#[cfg(target_os = "macos")]
const KEYCHAIN_ITEM_NOT_FOUND: i32 = -25300;

#[cfg(target_os = "macos")]
fn legacy_creds_path(app: &AppHandle) -> std::path::PathBuf {
    app.path()
        .app_data_dir()
        .unwrap_or_else(|_| std::path::PathBuf::from("."))
        .join("credentials.json")
}

#[cfg(target_os = "macos")]
fn read_legacy_creds(app: &AppHandle) -> serde_json::Map<String, serde_json::Value> {
    std::fs::read_to_string(legacy_creds_path(app))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

#[cfg(target_os = "macos")]
fn write_legacy_creds(
    app: &AppHandle,
    map: &serde_json::Map<String, serde_json::Value>,
) -> Result<(), String> {
    use std::fs::OpenOptions;
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;

    let path = legacy_creds_path(app);
    if map.is_empty() {
        let _ = std::fs::remove_file(&path);
        return Ok(());
    }

    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("create credentials dir failed: {e}"))?;
    }
    let data = serde_json::to_vec(map).map_err(|e| e.to_string())?;
    let tmp_path = path.with_file_name("credentials.json.tmp");

    let write_result = (|| -> Result<(), String> {
        let mut file = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .mode(0o600)
            .open(&tmp_path)
            .map_err(|e| format!("open credentials temp file failed: {e}"))?;
        file.write_all(&data)
            .map_err(|e| format!("write credentials failed: {e}"))?;
        file.sync_all()
            .map_err(|e| format!("sync credentials failed: {e}"))?;
        Ok(())
    })();

    if let Err(e) = write_result {
        let _ = std::fs::remove_file(&tmp_path);
        return Err(e);
    }

    if let Err(e) = std::fs::rename(&tmp_path, &path) {
        let _ = std::fs::remove_file(&tmp_path);
        return Err(format!("replace credentials failed: {e}"));
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn keychain_user(provider: &str) -> Result<&'static str, String> {
    user_for(provider).ok_or_else(|| format!("Unknown provider: {provider}"))
}

#[cfg(target_os = "macos")]
fn read_keychain(provider: &str) -> Result<Option<String>, String> {
    let user = keychain_user(provider)?;
    match get_generic_password(KEYCHAIN_SERVICE, user) {
        Ok(bytes) => String::from_utf8(bytes)
            .map(Some)
            .map_err(|e| format!("Keychain value for {provider} was not valid UTF-8: {e}")),
        Err(err) if err.code() == KEYCHAIN_ITEM_NOT_FOUND => Ok(None),
        Err(err) => Err(format!("Keychain read failed for {provider}: {err}")),
    }
}

#[cfg(target_os = "macos")]
pub fn set(provider: &str, key: &str) -> Result<(), String> {
    let key = normalize_key(key);
    let user = keychain_user(provider)?;
    if key.is_empty() {
        match delete_generic_password(KEYCHAIN_SERVICE, user) {
            Ok(()) => Ok(()),
            Err(err) if err.code() == KEYCHAIN_ITEM_NOT_FOUND => Ok(()),
            Err(err) => Err(format!("Keychain delete failed for {provider}: {err}")),
        }
    } else {
        set_generic_password(KEYCHAIN_SERVICE, user, key.as_bytes())
            .map_err(|err| format!("Keychain write failed for {provider}: {err}"))
    }
}

#[cfg(target_os = "macos")]
pub fn get(provider: &str) -> String {
    match read_keychain(provider) {
        Ok(Some(key)) => key,
        Ok(None) => String::new(),
        Err(e) => {
            log::error!("{e}");
            String::new()
        }
    }
}

#[cfg(target_os = "macos")]
pub fn has(provider: &str) -> bool {
    match read_keychain(provider) {
        Ok(Some(key)) => !key.is_empty(),
        Ok(None) => false,
        Err(e) => {
            log::warn!("{e}");
            false
        }
    }
}

#[cfg(any(windows, target_os = "macos"))]
pub fn delete(provider: &str) -> Result<(), String> {
    set(provider, "")
}

// ============================== Fallback (e.g. Linux) ==============================

#[cfg(not(any(windows, target_os = "macos")))]
pub fn set(_provider: &str, _key: &str) -> Result<(), String> {
    Ok(())
}

#[cfg(not(any(windows, target_os = "macos")))]
pub fn get(_provider: &str) -> String {
    String::new()
}

#[cfg(not(any(windows, target_os = "macos")))]
pub fn has(_provider: &str) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::normalize_key;

    #[test]
    fn normalize_key_trims_surrounding_whitespace() {
        assert_eq!(normalize_key("  key  "), "key");
    }

    #[test]
    fn normalize_key_treats_whitespace_only_input_as_empty() {
        assert_eq!(normalize_key("   \t  \n"), "");
    }
}

/// One-shot migration: moves any plaintext API keys from settings.json or the
/// legacy macOS credentials.json file into the OS secret store, then sets a flag
/// so it never runs again. Platform-agnostic — delegates to `set`/`get`.
pub fn migrate_from_store(_app: &AppHandle, store: &Store<Wry>) {
    if store
        .get(crate::data::store::CREDENTIALS_MIGRATED)
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
    {
        return;
    }

    #[cfg(target_os = "macos")]
    let mut legacy_creds = read_legacy_creds(_app);
    #[cfg(not(target_os = "macos"))]
    let mut legacy_creds: serde_json::Map<String, serde_json::Value> = serde_json::Map::new();

    #[cfg(target_os = "macos")]
    let legacy_creds_existed = legacy_creds_path(_app).exists();

    let mut any_failed = false;
    for (provider, store_key) in [
        (crate::data::store::GROQ, crate::data::store::KEY_GROQ),
        (crate::data::store::OPENAI, crate::data::store::KEY_OPENAI),
        (crate::data::store::GOOGLE, crate::data::store::KEY_GOOGLE),
    ] {
        let store_plaintext = store
            .get(store_key)
            .and_then(|v| v.as_str().map(String::from))
            .unwrap_or_default();
        let legacy_plaintext = user_for(provider)
            .and_then(|user| legacy_creds.get(user))
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        let plaintext = if store_plaintext.is_empty() {
            legacy_plaintext
        } else {
            store_plaintext
        };

        let already_migrated = has(provider);
        if plaintext.is_empty() && !already_migrated {
            continue;
        }

        // Only write if not already present — avoids overwriting a manually-set credential.
        if !already_migrated && !plaintext.is_empty() {
            if let Err(e) = set(provider, &plaintext) {
                log::error!("Migration: could not write {provider} key to secret store: {e}");
                any_failed = true;
                continue;
            }
            log::info!("Migration: moved {provider} API key to OS secret store");
        }

        let _ = store.delete(store_key);
        if let Some(user) = user_for(provider) {
            if legacy_creds.remove(user).is_some() {
                #[cfg(target_os = "macos")]
                log::debug!("Migration: removed legacy plaintext entry for {provider}");
            }
        }
    }

    #[cfg(target_os = "macos")]
    {
        if legacy_creds.is_empty() {
            if legacy_creds_existed {
                if let Err(e) = std::fs::remove_file(legacy_creds_path(_app)) {
                    log::warn!(
                        "Migration: could not remove legacy plaintext credentials file: {e}"
                    );
                    any_failed = true;
                }
            }
        } else if let Err(e) = write_legacy_creds(_app, &legacy_creds) {
            log::warn!("Migration: could not persist legacy credentials cleanup: {e}");
            any_failed = true;
        }
    }

    if !any_failed {
        store.set(
            crate::data::store::CREDENTIALS_MIGRATED,
            serde_json::json!(true),
        );
    }
    if let Err(e) = store.save() {
        log::warn!("Migration: could not persist settings.json after key removal: {e}");
    }
}
