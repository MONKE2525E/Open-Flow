use tauri::{AppHandle, Manager};
use tauri_plugin_store::StoreExt;

use crate::data::{db, store};
use crate::media::audio;
use crate::pipeline::{self, SharedState};
use crate::system::apps::{AppMapping, InstalledApp};
use crate::DbHandle;

// ---------- API keys ----------

#[tauri::command]
pub async fn save_api_key(app: AppHandle, provider: String, key: String) -> Result<(), String> {
    let store = app.store("settings.json").map_err(|e| e.to_string())?;
    let k = match provider.as_str() {
        "groq" => store::KEY_GROQ,
        "openai" => store::KEY_OPENAI,
        "google" => store::KEY_GOOGLE,
        _ => return Err(format!("Unknown provider: {provider}")),
    };
    store.set(k, serde_json::json!(key));
    store.save().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_api_key_status(app: AppHandle) -> Result<serde_json::Value, String> {
    let store = app.store("settings.json").map_err(|e| e.to_string())?;
    Ok(serde_json::json!({
        "groq":   store.get(store::KEY_GROQ).is_some(),
        "openai": store.get(store::KEY_OPENAI).is_some(),
        "google": store.get(store::KEY_GOOGLE).is_some(),
    }))
}

// ---------- generic settings ----------

#[tauri::command]
pub async fn save_setting(
    app: AppHandle,
    key: String,
    value: serde_json::Value,
) -> Result<(), String> {
    let store = app.store("settings.json").map_err(|e| e.to_string())?;
    store.set(key, value);
    store.save().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_setting(app: AppHandle, key: String) -> Result<Option<serde_json::Value>, String> {
    let store = app.store("settings.json").map_err(|e| e.to_string())?;
    Ok(store.get(&key))
}

// ---------- window management ----------

#[tauri::command]
pub async fn show_main(app: AppHandle) -> Result<(), String> {
    if let Some(w) = app.get_webview_window("main") {
        w.show().ok();
        w.set_focus().ok();
    }
    Ok(())
}

#[tauri::command]
pub async fn hide_main(app: AppHandle) -> Result<(), String> {
    if let Some(w) = app.get_webview_window("main") {
        w.hide().ok();
    }
    Ok(())
}

// ---------- history / stats ----------

#[tauri::command]
pub fn get_recent(app: AppHandle) -> Result<Vec<db::RecentEntry>, String> {
    let db = app.state::<DbHandle>();
    db::query_recent(&db).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_stats(app: AppHandle) -> Result<db::Stats, String> {
    let db = app.state::<DbHandle>();
    db::query_stats(&db).map_err(|e| e.to_string())
}

// ---------- microphone ----------

#[tauri::command]
pub fn get_microphones() -> Vec<String> {
    audio::list_input_devices()
}

// ---------- memory ----------

#[tauri::command]
pub fn get_memory_mb() -> u64 {
    crate::system::memory::measure()
}

// ---------- recording control ----------

#[tauri::command]
pub async fn stop_recording(
    app: AppHandle,
    state: tauri::State<'_, SharedState>,
) -> Result<(), String> {
    let session = {
        let mut st = state.lock().unwrap();
        st.handless = false;
        st.session.take()
    };
    if let Some(s) = session {
        let _ = s.stop();
        std::thread::spawn(|| crate::system::volume::unmute());
    }
    pipeline::hide_pill(&app);
    Ok(())
}

#[tauri::command]
pub async fn stop_handless_mode(
    app: AppHandle,
    state: tauri::State<'_, SharedState>,
) -> Result<(), String> {
    state.lock().unwrap().handless = false;
    tauri::async_runtime::spawn(pipeline::run_pipeline(app, state.inner().clone()));
    Ok(())
}

// ---------- app mappings ----------

#[tauri::command]
pub fn get_installed_apps() -> Vec<InstalledApp> {
    crate::system::apps::list_installed_apps()
}

#[tauri::command]
pub async fn get_app_mappings(app: AppHandle) -> Result<Vec<AppMapping>, String> {
    let store = app.store("settings.json").map_err(|e| e.to_string())?;
    let mappings = store
        .get(store::APP_MAPPINGS)
        .and_then(|v| serde_json::from_value::<Vec<AppMapping>>(v).ok())
        .unwrap_or_default();
    Ok(mappings)
}

#[tauri::command]
pub async fn save_app_mappings(app: AppHandle, mappings: Vec<AppMapping>) -> Result<(), String> {
    let store = app.store("settings.json").map_err(|e| e.to_string())?;
    store.set(store::APP_MAPPINGS, serde_json::to_value(mappings).unwrap());
    store.save().map_err(|e| e.to_string())
}

// ---------- snippets ----------

#[tauri::command]
pub fn get_snippets(app: AppHandle) -> Result<Vec<db::Snippet>, String> {
    let db = app.state::<DbHandle>();
    db::query_snippets(&db).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn create_snippet(
    app: AppHandle,
    trigger: String,
    expansion: String,
    instructions: String,
) -> Result<(), String> {
    let db = app.state::<DbHandle>();
    db::insert_snippet(&db, &trigger, &expansion, &instructions).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn edit_snippet(
    app: AppHandle,
    id: i64,
    trigger: String,
    expansion: String,
    instructions: String,
) -> Result<(), String> {
    let db = app.state::<DbHandle>();
    db::update_snippet(&db, id, &trigger, &expansion, &instructions).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn remove_snippet(app: AppHandle, id: i64) -> Result<(), String> {
    let db = app.state::<DbHandle>();
    db::delete_snippet(&db, id).map_err(|e| e.to_string())
}

// ---------- dictionary ----------

#[tauri::command]
pub fn get_dictionary(app: AppHandle) -> Result<Vec<db::DictionaryEntry>, String> {
    let db = app.state::<DbHandle>();
    db::query_dictionary(&db).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn create_dictionary_entry(
    app: AppHandle,
    term: String,
    mistake: Option<String>,
) -> Result<(), String> {
    let db = app.state::<DbHandle>();
    db::insert_dictionary_entry(&db, &term, mistake.as_deref()).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn edit_dictionary_entry(
    app: AppHandle,
    id: i64,
    term: String,
    mistake: Option<String>,
) -> Result<(), String> {
    let db = app.state::<DbHandle>();
    db::update_dictionary_entry(&db, id, &term, mistake.as_deref()).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn remove_dictionary_entry(app: AppHandle, id: i64) -> Result<(), String> {
    let db = app.state::<DbHandle>();
    db::delete_dictionary_entry(&db, id).map_err(|e| e.to_string())
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
    if vk2 == 0 {
        return Err(format!("Unrecognized key code: {key2}"));
    }
    crate::core::hotkey::update_keys(vk1, vk2);
    let store = app.store("settings.json").map_err(|e| e.to_string())?;
    store.set("hotkey", serde_json::json!([key1, key2]));
    store.save().map_err(|e| e.to_string())
}

// ---------- autostart ----------

#[tauri::command]
pub async fn set_autostart(_app: AppHandle, enabled: bool) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::ffi::OsStrExt;
        use windows::core::PCWSTR;
        use windows::Win32::System::Registry::{
            RegCloseKey, RegDeleteValueW, RegOpenKeyExW, RegSetValueExW, HKEY, HKEY_CURRENT_USER,
            KEY_WRITE, REG_SZ,
        };

        let app_path = std::env::current_exe()
            .map_err(|e| format!("Failed to get executable path: {}", e))?
            .to_string_lossy()
            .to_string();

        let subkey: Vec<u16> =
            std::ffi::OsStr::new("Software\\Microsoft\\Windows\\CurrentVersion\\Run")
                .encode_wide()
                .chain(std::iter::once(0))
                .collect();
        let value_name: Vec<u16> = std::ffi::OsStr::new("OpenFlow")
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
                let app_path_bytes =
                    std::mem::transmute::<*const u16, *const u8>(app_path_wide.as_ptr());
                let app_path_len = (app_path_wide.len() - 1) * 2;
                RegSetValueExW(
                    hkey,
                    PCWSTR(value_name.as_ptr()),
                    None,
                    REG_SZ,
                    Some(std::slice::from_raw_parts(app_path_bytes, app_path_len)),
                )
            } else {
                RegDeleteValueW(hkey, PCWSTR(value_name.as_ptr()))
            };

            RegCloseKey(hkey);

            if result.is_err() {
                return Err("Failed to set registry value".to_string());
            }
        }
    }

    let store = _app.store("settings.json").map_err(|e| e.to_string())?;
    store.set("autostart_enabled", serde_json::json!(enabled));
    store.save().map_err(|e| e.to_string())
}
