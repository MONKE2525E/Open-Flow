//! macOS Accessibility / Microphone / Keychain permission commands.

// ---------- macOS permissions ----------

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionSourceHints {
    pub hotkey_tap_active: bool,
    pub global_input_seen: bool,
    pub microphone_verified: bool,
    pub accessibility_verified: bool,
}

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MacPermissionSnapshot {
    pub accessibility: String,
    pub input_monitoring: String,
    pub microphone: String,
    pub keychain: String,
    pub all_core_granted: bool,
    pub needs_relaunch: bool,
    pub last_checked_at: String,
    pub source_hints: PermissionSourceHints,
    pub diagnostics: MacPermissionDiagnostics,
}

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MacPermissionDiagnostics {
    pub bundle_identifier: Option<String>,
    pub bundle_path: Option<String>,
    pub executable_path: Option<String>,
    pub process_id: u32,
    pub accessibility_trusted: bool,
    pub input_monitoring_raw: String,
}

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TccResetStep {
    pub service: String,
    pub ok: bool,
    pub message: String,
}

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TccResetResult {
    pub bundle_identifier: Option<String>,
    pub steps: Vec<TccResetStep>,
}

fn summarize_core_permissions(
    accessibility: &str,
    input_monitoring: &str,
    microphone: &str,
    source_hints: &PermissionSourceHints,
) -> (bool, bool) {
    let all_core_granted = accessibility == "authorized"
        && input_monitoring == "authorized"
        && microphone == "authorized";

    // IOHID/TCC decisions can be cached by the running process. If macOS says
    // Input Monitoring is granted but this process has not yet empirically seen
    // global input, a relaunch is the clearest recovery path after the user has
    // just changed the toggle in System Settings. The UI decides when to surface
    // the hint so long-time users are not nagged before interacting.
    let needs_relaunch = input_monitoring == "authorized" && !source_hints.global_input_seen;

    (all_core_granted, needs_relaunch)
}

#[cfg(target_os = "macos")]
fn permission_source_hints() -> PermissionSourceHints {
    PermissionSourceHints {
        hotkey_tap_active: crate::core::hotkey::is_tap_active(),
        global_input_seen: crate::core::hotkey::has_seen_global_input(),
        microphone_verified: crate::system::mac_app::is_microphone_verified(),
        accessibility_verified: crate::system::mac_app::is_accessibility_verified(),
    }
}

#[cfg(target_os = "macos")]
fn permission_diagnostics() -> MacPermissionDiagnostics {
    MacPermissionDiagnostics {
        bundle_identifier: crate::system::mac_app::bundle_identifier(),
        bundle_path: crate::system::mac_app::bundle_path(),
        executable_path: std::env::current_exe()
            .ok()
            .map(|p| p.to_string_lossy().into_owned()),
        process_id: std::process::id(),
        accessibility_trusted: check_accessibility_permission(false),
        input_monitoring_raw: crate::system::mac_app::input_monitoring_status().to_string(),
    }
}

#[cfg(not(target_os = "macos"))]
fn permission_diagnostics() -> MacPermissionDiagnostics {
    MacPermissionDiagnostics {
        bundle_identifier: None,
        bundle_path: None,
        executable_path: std::env::current_exe()
            .ok()
            .map(|p| p.to_string_lossy().into_owned()),
        process_id: std::process::id(),
        accessibility_trusted: true,
        input_monitoring_raw: "authorized".to_string(),
    }
}

#[cfg(not(target_os = "macos"))]
fn permission_source_hints() -> PermissionSourceHints {
    PermissionSourceHints {
        hotkey_tap_active: true,
        global_input_seen: true,
        microphone_verified: true,
        accessibility_verified: true,
    }
}

fn now_rfc3339() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

#[cfg(target_os = "macos")]
fn accessibility_permission_status() -> String {
    // A confirmed cross-process AX read is authoritative even when the raw
    // `AXIsProcessTrusted()` check is reporting a stale `false` (e.g. after an
    // ad-hoc rebuild changed the signature the TCC grant was tied to). The raw
    // check is still surfaced separately via `diagnostics.accessibility_trusted`.
    if crate::system::mac_app::is_accessibility_verified() || check_accessibility_permission(false) {
        "authorized".to_string()
    } else {
        "needs_permission".to_string()
    }
}

#[cfg(not(target_os = "macos"))]
fn accessibility_permission_status() -> String {
    "authorized".to_string()
}

#[cfg(target_os = "macos")]
fn input_monitoring_permission_status() -> String {
    if crate::core::hotkey::has_seen_global_input() {
        return "authorized".to_string();
    }
    crate::system::mac_app::input_monitoring_status().to_string()
}

#[cfg(not(target_os = "macos"))]
fn input_monitoring_permission_status() -> String {
    "authorized".to_string()
}

#[cfg(target_os = "macos")]
fn microphone_permission_status_string() -> String {
    crate::system::mac_app::microphone_permission_status().to_string()
}

