#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod api;
mod app_hotkey;
mod app_tray;
mod commands;
mod core;
mod data;
mod media;
mod pipeline;
mod system;
#[cfg(any(test, debug_assertions))]
mod testing;

use crate::core::window_geometry::WindowTarget;
use crate::data::db;
use crate::pipeline::{AppState, SharedState};

use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, Manager};

pub type DbHandle = db::Db;

pub(crate) use app_tray::apply_runtime_icons;

fn show_main_window(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        if w.is_minimized().unwrap_or(false) {
            w.unminimize().ok();
        }
        #[cfg(target_os = "macos")]
        {
            crate::system::mac_app::set_regular_activation_policy_on_main_thread(app);
            crate::system::mac_app::activate_current_app_on_main_thread(app);
        }
        w.show().ok();
        w.set_focus().ok();
    }
}

#[cfg(target_os = "macos")]
fn hide_main_window(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        crate::system::mac_app::set_accessory_activation_policy_on_main_thread(app);
        w.hide().ok();
    }
}

/// Canonical per-user data directory for Verenu, following each OS's convention.
///
/// This is the single source of truth for where Verenu stores its SQLite
/// database. Everything that touches the DB — startup `open`, the in-app
/// updater's pre-update backup, etc. — MUST derive its path from here (via
/// [`app_db_path`]). Do NOT use Tauri's `app.path().app_data_dir()` for the
/// database: that resolves against the bundle identifier and is not guaranteed
/// to equal this path, so backups would silently target a different file.
#[cfg(windows)]
pub(crate) fn app_data_dir() -> std::path::PathBuf {
    std::env::var("APPDATA")
        .map(|p| std::path::PathBuf::from(p).join("Verenu"))
        .unwrap_or_else(|_| std::path::PathBuf::from("."))
}

#[cfg(target_os = "macos")]
pub(crate) fn app_data_dir() -> std::path::PathBuf {
    std::env::var("HOME")
        .map(|h| std::path::PathBuf::from(h).join("Library/Application Support/Verenu"))
        .unwrap_or_else(|_| std::path::PathBuf::from("."))
}

#[cfg(not(any(windows, target_os = "macos")))]
pub(crate) fn app_data_dir() -> std::path::PathBuf {
    std::env::var("HOME")
        .map(|h| std::path::PathBuf::from(h).join(".config/Verenu"))
        .unwrap_or_else(|_| std::path::PathBuf::from("."))
}

/// Canonical path to the SQLite database file. Use this everywhere.
pub(crate) fn app_db_path() -> std::path::PathBuf {
    app_data_dir().join("verenu.db")
}

