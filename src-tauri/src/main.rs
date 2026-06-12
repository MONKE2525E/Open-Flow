#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod api;
mod commands;
mod core;
mod data;
mod media;
mod pipeline;
mod system;
#[cfg(any(test, debug_assertions))]
mod testing;

use crate::data::db;
use crate::pipeline::{hide_pill, start_recording_session, AppState, SharedState};

use std::sync::{Arc, Mutex, MutexGuard};
use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager, Theme,
};
use tauri_plugin_store::StoreExt;

pub type DbHandle = db::Db;

const TRAY_ID: &str = "verenu-tray";

#[derive(Clone, Copy, PartialEq, Eq)]
enum IconTheme {
    Light,
    Dark,
}

#[derive(Clone, Copy)]
struct IconRect {
    x: u32,
    y: u32,
    width: u32,
    height: u32,
    radius: u32,
}

fn lock_app_state(state: &SharedState) -> Option<MutexGuard<'_, AppState>> {
    match state.lock() {
        Ok(guard) => Some(guard),
        Err(_) => {
            log::error!("Recording state lock was poisoned");
            None
        }
    }
}

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

#[cfg(target_os = "macos")]
fn apply_native_main_window_chrome(app: &AppHandle, theme_hint: Option<Theme>) {
    if let Some(w) = app.get_webview_window("main") {
        let bg = match resolve_icon_theme(app, theme_hint) {
            IconTheme::Dark => tauri::utils::config::Color(20, 17, 14, 255),
            IconTheme::Light => tauri::utils::config::Color(249, 247, 243, 255),
        };
        w.set_decorations(true).ok();
        w.set_background_color(Some(bg)).ok();
        w.set_title("").ok();
        w.set_title_bar_style(tauri::TitleBarStyle::Transparent)
            .ok();
    }
}

/// Per-user directory for the SQLite database, following each OS's convention.
#[cfg(windows)]
fn app_data_dir() -> std::path::PathBuf {
    std::env::var("APPDATA")
        .map(|p| std::path::PathBuf::from(p).join("Verenu"))
        .unwrap_or_else(|_| std::path::PathBuf::from("."))
}

#[cfg(target_os = "macos")]
fn app_data_dir() -> std::path::PathBuf {
    std::env::var("HOME")
        .map(|h| std::path::PathBuf::from(h).join("Library/Application Support/Verenu"))
        .unwrap_or_else(|_| std::path::PathBuf::from("."))
}

#[cfg(not(any(windows, target_os = "macos")))]
fn app_data_dir() -> std::path::PathBuf {
    std::env::var("HOME")
        .map(|h| std::path::PathBuf::from(h).join(".config/Verenu"))
        .unwrap_or_else(|_| std::path::PathBuf::from("."))
}

#[cfg(windows)]
fn legacy_app_data_dir() -> std::path::PathBuf {
    std::env::var("APPDATA")
        .map(|p| std::path::PathBuf::from(p).join("OpenFlow"))
        .unwrap_or_else(|_| std::path::PathBuf::from("."))
}

#[cfg(target_os = "macos")]
fn legacy_app_data_dir() -> std::path::PathBuf {
    std::env::var("HOME")
        .map(|h| std::path::PathBuf::from(h).join("Library/Application Support/OpenFlow"))
        .unwrap_or_else(|_| std::path::PathBuf::from("."))
}

#[cfg(not(any(windows, target_os = "macos")))]
fn legacy_app_data_dir() -> std::path::PathBuf {
    std::env::var("HOME")
        .map(|h| std::path::PathBuf::from(h).join(".config/OpenFlow"))
        .unwrap_or_else(|_| std::path::PathBuf::from("."))
}

fn migrate_db_file_if_needed(new_db: &std::path::Path, old_db: &std::path::Path) -> std::io::Result<()> {
    if new_db.exists() || !old_db.exists() {
        return Ok(());
    }

    if let Some(parent) = new_db.parent() {
        std::fs::create_dir_all(parent)?;
    }

    copy_file_with_cleanup(old_db, new_db, |source, destination| {
        std::fs::copy(source, destination)
    })?;
    for suffix in ["-wal"] {
        let old_sidecar = path_with_suffix(old_db, suffix);
        if !old_sidecar.exists() {
            continue;
        }

        let new_sidecar = path_with_suffix(new_db, suffix);
        if let Err(err) = copy_file_with_cleanup(&old_sidecar, &new_sidecar, |source, destination| {
            std::fs::copy(source, destination)
        }) {
            let _ = std::fs::remove_file(new_db);
            return Err(err);
        }
    }

    Ok(())
}

