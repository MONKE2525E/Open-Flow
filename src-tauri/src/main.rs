#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

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

use std::sync::{
    atomic::AtomicBool,
    Arc, Mutex,
};
#[cfg(target_os = "windows")]
use std::sync::atomic::Ordering;
#[cfg(target_os = "windows")]
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};

pub type DbHandle = db::Db;

/// Startup readiness reported by each Tauri WebView after its frontend has
/// mounted successfully. A backend can be fully alive while WebView2 is
/// showing a stale connection-refused page, so backend liveness alone is not
/// enough to declare startup successful.
#[derive(Clone, Default)]
pub(crate) struct FrontendReadiness {
    pub(crate) main: Arc<AtomicBool>,
    /// Diagnostic state only. The pill is created after the main frontend has
    /// passed the startup gate, so its readiness is not a separate gate.
    pub(crate) pill: Arc<AtomicBool>,
}

#[cfg(target_os = "windows")]
impl FrontendReadiness {
    fn main_ready(&self) -> bool {
        self.main.load(Ordering::Acquire)
    }

    #[cfg(debug_assertions)]
    fn reset(&self) {
        self.main.store(false, Ordering::Release);
        self.pill.store(false, Ordering::Release);
    }
}

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

#[cfg(all(target_os = "windows", not(debug_assertions)))]
fn startup_recovery_was_attempted() -> bool {
    std::env::args_os().any(|arg| arg == "--startup-recovery-attempted")
}