#[cfg(not(target_os = "macos"))]
fn microphone_permission_status_string() -> String {
    "authorized".to_string()
}

#[cfg(target_os = "macos")]
async fn keychain_status_for_provider(provider: Option<String>) -> String {
    let Some(provider) = provider.filter(|p| !p.trim().is_empty()) else {
        return "unknown".to_string();
    };

    match tokio::task::spawn_blocking(move || crate::data::credentials::read_for_status(&provider))
        .await
    {
        Ok(Ok(true)) => "authorized".to_string(),
        Ok(Ok(false)) => "not_configured".to_string(),
        _ => "denied".to_string(),
    }
}

#[cfg(not(target_os = "macos"))]
async fn keychain_status_for_provider(_provider: Option<String>) -> String {
    "authorized".to_string()
}

async fn macos_permission_snapshot(provider: Option<String>) -> MacPermissionSnapshot {
    let accessibility = accessibility_permission_status();
    let input_monitoring = input_monitoring_permission_status();
    let microphone = microphone_permission_status_string();
    let keychain = keychain_status_for_provider(provider).await;
    let source_hints = permission_source_hints();
    let (all_core_granted, needs_relaunch) = summarize_core_permissions(
        &accessibility,
        &input_monitoring,
        &microphone,
        &source_hints,
    );

    MacPermissionSnapshot {
        accessibility,
        input_monitoring,
        microphone,
        keychain,
        all_core_granted,
        needs_relaunch,
        last_checked_at: now_rfc3339(),
        source_hints,
        diagnostics: permission_diagnostics(),
    }
}

#[tauri::command]
pub async fn get_macos_permission_snapshot(provider: Option<String>) -> MacPermissionSnapshot {
    macos_permission_snapshot(provider).await
}

/// Whether Verenu is trusted for the Accessibility API (needed for the global
/// hotkey, Cmd+V injection, and auto-learn). When `prompt` is true, macOS shows
/// the system permission dialog. Always true on non-macOS platforms.
#[cfg(target_os = "macos")]
#[tauri::command]
pub fn check_accessibility_permission(prompt: bool) -> bool {
    use accessibility_sys::{
        kAXTrustedCheckOptionPrompt, AXIsProcessTrusted, AXIsProcessTrustedWithOptions,
    };
    use core_foundation::base::TCFType;
    use core_foundation::boolean::CFBoolean;
    use core_foundation::dictionary::CFDictionary;
    use core_foundation::string::CFString;

    if !prompt {
        return unsafe { AXIsProcessTrusted() };
    }

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
    open_macos_settings(&[
        "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility",
        "x-apple.systempreferences:com.apple.preference.security",
    ])
}

#[tauri::command]
pub fn get_accessibility_permission_status() -> String {
    accessibility_permission_status()
}

#[tauri::command]
pub async fn request_accessibility_permission(provider: Option<String>) -> MacPermissionSnapshot {
    let _ = check_accessibility_permission(true);
    macos_permission_snapshot(provider).await
}

/// Returns `true` once the macOS CGEventTap has been successfully created and
/// enabled. Useful as a permission signal when Accessibility status is stale
/// immediately after the user grants access. Always true off macOS.
#[tauri::command]
pub fn is_hotkey_tap_active() -> bool {
    #[cfg(target_os = "macos")]
    {
        crate::core::hotkey::is_tap_active()
    }
    #[cfg(not(target_os = "macos"))]
    {
        true
    }
}

#[tauri::command]
pub fn get_microphone_permission_status() -> String {
    microphone_permission_status_string()
}

/// Triggers the macOS microphone consent prompt when access is undetermined,
/// then returns the resulting status. Lets the permissions UI request the mic
/// directly instead of waiting for the first recording. No-op off macOS.
#[tauri::command]
pub async fn request_microphone_permission() -> String {
    #[cfg(target_os = "macos")]
    {
        let _ = crate::system::mac_app::request_microphone().await;
        crate::system::mac_app::microphone_permission_status().to_string()
    }
    #[cfg(not(target_os = "macos"))]
    {
        "authorized".to_string()
    }
}

#[tauri::command]
pub async fn request_microphone_permission_snapshot(
    provider: Option<String>,
) -> MacPermissionSnapshot {
    #[cfg(target_os = "macos")]
    {
        let _ = crate::system::mac_app::request_microphone().await;
    }
    macos_permission_snapshot(provider).await
}

/// Opens the macOS Microphone privacy pane so the user can grant permission.
/// No-op on other platforms.
#[tauri::command]
pub fn open_microphone_settings() -> Result<(), String> {
    open_macos_settings(&[
        "x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone",
        "x-apple.systempreferences:com.apple.preference.security",
    ])
}

/// Relaunches the app. macOS caches Input Monitoring (and other TCC) decisions
/// for the life of the process, and the global event tap only picks up a newly
/// granted Input Monitoring permission after a restart.
#[tauri::command]
pub fn restart_app(handle: tauri::AppHandle) {
    handle.restart();
}