fn path_with_suffix(path: &std::path::Path, suffix: &str) -> std::path::PathBuf {
    let mut path_with_suffix = path.to_path_buf().into_os_string();
    path_with_suffix.push(suffix);
    std::path::PathBuf::from(path_with_suffix)
}

fn copy_file_with_cleanup<F>(
    source: &std::path::Path,
    destination: &std::path::Path,
    copier: F,
) -> std::io::Result<()>
where
    F: FnOnce(&std::path::Path, &std::path::Path) -> std::io::Result<u64>,
{
    if let Err(err) = copier(source, destination) {
        let _ = std::fs::remove_file(destination);
        return Err(err);
    }

    Ok(())
}

fn migrate_legacy_db(new_dir: &std::path::Path) {
    let new_db = new_dir.join("verenu.db");
    let old_db = legacy_app_data_dir().join("openflow.db");
    if let Err(err) = migrate_db_file_if_needed(&new_db, &old_db) {
        log::warn!(
            "Could not migrate legacy database from {} to {}: {}",
            old_db.display(),
            new_db.display(),
            err
        );
    }
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
        target_hwnd: 0,
        retry_capture: None,
    }));

    let db_dir = app_data_dir();
    std::fs::create_dir_all(&db_dir).ok();
    migrate_legacy_db(&db_dir);
    let db_handle: DbHandle =
        db::open(db_dir.join("verenu.db").to_str().unwrap()).expect("failed to open database");
    let _ = db::cleanup_cache_prune_expired(&db_handle);

    tauri::Builder::default()
        .plugin(tauri_plugin_store::Builder::default().build())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            show_main_window(app);
        }))
        .manage(shared.clone())
        .manage(db_handle.clone())
        .setup(move |app| {
            crate::system::logger::init(app.handle())?;
            let _first_launch = if let Ok(store) =
                tauri_plugin_store::StoreExt::store(app.handle(), "settings.json")
            {
                let _ = store.reload();
                crate::data::credentials::migrate_from_store(app.handle(), &store);
                if let Some(val) = store.get("hotkey") {
                    if let Some(arr) = val.as_array() {
                        if arr.len() == 2 {
                            if let (Some(k1), Some(k2)) = (arr[0].as_str(), arr[1].as_str()) {
                                let vk1 = crate::core::hotkey::map_code_to_vk(k1);
                                let vk2 = crate::core::hotkey::map_code_to_vk(k2);
                                crate::core::hotkey::update_keys(vk1, vk2);
                            }
                        }
                    }
                }
                !store
                    .get("setup_complete")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false)
            } else {
                true
            };

            setup_tray(app)?;
            setup_hotkey(app, shared.clone());
            #[cfg(target_os = "macos")]
            apply_native_main_window_chrome(app.handle(), None);
            #[cfg(target_os = "macos")]
            {
                crate::system::mac_app::set_accessory_activation_policy_on_main_thread(
                    app.handle(),
                );
            }
            // macOS requires Accessibility permission for the global hotkey, Cmd+V
            // injection, and auto-learn. Prompt on launch when not yet trusted.
            #[cfg(target_os = "macos")]
            if !commands::check_accessibility_permission(false) {
                let _ = commands::check_accessibility_permission(true);
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
                        if appearance_mode(app).as_deref().unwrap_or("system") == "system" {
                            apply_runtime_icons(app, Some(*theme));
                            #[cfg(target_os = "macos")]
                            apply_native_main_window_chrome(app, Some(*theme));
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
            commands::save_setting,
            commands::get_setting,
            commands::get_all_settings,
            commands::set_autostart,
            commands::check_accessibility_permission,
            commands::open_accessibility_settings,
            commands::get_accessibility_permission_status,
            commands::get_microphone_permission_status,
            commands::open_microphone_settings,
            commands::check_keychain_access,
            commands::show_main,
            commands::hide_main,
            commands::get_recent,
            commands::get_stats,
            commands::get_cleanup_cache_status,
            commands::clear_cleanup_cache,
            commands::get_microphones,
            commands::get_memory_mb,
            commands::start_input_recording,
            commands::start_calibration_monitoring,
            commands::stop_calibration_monitoring,
            commands::stop_and_transcribe_input,
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
            commands::get_source_repo,
            commands::check_connectivity,
            commands::get_recent_logs,
            commands::download_logs,
            commands::set_dev_logging_enabled,
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

fn setup_tray(app: &mut tauri::App) -> tauri::Result<()> {
    let title_i = MenuItem::with_id(app, "title", "Verenu", false, None::<&str>)?;
    let sep = PredefinedMenuItem::separator(app)?;
    let show_i = MenuItem::with_id(app, "show", "Show Window", true, None::<&str>)?;
    let quit_i = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&title_i, &sep, &show_i, &quit_i])?;

    let icon_theme = resolve_icon_theme(app.handle(), None);
    let tray_icon = runtime_tray_icon_image(icon_theme, 32);

    TrayIconBuilder::with_id(TRAY_ID)
        .icon(tray_icon)
        .menu(&menu)
        .show_menu_on_left_click(false)
        .tooltip("Verenu - Ctrl+Windows to record")
        .on_menu_event(|app, ev| match ev.id.as_ref() {
            "show" => {
                show_main_window(app);
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, ev| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = ev
            {
                let app = tray.app_handle();
                if let Some(w) = app.get_webview_window("main") {
                    if w.is_visible().unwrap_or(false) {
                        w.hide().ok();
                    } else {
                        show_main_window(app);
                    }
                }
            }
        })
        .build(app)?;

    apply_runtime_icons(app.handle(), None);

    Ok(())
}

pub(crate) fn apply_runtime_icons(app: &AppHandle, theme_hint: Option<Theme>) {
    let icon_theme = resolve_icon_theme(app, theme_hint);

    if let Some(w) = app.get_webview_window("main") {
        if let Err(err) = w.set_icon(runtime_icon_image(icon_theme, 128)) {
            log::warn!("Failed to update window icon: {err}");
        }
    }

    #[cfg(target_os = "macos")]
    apply_native_main_window_chrome(app, theme_hint);

    #[cfg(target_os = "macos")]
    if !crate::system::mac_app::apply_dock_icon() {
        log::warn!("Failed to update macOS Dock icon");
    }

    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        if let Err(err) = tray.set_icon(Some(runtime_tray_icon_image(icon_theme, 32))) {
            log::warn!("Failed to update tray icon: {err}");
        }
    }
}

#[cfg(target_os = "macos")]
fn runtime_tray_icon_image(theme: IconTheme, size: u32) -> tauri::image::Image<'static> {
    let mut rgba = vec![0_u8; (size * size * 4) as usize];
    let color = runtime_tray_icon_color(theme);

    for (x, y, width, height, radius) in [
        (64, 304, 64, 96, 30),
        (144, 208, 64, 192, 30),
        (224, 112, 64, 288, 30),
        (304, 240, 64, 160, 30),
        (384, 320, 64, 80, 30),
    ] {
        draw_rounded_rect(
            &mut rgba,
            size,
            IconRect {
                x: scale(size, x),
                y: scale(size, y),
                width: scale(size, width),
                height: scale(size, height),
                radius: scale(size, radius),
            },
            color,
        );
    }

    tauri::image::Image::new_owned(rgba, size, size)
}

