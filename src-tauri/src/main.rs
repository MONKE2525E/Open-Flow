#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod api;
mod app_hotkey;
mod app_setup;
mod app_tray;
mod commands;
mod core;
mod data;
mod local_llm;
mod local_stt;
mod media;
mod pipeline;
mod sync;
mod system;
#[cfg(any(test, debug_assertions))]
mod testing;

use crate::core::window_geometry::WindowTarget;
use crate::data::db;
use crate::pipeline::{AppState, SharedState};

use std::sync::{Arc, Mutex};
use tauri::{Emitter, Manager};

pub type DbHandle = db::Db;

// Startup helpers live in app_setup.rs; re-exported here so the rest of the
// crate can keep using `crate::` paths.
#[cfg(target_os = "macos")]
pub(crate) use app_setup::hide_main_window;
pub(crate) use app_setup::{
    app_data_dir, app_db_path, fatal_startup_error, show_main_window, start_frontend_watchdog,
    FrontendReadiness,
};
#[cfg(target_os = "windows")]
pub(crate) use app_setup::{cleanup_update_helper_if_requested, wait_for_relaunch_parent_exit};
pub(crate) use app_tray::apply_runtime_icons;

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
        failover_session_id: None,
        failover_reuse_id: false,
        failover_started_at_unix: 0,
    }));

    std::fs::create_dir_all(app_data_dir()).ok();
    // The logger must be live before the database opens: a corrupt DB,
    // failed migration, or quarantine decision happens before Tauri's setup
    // callback, where the AppHandle (and thus `verenu:log` emission) first
    // exists. init_early captures those records in the ring buffer;
    // attach_app in setup enables frontend forwarding.
    crate::system::logger::init_early();
    pipeline::failover::restore_into_state(&shared);
    let db_handle: DbHandle = match db::open_with_recovery(app_db_path()) {
        Ok(db) => db,
        Err(err) => fatal_startup_error(&format!("failed to open database: {err}")),
    };
    let _ = db::cleanup_cache_prune_expired(&db_handle);
    let _ = db::prune_auto_learn_retention(&db_handle);
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
            crate::system::logger::attach_app(app.handle());
            let build_mode = if cfg!(debug_assertions) {
                "debug"
            } else {
                "release"
            };
            log::info!(
                "verenu {} starting — {} {}, {} build — verbose logging {}",
                env!("CARGO_PKG_VERSION"),
                std::env::consts::OS,
                std::env::consts::ARCH,
                build_mode,
                if crate::system::logger::is_verbose() {
                    "on"
                } else {
                    "off"
                }
            );
            let settings = crate::data::store::SettingsHandle::open(app.handle())
                .map_err(std::io::Error::other)?;
            crate::data::credentials::migrate_from_store(app.handle(), &settings);
            if let Err(error) = crate::data::store::migrate_contextual_formatting(&settings) {
                log::warn!("Failed to migrate contextual formatting setting: {error}");
            }
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

            #[cfg(target_os = "windows")]
            {
                // Non-critical: a temp-dir write failure here must not abort
                // startup, it only skips the themed toast identity.
                match app_tray::prepare_windows_shell_icon(app.handle()) {
                    Ok(shell_icon) => {
                        crate::system::notify::prepare_windows_notification_identity(
                            &shell_icon,
                        );
                    }
                    Err(err) => {
                        log::warn!("Continuing without a themed shell icon: {err}");
                    }
                }
            }

            // LAN device sync: identity, mDNS discovery, listener, sessions.
            // Soft-fails internally — never blocks startup.
            let sync_enabled = settings
                .get(crate::data::store::SYNC_ENABLED)
                .and_then(|value| value.as_bool())
                .unwrap_or(false);
            if sync_enabled {
                app.manage(sync::SyncManager::start(
                    app.handle().clone(),
                    app.state::<DbHandle>().inner().clone(),
                ));
            } else {
                log::info!("sync: disabled by setting");
            }

            app_tray::setup_tray(app)?;
            #[cfg(target_os = "windows")]
            if let Some(window) = app.get_webview_window("main") {
                let theme = window.theme().ok();
                crate::system::windows_titlebar::enable(&window, theme)
                    .map_err(|error| {
                        let source: Box<dyn std::error::Error> = Box::new(std::io::Error::other(error));
                        tauri::Error::Setup(source.into())
                    })?;
                // AppWindow initialization can restore the executable-derived taskbar icon.
                // Apply the generated runtime icons only after custom chrome owns the window.
                app_tray::apply_runtime_icons(app.handle(), theme);
            }
            app_hotkey::setup_hotkey(app, shared.clone());
            // setup_tray() already applies runtime icons (both platforms) via
            // apply_runtime_icons() — no need to call it again here.
            #[cfg(target_os = "macos")]
            {
                crate::system::mac_app::set_accessory_activation_policy_on_main_thread(
                    app.handle(),
                );
            }
            start_frontend_watchdog(app.handle(), frontend_readiness.clone());
            // macOS logout sends SIGTERM to the app. Tauri delivers
            // RunEvent::Exit only for its own quit paths, so without this
            // hook a SIGTERM would kill the process without unloading the
            // local cleanup model, orphaning llama-server. Routing it through
            // app.exit(0) runs the normal Exit cleanup (see the `run` closure
            // below). Unix-only: console signals do not reach Windows GUI
            // apps, and Windows logoff is handled separately in
            // on_window_event.
            #[cfg(unix)]
            {
                use tokio::signal::unix::{signal, SignalKind};
                let app_handle = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    let mut sigterm = match signal(SignalKind::terminate()) {
                        Ok(sigterm) => sigterm,
                        Err(err) => {
                            log::warn!("could not register SIGTERM handler: {err}");
                            return;
                        }
                    };
                    sigterm.recv().await;
                    log::info!("received SIGTERM; exiting cleanly");
                    app_handle.exit(0);
                });
            }
            let local_stt_manager = local_transcription_manager.clone();
            let local_llm_manager = local_cleanup_manager.clone();
            let capture_state = shared.clone();
            let app_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                loop {
                    tokio::time::sleep(std::time::Duration::from_secs(30)).await;
                    pipeline::release_expired_capture_audio(&capture_state);
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
                        // The OS is tearing the session down (Windows logoff or
                        // shutdown): let the window actually close instead of
                        // hiding to tray, or the process outlives the session
                        // (Windows waits ~5s then force-kills it, which also
                        // skips RunEvent::Exit and can orphan llama-server.exe).
                        // On other platforms the session helper reports false
                        // and the normal hide-to-tray path below runs.
                        if crate::system::session::system_is_shutting_down() {
                            return;
                        }
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
                        #[cfg(target_os = "windows")]
                        apply_runtime_icons(window.app_handle(), window.theme().ok());
                        #[cfg(target_os = "macos")]
                        {
                            crate::system::mac_app::set_regular_activation_policy_on_main_thread(
                                window.app_handle(),
                            );
                        }
                    }
                    tauri::WindowEvent::ThemeChanged(theme) => {
                        let app = window.app_handle();
                        #[cfg(target_os = "windows")]
                        if let Some(webview) = app.get_webview_window("main") {
                            crate::system::windows_titlebar::refresh(&webview, Some(*theme));
                        }
                        if app_tray::appearance_mode(app)
                            .as_deref()
                            .unwrap_or("system")
                            == "system"
                        {
                            // apply_runtime_icons() already applies runtime icons
                            // internally — no separate call needed here.
                            apply_runtime_icons(app, Some(*theme));
                        }
                    }
                    #[cfg(target_os = "windows")]
                    tauri::WindowEvent::Moved(_) => {
                        // Intentionally a no-op. Dragging changes neither the
                        // title-bar geometry the frontend mirrors
                        // (height/insets/scale are DPI- and theme-dependent,
                        // not position-dependent) nor the themed icon artwork
                        // (theme/accent/DPI-dependent). Refreshing either here
                        // re-ran WinRT title-bar updates, child-window
                        // enumeration, full icon rasterization, and a frontend
                        // style recalc on every mouse-move event of a drag —
                        // the stutter when jiggling the window. A
                        // cross-monitor move that changes DPI arrives
                        // separately as ScaleFactorChanged below.
                    }
                    #[cfg(target_os = "windows")]
                    tauri::WindowEvent::Resized(_) => {
                        // A live resize delivers one event per frame;
                        // refreshing the native title bar synchronously here
                        // (WinRT calls plus child-window enumeration) stalls
                        // the drag. Coalesce to a single refresh once the size
                        // settles so maximize/snap changes are still picked
                        // up. Icons are size-independent — they refresh on
                        // ScaleFactorChanged/ThemeChanged/settings instead.
                        schedule_settled_titlebar_refresh(window.app_handle());
                    }
                    #[cfg(target_os = "windows")]
                    tauri::WindowEvent::ScaleFactorChanged { .. } => {
                        if let Some(webview) = window.app_handle().get_webview_window("main") {
                            crate::system::windows_titlebar::refresh(&webview, window.theme().ok());
                            apply_runtime_icons(window.app_handle(), window.theme().ok());
                        }
                    }
                    _ => {}
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            system::windows_titlebar::get_native_titlebar_metrics,
            system::windows_titlebar::set_native_titlebar_theme,
            commands::save_hotkey,
            commands::check_hotkey,
            commands::save_api_key,
            commands::delete_api_key,
            commands::get_api_key_status,
            commands::validate_api_key,
            commands::list_provider_models,
            commands::open_notifications_settings,
            commands::request_notification_permission,
            commands::check_keychain_access,
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
            commands::get_local_transcription_state,
            commands::get_local_llm_state,
            commands::get_local_llm_runtime_info,
            commands::download_local_llm_runtime,
            commands::cancel_local_llm_runtime_download,
            commands::delete_local_llm_runtime,
            commands::set_autostart,
            commands::get_macos_permission_snapshot,
            commands::request_accessibility_permission,
            commands::open_accessibility_settings,
            commands::request_microphone_permission,
            commands::request_microphone_permission_snapshot,
            commands::open_microphone_settings,
            commands::restart_app,
            commands::frontend_ready,
            commands::reset_macos_core_permissions,
            commands::get_recent,
            commands::get_history_apps,
            commands::get_stats,
            commands::get_insights,
            commands::get_insights_pricing,
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
            commands::get_cancelled_capture,
            commands::copy_paste_failure_to_clipboard,
            commands::set_pill_size,
            commands::get_installed_apps,
            commands::get_app_icon,
            commands::get_site_icon,
             commands::get_app_mappings,
             commands::save_app_mappings,
             commands::get_contexts,
             commands::create_context,
             commands::update_context,
             commands::update_context_settings,
             commands::update_context_color,
             commands::set_context_pinned,
             commands::get_context_stats,
             commands::delete_context,
             commands::get_context_targets,
             commands::assign_context_target,
             commands::remove_context_target,
             commands::get_context_websites,
             commands::check_domain_exists,
             commands::assign_context_website,
             commands::remove_context_website,
             commands::get_context_dictionary,
             commands::get_context_snippets,
             commands::set_dictionary_context_assignment,
             commands::set_snippet_context_assignment,
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
            commands::sync_get_status,
            commands::sync_set_device_name,
            commands::sync_start_pairing,
            commands::sync_respond_to_pairing,
            commands::sync_cancel_pairing,
            commands::sync_remove_device,
            commands::sync_now,
            commands::sync_get_diagnostics,
        ])
        .build(tauri::generate_context!())
        .expect("error building Verenu")
        .run(|_app, _event| {
            #[cfg(target_os = "macos")]
            if let tauri::RunEvent::Reopen { .. } = _event {
                show_main_window(_app);
            }
            if let tauri::RunEvent::ExitRequested { .. } = _event {
                log::info!("app exit requested");
            }
            // Local cleanup runs llama-server.exe as a real child OS process
            // (unlike local_stt, which is in-process). Child processes are
            // not automatically killed when their parent exits on Windows —
            // without this, quitting Verenu while a local cleanup model is
            // loaded would orphan llama-server.exe, leaving it running
            // indefinitely and holding the loaded model's RAM/VRAM.
            if let tauri::RunEvent::Exit = _event {
                log::info!("app exiting; unloading local models");
                crate::pipeline::failover::flush_on_exit(_app);
                crate::system::shutdown_local_models(_app);
                #[cfg(target_os = "windows")]
                app_tray::cleanup_runtime_icon_files();
                log::info!("app shutdown complete");
            }
        });
}

#[cfg(target_os = "windows")]
static TITLEBAR_REFRESH_GEN: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Re-read native title-bar metrics once the window size has settled (150ms
/// with no further `Resized` event). A live resize delivers one event per
/// frame, so each event just bumps the generation and schedules a check —
/// only the last one in a burst performs the refresh. See the `Resized` arm
/// in `on_window_event`.
#[cfg(target_os = "windows")]
fn schedule_settled_titlebar_refresh(app: &tauri::AppHandle) {
    use std::sync::atomic::Ordering;
    let gen = TITLEBAR_REFRESH_GEN.fetch_add(1, Ordering::Relaxed) + 1;
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(150)).await;
        if TITLEBAR_REFRESH_GEN.load(Ordering::Relaxed) != gen {
            return;
        }
        if let Some(webview) = app.get_webview_window("main") {
            crate::system::windows_titlebar::refresh(&webview, webview.theme().ok());
        }
    });
}

