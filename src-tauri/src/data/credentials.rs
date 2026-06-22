#[cfg(target_os = "macos")]
use tauri::Manager;
use tauri::{AppHandle, Wry};
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
const SERVICE: &str = "verenu";

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
        .trim_end_matches(|c: char| c == '\0' || c.is_control())
        .trim()
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
            match CredDeleteW(PCWSTR(target_wide.as_ptr()), CRED_TYPE_GENERIC, Some(0)) {
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
                let normalized = normalize_key(&pw).to_string();
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

#[cfg(windows)]
pub fn has(provider: &str) -> bool {
    !get(provider).is_empty()
}

// ================================ macOS: Keychain ================================

#[cfg(target_os = "macos")]
use security_framework::passwords::{
    delete_generic_password, get_generic_password, set_generic_password,
};
#[cfg(target_os = "macos")]
use std::path::{Path, PathBuf};

#[cfg(target_os = "macos")]
const KEYCHAIN_SERVICE: &str = "com.verenu.app";
#[cfg(target_os = "macos")]
const KEYCHAIN_ITEM_NOT_FOUND: i32 = -25300;
#[cfg(target_os = "macos")]
const KEYCHAIN_DUPLICATE_ITEM: i32 = -25299;

#[cfg(target_os = "macos")]
fn tauri_legacy_creds_path(app: &AppHandle) -> PathBuf {
    app.path()
        .app_data_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("credentials.json")
}

#[cfg(target_os = "macos")]
fn manual_legacy_creds_path_from_home(home: &Path, app_dir: &str) -> PathBuf {
    home.join(format!(
        "Library/Application Support/{app_dir}/credentials.json"
    ))
}

#[cfg(target_os = "macos")]
fn manual_legacy_creds_paths(app: &AppHandle) -> Vec<PathBuf> {
    let home = app.path().home_dir().unwrap_or_else(|_| PathBuf::from("."));

    ["Verenu"]
        .into_iter()
        .map(|app_dir| manual_legacy_creds_path_from_home(Path::new(&home), app_dir))
        .collect()
}

#[cfg(target_os = "macos")]
fn legacy_creds_paths(app: &AppHandle) -> Vec<PathBuf> {
    let mut paths = vec![tauri_legacy_creds_path(app)];
    for path in manual_legacy_creds_paths(app) {
        if !paths.iter().any(|existing| existing == &path) {
            paths.push(path);
        }
    }
    paths
}

#[cfg(target_os = "macos")]
fn read_legacy_creds_file(path: &Path) -> serde_json::Map<String, serde_json::Value> {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

#[cfg(target_os = "macos")]
fn write_legacy_creds_file(
    path: &Path,
    map: &serde_json::Map<String, serde_json::Value>,
) -> Result<(), String> {
    use std::fs::OpenOptions;
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;

    if map.is_empty() {
        if let Err(e) = std::fs::remove_file(path) {
            if e.kind() != std::io::ErrorKind::NotFound {
                return Err(format!("failed to remove legacy credentials file: {e}"));
            }
        }
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

    if let Err(e) = std::fs::rename(&tmp_path, path) {
        let _ = std::fs::remove_file(&tmp_path);
        return Err(format!("replace credentials failed: {e}"));
    }
    Ok(())
}

#[cfg(target_os = "macos")]
struct LegacyCredFile {
    path: PathBuf,
    existed: bool,
    map: serde_json::Map<String, serde_json::Value>,
}

#[cfg(target_os = "macos")]
fn load_legacy_cred_files(app: &AppHandle) -> Vec<LegacyCredFile> {
    legacy_creds_paths(app)
        .into_iter()
        .map(|path| LegacyCredFile {
            existed: path.exists(),
            map: read_legacy_creds_file(&path),
            path,
        })
        .collect()
}

#[cfg(target_os = "macos")]
fn legacy_file_contains_provider_key(file: &LegacyCredFile, provider: &str) -> bool {
    user_for(provider).is_some_and(|user| file.map.contains_key(user))
}

#[cfg(target_os = "macos")]
fn keychain_user(provider: &str) -> Result<&'static str, String> {
    user_for(provider).ok_or_else(|| format!("Unknown provider: {provider}"))
}

#[cfg(target_os = "macos")]
fn read_keychain_service(service: &str, provider: &str) -> Result<Option<String>, String> {
    let user = keychain_user(provider)?;
    match get_generic_password(service, user) {
        Ok(bytes) => String::from_utf8(bytes)
            .map(Some)
            .map_err(|e| format!("Keychain value for {provider} was not valid UTF-8: {e}")),
        Err(err) if err.code() == KEYCHAIN_ITEM_NOT_FOUND => Ok(None),
        Err(err) => Err(format!("Keychain read failed for {provider}: {err}")),
    }
}

#[cfg(target_os = "macos")]
fn read_keychain(provider: &str) -> Result<Option<String>, String> {
    read_keychain_service(KEYCHAIN_SERVICE, provider)
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
        match set_generic_password(KEYCHAIN_SERVICE, user, key.as_bytes()) {
            Ok(()) => Ok(()),
            Err(err) if err.code() == KEYCHAIN_DUPLICATE_ITEM => {
                match delete_generic_password(KEYCHAIN_SERVICE, user) {
                    Ok(()) => {}
                    Err(err) if err.code() == KEYCHAIN_ITEM_NOT_FOUND => {}
                    Err(err) => {
                        return Err(format!(
                            "Keychain overwrite cleanup failed for {provider}: {err}"
                        ));
                    }
                }

                set_generic_password(KEYCHAIN_SERVICE, user, key.as_bytes())
                    .map_err(|err| format!("Keychain overwrite failed for {provider}: {err}"))
            }
            Err(err) => Err(format!("Keychain write failed for {provider}: {err}")),
        }
    }
}

#[cfg(target_os = "macos")]
fn verify_keychain_write(provider: &str, expected_key: &str) -> Result<(), String> {
    match read_keychain(provider)? {
        Some(saved) if saved == expected_key => Ok(()),
        Some(_) => Err(format!(
            "Keychain verification failed for {provider}: value mismatch"
        )),
        None => Err(format!(
            "Keychain verification failed for {provider}: item missing"
        )),
    }
}

#[cfg(target_os = "macos")]
fn cleanup_legacy_plaintext_entries(app: &AppHandle, providers: &[&str]) {
    for mut legacy in load_legacy_cred_files(app) {
        let mut changed = false;
        for provider in providers {
            if let Some(user) = user_for(provider) {
                if legacy.map.remove(user).is_some() {
                    changed = true;
                }
            }
        }
        if !changed {
            continue;
        }
        if let Err(err) = write_legacy_creds_file(&legacy.path, &legacy.map) {
            let legacy_label = legacy
                .path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("credentials.json");
            log::warn!(
                "Could not clean legacy plaintext credentials file {}: {}",
                legacy_label,
                err
            );
        }
    }
}

#[cfg(target_os = "macos")]
pub fn get(provider: &str) -> String {
    match read_keychain(provider) {
        Ok(Some(key)) => normalize_key(&key).to_string(),
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
        Ok(Some(key)) => !normalize_key(&key).is_empty(),
        Ok(None) => false,
        Err(e) => {
            log::warn!("{e}");
            false
        }
    }
}

#[cfg(target_os = "macos")]
pub fn save(app: &AppHandle, provider: &str, key: &str) -> Result<(), String> {
    let expected_key = normalize_key(key).to_string();
    set(provider, &expected_key)?;
    verify_keychain_write(provider, &expected_key)?;
    cleanup_legacy_plaintext_entries(app, &[provider]);
    Ok(())
}

#[cfg(not(target_os = "macos"))]
pub fn save(_app: &AppHandle, provider: &str, key: &str) -> Result<(), String> {
    set(provider, key)
}

#[cfg(any(windows, target_os = "macos"))]
pub fn delete(provider: &str) -> Result<(), String> {
    set(provider, "")
}

/// Returns `Ok(true)` if a key exists and is readable, `Ok(false)` if absent, `Err` on access failure.
/// On macOS this triggers the Keychain prompt if the app hasn't been granted Always Allow yet.
#[cfg(target_os = "macos")]
pub fn read_for_status(provider: &str) -> Result<bool, String> {
    match read_keychain(provider) {
        Ok(Some(k)) => Ok(!normalize_key(&k).is_empty()),
        Ok(None) => Ok(false),
        Err(e) => Err(e),
    }
}

#[cfg(target_os = "macos")]
pub fn delete_saved(app: &AppHandle, provider: &str) -> Result<(), String> {
    delete(provider)?;
    cleanup_legacy_plaintext_entries(app, &[provider]);
    Ok(())
}

#[cfg(not(target_os = "macos"))]
pub fn delete_saved(_app: &AppHandle, provider: &str) -> Result<(), String> {
    delete(provider)
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

#[cfg(not(any(windows, target_os = "macos")))]
pub fn delete(_provider: &str) -> Result<(), String> {
    Ok(())
}

/// Moves any plaintext API keys from settings.json or legacy macOS
/// credentials.json files into the OS secret store, then keeps retrying cleanup
/// until plaintext remnants are gone.
pub fn migrate_from_store(_app: &AppHandle, store: &Store<Wry>) {
    let already_marked = store
        .get(crate::data::store::CREDENTIALS_MIGRATED)
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    #[cfg(target_os = "macos")]
    let mut legacy_files = load_legacy_cred_files(_app);

    let has_store_plaintext = [
        crate::data::store::KEY_GROQ,
        crate::data::store::KEY_OPENAI,
        crate::data::store::KEY_GOOGLE,
    ]
    .into_iter()
    .any(|store_key| {
        store
            .get(store_key)
            .and_then(|v| v.as_str().map(str::to_owned))
            .is_some_and(|value| !normalize_key(&value).is_empty())
    });

    #[cfg(target_os = "macos")]
    let legacy_cleanup_needed = legacy_files.iter().any(|file| {
        file.existed
            && [
                crate::data::store::GROQ,
                crate::data::store::OPENAI,
                crate::data::store::GOOGLE,
            ]
            .into_iter()
            .any(|provider| legacy_file_contains_provider_key(file, provider))
    });
    #[cfg(not(target_os = "macos"))]
    let legacy_cleanup_needed = false;

    if already_marked && !has_store_plaintext && !legacy_cleanup_needed {
        return;
    }

    let mut any_failed = false;
    for (provider, store_key) in [
        (crate::data::store::GROQ, crate::data::store::KEY_GROQ),
        (crate::data::store::OPENAI, crate::data::store::KEY_OPENAI),
        (crate::data::store::GOOGLE, crate::data::store::KEY_GOOGLE),
    ] {
        let store_plaintext = store
            .get(store_key)
            .and_then(|v| v.as_str().map(String::from))
            .map(|value| normalize_key(&value).to_string())
            .unwrap_or_default();
        #[cfg(target_os = "macos")]
        let legacy_plaintext = user_for(provider)
            .and_then(|user| {
                legacy_files.iter().find_map(|file| {
                    file.map
                        .get(user)
                        .and_then(|v| v.as_str())
                        .filter(|value| !normalize_key(value).is_empty())
                        .map(|value| normalize_key(value).to_string())
                })
            })
            .unwrap_or_default();
        #[cfg(not(target_os = "macos"))]
        let legacy_plaintext = String::new();
        let plaintext = if store_plaintext.is_empty() {
            legacy_plaintext
        } else {
            store_plaintext
        };

        let already_migrated = !get(provider).is_empty();
        if plaintext.is_empty() && !already_migrated {
            continue;
        }

        if !already_migrated && !plaintext.is_empty() {
            if let Err(e) = set(provider, &plaintext) {
                log::error!("Migration: could not write {provider} key to secret store: {e}");
                any_failed = true;
                continue;
            }
            log::info!("Migration: moved {provider} API key to OS secret store");
        }

        let _ = store.delete(store_key);
        #[cfg(target_os = "macos")]
        if let Some(user) = user_for(provider) {
            for legacy in &mut legacy_files {
                if legacy.map.remove(user).is_some() {
                    log::debug!("Migration: removed legacy plaintext entry for {provider}");
                }
            }
        }
    }

    #[cfg(target_os = "macos")]
    for legacy in &legacy_files {
        if let Err(e) = write_legacy_creds_file(&legacy.path, &legacy.map) {
            let legacy_label = legacy
                .path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("credentials.json");
            log::warn!(
                "Migration: could not persist legacy credentials cleanup for {}: {}",
                legacy_label,
                e
            );
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

#[cfg(test)]
mod tests {
    #[cfg(target_os = "macos")]
    use super::legacy_file_contains_provider_key;
    #[cfg(target_os = "macos")]
    use super::manual_legacy_creds_path_from_home;
    use super::normalize_key;
    #[cfg(target_os = "macos")]
    use super::write_legacy_creds_file;
    #[cfg(target_os = "macos")]
    use super::LegacyCredFile;
    #[cfg(target_os = "macos")]
    use std::path::Path;
    #[cfg(target_os = "macos")]
    use std::path::PathBuf;

    #[test]
    fn normalize_key_trims_surrounding_whitespace() {
        assert_eq!(normalize_key("  key  "), "key");
    }

    #[test]
    fn normalize_key_treats_whitespace_only_input_as_empty() {
        assert_eq!(normalize_key("   \t  \n"), "");
    }

    #[test]
    fn normalize_key_strips_trailing_null_bytes() {
        assert_eq!(normalize_key("sk-test\0\0"), "sk-test");
    }

    #[test]
    fn normalize_key_strips_trailing_control_characters() {
        assert_eq!(normalize_key("gsk-test\r\n\0"), "gsk-test");
    }

    #[test]
    fn normalize_key_preserves_normal_key_content() {
        assert_eq!(normalize_key("AIza-normal-key_123"), "AIza-normal-key_123");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn manual_legacy_creds_path_supports_verenu_directory() {
        let path = manual_legacy_creds_path_from_home(Path::new("/Users/tester"), "Verenu");
        assert_eq!(
            path,
            Path::new("/Users/tester/Library/Application Support/Verenu/credentials.json")
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn legacy_cleanup_only_tracks_provider_keys() {
        let mut map = serde_json::Map::new();
        map.insert(
            crate::data::store::KEY_GROQ.to_string(),
            serde_json::Value::String("gsk_test".into()),
        );
        map.insert(
            "custom_key".into(),
            serde_json::Value::String("value".into()),
        );

        let file = LegacyCredFile {
            path: PathBuf::from("/tmp/credentials.json"),
            existed: true,
            map,
        };

        assert!(legacy_file_contains_provider_key(
            &file,
            crate::data::store::GROQ
        ));
        assert!(!legacy_file_contains_provider_key(
            &file,
            crate::data::store::OPENAI
        ));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn empty_legacy_write_ignores_missing_file() {
        let path = std::env::temp_dir().join(format!(
            "verenu-missing-legacy-credentials-{}.json",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        let map = serde_json::Map::new();

        assert!(write_legacy_creds_file(&path, &map).is_ok());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn empty_legacy_write_propagates_delete_errors() {
        let path = std::env::temp_dir().join(format!(
            "verenu-legacy-credentials-delete-error-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir(&path).expect("create temp directory");
        let map = serde_json::Map::new();

        let result = write_legacy_creds_file(&path, &map);
        std::fs::remove_dir(&path).expect("remove temp directory");

        assert!(result.is_err());
    }
}
