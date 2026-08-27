#![allow(dead_code)]

//! macOS Accessibility / Microphone / Keychain permission commands.
//!
//! The global hotkey uses Carbon `RegisterEventHotKey` (see `core::hotkey::mac`),
//! which needs no Input Monitoring permission — so the only macOS permissions the
//! app requests are **Accessibility** (Cmd+V injection + AX reads) and
//! **Microphone**. Keychain access is surfaced separately when a provider key is
//! stored.

use std::sync::atomic::{AtomicU64, Ordering};

static PERMISSION_QUERY_GENERATION: AtomicU64 = AtomicU64::new(0);

// ---------- macOS permissions ----------

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MacPermissionSnapshot {
    pub accessibility: String,
    pub microphone: String,
    pub notifications: NotificationPermissionSnapshot,
    pub keychain: String,
    pub all_core_granted: bool,
    pub last_checked_at: String,
    pub diagnostics: MacPermissionDiagnostics,
}

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MacPermissionDiagnostics {
    pub bundle_identifier: Option<String>,
    pub bundle_display_name: Option<String>,
    pub bundle_name: Option<String>,
    pub bundle_path: Option<String>,
    pub executable_path: Option<String>,
    pub bundle_url: Option<String>,
    pub executable_url: Option<String>,
    pub bundle_url_extension: Option<String>,
    pub is_running_inside_app: bool,
    pub process_id: u32,
    pub process_name: String,
    pub macos_version: String,
    pub signing_identity: Option<String>,
    pub team_identifier: Option<String>,
    pub build_profile: String,
    pub snapshot_generation: u64,
    pub accessibility_trusted: bool,
    pub microphone_av_audio_status: Option<String>,
    pub microphone_av_audio_raw: Option<i64>,
    pub microphone_av_audio_fourcc: Option<String>,
    pub microphone_av_capture_status: String,
    pub microphone_av_capture_raw: i64,
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

/// Core permissions are granted once both Accessibility and Microphone are
/// authorized. (The hotkey no longer needs a separate permission.)
fn core_permissions_granted(accessibility: &str, microphone: &str) -> bool {
    accessibility == "authorized" && microphone == "authorized"
}

#[cfg(target_os = "macos")]
fn permission_diagnostics() -> MacPermissionDiagnostics {
    let bundle_path = crate::system::mac_app::bundle_path();
    let (signing_identity, team_identifier) = bundle_path
        .as_deref()
        .map(read_signing_identity)
        .unwrap_or((None, None));
    MacPermissionDiagnostics {
        bundle_identifier: crate::system::mac_app::bundle_identifier(),
        bundle_display_name: crate::system::mac_app::bundle_display_name(),
        bundle_name: crate::system::mac_app::bundle_name(),
        bundle_path,
        executable_path: std::env::current_exe()
            .ok()
            .map(|p| p.to_string_lossy().into_owned()),
        bundle_url: crate::system::mac_app::bundle_url(),
        executable_url: crate::system::mac_app::bundle_executable_url(),
        bundle_url_extension: crate::system::mac_app::bundle_url_extension(),
        is_running_inside_app: crate::system::mac_app::bundle_url_extension().as_deref()
            == Some("app"),
        process_id: std::process::id(),
        process_name: process_name(),
        macos_version: macos_version(),
        signing_identity,
        team_identifier,
        build_profile: if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        }
        .into(),
        snapshot_generation: PERMISSION_QUERY_GENERATION.load(Ordering::Relaxed),
        accessibility_trusted: check_accessibility_permission(false),
        microphone_av_audio_status: crate::system::mac_app::av_audio_microphone_permission_status()
            .map(str::to_string),
        microphone_av_audio_raw: crate::system::mac_app::av_audio_microphone_permission_raw()
            .map(|v| v as i64),
        microphone_av_audio_fourcc: crate::system::mac_app::av_audio_microphone_permission_raw()
            .map(fourcc),
        microphone_av_capture_status:
            crate::system::mac_app::av_capture_microphone_permission_status().to_string(),
        microphone_av_capture_raw: crate::system::mac_app::av_capture_microphone_permission_raw()
            as i64,
    }
}

