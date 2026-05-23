use std::ptr;
use tauri::Wry;
use tauri_plugin_store::Store;
use windows::Win32::Security::Credentials::{
    CredDeleteW, CredFree, CredReadW, CredWriteW, CREDENTIALW, CRED_PERSIST_LOCAL_MACHINE,
    CRED_TYPE_GENERIC,
};
use windows::core::{PCWSTR, PWSTR};

const SERVICE: &str = "open-flow";

// HRESULT_FROM_WIN32(ERROR_NOT_FOUND) — credential entry absent, not an error
const HRESULT_NOT_FOUND: i32 = 0x80070490_u32 as i32;

fn user_for(provider: &str) -> Option<&'static str> {
    match provider {
        "groq" => Some("api_key_groq"),
        "openai" => Some("api_key_openai"),
        "google" => Some("api_key_google"),
        _ => None,
    }
}

fn wide_null(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

pub fn set(provider: &str, key: &str) -> Result<(), String> {
    let user = user_for(provider).ok_or_else(|| format!("Unknown provider: {provider}"))?;
    let target = format!("{SERVICE}/{user}");
    let mut target_wide = wide_null(&target);

    if key.is_empty() {
        unsafe {
            match CredDeleteW(PCWSTR(target_wide.as_ptr()), CRED_TYPE_GENERIC, None) {
                Ok(_) => Ok(()),
                Err(e) if e.code().0 == HRESULT_NOT_FOUND => Ok(()),
                Err(e) => Err(format!("Credential Manager delete failed for {provider}: {e}")),
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

pub fn get(provider: &str) -> String {
    let user = match user_for(provider) {
        Some(u) => u,
        None => {
            log::warn!("credentials::get called with unknown provider: {provider}");
            return String::new();
        }
    };
    let target = format!("{SERVICE}/{user}");
    let target_wide = wide_null(&target);

    unsafe {
        let mut p_cred: *mut CREDENTIALW = ptr::null_mut();
        match CredReadW(PCWSTR(target_wide.as_ptr()), CRED_TYPE_GENERIC, None, &mut p_cred) {
            Ok(()) => {
                let cred = &*p_cred;
                let pw = if cred.CredentialBlobSize == 0 {
                    String::new()
                } else {
                    let blob = std::slice::from_raw_parts(
                        cred.CredentialBlob,
                        cred.CredentialBlobSize as usize,
                    );
                    // Decode UTF-16-LE back to a Rust String
                    let utf16: Vec<u16> = blob
                        .chunks_exact(2)
                        .map(|b| u16::from_le_bytes([b[0], b[1]]))
                        .collect();
                    String::from_utf16_lossy(&utf16)
                };
                CredFree(p_cred as *const _);
                pw
            }
            Err(e) => {
                if e.code().0 != HRESULT_NOT_FOUND {
                    log::error!("Credential Manager read failed for {provider}: {e}");
                }
                String::new()
            }
        }
    }
}

pub fn has(provider: &str) -> bool {
    !get(provider).is_empty()
}

/// One-shot migration: moves any plaintext API keys from settings.json into
/// Windows Credential Manager, then sets a flag so it never runs again.
pub fn migrate_from_store(store: &Store<Wry>) {
    if store
        .get(crate::data::store::CREDENTIALS_MIGRATED)
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
    {
        return;
    }

    let mut any_failed = false;
    for (provider, store_key) in [
        ("groq", crate::data::store::KEY_GROQ),
        ("openai", crate::data::store::KEY_OPENAI),
        ("google", crate::data::store::KEY_GOOGLE),
    ] {
        let plaintext = store
            .get(store_key)
            .and_then(|v| v.as_str().map(String::from))
            .unwrap_or_default();

        if plaintext.is_empty() {
            continue;
        }

        // Only write if not already present — avoids overwriting a manually-set credential.
        if get(provider).is_empty() {
            if let Err(e) = set(provider, &plaintext) {
                log::error!("Migration: could not write {provider} key to Credential Manager: {e}");
                any_failed = true;
                continue;
            }
            log::info!("Migration: moved {provider} API key to Credential Manager");
        }
        let _ = store.delete(store_key);
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
