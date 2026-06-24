//! Window management, memory, hotkey, autostart, connectivity, dev logs.

use super::*;

// ---------- window management ----------

#[tauri::command]
pub async fn show_main(app: AppHandle) -> Result<(), String> {
    if let Some(w) = app.get_webview_window("main") {
        #[cfg(target_os = "macos")]
        {
            crate::system::mac_app::set_regular_activation_policy_on_main_thread(&app);
            crate::system::mac_app::activate_current_app_on_main_thread(&app);
        }
        w.show().ok();
        w.set_focus().ok();
    }
    Ok(())
}

#[tauri::command]
pub async fn hide_main(app: AppHandle) -> Result<(), String> {
    if let Some(w) = app.get_webview_window("main") {
        #[cfg(target_os = "macos")]
        {
            crate::system::mac_app::set_accessory_activation_policy_on_main_thread(&app);
        }
        w.hide().ok();
    }
    Ok(())
}

// ---------- memory ----------

#[tauri::command]
pub async fn get_memory_mb() -> u64 {
    match run_blocking("get_memory_mb", || Ok(crate::system::memory::measure())).await {
        Ok(v) => v,
        Err(e) => {
            log::error!("{e}");
            0
        }
    }
}

// ---------- hotkey ----------

#[tauri::command]
pub async fn check_hotkey(key1: String, key2: String) -> Result<bool, String> {
    Ok(crate::core::hotkey::is_hotkey_available(&key1, &key2))
}

#[tauri::command]
pub async fn save_hotkey(app: AppHandle, key1: String, key2: String) -> Result<(), String> {
    let vk1 = crate::core::hotkey::map_code_to_vk(&key1);
    let vk2 = crate::core::hotkey::map_code_to_vk(&key2);
    if vk1 == 0 {
        return Err(format!("Unrecognized key code: {key1}"));
    }
    // An empty second slot is allowed (a single-key hotkey, e.g. macOS F5);
    // only reject a non-empty key code that we can't recognise.
    if !key2.is_empty() && vk2 == 0 {
        return Err(format!("Unrecognized key code: {key2}"));
    }
    crate::core::hotkey::update_keys(vk1, vk2);
    let settings = store::settings_handle(&app)?;
    run_blocking("save_hotkey", move || {
        settings.save_value(store::HOTKEY, serde_json::json!([key1, key2]))
    })
    .await
}

// ---------- autostart ----------

#[tauri::command]
pub async fn set_autostart(_app: AppHandle, enabled: bool) -> Result<(), String> {
    // Registry/file/process operations below are all blocking I/O - run them
    // off the async executor so a slow disk or registry call can't stall
    // other Tokio tasks (audio capture, hotkey handling, etc.).
    #[cfg(target_os = "windows")]
    {
        tokio::task::spawn_blocking(move || -> Result<(), String> {
            use std::os::windows::ffi::OsStrExt;
            use windows::core::PCWSTR;
            use windows::Win32::System::Registry::{
                RegCloseKey, RegDeleteValueW, RegOpenKeyExW, RegSetValueExW, HKEY,
                HKEY_CURRENT_USER, KEY_WRITE, REG_SZ,
            };

            // Quote the path: a Run value is a command line, so an unquoted path
            // containing spaces (e.g. "C:\Program Files\Verenu\verenu.exe") would be
            // parsed as multiple arguments and fail to launch.
            let app_path = format!(
                "\"{}\"",
                std::env::current_exe()
                    .map_err(|e| format!("Failed to get executable path: {e}"))?
                    .to_string_lossy()
            );

            let subkey: Vec<u16> =
                std::ffi::OsStr::new("Software\\Microsoft\\Windows\\CurrentVersion\\Run")
                    .encode_wide()
                    .chain(std::iter::once(0))
                    .collect();
            let value_name: Vec<u16> = std::ffi::OsStr::new("Verenu")
                .encode_wide()
                .chain(std::iter::once(0))
                .collect();

            unsafe {
                let mut hkey = HKEY::default();
                let status = RegOpenKeyExW(
                    HKEY_CURRENT_USER,
                    PCWSTR(subkey.as_ptr()),
                    None,
                    KEY_WRITE,
                    std::ptr::addr_of_mut!(hkey),
                );

                if status.is_err() {
                    return Err("Failed to open registry key".to_string());
                }

                let result = if enabled {
                    let app_path_wide: Vec<u16> = std::ffi::OsStr::new(&app_path)
                        .encode_wide()
                        .chain(std::iter::once(0))
                        .collect();
                    RegSetValueExW(
                        hkey,
                        PCWSTR(value_name.as_ptr()),
                        None,
                        REG_SZ,
                        Some(std::slice::from_raw_parts(
                            app_path_wide.as_ptr() as *const u8,
                            app_path_wide.len() * 2,
                        )),
                    )
                } else {
                    RegDeleteValueW(hkey, PCWSTR(value_name.as_ptr()))
                };

                let _ = RegCloseKey(hkey);

                if result.is_err() {
                    return Err("Failed to set registry value".to_string());
                }
            }
            Ok(())
        })
        .await
        .map_err(|e| e.to_string())??;
    }

    // macOS: write/remove a LaunchAgent plist that launches the app at login.
    #[cfg(target_os = "macos")]
    {
        let app_handle = _app.clone();
        tokio::task::spawn_blocking(move || -> Result<(), String> {
            let label = "com.verenu.app";
            let domain = format!("gui/{}", unsafe { libc::getuid() });
            let service_target = format!("{domain}/{label}");
            let home = app_handle
                .path()
                .home_dir()
                .map_err(|e| format!("Failed to get home directory: {e}"))?;
            let dir = home.join("Library/LaunchAgents");
            let plist_path = dir.join(format!("{label}.plist"));

            if enabled {
                let app_path = std::env::current_exe()
                    .map_err(|e| format!("Failed to get executable path: {e}"))?
                    .to_string_lossy()
                    .to_string();
                let mut use_open = false;
                let mut target_path = app_path.clone();
                if let Some(index) = app_path.find(".app/Contents/MacOS/") {
                    target_path = app_path[..index + 4].to_string();
                    use_open = true;
                }

                let escaped_target_path = target_path
                    .replace('&', "&amp;")
                    .replace('<', "&lt;")
                    .replace('>', "&gt;");
                std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
                let plist = if use_open {
                    format!(
                        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
                         <!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
                         <plist version=\"1.0\">\n\
                         <dict>\n\
                           <key>Label</key><string>{label}</string>\n\
                           <key>ProgramArguments</key><array><string>open</string><string>-g</string><string>{escaped_target_path}</string></array>\n\
                           <key>RunAtLoad</key><true/>\n\
                         </dict>\n\
                         </plist>\n"
                    )
                } else {
                    format!(
                        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
                         <!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
                         <plist version=\"1.0\">\n\
                         <dict>\n\
                           <key>Label</key><string>{label}</string>\n\
                           <key>ProgramArguments</key><array><string>{escaped_target_path}</string></array>\n\
                           <key>RunAtLoad</key><true/>\n\
                         </dict>\n\
                         </plist>\n"
                    )
                };
                std::fs::write(&plist_path, plist).map_err(|e| e.to_string())?;
                let _ = launchctl_bootout(&service_target);
                launchctl_bootstrap(&domain, &plist_path)?;
            } else {
                let _ = launchctl_bootout(&service_target);
                if plist_path.exists() {
                    std::fs::remove_file(&plist_path).map_err(|e| e.to_string())?;
                }
            }
            Ok(())
        })
        .await
        .map_err(|e| e.to_string())??;
    }

    let settings = store::settings_handle(&_app)?;
    run_blocking("set_autostart", move || {
        settings.save_value(store::AUTOSTART_ENABLED, serde_json::json!(enabled))
    })
    .await
}