#[cfg(not(target_os = "macos"))]
fn permission_diagnostics() -> MacPermissionDiagnostics {
    MacPermissionDiagnostics {
        bundle_identifier: None,
        bundle_display_name: None,
        bundle_name: None,
        bundle_path: None,
        executable_path: std::env::current_exe()
            .ok()
            .map(|p| p.to_string_lossy().into_owned()),
        bundle_url: None,
        executable_url: None,
        bundle_url_extension: None,
        is_running_inside_app: false,
        process_id: std::process::id(),
        process_name: "verenu".into(),
        macos_version: "non-macOS".into(),
        signing_identity: None,
        team_identifier: None,
        build_profile: if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        }
        .into(),
        snapshot_generation: PERMISSION_QUERY_GENERATION.load(Ordering::Relaxed),
        accessibility_trusted: true,
        microphone_av_audio_status: None,
        microphone_av_audio_raw: None,
        microphone_av_audio_fourcc: None,
        microphone_av_capture_status: "authorized".to_string(),
        microphone_av_capture_raw: 3,
    }
}

fn now_rfc3339() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

#[cfg(target_os = "macos")]
fn accessibility_permission_status() -> String {
    // Always ask TCC for the current state. A previous successful AX operation
    // only proves that access existed at that moment; treating it as a permanent
    // grant makes revocations invisible until the app is restarted.
    if check_accessibility_permission(false) {
        log::info!("[permissions] accessibility API=AXIsProcessTrusted raw=true state=authorized");
        "authorized".to_string()
    } else {
        // AXIsProcessTrusted only exposes trusted/not trusted. Do not invent
        // a never-asked vs denied distinction that macOS did not provide.
        log::info!(
            "[permissions] accessibility API=AXIsProcessTrusted raw=false state=not_granted"
        );
        "not_granted".to_string()
    }
}

#[cfg(not(target_os = "macos"))]
fn accessibility_permission_status() -> String {
    "authorized".to_string()
}

#[cfg(target_os = "macos")]
fn microphone_permission_status_string() -> String {
    let modern = crate::system::mac_app::av_audio_microphone_permission_status();
    let modern_raw = crate::system::mac_app::av_audio_microphone_permission_raw();
    let capture = crate::system::mac_app::av_capture_microphone_permission_status();
    let capture_raw = crate::system::mac_app::av_capture_microphone_permission_raw();
    let final_state = crate::system::mac_app::microphone_permission_status();
    log::info!("[permissions][mic] query pid={} bundle={:?} AVAudioApplication raw={:?} fourcc={:?} state={:?} AVCaptureDevice raw={} state={} final={}", std::process::id(), crate::system::mac_app::bundle_identifier(), modern_raw, modern_raw.map(fourcc), modern, capture_raw, capture, final_state);
    final_state.to_string()
}

fn fourcc(raw: isize) -> String {
    let bytes = (raw as u32).to_be_bytes();
    if bytes.iter().all(|b| b.is_ascii_graphic() || *b == b' ') {
        String::from_utf8_lossy(&bytes).into_owned()
    } else {
        format!("0x{:08x}", raw as u32)
    }
}

#[cfg(target_os = "macos")]
fn process_name() -> String {
    crate::system::mac_app::process_name().unwrap_or_else(|| "Verenu".into())
}

#[cfg(not(target_os = "macos"))]
fn process_name() -> String {
    "verenu".into()
}

#[cfg(target_os = "macos")]
fn macos_version() -> String {
    std::process::Command::new("/usr/bin/sw_vers")
        .arg("-productVersion")
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "unknown".into())
}

#[cfg(not(target_os = "macos"))]
fn macos_version() -> String {
    "non-macOS".into()
}

#[cfg(target_os = "macos")]
fn read_signing_identity(bundle_path: &str) -> (Option<String>, Option<String>) {
    let output = std::process::Command::new("/usr/bin/codesign")
        .args(["-dv", "--verbose=4", bundle_path])
        .output();
    let text = output
        .map(|o| String::from_utf8_lossy(&o.stderr).into_owned())
        .unwrap_or_default();
    let identity = text
        .lines()
        .find_map(|line| line.strip_prefix("Authority=").map(str::to_string));
    let team = text
        .lines()
        .find_map(|line| line.strip_prefix("TeamIdentifier=").map(str::to_string));
    (identity, team)
}

#[cfg(not(target_os = "macos"))]
fn read_signing_identity(_bundle_path: &str) -> (Option<String>, Option<String>) {
    (None, None)
}

#[cfg(not(target_os = "macos"))]
fn microphone_permission_status_string() -> String {
    "authorized".to_string()
}

