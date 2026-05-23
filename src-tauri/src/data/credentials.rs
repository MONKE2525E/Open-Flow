use keyring::{Entry, Error as KeyringError};
use tauri::Wry;
use tauri_plugin_store::Store;

const SERVICE: &str = "open-flow";

fn user_for(provider: &str) -> Option<&'static str> {
    match provider {
        "groq" => Some("api_key_groq"),
        "openai" => Some("api_key_openai"),
        "google" => Some("api_key_google"),
        _ => None,
    }
}

pub fn set(provider: &str, key: &str) -> Result<(), String> {
    let user = user_for(provider).ok_or_else(|| format!("Unknown provider: {provider}"))?;
    Entry::new(SERVICE, user)
        .and_then(|e| e.set_password(key))
        .map_err(|e| format!("Credential Manager write failed for {provider}: {e}"))
}

pub fn get(provider: &str) -> String {
    let user = match user_for(provider) {
        Some(u) => u,
        None => {
            log::warn!("credentials::get called with unknown provider: {provider}");
            return String::new();
        }
    };
    match Entry::new(SERVICE, user).and_then(|e| e.get_password()) {
        Ok(pw) => pw,
        Err(KeyringError::NoEntry) => String::new(),
        Err(e) => {
            log::error!("Credential Manager read failed for {provider}: {e}");
            String::new()
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

        // Distinguish "entry is absent" from "read error" so a transient system
        // failure never causes us to overwrite an existing key or delete plaintext.
        let user = user_for(provider).unwrap();
        let missing = match Entry::new(SERVICE, user).and_then(|e| e.get_password()) {
            Ok(_) => false,
            Err(KeyringError::NoEntry) => true,
            Err(e) => {
                log::error!("Migration: could not read {provider} from Credential Manager: {e}");
                any_failed = true;
                continue;
            }
        };

        if missing {
            if let Err(e) = set(provider, &plaintext) {
                log::error!("Migration: could not write {provider} key to Credential Manager: {e}");
                // Leave plaintext copy intact — better than losing the key.
                any_failed = true;
                continue;
            }
            log::info!("Migration: moved {provider} API key to Credential Manager");
        }
        let _ = store.delete(store_key);
    }

    // Only mark done if every key that needed moving succeeded.
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
