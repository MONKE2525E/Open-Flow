#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod core;
mod api;
mod data;
mod media;

use crate::core::hotkey;
use crate::core::injection;
use crate::core::window_context;
use crate::api::cleanup;
use crate::api::transcription;
use crate::data::db;
use crate::data::store;
use crate::media::audio;

use std::sync::{Arc, Mutex};
use tauri::{
    AppHandle, Emitter, Manager,
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
};
use tauri_plugin_store::StoreExt;

pub type DbHandle = db::Db;

// ---------- shared state ----------

struct AppState {
    session: Option<audio::RecordingSession>,
    handless: bool,
}

type SharedState = Arc<Mutex<AppState>>;

// ---------- commands ----------

#[tauri::command]
async fn save_api_key(app: AppHandle, provider: String, key: String) -> Result<(), String> {
    let store = app.store("settings.json").map_err(|e| e.to_string())?;
    let k = match provider.as_str() {
        "groq"   => store::KEY_GROQ,
        "openai" => store::KEY_OPENAI,
        "google" => store::KEY_GOOGLE,
        _        => return Err(format!("Unknown provider: {provider}")),
    };
    store.set(k, serde_json::json!(key));
    store.save().map_err(|e| e.to_string())
}

#[tauri::command]
async fn get_api_key_status(app: AppHandle) -> Result<serde_json::Value, String> {
    let store = app.store("settings.json").map_err(|e| e.to_string())?;
    Ok(serde_json::json!({
        "groq":   store.get(store::KEY_GROQ).is_some(),
        "openai": store.get(store::KEY_OPENAI).is_some(),
        "google": store.get(store::KEY_GOOGLE).is_some(),
    }))
}

#[tauri::command]
async fn save_setting(app: AppHandle, key: String, value: serde_json::Value) -> Result<(), String> {
    let store = app.store("settings.json").map_err(|e| e.to_string())?;
    store.set(key, value);
    store.save().map_err(|e| e.to_string())
}

#[tauri::command]
async fn get_setting(app: AppHandle, key: String) -> Result<Option<serde_json::Value>, String> {
    let store = app.store("settings.json").map_err(|e| e.to_string())?;
    Ok(store.get(&key))
}

#[tauri::command]
fn get_memory_mb() -> u64 {
    #[cfg(not(target_os = "windows"))]
    return 0;

    #[cfg(target_os = "windows")]
    unsafe {
        use windows::Win32::Foundation::CloseHandle;
        use windows::Win32::System::ProcessStatus::{
            GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS, PROCESS_MEMORY_COUNTERS_EX2,
        };
        use windows::Win32::System::Threading::{
            GetCurrentProcess, GetCurrentProcessId, OpenProcess,
            PROCESS_QUERY_INFORMATION, PROCESS_VM_READ,
        };
        use windows::Win32::System::Diagnostics::ToolHelp::{
            CreateToolhelp32Snapshot, Process32FirstW, Process32NextW,
            PROCESSENTRY32W, TH32CS_SNAPPROCESS,
        };

        use std::collections::{HashMap, VecDeque};

        // PrivateWorkingSetSize from EX2 matches Task Manager's "Memory" column exactly
        // (resident private pages only). PrivateUsage/EX would include pre-committed
        // virtual memory that Chromium/WebView2 reserves but hasn't touched yet.
        let private_bytes = |h| -> usize {
            let mut pmc: PROCESS_MEMORY_COUNTERS_EX2 = std::mem::zeroed();
            if GetProcessMemoryInfo(
                h,
                &mut pmc as *mut PROCESS_MEMORY_COUNTERS_EX2 as *mut PROCESS_MEMORY_COUNTERS,
                std::mem::size_of::<PROCESS_MEMORY_COUNTERS_EX2>() as u32,
            ).is_ok() { pmc.PrivateWorkingSetSize } else { 0 }
        };

        let our_pid = GetCurrentProcessId();
        let mut total = private_bytes(GetCurrentProcess());

        // Build a full parent→children map in one snapshot pass, then BFS the
        // entire subtree. WebView2 renderer/GPU processes are grandchildren (children
        // of the browser process), so a single-level walk misses most of the memory.
        let mut children_map: HashMap<u32, Vec<u32>> = HashMap::new();
        if let Ok(snap) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) {
            let mut entry: PROCESSENTRY32W = std::mem::zeroed();
            entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
            if Process32FirstW(snap, &mut entry).is_ok() {
                loop {
                    let pid = entry.th32ProcessID;
                    let ppid = entry.th32ParentProcessID;
                    if pid != ppid {
                        children_map.entry(ppid).or_default().push(pid);
                    }
                    if Process32NextW(snap, &mut entry).is_err() { break; }
                }
            }
            CloseHandle(snap).ok();
        }

        let mut queue = VecDeque::new();
        if let Some(kids) = children_map.get(&our_pid) {
            queue.extend(kids.iter().copied());
        }
        while let Some(pid) = queue.pop_front() {
            if let Ok(h) = OpenProcess(PROCESS_QUERY_INFORMATION | PROCESS_VM_READ, false, pid) {
                total += private_bytes(h);
                CloseHandle(h).ok();
            }
            if let Some(kids) = children_map.get(&pid) {
                queue.extend(kids.iter().copied());
            }
        }

        (total / (1024 * 1024)) as u64
    }
}