#[cfg(target_os = "macos")]
fn launchctl_bootstrap(domain: &str, plist_path: &std::path::Path) -> Result<(), String> {
    run_launchctl(&[
        "bootstrap",
        domain,
        plist_path.to_str().ok_or("Invalid plist path")?,
    ])
}

#[cfg(target_os = "macos")]
fn launchctl_bootout(service_target: &str) -> Result<(), String> {
    run_launchctl(&["bootout", service_target])
}

#[cfg(target_os = "macos")]
fn run_launchctl(args: &[&str]) -> Result<(), String> {
    let output = std::process::Command::new("launchctl")
        .args(args)
        .output()
        .map_err(|e| format!("Failed to run launchctl: {e}"))?;

    if output.status.success() {
        return Ok(());
    }

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let detail = if !stderr.is_empty() {
        stderr
    } else if !stdout.is_empty() {
        stdout
    } else {
        format!("exit status {}", output.status)
    };

    Err(format!("launchctl {:?} failed: {detail}", args))
}

// ---------- connectivity ----------

#[tauri::command]
pub async fn check_connectivity() -> bool {
    // Probe github.com (a host Verenu already contacts for release downloads)
    // rather than a third-party beacon like google.com, so the connectivity check
    // doesn't quietly phone a separate domain. Deliberately NOT api.github.com:
    // at a 60s poll that would consume the entire 60/hr unauthenticated GitHub
    // API budget and starve the updater's release checks with 403s. Reuses the
    // shared client for connection pooling; GitHub requires a User-Agent.
    crate::api::client::get()
        .head("https://github.com")
        .header("User-Agent", "verenu")
        .timeout(std::time::Duration::from_secs(3))
        .send()
        .await
        .is_ok()
}

// ---------- developer logs ----------

#[tauri::command]
pub fn log_frontend(level: String, message: String) {
    let message = crate::system::logger::sanitize_frontend_log_message(&message);
    match level.as_str() {
        "warn" => log::warn!("fe: {message}"),
        "error" => log::error!("fe: {message}"),
        _ => log::info!("fe: {message}"),
    }
}

#[tauri::command]
pub fn get_recent_logs(limit: Option<usize>) -> Vec<String> {
    crate::system::logger::recent(limit)
}

#[tauri::command]
pub async fn download_logs(app: AppHandle) -> Result<String, String> {
    run_blocking("download_logs", move || {
        crate::system::logger::export_to_downloads(&app)
    })
    .await
}

#[tauri::command]
pub fn set_dev_logging_enabled(enabled: bool) {
    crate::system::logger::set_verbose(enabled);
}

#[tauri::command]
pub fn get_dev_logging_enabled() -> bool {
    crate::system::logger::is_verbose()
}