#[cfg(target_os = "macos")]
async fn keychain_status_for_provider(_provider: Option<String>) -> String {
    // Keychain is intentionally not a passive permission query. Reading a
    // credential can prompt and only proves access to one item. It is checked
    // exclusively by the explicit Check Access action below.
    "not_checked".to_string()
}

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NotificationPermissionSnapshot {
    pub authorization: String,
    pub alerts: String,
    pub sounds: String,
    pub badges: String,
    pub notification_center: String,
    pub lock_screen: String,
    pub raw_authorization: Option<i64>,
}

#[cfg(not(target_os = "macos"))]
async fn keychain_status_for_provider(_provider: Option<String>) -> String {
    "authorized".to_string()
}

fn notification_setting(value: i64) -> String {
    match value {
        0 => "not_supported",
        1 => "disabled",
        2 => "enabled",
        _ => "unknown",
    }
    .to_string()
}

#[cfg(target_os = "macos")]
async fn notification_permission_snapshot() -> NotificationPermissionSnapshot {
    match crate::system::mac_app::notification_settings().await {
        Ok([authorization, alerts, sounds, badges, center, lock_screen]) => {
            let name = match authorization {
                0 => "not_determined",
                1 => "denied",
                2 => "authorized",
                3 => "provisional",
                _ => "unknown",
            };
            log::info!("[permissions] notifications authorization_raw={} authorization={} alert={} sound={} badge={} center={} lock_screen={}", authorization, name, alerts, sounds, badges, center, lock_screen);
            NotificationPermissionSnapshot {
                authorization: name.into(),
                alerts: notification_setting(alerts),
                sounds: notification_setting(sounds),
                badges: notification_setting(badges),
                notification_center: notification_setting(center),
                lock_screen: notification_setting(lock_screen),
                raw_authorization: Some(authorization),
            }
        }
        Err(error) => {
            log::warn!("[permissions] notifications query error={error}");
            NotificationPermissionSnapshot {
                authorization: "error".into(),
                alerts: "unknown".into(),
                sounds: "unknown".into(),
                badges: "unknown".into(),
                notification_center: "unknown".into(),
                lock_screen: "unknown".into(),
                raw_authorization: None,
            }
        }
    }
}

#[cfg(not(target_os = "macos"))]
async fn notification_permission_snapshot() -> NotificationPermissionSnapshot {
    NotificationPermissionSnapshot {
        authorization: "authorized".into(),
        alerts: "enabled".into(),
        sounds: "enabled".into(),
        badges: "enabled".into(),
        notification_center: "enabled".into(),
        lock_screen: "enabled".into(),
        raw_authorization: Some(2),
    }
}

async fn macos_permission_snapshot(provider: Option<String>) -> MacPermissionSnapshot {
    let generation = PERMISSION_QUERY_GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
    log::info!(
        "[permissions][refresh #{}] begin pid={} bundle={:?}",
        generation,
        std::process::id(),
        crate::system::mac_app::bundle_identifier()
    );
    let accessibility = accessibility_permission_status();
    let microphone = microphone_permission_status_string();
    let keychain = keychain_status_for_provider(provider).await;
    let notifications = notification_permission_snapshot().await;
    let all_core_granted = core_permissions_granted(&accessibility, &microphone);

    MacPermissionSnapshot {
        accessibility,
        microphone,
        notifications,
        keychain,
        all_core_granted,
        last_checked_at: now_rfc3339(),
        diagnostics: {
            let mut diagnostics = permission_diagnostics();
            diagnostics.snapshot_generation = generation;
            diagnostics
        },
    }
}

#[tauri::command]
pub async fn get_macos_permission_snapshot(provider: Option<String>) -> MacPermissionSnapshot {
    let snapshot = macos_permission_snapshot(provider).await;
    write_debug_probe(&snapshot);
    log::info!("[permissions][refresh #{}] complete accessibility={} microphone={} notifications={} keychain={}", snapshot.diagnostics.snapshot_generation, snapshot.accessibility, snapshot.microphone, snapshot.notifications.authorization, snapshot.keychain);
    snapshot
}

