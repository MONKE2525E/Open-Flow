#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod api;
mod app_hotkey;
mod app_tray;
mod commands;
mod core;
mod data;
mod local_llm;
mod local_stt;
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
fn app_data_dir_override() -> Option<std::path::PathBuf> {
    std::env::var_os("VERENU_APP_DATA_DIR_OVERRIDE").map(std::path::PathBuf::from)
}

#[cfg(windows)]
pub(crate) fn app_data_dir() -> std::path::PathBuf {
    if let Some(path) = app_data_dir_override() {
        return path;
    }
    std::env::var("APPDATA")
        .map(|p| std::path::PathBuf::from(p).join("Verenu"))
        .unwrap_or_else(|_| std::path::PathBuf::from("."))
}

#[cfg(target_os = "macos")]
pub(crate) fn app_data_dir() -> std::path::PathBuf {
    if let Some(path) = app_data_dir_override() {
        return path;
    }
    std::env::var("HOME")
        .map(|h| std::path::PathBuf::from(h).join("Library/Application Support/Verenu"))
        .unwrap_or_else(|_| std::path::PathBuf::from("."))
}

#[cfg(not(any(windows, target_os = "macos")))]
pub(crate) fn app_data_dir() -> std::path::PathBuf {
    if let Some(path) = app_data_dir_override() {
        return path;
    }
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

    let shared: SharedState = Arc::new(Mutex::new(AppState {
        session: None,
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
    let local_cleanup_manager = crate::local_llm::LocalLlmManager::new();
    let local_transcription_manager = crate::local_stt::LocalTranscriptionManager::new();

    tauri::Builder::default()
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            show_main_window(app);
        }))
        .manage(shared.clone())
        .manage(db_handle.clone())
        .manage(local_cleanup_manager.clone())
        .manage(local_transcription_manager.clone())
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
            let local_stt_manager = local_transcription_manager.clone();
            let local_llm_manager = local_cleanup_manager.clone();
            let app_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                loop {
                    tokio::time::sleep(std::time::Duration::from_secs(30)).await;
                    if local_stt_manager
                        .shutdown_signal
                        .load(std::sync::atomic::Ordering::Relaxed)
                        || local_llm_manager
                            .shutdown_signal
                            .load(std::sync::atomic::Ordering::Relaxed)
                    {
                        break;
                    }
                    let _ = local_stt_manager.unload_if_idle(&app_handle);
                    let _ = local_llm_manager.unload_if_idle(&app_handle);

                    // Proactive safety unload: don't wait out the configured
                    // idle timeout if the system is genuinely low on RAM or
                    // (NVIDIA) VRAM right now — e.g. launching a demanding
                    // game shouldn't have to wait 15 minutes for Verenu to
                    // give back memory its local models are holding.
                    let stt_loaded = local_stt_manager
                        .current_model_id
                        .lock()
                        .map(|guard| guard.is_some())
                        .unwrap_or(false);
                    let llm_loaded = local_llm_manager
                        .current_model_id
                        .lock()
                        .map(|guard| guard.is_some())
                        .unwrap_or(false);
                    if stt_loaded || llm_loaded {
                        // detect_resource_pressure() does blocking process
                        // I/O (bounded by its own internal timeout) — run it
                        // off the async worker thread so a slow/wedged
                        // nvidia-smi can never stall this loop or other
                        // tasks sharing the runtime.
                        let pressure = tauri::async_runtime::spawn_blocking(
                            crate::system::memory::detect_resource_pressure,
                        )
                        .await
                        .unwrap_or(None);
                        if let Some(reason) = pressure {
                            log::warn!(
                                "local models: system resource pressure detected ({reason}), unloading proactively"
                            );
                            local_stt_manager.unload_for_resource_pressure(&app_handle);
                            local_llm_manager.unload_for_resource_pressure(&app_handle);
                        }
                    }
                }
            });

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
            commands::list_local_stt_models,
            commands::list_local_llm_models,
            commands::download_local_stt_model,
            commands::download_local_llm_model,
            commands::cancel_local_stt_model_download,
            commands::cancel_local_llm_model_download,
            commands::delete_local_stt_model,
            commands::delete_local_llm_model,
            commands::open_local_stt_models_folder,
            commands::open_local_models_folder,
            commands::get_local_transcription_state,
            commands::get_local_llm_state,
            commands::get_local_llm_runtime_info,
            commands::download_local_llm_runtime,
            commands::cancel_local_llm_runtime_download,
            commands::delete_local_llm_runtime,
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
            // Local cleanup runs llama-server.exe as a real child OS process
            // (unlike local_stt, which is in-process). Child processes are
            // not automatically killed when their parent exits on Windows —
            // without this, quitting Verenu while a local cleanup model is
            // loaded would orphan llama-server.exe, leaving it running
            // indefinitely and holding the loaded model's RAM/VRAM.
            if let tauri::RunEvent::Exit = _event {
                _app.state::<crate::local_llm::LocalLlmManager>()
                    .unload(_app);
            }
        });
}