#[tauri::command]
fn get_microphones() -> Vec<String> {
    audio::list_input_devices()
}

#[tauri::command]
async fn show_main(app: AppHandle) -> Result<(), String> {
    if let Some(w) = app.get_webview_window("main") {
        w.show().ok(); w.set_focus().ok();
    }
    Ok(())
}

#[tauri::command]
async fn hide_main(app: AppHandle) -> Result<(), String> {
    if let Some(w) = app.get_webview_window("main") { w.hide().ok(); }
    Ok(())
}

#[tauri::command]
fn get_recent(app: AppHandle) -> Result<Vec<db::RecentEntry>, String> {
    let db = app.state::<DbHandle>();
    db::query_recent(&db).map_err(|e| e.to_string())
}

#[tauri::command]
fn get_stats(app: AppHandle) -> Result<db::Stats, String> {
    let db = app.state::<DbHandle>();
    db::query_stats(&db).map_err(|e| e.to_string())
}

// ---------- pill helpers ----------

fn show_pill(app: &AppHandle, state: &str) {
    if let Some(pill) = app.get_webview_window("pill") {
        pill.emit("pill-state", state).ok();
        // Use SW_SHOWNOACTIVATE so the pill appears without stealing keyboard
        // focus from whatever window the user is dictating into.
        #[cfg(target_os = "windows")]
        {
            use windows::Win32::UI::WindowsAndMessaging::{ShowWindow, SW_SHOWNOACTIVATE};
            if let Ok(hwnd) = pill.hwnd() {
                unsafe { ShowWindow(hwnd, SW_SHOWNOACTIVATE); }
            }
        }
        #[cfg(not(target_os = "windows"))]
        pill.show().ok();

        if let Ok(Some(m)) = pill.primary_monitor() {
            let sz = m.size();
            let sf = m.scale_factor();
            let x = ((sz.width  as f64 / sf - 220.0) / 2.0 * sf) as i32;
            let y = ((sz.height as f64 / sf - 60.0 - 64.0) * sf) as i32;
            pill.set_position(tauri::PhysicalPosition::new(x, y)).ok();
        }
    }
}

fn hide_pill(app: &AppHandle) {
    if let Some(pill) = app.get_webview_window("pill") {
        pill.emit("pill-state", "idle").ok();
        pill.hide().ok();
    }
}

// ---------- pipeline ----------

/// Cap error strings to ~120 chars for the toast; pill always says "Failed".
fn trim_err(s: &str) -> String {
    let s = s.trim();
    if s.chars().count() > 120 { format!("{}…", s.chars().take(117).collect::<String>()) } else { s.to_string() }
}

async fn show_error_pill(app: &AppHandle, msg: &str) {
    log::error!("pipeline error: {msg}");
    app.emit("open-flow:error", msg).ok();
    show_pill(app, "error");
    // Bring main window to front so the error toast is always visible
    if let Some(w) = app.get_webview_window("main") {
        w.show().ok();
        w.set_focus().ok();
    }
    tokio::time::sleep(std::time::Duration::from_secs(4)).await;
    hide_pill(app);
}