/// Debug-only native probe. It performs the same passive snapshot as the page
/// inside the current process and never touches Keychain.
#[tauri::command]
pub async fn debug_permission_probe() -> MacPermissionSnapshot {
    let snapshot = macos_permission_snapshot(None).await;
    write_debug_probe(&snapshot);
    log::info!("[permissions][probe] pid={} bundle={:?} executable={:?} mic_capture={} mic_audio={:?} mic_final={} keychain=NO_PASSIVE_OPERATION", snapshot.diagnostics.process_id, snapshot.diagnostics.bundle_identifier, snapshot.diagnostics.executable_path, snapshot.diagnostics.microphone_av_capture_status, snapshot.diagnostics.microphone_av_audio_status, snapshot.microphone);
    snapshot
}

fn write_debug_probe(snapshot: &MacPermissionSnapshot) {
    if let Ok(payload) = serde_json::to_string_pretty(snapshot) {
        let _ = std::fs::write("/tmp/verenu-permission-probe.json", payload);
        let _ = std::fs::write(
            format!(
                "/tmp/verenu-permission-probe-{}.json",
                snapshot.diagnostics.process_id
            ),
            serde_json::to_string_pretty(snapshot).unwrap_or_default(),
        );
    }
}

/// Whether Verenu is trusted for the Accessibility API (needed for Cmd+V
/// injection and auto-learn). When `prompt` is true, macOS shows the system
/// permission dialog. Always true on non-macOS platforms.
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
pub async fn request_accessibility_permission(
    app: tauri::AppHandle,
    provider: Option<String>,
) -> MacPermissionSnapshot {
    #[cfg(target_os = "macos")]
    {
        let (tx, rx) = tokio::sync::oneshot::channel();
        let _ = app.run_on_main_thread(move || {
            let prompted = check_accessibility_permission(true);
            let _ = tx.send(prompted);
        });
        let _ = tokio::time::timeout(std::time::Duration::from_secs(3), rx).await;
    }
    macos_permission_snapshot(provider).await
}

#[tauri::command]
pub fn get_microphone_permission_status() -> String {
    microphone_permission_status_string()
}

/// Triggers the macOS microphone consent prompt when access is undetermined,
/// then returns the resulting status. Lets the permissions UI request the mic
/// directly instead of waiting for the first recording. No-op off macOS.
#[tauri::command]
pub async fn request_microphone_permission(app: tauri::AppHandle) -> Result<String, String> {
    #[cfg(target_os = "macos")]
    {
        let before = crate::system::mac_app::microphone_permission_status();
        if before == "not_determined" {
            crate::system::mac_app::request_microphone_on_main_thread(&app).await?;
        }
        Ok(crate::system::mac_app::microphone_permission_status().to_string())
    }
    #[cfg(not(target_os = "macos"))]
    {
        Ok("authorized".to_string())
    }
}

#[tauri::command]
pub async fn request_microphone_permission_snapshot(
    app: tauri::AppHandle,
    provider: Option<String>,
) -> Result<MacPermissionSnapshot, String> {
    #[cfg(target_os = "macos")]
    {
        let before = crate::system::mac_app::microphone_permission_status();
        log::info!("[permissions][mic][request] before={}", before);
        write_microphone_request_trace("started", before, None, None);
        if before == "not_determined" {
            let request =
                if crate::system::mac_app::av_audio_microphone_permission_status().is_some() {
                    crate::system::mac_app::request_audio_application_on_main_thread(&app).await
                } else {
                    crate::system::mac_app::request_microphone_on_main_thread(&app).await
                };
            let mut callback = match request {
                Ok(value) => value,
                Err(error) => {
                    write_microphone_request_trace("error", before, None, Some(&error));
                    return Err(error);
                }
            };
            let mut after = crate::system::mac_app::microphone_permission_status();
            if !callback && after == "not_determined" {
                callback =
                    crate::system::mac_app::request_microphone_via_device_input(&app).await?;
                after = crate::system::mac_app::microphone_permission_status();
            }
            log::info!(
                "[permissions][mic][request] callback={} after={}",
                callback,
                after
            );
            write_microphone_request_trace("completed", after, Some(callback), None);
        } else {
            write_microphone_request_trace("already_decided", before, None, None);
        }
    }
    Ok(macos_permission_snapshot(provider).await)
}