#[cfg(target_os = "macos")]
fn runtime_tray_icon_color(theme: IconTheme) -> [u8; 4] {
    match theme {
        IconTheme::Light => [0, 0, 0, 255],
        IconTheme::Dark => [255, 255, 255, 255],
    }
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::{runtime_tray_icon_color, IconTheme};

    #[test]
    fn tray_icon_uses_black_in_light_mode() {
        assert_eq!(runtime_tray_icon_color(IconTheme::Light), [0, 0, 0, 255]);
    }

    #[test]
    fn tray_icon_uses_white_in_dark_mode() {
        assert_eq!(
            runtime_tray_icon_color(IconTheme::Dark),
            [255, 255, 255, 255]
        );
    }
}

#[cfg(test)]
mod migration_tests {
    use super::migrate_db_file_if_needed;

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "verenu-{name}-{}-{nanos}",
            std::process::id()
        ))
    }

    #[test]
    fn migrate_db_file_if_needed_copies_legacy_db_and_wal_only() {
        let root = temp_dir("db-migration");
        let old_dir = root.join("OpenFlow");
        let new_dir = root.join("Verenu");
        std::fs::create_dir_all(&old_dir).expect("create old dir");

        let old_db = old_dir.join("openflow.db");
        let old_wal = old_dir.join("openflow.db-wal");
        let old_shm = old_dir.join("openflow.db-shm");
        std::fs::write(&old_db, b"db").expect("write old db");
        std::fs::write(&old_wal, b"wal").expect("write old wal");
        std::fs::write(&old_shm, b"shm").expect("write old shm");

        let new_db = new_dir.join("verenu.db");
        migrate_db_file_if_needed(&new_db, &old_db).expect("migrate db");

        assert_eq!(std::fs::read(&new_db).expect("new db"), b"db");
        assert_eq!(
            std::fs::read(new_dir.join("verenu.db-wal")).expect("new wal"),
            b"wal"
        );
        assert!(!new_dir.join("verenu.db-shm").exists());

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn migrate_db_file_if_needed_keeps_existing_new_db() {
        let root = temp_dir("db-migration-existing");
        let old_dir = root.join("OpenFlow");
        let new_dir = root.join("Verenu");
        std::fs::create_dir_all(&old_dir).expect("create old dir");
        std::fs::create_dir_all(&new_dir).expect("create new dir");

        let old_db = old_dir.join("openflow.db");
        let new_db = new_dir.join("verenu.db");
        std::fs::write(&old_db, b"old").expect("write old db");
        std::fs::write(&new_db, b"new").expect("write new db");

        migrate_db_file_if_needed(&new_db, &old_db).expect("skip migration");

        assert_eq!(std::fs::read(&new_db).expect("new db"), b"new");
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn copy_file_with_cleanup_removes_partial_destination_on_failure() {
        let root = temp_dir("db-copy-cleanup");
        std::fs::create_dir_all(&root).expect("create temp dir");
        let source = root.join("source.db");
        let destination = root.join("destination.db");
        std::fs::write(&source, b"source").expect("write source");

        let err = super::copy_file_with_cleanup(&source, &destination, |_, dest| {
            std::fs::write(dest, b"partial").expect("write partial");
            Err(std::io::Error::other("copy failed"))
        })
        .expect_err("copy should fail");

        assert_eq!(err.kind(), std::io::ErrorKind::Other);
        assert!(!destination.exists(), "partial destination should be removed");

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn path_with_suffix_appends_without_touching_extension() {
        let path = std::path::Path::new("Verenu/verenu.db");
        assert_eq!(
            super::path_with_suffix(path, "-wal"),
            std::path::PathBuf::from("Verenu/verenu.db-wal")
        );
    }
}

#[cfg(not(target_os = "macos"))]
fn runtime_tray_icon_image(theme: IconTheme, size: u32) -> tauri::image::Image<'static> {
    runtime_icon_image(theme, size)
}

fn resolve_icon_theme(app: &AppHandle, theme_hint: Option<Theme>) -> IconTheme {
    match appearance_mode(app).as_deref() {
        Some("dark") => IconTheme::Dark,
        Some("light") => IconTheme::Light,
        _ => match theme_hint.or_else(|| {
            app.get_webview_window("main")
                .and_then(|window| window.theme().ok())
        }) {
            Some(Theme::Dark) => IconTheme::Dark,
            _ => IconTheme::Light,
        },
    }
}

fn appearance_mode(app: &AppHandle) -> Option<String> {
    app.store("settings.json")
        .ok()
        .and_then(|store| store.get(crate::data::store::APPEARANCE_MODE))
        .and_then(|value| value.as_str().map(String::from))
}

fn runtime_icon_image(theme: IconTheme, size: u32) -> tauri::image::Image<'static> {
    let mut rgba = vec![0_u8; (size * size * 4) as usize];
    let background = match theme {
        IconTheme::Light => [249, 247, 243, 255],
        IconTheme::Dark => [20, 17, 14, 255],
    };
    let accent = [217, 119, 87, 255];

    #[cfg(target_os = "macos")]
    let background_rect = IconRect {
        x: scale(size, 64),
        y: scale(size, 64),
        width: scale(size, 384),
        height: scale(size, 384),
        radius: scale(size, 76),
    };

    #[cfg(not(target_os = "macos"))]
    let background_rect = IconRect {
        x: 0,
        y: 0,
        width: size,
        height: size,
        radius: scale(size, 96),
    };

    draw_rounded_rect(&mut rgba, size, background_rect, background);

    #[cfg(target_os = "macos")]
    let bar_rects = [
        (129, 290, 38, 70, 19),
        (183, 220, 38, 140, 19),
        (237, 152, 38, 208, 19),
        (291, 240, 38, 120, 19),
        (345, 298, 38, 62, 19),
    ];

    #[cfg(not(target_os = "macos"))]
    let bar_rects = [
        (76, 302, 56, 98, 28),
        (152, 204, 56, 196, 28),
        (228, 120, 56, 280, 28),
        (304, 246, 56, 154, 28),
        (380, 330, 56, 70, 28),
    ];

    for (x, y, width, height, radius) in bar_rects {
        draw_rounded_rect(
            &mut rgba,
            size,
            IconRect {
                x: scale(size, x),
                y: scale(size, y),
                width: scale(size, width),
                height: scale(size, height),
                radius: scale(size, radius),
            },
            accent,
        );
    }

    tauri::image::Image::new_owned(rgba, size, size)
}