async fn run_pipeline(app: AppHandle, state: SharedState) {
    let session = {
        let mut st = state.lock().unwrap();
        st.handless = false;
        st.session.take()
    };
    let session = match session { Some(s) => s, None => return };

    show_pill(&app, "processing");

    let (wav, duration_ms) = match session.stop() {
        Ok(v) => v,
        Err(e) => { log::error!("audio stop: {e}"); hide_pill(&app); return; }
    };
    if duration_ms < 300 { hide_pill(&app); return; }

    let store = match app.store("settings.json") {
        Ok(s) => s,
        Err(e) => { log::error!("store: {e}"); hide_pill(&app); return; }
    };

    let t_prov = store.get(store::TRANSCRIPTION_PROVIDER)
        .and_then(|v| v.as_str().map(String::from))
        .unwrap_or_else(|| "groq".into());
    let c_prov = store.get(store::CLEANUP_PROVIDER)
        .and_then(|v| v.as_str().map(String::from))
        .unwrap_or_else(|| "groq".into());
    let cleanup_on = store.get(store::CLEANUP_ENABLED)
        .and_then(|v| v.as_bool()).unwrap_or(true);

    let key_for = |p: &str| -> String {
        let k = match p { "openai" => store::KEY_OPENAI, "google" => store::KEY_GOOGLE, _ => store::KEY_GROQ };
        store.get(k).and_then(|v| v.as_str().map(String::from)).unwrap_or_default()
    };

    let t_key = key_for(&t_prov);
    if t_key.is_empty() {
        show_error_pill(&app, &format!("No API key saved for {t_prov}")).await;
        return;
    }

    let t_provider = match t_prov.as_str() {
        "openai" => transcription::Provider::OpenAI,
        "google" => transcription::Provider::Google,
        _        => transcription::Provider::Groq,
    };

    let process_name = crate::core::window_context::get_active_process_name().unwrap_or_else(|| "unknown".into());
    let profile = match process_name.as_str() {
        "code.exe" | "cursor.exe" | "idea64.exe" | "devenv.exe" | "webstorm64.exe" => "code",
        "winword.exe" | "outlook.exe" => "formal",
        "notepad.exe" => "plain",
        _ => "casual", // TODO: Read from store
    };

    let c_key = key_for(&c_prov);
    let api_used = format!("{t_prov}/transcription");

    // When both providers are Google and cleanup is on, fuse transcription and
    // cleanup into a single Gemini call — one round trip instead of two.
    let (raw, final_text) = if t_prov == "google" && c_prov == "google" && cleanup_on && !c_key.is_empty() {
        let profile_prompt = cleanup::get_system_prompt(profile);
        match transcription::transcribe_and_cleanup_gemini(wav, &t_key, &profile_prompt).await {
            Ok(text) => {
                if text.is_empty() { hide_pill(&app); return; }
                (text.clone(), text)
            }
            Err(e) => {
                log::error!("transcribe+cleanup: {e}");
                show_error_pill(&app, &trim_err(&e.to_string())).await;
                return;
            }
        }
    } else {
        let raw = match transcription::transcribe(wav, t_provider, &t_key).await {
            Ok(t) => t,
            Err(e) => {
                log::error!("transcribe: {e}");
                show_error_pill(&app, &trim_err(&e.to_string())).await;
                return;
            }
        };
        if raw.is_empty() { hide_pill(&app); return; }

        let final_text = if cleanup_on && !c_key.is_empty() {
            let cp = match c_prov.as_str() {
                "openai" => cleanup::CleanupProvider::OpenAI,
                "google" => cleanup::CleanupProvider::Google,
                _        => cleanup::CleanupProvider::Groq,
            };
            cleanup::cleanup(&raw, cp, &c_key, profile).await.unwrap_or(raw.clone())
        } else { raw.clone() };

        (raw, final_text)
    };

    // Persist to DB
    let words = final_text.split_whitespace().count() as i64;
    let db = app.state::<DbHandle>();
    let _ = db::insert_transcription(&db, &raw, &final_text, words, duration_ms as i64, &api_used);

    if let Err(e) = injection::inject_text(&final_text).await {
        log::error!("inject: {e}");
    }

    app.emit("open-flow:transcribed", &final_text).ok();
    hide_pill(&app);
}

// ---------- recording control commands ----------

#[tauri::command]
async fn stop_recording(
    app: AppHandle,
    state: tauri::State<'_, SharedState>,
) -> Result<(), String> {
    let session = {
        let mut st = state.lock().unwrap();
        st.handless = false;
        st.session.take()
    };
    if let Some(s) = session {
        let _ = s.stop(); // join thread, discard audio
    }
    hide_pill(&app);
    Ok(())
}

#[tauri::command]
async fn stop_handless_mode(
    app: AppHandle,
    state: tauri::State<'_, SharedState>,
) -> Result<(), String> {
    state.lock().unwrap().handless = false;
    tauri::async_runtime::spawn(run_pipeline(app, state.inner().clone()));
    Ok(())
}

// ---------- entry ----------