#[cfg(target_os = "macos")]
fn write_microphone_request_trace(
    stage: &str,
    state: &str,
    callback: Option<bool>,
    error: Option<&str>,
) {
    let payload = serde_json::json!({
        "timestamp": now_rfc3339(),
        "pid": std::process::id(),
        "bundleIdentifier": crate::system::mac_app::bundle_identifier(),
        "stage": stage,
        "state": state,
        "callback": callback,
        "error": error,
        "captureRaw": crate::system::mac_app::av_capture_microphone_permission_raw(),
        "captureState": crate::system::mac_app::av_capture_microphone_permission_status(),
    });
    let _ = std::fs::write("/tmp/verenu-microphone-request.json", payload.to_string());
}

/// Request notification authorization only after an explicit user action,
/// then re-query UNNotificationSettings for the authoritative result.
#[tauri::command]
pub async fn request_notification_permission(
    app: tauri::AppHandle,
) -> Result<NotificationPermissionSnapshot, String> {
    #[cfg(target_os = "macos")]
    {
        let current = notification_permission_snapshot().await;
        if current.authorization == "not_determined" {
            crate::system::mac_app::request_notifications_on_main_thread(&app).await?;
        }
    }
    Ok(notification_permission_snapshot().await)
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

#[tauri::command]
pub fn open_notifications_settings() -> Result<(), String> {
    open_macos_settings(&[
        "x-apple.systempreferences:com.apple.preference.notifications",
        "x-apple.systempreferences:com.apple.preference.security",
    ])
}

/// Relaunches the app. macOS can cache a TCC decision for the life of the
/// process, so synthesised Cmd+V injection may only start working after a
/// restart once Accessibility has just been granted.
#[tauri::command]
pub fn restart_app(handle: tauri::AppHandle) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let bundle = crate::system::mac_app::bundle_path()
            .filter(|path| path.ends_with(".app"))
            .ok_or_else(|| {
                "Cannot relaunch: current process is not inside an app bundle".to_string()
            })?;
        std::process::Command::new("/bin/sh")
            .args([
                "-c",
                "sleep 0.5; exec /usr/bin/open -n \"$1\"",
                "verenu-relaunch",
                &bundle,
            ])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .map_err(|error| format!("Could not schedule LaunchServices relaunch: {error}"))?;
        handle.exit(0);
        Ok(())
    }
    #[cfg(not(target_os = "macos"))]
    {
        handle.restart();
        Ok(())
    }
}

#[cfg(target_os = "macos")]
fn open_macos_settings(urls: &[&str]) -> Result<(), String> {
    let mut last_error = None;
    for url in urls {
        match std::process::Command::new("/usr/bin/open")
            .arg(url)
            .status()
        {
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

/// Clears the app's stale Accessibility TCC grant so the user can re-add this
/// build fresh. Rarely needed now that builds are signed with a stable identity,
/// but kept as a last-resort repair.
#[tauri::command]
pub async fn reset_macos_core_permissions() -> TccResetResult {
    #[cfg(target_os = "macos")]
    {
        tokio::task::spawn_blocking(|| {
            let bundle_identifier = crate::system::mac_app::bundle_identifier();
            let mut steps = Vec::new();

            if let Some(bundle_id) = bundle_identifier.as_deref() {
                let service = "Accessibility";
                let output = std::process::Command::new("/usr/bin/tccutil")
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
        })
        .await
        .unwrap_or_else(|_| TccResetResult {
            bundle_identifier: None,
            steps: Vec::new(),
        })
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
pub async fn check_keychain_access(
    provider: String,
) -> crate::data::credentials::KeychainDiagnostic {
    let _ = provider;
    #[cfg(target_os = "macos")]
    {
        match tokio::task::spawn_blocking(crate::data::credentials::check_access_sentinel).await {
            Ok(result) => {
                log::info!(
                    "[permissions] keychain explicit operation={} os_status={} meaning={} state={}",
                    result.operation,
                    result.os_status,
                    result.os_status_meaning,
                    result.state
                );
                result
            }
            Err(error) => crate::data::credentials::KeychainDiagnostic {
                state: "error".into(),
                operation: "task".into(),
                os_status: -1,
                os_status_meaning: format!("worker failed: {error}"),
            },
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        crate::data::credentials::KeychainDiagnostic {
            state: "available".into(),
            operation: "platform credential check".into(),
            os_status: 0,
            os_status_meaning: "success".into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::core_permissions_granted;

    #[test]
    fn summary_requires_accessibility_and_microphone() {
        assert!(core_permissions_granted("authorized", "authorized"));
        assert!(!core_permissions_granted("authorized", "denied"));
        assert!(!core_permissions_granted("not_granted", "authorized"));
    }
}