/// Current Input Monitoring permission status (`authorized`, `denied`, or
/// `not_determined`). Required for the global keyboard tap to see keystrokes
/// while other apps are frontmost. Always `authorized` on non-macOS platforms.
#[tauri::command]
pub fn get_input_monitoring_permission_status() -> String {
    input_monitoring_permission_status()
}

/// Requests Input Monitoring access, showing the macOS consent prompt when the
/// permission is undetermined. Returns the resulting status. No-op elsewhere.
#[tauri::command]
pub fn request_input_monitoring_permission() -> String {
    #[cfg(target_os = "macos")]
    {
        let _ = crate::system::mac_app::request_input_monitoring();
        crate::system::mac_app::input_monitoring_status().to_string()
    }
    #[cfg(not(target_os = "macos"))]
    {
        "authorized".to_string()
    }
}

#[tauri::command]
pub async fn request_input_monitoring_permission_snapshot(
    provider: Option<String>,
) -> MacPermissionSnapshot {
    #[cfg(target_os = "macos")]
    {
        let _ = crate::system::mac_app::request_input_monitoring();
    }
    macos_permission_snapshot(provider).await
}

/// Opens the macOS Input Monitoring privacy pane so the user can grant access.
/// No-op on other platforms.
#[tauri::command]
pub fn open_input_monitoring_settings() -> Result<(), String> {
    open_macos_settings(&[
        "x-apple.systempreferences:com.apple.preference.security?Privacy_ListenEvent",
        "x-apple.systempreferences:com.apple.preference.security",
    ])
}

#[cfg(target_os = "macos")]
fn open_macos_settings(urls: &[&str]) -> Result<(), String> {
    let mut last_error = None;
    for url in urls {
        match std::process::Command::new("open").arg(url).status() {
            Ok(status) if status.success() => return Ok(()),
            Ok(status) => last_error = Some(format!("open exited with status: {status}")),
            Err(err) => last_error = Some(err.to_string()),
        }
    }
    Err(last_error.unwrap_or_else(|| "No System Settings URL provided.".to_string()))
}

#[cfg(not(target_os = "macos"))]
fn open_macos_settings(_urls: &[&str]) -> Result<(), String> {
    Ok(())
}

/// Opens the macOS Privacy & Security settings root. Used as a fallback from
/// menu-bar actions where landing near the right pane matters more than the
/// specific sub-pane anchor.
#[tauri::command]
pub fn open_privacy_security_settings() -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        open_macos_settings(&["x-apple.systempreferences:com.apple.preference.security"])?;
    }
    Ok(())
}

#[tauri::command]
pub async fn reset_macos_core_permissions() -> TccResetResult {
    #[cfg(target_os = "macos")]
    {
        let bundle_identifier = crate::system::mac_app::bundle_identifier();
        let mut steps = Vec::new();

        if let Some(bundle_id) = bundle_identifier.as_deref() {
            for service in ["Accessibility", "ListenEvent"] {
                let output = std::process::Command::new("tccutil")
                    .args(["reset", service, bundle_id])
                    .output();
                match output {
                    Ok(output) if output.status.success() => {
                        steps.push(TccResetStep {
                            service: service.to_string(),
                            ok: true,
                            message: "Reset".to_string(),
                        });
                    }
                    Ok(output) => {
                        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
                        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
                        steps.push(TccResetStep {
                            service: service.to_string(),
                            ok: false,
                            message: if stderr.is_empty() { stdout } else { stderr },
                        });
                    }
                    Err(err) => {
                        steps.push(TccResetStep {
                            service: service.to_string(),
                            ok: false,
                            message: err.to_string(),
                        });
                    }
                }
            }
        } else {
            steps.push(TccResetStep {
                service: "All".to_string(),
                ok: false,
                message: "Could not determine this app's bundle identifier.".to_string(),
            });
        }

        TccResetResult {
            bundle_identifier,
            steps,
        }
    }

    #[cfg(not(target_os = "macos"))]
    {
        TccResetResult {
            bundle_identifier: None,
            steps: Vec::new(),
        }
    }
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

#[cfg(test)]
mod tests {
    use super::{summarize_core_permissions, PermissionSourceHints};

    fn hints(global_input_seen: bool) -> PermissionSourceHints {
        PermissionSourceHints {
            hotkey_tap_active: true,
            global_input_seen,
            microphone_verified: true,
            accessibility_verified: true,
        }
    }

    #[test]
    fn summary_requires_all_three_core_permissions() {
        let (all, relaunch) =
            summarize_core_permissions("authorized", "authorized", "authorized", &hints(true));
        assert!(all);
        assert!(!relaunch);

        let (all, _) =
            summarize_core_permissions("authorized", "denied", "authorized", &hints(true));
        assert!(!all);
    }

    #[test]
    fn summary_flags_possible_input_monitoring_relaunch_cache() {
        let (all, relaunch) =
            summarize_core_permissions("authorized", "authorized", "authorized", &hints(false));
        assert!(all);
        assert!(relaunch);
    }
}