fn scale(size: u32, value: u32) -> u32 {
    ((value * size) / 512).max(1)
}

fn draw_rounded_rect(rgba: &mut [u8], canvas_size: u32, rect: IconRect, color: [u8; 4]) {
    let right = rect.x.saturating_add(rect.width).min(canvas_size);
    let bottom = rect.y.saturating_add(rect.height).min(canvas_size);
    let radius = rect.radius.min(rect.width / 2).min(rect.height / 2) as i32;

    for py in rect.y..bottom {
        for px in rect.x..right {
            if is_inside_rounded_rect(
                px as i32,
                py as i32,
                rect.x as i32,
                rect.y as i32,
                right as i32,
                bottom as i32,
                radius,
            ) {
                let idx = ((py * canvas_size + px) * 4) as usize;
                rgba[idx..idx + 4].copy_from_slice(&color);
            }
        }
    }
}

fn is_inside_rounded_rect(
    px: i32,
    py: i32,
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
    radius: i32,
) -> bool {
    if radius <= 0 {
        return true;
    }

    let cx = if px < left + radius {
        left + radius
    } else if px >= right - radius {
        right - radius - 1
    } else {
        px
    };
    let cy = if py < top + radius {
        top + radius
    } else if py >= bottom - radius {
        bottom - radius - 1
    } else {
        py
    };

    let dx = px - cx;
    let dy = py - cy;
    dx * dx + dy * dy <= radius * radius
}

