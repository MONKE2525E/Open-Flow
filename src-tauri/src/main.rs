#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod api;
mod commands;
mod core;
mod data;
mod media;
mod pipeline;
mod system;

use crate::data::db;
use crate::pipeline::{AppState, SharedState, hide_pill, start_recording_session};

use std::sync::{Arc, Mutex};
use tauri::{
    Manager,
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
};

pub type DbHandle = db::Db;

fn main() {
    let shared: SharedState = Arc::new(Mutex::new(AppState { session: None, handless: false }));

    let db_dir = std::env::var("APPDATA")
        .map(|p| std::path::PathBuf::from(p).join("OpenFlow"))
        .unwrap_or_else(|_| std::path::PathBuf::from("."));
    std::fs::create_dir_all(&db_dir).ok();
    let db_handle: DbHandle = db::open(db_dir.join("openflow.db").to_str().unwrap())
        .expect("failed to open database");

    tauri::Builder::default()
        .plugin(tauri_plugin_store::Builder::default().build())
        .plugin(tauri_plugin_shell::init())
        .manage(shared.clone())
        .manage(db_handle.clone())
        .setup(move |app| {
            setup_tray(app)?;
            setup_hotkey(app, shared.clone());
            Ok(())
        })
        .on_window_event(|window, event| {
            if window.label() == "main" {
                if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    window.hide().ok();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::save_api_key,    commands::get_api_key_status,
            commands::save_setting,    commands::get_setting,
            commands::show_main,       commands::hide_main,
            commands::get_recent,      commands::get_stats,
            commands::get_microphones, commands::get_memory_mb,
            commands::stop_recording,  commands::stop_handless_mode,
            commands::get_installed_apps,
            commands::get_app_mappings, commands::save_app_mappings,
        ])
        .run(tauri::generate_context!())
        .expect("error running Open Flow");
}

fn setup_tray(app: &mut tauri::App) -> tauri::Result<()> {
    let title_i = MenuItem::with_id(app, "title", "Open Flow", false, None::<&str>)?;
    let sep     = PredefinedMenuItem::separator(app)?;
    let show_i  = MenuItem::with_id(app, "show", "Show Window", true, None::<&str>)?;
    let quit_i  = MenuItem::with_id(app, "quit", "Quit",        true, None::<&str>)?;
    let menu    = Menu::with_items(app, &[&title_i, &sep, &show_i, &quit_i])?;

    if let Some(w) = app.get_webview_window("main") {
        if let Ok(icon) = tauri::image::Image::from_bytes(include_bytes!("../icons/128x128.png")) {
            w.set_icon(icon).ok();
        }
    }

    let tray_icon = tauri::image::Image::from_bytes(
        include_bytes!("../icons/32x32.png")
    ).expect("tray icon");

    TrayIconBuilder::new()
        .icon(tray_icon)
        .menu(&menu)
        .menu_on_left_click(false)
        .tooltip("Open Flow — Alt+Space to record")
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
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = ev {
                let app = tray.app_handle();
                if let Some(w) = app.get_webview_window("main") {
                    if w.is_visible().unwrap_or(false) { w.hide().ok(); }
                    else { w.show().ok(); w.set_focus().ok(); }
                }
            }
        })
        .build(app)?;

    Ok(())
}

fn setup_hotkey(app: &mut tauri::App, shared: SharedState) {
    // The WH_KEYBOARD_LL hook callback must return within Windows' hook timeout
    // (~300ms) or the hook is silently removed. All real work happens in a Tokio
    // task below; callbacks only send a lightweight channel message.
    enum HotkeyEvent { Press, Release, HandlessToggle, Cancel }

    let (hotkey_tx, mut hotkey_rx) = tokio::sync::mpsc::channel::<HotkeyEvent>(8);
    let tx_press    = hotkey_tx.clone();
    let tx_handless = hotkey_tx.clone();
    let tx_cancel   = hotkey_tx.clone();
    let tx_release  = hotkey_tx;

    core::hotkey::start(
        move || { let _ = tx_press.try_send(HotkeyEvent::Press); },
        move || { let _ = tx_release.try_send(HotkeyEvent::Release); },
        move || { let _ = tx_handless.try_send(HotkeyEvent::HandlessToggle); },
        move || { let _ = tx_cancel.try_send(HotkeyEvent::Cancel); },
    );

    let app_hk   = app.handle().clone();
    let state_hk = shared;

    tauri::async_runtime::spawn(async move {
        while let Some(event) = hotkey_rx.recv().await {
            match event {
                HotkeyEvent::Press => {
                    let (already, is_handless) = {
                        let st = state_hk.lock().unwrap();
                        (st.session.is_some(), st.handless)
                    };
                    if !already && !is_handless {
                        start_recording_session(&app_hk, &state_hk, "recording", false);
                    }
                }

                HotkeyEvent::Release => {
                    state_hk.lock().unwrap().handless = false;
                    tauri::async_runtime::spawn(pipeline::run_pipeline(app_hk.clone(), state_hk.clone()));
                }

                HotkeyEvent::HandlessToggle => {
                    let (is_handless, has_session) = {
                        let st = state_hk.lock().unwrap();
                        (st.handless, st.session.is_some())
                    };
                    if is_handless {
                        state_hk.lock().unwrap().handless = false;
                        tauri::async_runtime::spawn(pipeline::run_pipeline(app_hk.clone(), state_hk.clone()));
                    } else if !has_session {
                        start_recording_session(&app_hk, &state_hk, "handsfree", true);
                    }
                }

                HotkeyEvent::Cancel => {
                    // Discard a very brief recording started by the first click of
                    // an Alt+Space double-click gesture. No-op in handless mode so
                    // we don't drop the ongoing handless session.
                    let is_handless = state_hk.lock().unwrap().handless;
                    if !is_handless {
                        state_hk.lock().unwrap().session.take();
                        hide_pill(&app_hk);
                    }
                }
            }
        }
    });
}