fn main() {
    #[cfg(target_os = "macos")]
    {
        let _ = crate::system::mac_app::set_process_name("Verenu");
    }

    #[cfg(target_os = "windows")]
    wait_for_relaunch_parent_exit();

    let shared: SharedState = Arc::new(Mutex::new(AppState {
        session: None,
        exclusive_mic_session_id: None,
        starting: false,
        handless: false,
        target: WindowTarget::default(),
        pill_placement: None,
        pill_placement_stale: false,
        retry_capture: None,
    }));

    std::fs::create_dir_all(app_data_dir()).ok();
    let db_handle: DbHandle = db::open(app_db_path()).expect("failed to open database");
    let _ = db::cleanup_cache_prune_expired(&db_handle);

    tauri::Builder::default()
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            show_main_window(app);
        }))
        .manage(shared.clone())
        .manage(db_handle.clone())
        .setup(move |app| {
            crate::system::logger::init(app.handle())?;
            let settings = crate::data::store::SettingsHandle::open(app.handle())
                .map_err(std::io::Error::other)?;
            crate::data::credentials::migrate_from_store(app.handle(), &settings);
            let _first_launch = {
                if let Some(val) = settings.get(crate::data::store::HOTKEY) {
                    if let Some(arr) = val.as_array() {
                        if arr.len() == 2 {
                            if let (Some(k1), Some(k2)) = (arr[0].as_str(), arr[1].as_str()) {
                                let (k1, k2) = (k1, k2);
                                // On macOS the hotkey is now a modifier+key combo
                                // (RegisterEventHotKey, no Input Monitoring). A stored
                                // modifier-only chord from an earlier build (e.g. Fn+Control)
                                // is not registrable — migrate it to the ⌥+Space default so
                                // the backend and the settings label stay in sync.
                                #[cfg(target_os = "macos")]
                                let (k1, k2) = if !crate::core::hotkey::is_hotkey_available(k1, k2)
                                {
                                    let _ = settings.set(
                                        crate::data::store::HOTKEY,
                                        serde_json::json!(["AltLeft", "Space"]),
                                    );
                                    if let Err(e) = settings.save() {
                                        log::warn!(
                                            "Failed to save migrated hotkey to settings.json: {e:?}"
                                        );
                                    }
                                    ("AltLeft", "Space")
                                } else {
                                    (k1, k2)
                                };
                                let vk1 = crate::core::hotkey::map_code_to_vk(k1);
                                let vk2 = crate::core::hotkey::map_code_to_vk(k2);
                                crate::core::hotkey::update_keys(vk1, vk2);
                            }
                        }
                    }
                }
                let retention_value = settings.get(crate::data::store::HISTORY_RETENTION);
                let retention = retention_value
                    .as_ref()
                    .and_then(|v| v.as_str())
                    .unwrap_or("30 days");
                if let Some(days) = crate::data::store::history_retention_days(retention) {
                    let db = app.state::<DbHandle>().inner().clone();
                    let app_handle = app.handle().clone();
                    tauri::async_runtime::spawn_blocking(move || {
                        match db::prune_transcriptions_older_than(&db, days) {
                            Ok(deleted) if deleted > 0 => {
                                let _ = app_handle.emit("verenu:history-pruned", ());
                            }
                            Ok(_) => {}
                            Err(e) => {
                                log::warn!(
                                    "Failed to prune old transcriptions during startup: {e:?}"
                                );
                            }
                        }
                    });
                }
                let first_launch = !settings
                    .get(crate::data::store::SETUP_COMPLETE)
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                app.manage(settings.clone());
                first_launch
            };

            app_tray::setup_tray(app)?;
            app_hotkey::setup_hotkey(app, shared.clone());
            // setup_tray() already applies native window chrome (both platforms) via
            // apply_runtime_icons() — no need to call apply_native_main_window_chrome again here.
            #[cfg(target_os = "macos")]
            {
                crate::system::mac_app::set_accessory_activation_policy_on_main_thread(
                    app.handle(),
                );
            }
            crate::pipeline::show_pill(app.handle(), "idle");

            // Keep the main UI visible on startup so normal launches don't
            // feel like the app disappeared into the tray.
            show_main_window(app.handle());

            Ok(())
        })
        .on_window_event(|window, event| {
            if window.label() == "main" {
                match event {
                    tauri::WindowEvent::CloseRequested { api, .. } => {
                        api.prevent_close();
                        #[cfg(target_os = "macos")]
                        {
                            hide_main_window(window.app_handle());
                        }
                        #[cfg(not(target_os = "macos"))]
                        {
                            window.hide().ok();
                        }
                    }
                    #[cfg(target_os = "macos")]
                    tauri::WindowEvent::Resized(_) if window.is_minimized().unwrap_or(false) => {
                        crate::system::mac_app::set_regular_activation_policy_on_main_thread(
                            window.app_handle(),
                        );
                    }
                    tauri::WindowEvent::Focused(true) => {
                        #[cfg(target_os = "macos")]
                        {
                            crate::system::mac_app::set_regular_activation_policy_on_main_thread(
                                window.app_handle(),
                            );
                        }
                    }
                    tauri::WindowEvent::ThemeChanged(theme) => {
                        let app = window.app_handle();
                        if app_tray::appearance_mode(app)
                            .as_deref()
                            .unwrap_or("system")
                            == "system"
                        {
                            // apply_runtime_icons() already applies native window chrome
                            // (both platforms) internally — no separate call needed here.
                            apply_runtime_icons(app, Some(*theme));
                        }
                    }
                    _ => {}
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::save_hotkey,
            commands::check_hotkey,
            commands::save_api_key,
            commands::delete_api_key,
            commands::get_api_key_status,
            commands::validate_api_key,
            commands::save_setting,
            commands::get_setting,
            commands::get_all_settings,
            commands::set_autostart,
            commands::check_accessibility_permission,
            commands::get_macos_permission_snapshot,
            commands::request_accessibility_permission,
            commands::open_accessibility_settings,
            commands::get_accessibility_permission_status,
            commands::get_microphone_permission_status,
            commands::request_microphone_permission,
            commands::request_microphone_permission_snapshot,
            commands::open_microphone_settings,
            commands::restart_app,
            commands::open_privacy_security_settings,
            commands::reset_macos_core_permissions,
            commands::check_keychain_access,
            commands::show_main,
            commands::hide_main,
            commands::get_recent,
            commands::get_stats,
            commands::count_old_transcriptions,
            commands::get_cleanup_cache_status,
            commands::clear_cleanup_cache,
            commands::get_default_cleanup_prompt,
            commands::lint_cleanup_prompt,
            commands::test_cleanup_prompt,
            commands::get_microphones,
            commands::get_memory_mb,
            commands::start_input_recording,
            commands::start_setup_try_recording,
            commands::start_calibration_monitoring,
            commands::stop_calibration_monitoring,
            commands::stop_and_transcribe_input,
            commands::stop_setup_try_recording,
            commands::stop_recording,
            commands::stop_handless_mode,
            commands::get_installed_apps,
            commands::get_app_mappings,
            commands::save_app_mappings,
            commands::get_snippets,
            commands::create_snippet,
            commands::edit_snippet,
            commands::remove_snippet,
            commands::get_dictionary,
            commands::create_dictionary_entry,
            commands::edit_dictionary_entry,
            commands::remove_dictionary_entry,
            commands::get_auto_learn_status_summary,
            commands::get_recent_auto_learn_activity,
            commands::retry_transcription,
            commands::check_for_update,
            commands::install_update,
            commands::check_connectivity,
            commands::get_recent_logs,
            commands::download_logs,
            commands::set_dev_logging_enabled,
            commands::get_dev_logging_enabled,
            commands::export_data,
            commands::import_data,
            commands::log_frontend,
        ])
        .build(tauri::generate_context!())
        .expect("error building Verenu")
        .run(|_app, _event| {
            #[cfg(target_os = "macos")]
            if let tauri::RunEvent::Reopen { .. } = _event {
                show_main_window(_app);
            }
        });
}

#[cfg(target_os = "windows")]
fn wait_for_relaunch_parent_exit() {
    use windows::Win32::Foundation::{CloseHandle, ERROR_INVALID_PARAMETER, WAIT_OBJECT_0};
    use windows::Win32::System::Threading::{
        OpenProcess, WaitForSingleObject, PROCESS_SYNCHRONIZE,
    };

    let Some(parent_pid) = relaunch_parent_pid() else {
        return;
    };

    unsafe {
        let handle = match OpenProcess(PROCESS_SYNCHRONIZE, false, parent_pid) {
            Ok(handle) => handle,
            Err(err) => {
                if err.code()
                    != windows::core::HRESULT::from_win32(ERROR_INVALID_PARAMETER.0)
                {
                    early_startup_warn(&format!(
                        "Relaunch requested but could not open parent process {parent_pid}: {err}"
                    ));
                }
                return;
            }
        };

        let wait_result = WaitForSingleObject(handle, 5_000);
        if wait_result != WAIT_OBJECT_0 {
            early_startup_warn(&format!(
                "Relaunch waited for parent process {parent_pid} but got result {}",
                wait_result.0
            ));
        }

        let _ = CloseHandle(handle);
    }
}

#[cfg(target_os = "windows")]
fn relaunch_parent_pid() -> Option<u32> {
    let mut args = std::env::args_os().skip(1);

    while let Some(arg) = args.next() {
        let Some(text) = arg.to_str() else {
            continue;
        };

        if let Some(value) = text.strip_prefix("--relaunch-parent-pid=") {
            if let Ok(pid) = value.parse::<u32>() {
                return Some(pid);
            }
            early_startup_warn(&format!(
                "Ignoring invalid relaunch parent pid argument: {value}"
            ));
            return None;
        }

        if text == "--relaunch-parent-pid" {
            let Some(value) = args.next() else {
                early_startup_warn("Ignoring missing relaunch parent pid value");
                return None;
            };

            match value.to_string_lossy().parse::<u32>() {
                Ok(pid) => return Some(pid),
                Err(_) => {
                    early_startup_warn(&format!(
                        "Ignoring invalid relaunch parent pid argument: {}",
                        value.to_string_lossy()
                    ));
                    return None;
                }
            }
        }
    }

    None
}

#[cfg(target_os = "windows")]
fn early_startup_warn(message: &str) {
    use windows::core::PCWSTR;
    use windows::Win32::System::Diagnostics::Debug::OutputDebugStringW;

    eprintln!("WARN: {message}");

    let mut wide: Vec<u16> = format!("WARN: {message}").encode_utf16().collect();
    wide.push(0);

    unsafe {
        OutputDebugStringW(PCWSTR(wide.as_ptr()));
    }
}
