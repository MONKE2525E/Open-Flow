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
    Emitter, Manager,
};

pub type DbHandle = db::Db;

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
    }));

    let db_dir = std::env::var("APPDATA")
        .map(|p| std::path::PathBuf::from(p).join("OpenFlow"))
        .unwrap_or_else(|_| std::path::PathBuf::from("."));
    std::fs::create_dir_all(&db_dir).ok();
    let db_handle: DbHandle =
        db::open(db_dir.join("openflow.db").to_str().unwrap()).expect("failed to open database");

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
            let first_launch = if let Ok(store) =
                tauri_plugin_store::StoreExt::store(app.handle(), "settings.json")
            {
                let _ = store.reload();
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
                if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    window.hide().ok();
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
            commands::get_microphones,
            commands::get_memory_mb,
            commands::start_input_recording,
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
            commands::check_for_update,
            commands::install_update,
            commands::check_connectivity,
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

    if let Some(w) = app.get_webview_window("main") {
        if let Ok(icon) = tauri::image::Image::from_bytes(include_bytes!("../icons/128x128.png")) {
            w.set_icon(icon).ok();
        }
    }

    let tray_icon =
        tauri::image::Image::from_bytes(include_bytes!("../icons/32x32.png")).expect("tray icon");

    TrayIconBuilder::new()
        .icon(tray_icon)
        .menu(&menu)
        .show_menu_on_left_click(false)
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

    Ok(())
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
    }

    let (hotkey_tx, mut hotkey_rx) = tokio::sync::mpsc::channel::<HotkeyEvent>(8);
    let tx_press = hotkey_tx.clone();
    let tx_handless = hotkey_tx.clone();
    let tx_cancel = hotkey_tx.clone();
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
            }
        }
    });
}
