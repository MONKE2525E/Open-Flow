#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod api;
mod commands;
mod core;
mod data;
mod media;
mod pipeline;
mod system;

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

const TRAY_ID: &str = "open-flow-tray";

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

fn main() {
    let shared: SharedState = Arc::new(Mutex::new(AppState {
        session: None,
        handless: false,
        target_hwnd: 0,
        retry_capture: None,
    }));

    let db_dir = std::env::var("APPDATA")
        .map(|p| std::path::PathBuf::from(p).join("OpenFlow"))
        .unwrap_or_else(|_| std::path::PathBuf::from("."));
    std::fs::create_dir_all(&db_dir).ok();
    let db_handle: DbHandle =
        db::open(db_dir.join("openflow.db").to_str().unwrap()).expect("failed to open database");
    let _ = db::cleanup_cache_prune_expired(&db_handle);

    tauri::Builder::default()
        .plugin(tauri_plugin_store::Builder::default().build())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            if let Some(w) = app.get_webview_window("main") {
                w.show().ok();
                w.set_focus().ok();
            }
        }))
        .manage(shared.clone())
        .manage(db_handle.clone())
        .setup(move |app| {
            crate::system::logger::init(app.handle())?;
            let first_launch = if let Ok(store) =
                tauri_plugin_store::StoreExt::store(app.handle(), "settings.json")
            {
                let _ = store.reload();
                crate::data::credentials::migrate_from_store(&store);
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
            crate::pipeline::show_pill(app.handle(), "idle");

            if first_launch {
                if let Some(w) = app.get_webview_window("main") {
                    w.show().ok();
                    w.set_focus().ok();
                }
            }

            Ok(())
        })
        .on_window_event(|window, event| {
            if window.label() == "main" {
                match event {
                    tauri::WindowEvent::CloseRequested { api, .. } => {
                        api.prevent_close();
                        window.hide().ok();
                    }
                    tauri::WindowEvent::ThemeChanged(theme) => {
                        let app = window.app_handle();
                        if appearance_mode(app).as_deref().unwrap_or("system") == "system" {
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
            commands::get_api_key_status,
            commands::save_setting,
            commands::get_setting,
            commands::get_all_settings,
            commands::set_autostart,
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
            commands::check_connectivity,
            commands::get_recent_logs,
            commands::download_logs,
            commands::set_dev_logging_enabled,
        ])
        .run(tauri::generate_context!())
        .expect("error running Open Flow");
}

fn setup_tray(app: &mut tauri::App) -> tauri::Result<()> {
    let title_i = MenuItem::with_id(app, "title", "Open Flow", false, None::<&str>)?;
    let sep = PredefinedMenuItem::separator(app)?;
    let show_i = MenuItem::with_id(app, "show", "Show Window", true, None::<&str>)?;
    let quit_i = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&title_i, &sep, &show_i, &quit_i])?;

    let icon_theme = resolve_icon_theme(app.handle(), None);
    let tray_icon = runtime_icon_image(icon_theme, 32);

    TrayIconBuilder::with_id(TRAY_ID)
        .icon(tray_icon)
        .menu(&menu)
        .show_menu_on_left_click(false)
        .tooltip("Open Flow - Ctrl+Windows to record")
        .on_menu_event(|app, ev| match ev.id.as_ref() {
            "show" => {
                if let Some(w) = app.get_webview_window("main") {
                    w.show().ok();
                    w.set_focus().ok();
                }
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
                        w.show().ok();
                        w.set_focus().ok();
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

    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        if let Err(err) = tray.set_icon(Some(runtime_icon_image(icon_theme, 32))) {
            log::warn!("Failed to update tray icon: {err}");
        }
    }
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

    draw_rounded_rect(
        &mut rgba,
        size,
        IconRect {
            x: 0,
            y: 0,
            width: size,
            height: size,
            radius: scale(size, 96),
        },
        background,
    );

    for (x, y, width, height, radius) in [
        (76, 302, 56, 98, 28),
        (152, 204, 56, 196, 28),
        (228, 120, 56, 280, 28),
        (304, 246, 56, 154, 28),
        (380, 330, 56, 70, 28),
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
                        "open-flow:error",
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
                        std::thread::spawn(move || { let _ = s.stop(); });
                        std::thread::spawn(crate::system::volume::unmute);
                    }
                    hide_pill(&app_hk);
                }
            }
        }
    });
}
