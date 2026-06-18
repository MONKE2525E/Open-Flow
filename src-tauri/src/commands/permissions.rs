//! macOS Accessibility / Microphone / Keychain permission commands.


// ---------- macOS permissions ----------

/// Whether Verenu is trusted for the Accessibility API (needed for the global
/// hotkey, Cmd+V injection, and auto-learn). When `prompt` is true, macOS shows
/// the system permission dialog. Always true on non-macOS platforms.
#[cfg(target_os = "macos")]
#[tauri::command]
pub fn check_accessibility_permission(prompt: bool) -> bool {
    use accessibility_sys::{kAXTrustedCheckOptionPrompt, AXIsProcessTrustedWithOptions};
    use core_foundation::base::TCFType;
    use core_foundation::boolean::CFBoolean;
    use core_foundation::dictionary::CFDictionary;
    use core_foundation::string::CFString;

    unsafe {
        let key = CFString::wrap_under_get_rule(kAXTrustedCheckOptionPrompt);
        let value = CFBoolean::from(prompt);
        let options = CFDictionary::from_CFType_pairs(&[(key.as_CFType(), value.as_CFType())]);
        AXIsProcessTrustedWithOptions(options.as_concrete_TypeRef())
    }
}

#[cfg(not(target_os = "macos"))]
#[tauri::command]
pub fn check_accessibility_permission(_prompt: bool) -> bool {
    true
}

/// Opens the macOS Accessibility privacy pane so the user can grant permission.
/// No-op on other platforms.
#[tauri::command]
pub fn open_accessibility_settings() -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility")
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub fn get_accessibility_permission_status() -> String {
    #[cfg(target_os = "macos")]
    {
        if check_accessibility_permission(false) {
            "authorized".to_string()
        } else {
            "needs_permission".to_string()
        }
    }

    #[cfg(not(target_os = "macos"))]
    {
        "authorized".to_string()
    }
}

#[tauri::command]
pub fn get_microphone_permission_status() -> String {
    #[cfg(target_os = "macos")]
    {
        crate::system::mac_app::microphone_permission_status().to_string()
    }

    #[cfg(not(target_os = "macos"))]
    {
        "authorized".to_string()
    }
}

/// Opens the macOS Microphone privacy pane so the user can grant permission.
/// No-op on other platforms.
#[tauri::command]
pub fn open_microphone_settings() -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone")
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Reads the stored API key for `provider` from the system credential store to check
/// whether the app has been granted Keychain access. On macOS this triggers the native
/// Keychain dialog if the app hasn't been granted "Always Allow" yet.
/// Returns "authorized" | "not_configured" | "denied".
#[tauri::command]
#[cfg_attr(not(target_os = "macos"), allow(unused_variables))]
pub async fn check_keychain_access(provider: String) -> String {
    #[cfg(target_os = "macos")]
    {
        match tokio::task::spawn_blocking(move || {
            crate::data::credentials::read_for_status(&provider)
        })
        .await
        {
            Ok(Ok(true)) => "authorized".to_string(),
            Ok(Ok(false)) => "not_configured".to_string(),
            _ => "denied".to_string(),
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        "authorized".to_string()
    }
}