#[cfg(all(debug_assertions, target_os = "windows"))]
async fn wait_for_dev_frontend() -> bool {
    let Ok(client) = reqwest::Client::builder()
        .connect_timeout(Duration::from_millis(500))
        .timeout(Duration::from_secs(1))
        .build()
    else {
        return false;
    };

    let deadline = tokio::time::Instant::now() + Duration::from_secs(8);
    while tokio::time::Instant::now() < deadline {
        if let Ok(response) = client.get("http://127.0.0.1:1420/").send().await {
            if response.status().is_success() {
                return true;
            }
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    false
}

#[cfg(target_os = "windows")]
fn start_frontend_watchdog(app: &AppHandle, readiness: FrontendReadiness) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let reveal_windows = || {
            // The main window is the startup gate. Create the pill only after
            // the main UI is healthy because a hidden WebView2 renderer can
            // suspend before it reports its own readiness.
            show_main_window(&app);
            crate::pipeline::show_pill(&app, "idle");
        };

        // WebView2 normally mounts in well under a second. This grace period
        // leaves room for a cold Vite/WebView2 start without delaying normal
        // startup or flashing a recovery window.
        let deadline = tokio::time::Instant::now() + Duration::from_secs(12);
        while tokio::time::Instant::now() < deadline {
            if readiness.main_ready() {
                log::debug!("startup handshake completed");
                reveal_windows();
                return;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }

        if readiness.main_ready() {
            reveal_windows();
            return;
        }

        // In development, the WebView can fail its first navigation while
        // Vite is restarting or optimizing dependencies. Reload the existing
        // windows once the server is reachable before restarting the entire
        // process. This also avoids creating an orphaned app if the parent
        // `tauri dev` session has already stopped its Vite child.
        #[cfg(debug_assertions)]
        if wait_for_dev_frontend().await {
            readiness.reset();
            if let Some(window) = app.get_webview_window("main") {
                window.reload().ok();
            }

            let retry_deadline = tokio::time::Instant::now() + Duration::from_secs(8);
            while tokio::time::Instant::now() < retry_deadline {
                if readiness.main_ready() {
                    reveal_windows();
                    return;
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        }

        #[cfg(debug_assertions)]
        log::error!(
            "startup handshake did not complete in development; keeping the dev process alive for diagnosis"
        );
        reveal_windows();

        #[cfg(not(debug_assertions))]
        {
            if startup_recovery_was_attempted() {
                log::error!(
                    "startup handshake did not complete after the automatic recovery attempt; leaving the app running for diagnosis"
                );
                reveal_windows();
                return;
            }

            log::warn!(
                "startup handshake timed out: main_ready={} pill_ready={}; relaunching once",
                readiness.main.load(Ordering::Acquire),
                readiness.pill.load(Ordering::Acquire)
            );
            crate::app_tray::relaunch_for_startup_recovery(&app);
        }
    });
}

#[cfg(not(target_os = "windows"))]
fn start_frontend_watchdog(app: &AppHandle, _readiness: FrontendReadiness) {
    crate::pipeline::show_pill(app, "idle");
    show_main_window(app);
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
    #[cfg(target_os = "windows")]
    {
        cleanup_update_helper_if_requested();
        if crate::commands::run_update_helper_if_requested() {
            return;
        }
    }

    #[cfg(target_os = "macos")]
    {
        let _ = crate::system::mac_app::set_process_name("Verenu");
    }

    #[cfg(target_os = "windows")]
    wait_for_relaunch_parent_exit();

    let shared: SharedState = Arc::new(Mutex::new(AppState {
        lifecycle: pipeline::DictationLifecycle::Idle,
        target: WindowTarget::default(),
        pill_placement: None,
        pill_placement_stale: false,
        pill_width_points: pipeline::DEFAULT_PILL_WIDTH_POINTS,
        pill_height_points: pipeline::DEFAULT_PILL_HEIGHT_POINTS,
        retry_capture: None,
        cancelled_capture: None,
        paste_failure: None,
    }));

    std::fs::create_dir_all(app_data_dir()).ok();
    let db_handle: DbHandle = db::open(app_db_path()).expect("failed to open database");
    let _ = db::cleanup_cache_prune_expired(&db_handle);
    if let Err(e) = db::seed_default_dictionary_entries(&db_handle) {
        log::warn!("failed to seed default dictionary entries: {e}");
    }
    let local_cleanup_manager = crate::local_llm::LocalLlmManager::new();
    let local_transcription_manager = crate::local_stt::LocalTranscriptionManager::new();
    let frontend_readiness = FrontendReadiness::default();

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
        .manage(frontend_readiness.clone())
        .setup(move |app| {
            crate::system::logger::init(app.handle())?;
            crate::system::notify::prepare_windows_notification_identity();
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
            start_frontend_watchdog(app.handle(), frontend_readiness.clone());
            let local_stt_manager = local_transcription_manager.clone();
            let local_llm_manager = local_cleanup_manager.clone();
            let app_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                loop {
                    tokio::time::sleep(std::time::Duration::from_secs(30)).await;
                    // && (not ||): either manager signalling shutdown on its
                    // own must not stop monitoring the other still-active one.
                    if local_stt_manager
                        .shutdown_signal
                        .load(std::sync::atomic::Ordering::Relaxed)
                        && local_llm_manager
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
            commands::frontend_ready,
            commands::open_privacy_security_settings,
            commands::reset_macos_core_permissions,
            commands::check_keychain_access,
            commands::show_main,
            commands::hide_main,
            commands::get_recent,
            commands::get_stats,
            commands::get_insights,
            commands::count_old_transcriptions,
            commands::get_cleanup_cache_status,
            commands::clear_cleanup_cache,
            commands::get_default_cleanup_prompt,
            commands::lint_cleanup_prompt,
            commands::test_cleanup_prompt,
            commands::get_microphones,
            commands::get_memory_mb,
            commands::get_hardware_capabilities,
            commands::local_models_supported_on_this_platform,
            commands::start_input_recording,
            commands::start_setup_try_recording,
            commands::start_calibration_monitoring,
            commands::stop_calibration_monitoring,
            commands::stop_and_transcribe_input,
            commands::stop_setup_try_recording,
            commands::stop_recording,
            commands::stop_handless_mode,
            commands::resume_cancelled_capture,
            commands::dismiss_cancelled_capture,
            commands::copy_paste_failure_to_clipboard,
            commands::set_pill_size,
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
            commands::reinstall_latest_update,
            commands::install_update,
            commands::check_provider_status,
            commands::check_provider_status_raw,
            commands::check_global_message,
            commands::check_verenu_api_health,
            commands::check_connectivity,
            commands::get_recent_logs,
            commands::download_logs,
            commands::set_dev_logging_enabled,
            commands::get_dev_logging_enabled,
            commands::notify_update_available,
            commands::notify_provider_and_global_message,
            commands::test_notifications,
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

#[cfg(target_os = "windows")]
fn cleanup_update_helper_if_requested() {
    let Some(helper) = std::env::args_os().find_map(|arg| {
        let text = arg.to_string_lossy();
        text.strip_prefix("--cleanup-update-helper=")
            .map(std::path::PathBuf::from)
    }) else {
        return;
    };

    // The helper has just spawned this process and is exiting. Retry briefly so
    // Windows has released the helper image before removing its temp copy.
    for _ in 0..20 {
        match std::fs::remove_file(&helper) {
            Ok(()) => return,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return,
            Err(_) => std::thread::sleep(std::time::Duration::from_millis(100)),
        }
    }
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
                if err.code() != windows::core::HRESULT::from_win32(ERROR_INVALID_PARAMETER.0) {
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