fn setup_hotkey(app: &mut tauri::App, shared: SharedState) {
    // The WH_KEYBOARD_LL hook callback must return within Windows' hook timeout
    // (~300ms) or the hook is silently removed. All real work happens in a Tokio
    // task below; callbacks only send a lightweight channel message.
    enum HotkeyEvent {
        Press,
        Release,
        HandlessToggle,
        Cancel,
        EscapeCancel,
    }

    let (hotkey_tx, mut hotkey_rx) = tokio::sync::mpsc::channel::<HotkeyEvent>(8);
    let tx_press = hotkey_tx.clone();
    let tx_handless = hotkey_tx.clone();
    let tx_cancel = hotkey_tx.clone();
    let tx_escape = hotkey_tx.clone();
    let tx_release = hotkey_tx;

    match core::hotkey::start(
        move || {
            let _ = tx_press.try_send(HotkeyEvent::Press);
        },
        move || {
            let _ = tx_release.try_send(HotkeyEvent::Release);
        },
        move || {
            let _ = tx_handless.try_send(HotkeyEvent::HandlessToggle);
        },
        move || {
            let _ = tx_cancel.try_send(HotkeyEvent::Cancel);
        },
        move || {
            let _ = tx_escape.try_send(HotkeyEvent::EscapeCancel);
        },
    ) {
        Ok(_handle) => { /* hook thread running */ }
        Err(e) => {
            log::error!("Hotkey hook failed to start: {e}");
            let app_h = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                // Give the webview a moment to initialise before emitting.
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                app_h
                    .emit(
                        "verenu:error",
                        format!("Keyboard hook failed to install — hotkey unavailable. {e}"),
                    )
                    .ok();
            });
            return;
        }
    }

    let app_hk = app.handle().clone();
    let state_hk = shared;

    tauri::async_runtime::spawn(async move {
        while let Some(event) = hotkey_rx.recv().await {
            match event {
                HotkeyEvent::Press => {
                    let (already, is_handless) = {
                        let Some(st) = lock_app_state(&state_hk) else {
                            continue;
                        };
                        (st.session.is_some(), st.handless)
                    };
                    if !already && !is_handless {
                        // Capture the target window before recording starts so
                        // inject_text can restore focus to it after the pipeline,
                        // even if the user switched windows during transcription.
                        let hwnd = crate::core::window_context::get_foreground_hwnd();
                        if let Some(mut st) = lock_app_state(&state_hk) {
                            st.target_hwnd = hwnd;
                        }
                        start_recording_session(&app_hk, &state_hk, "recording", false);
                    }
                }

                HotkeyEvent::Release => {
                    if let Some(mut st) = lock_app_state(&state_hk) {
                        st.handless = false;
                    }
                    tauri::async_runtime::spawn(pipeline::run_pipeline(
                        app_hk.clone(),
                        state_hk.clone(),
                    ));
                }

                HotkeyEvent::HandlessToggle => {
                    let (is_handless, has_session) = {
                        let Some(st) = lock_app_state(&state_hk) else {
                            continue;
                        };
                        (st.handless, st.session.is_some())
                    };
                    if is_handless {
                        core::hotkey::set_handless_active(false);
                        if let Some(mut st) = lock_app_state(&state_hk) {
                            st.handless = false;
                        }
                        tauri::async_runtime::spawn(pipeline::run_pipeline(
                            app_hk.clone(),
                            state_hk.clone(),
                        ));
                    } else if !has_session {
                        let hwnd = crate::core::window_context::get_foreground_hwnd();
                        if let Some(mut st) = lock_app_state(&state_hk) {
                            st.target_hwnd = hwnd;
                        }
                        start_recording_session(&app_hk, &state_hk, "handsfree", true);
                        core::hotkey::set_handless_active(true);
                    }
                }

                HotkeyEvent::Cancel => {
                    let Some(is_handless) = lock_app_state(&state_hk).map(|st| st.handless) else {
                        continue;
                    };
                    if is_handless {
                        // Quick tap while in handsfree = stop. Clear chord state
                        // immediately so the still-open double-tap window can't
                        // re-trigger a fresh handsfree session.
                        core::hotkey::set_handless_active(false);
                        core::hotkey::reset_chord_state();
                        if let Some(mut st) = lock_app_state(&state_hk) {
                            st.handless = false;
                        }
                        tauri::async_runtime::spawn(pipeline::run_pipeline(
                            app_hk.clone(),
                            state_hk.clone(),
                        ));
                    } else {
                        // First click of a double-tap gesture outside handsfree —
                        // discard the short recording that just started.
                        let had_session = lock_app_state(&state_hk)
                            .and_then(|mut st| st.session.take())
                            .is_some();
                        if had_session {
                            std::thread::spawn(crate::system::volume::unmute);
                        }
                        hide_pill(&app_hk);
                    }
                }

                HotkeyEvent::EscapeCancel => {
                    core::hotkey::set_handless_active(false);
                    let session = {
                        let Some(mut st) = lock_app_state(&state_hk) else {
                            continue;
                        };
                        st.handless = false;
                        st.session.take()
                    };
                    if let Some(s) = session {
                        std::thread::spawn(move || {
                            let _ = s.stop();
                        });
                        std::thread::spawn(crate::system::volume::unmute);
                    }
                    hide_pill(&app_hk);
                }
            }
        }
    });
}