fn main() {
    let shared: SharedState = Arc::new(Mutex::new(AppState { session: None, handless: false }));

    // Open DB before the builder so we have a real path.
    // Use %APPDATA%\OpenFlow\ on Windows, or the current dir as fallback.
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
            // ── Tray ──────────────────────────────────────────────
            let title_i = MenuItem::with_id(app, "title", "Open Flow", false, None::<&str>)?;
            let sep     = PredefinedMenuItem::separator(app)?;
            let show_i  = MenuItem::with_id(app, "show", "Show Window", true, None::<&str>)?;
            let quit_i  = MenuItem::with_id(app, "quit", "Quit",        true, None::<&str>)?;
            let menu    = Menu::with_items(app, &[&title_i, &sep, &show_i, &quit_i])?;

            // Set window icon so Task Manager shows the logo (HWND HICON, not just PE resource)
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

            // ── Low-level keyboard hook: Alt+Space ─────────────────
            // The WH_KEYBOARD_LL hook callback must return within Windows'
            // hook timeout (~300ms) or the hook is silently removed. Audio
            // device init easily exceeds that, so callbacks only send a
            // channel message; all real work happens in a Tokio task below.
            enum HotkeyEvent { Press, Release, HandlessToggle }
            let (hotkey_tx, mut hotkey_rx) = tokio::sync::mpsc::channel::<HotkeyEvent>(8);
            let tx_press    = hotkey_tx.clone();
            let tx_handless = hotkey_tx.clone();
            let tx_release  = hotkey_tx;

            hotkey::start(
                move || { let _ = tx_press.try_send(HotkeyEvent::Press); },
                move || { let _ = tx_release.try_send(HotkeyEvent::Release); },
                move || { let _ = tx_handless.try_send(HotkeyEvent::HandlessToggle); },
            );

            let app_hk    = app.handle().clone();
            let state_hk  = shared.clone();
            tauri::async_runtime::spawn(async move {
                use std::sync::atomic::Ordering;
                while let Some(event) = hotkey_rx.recv().await {
                    match event {
                        HotkeyEvent::Press => {
                            let (already, is_handless) = {
                                let st = state_hk.lock().unwrap();
                                (st.session.is_some(), st.handless)
                            };
                            if !already && !is_handless {
                                let device = app_hk
                                    .store("settings.json").ok()
                                    .and_then(|s| s.get(store::MICROPHONE_DEVICE))
                                    .and_then(|v| v.as_str().map(String::from));
                                match audio::RecordingSession::start(device) {
                                    Ok(session) => {
                                        let level_arc  = session.level.clone();
                                        let active_arc = session.active.clone();
                                        state_hk.lock().unwrap().session = Some(session);
                                        show_pill(&app_hk, "recording");

                                        let pill_handle = app_hk.clone();
                                        tauri::async_runtime::spawn(async move {
                                            loop {
                                                if !active_arc.load(Ordering::Relaxed) { break; }
                                                let level = f32::from_bits(level_arc.load(Ordering::Relaxed));
                                                if let Some(pill) = pill_handle.get_webview_window("pill") {
                                                    pill.emit("audio-level", level).ok();
                                                }
                                                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                                            }
                                        });
                                    }
                                    Err(e) => log::error!("start recording: {e}"),
                                }
                            }
                        }
                        HotkeyEvent::Release => {
                            tauri::async_runtime::spawn(run_pipeline(app_hk.clone(), state_hk.clone()));
                        }
                        HotkeyEvent::HandlessToggle => {
                            let is_handless = state_hk.lock().unwrap().handless;
                            if is_handless {
                                // Stop handless mode — transcribe and inject
                                state_hk.lock().unwrap().handless = false;
                                tauri::async_runtime::spawn(run_pipeline(app_hk.clone(), state_hk.clone()));
                            } else {
                                // Start handless mode
                                let already = state_hk.lock().unwrap().session.is_some();
                                if !already {
                                    let device = app_hk
                                        .store("settings.json").ok()
                                        .and_then(|s| s.get(store::MICROPHONE_DEVICE))
                                        .and_then(|v| v.as_str().map(String::from));
                                    match audio::RecordingSession::start(device) {
                                        Ok(session) => {
                                            let level_arc  = session.level.clone();
                                            let active_arc = session.active.clone();
                                            {
                                                let mut st = state_hk.lock().unwrap();
                                                st.session = Some(session);
                                                st.handless = true;
                                            }
                                            show_pill(&app_hk, "handsfree");

                                            let pill_handle = app_hk.clone();
                                            tauri::async_runtime::spawn(async move {
                                                loop {
                                                    if !active_arc.load(Ordering::Relaxed) { break; }
                                                    let level = f32::from_bits(level_arc.load(Ordering::Relaxed));
                                                    if let Some(pill) = pill_handle.get_webview_window("pill") {
                                                        pill.emit("audio-level", level).ok();
                                                    }
                                                    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                                                }
                                            });
                                        }
                                        Err(e) => log::error!("start handless: {e}"),
                                    }
                                }
                            }
                        }
                    }
                }
            });

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
            save_api_key, get_api_key_status,
            save_setting,  get_setting,
            show_main,     hide_main,
            get_recent,    get_stats,
            get_microphones, get_memory_mb,
            stop_recording, stop_handless_mode,
        ])
        .run(tauri::generate_context!())
        .expect("error running Open Flow");
}
