use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_store::StoreExt;

use crate::data::{db, store};
use crate::media::audio;
use crate::pipeline::{self, SharedState};
use crate::system::apps::{AppMapping, InstalledApp};
use crate::DbHandle;

const SPACE_CONSTRAINED_THRESHOLD_BYTES: u64 = 1_073_741_824;

fn lock_state<'a>(
    state: &'a tauri::State<'_, SharedState>,
) -> Result<std::sync::MutexGuard<'a, pipeline::AppState>, String> {
    state
        .lock()
        .map_err(|_| "Recording state lock was poisoned".to_string())
}

fn validate_setting(key: &str, value: &serde_json::Value) -> Result<(), String> {
    let is_model_map = |v: &serde_json::Value| {
        let Some(obj) = v.as_object() else {
            return false;
        };
        obj.keys().all(|k| store::PROVIDERS.contains(&k.as_str()))
            && obj.values().all(|val| {
                val.as_array().is_some_and(|arr| {
                    arr.iter()
                        .all(|x| x.as_str().is_some_and(|s| !s.trim().is_empty()))
                })
            })
    };
    let is_non_empty_string_array = |v: &serde_json::Value| {
        v.as_array().is_some_and(|arr| {
            arr.iter()
                .all(|x| x.as_str().is_some_and(|s| !s.trim().is_empty()))
        })
    };
    let valid = match key {
        store::TRANSCRIPTION_PROVIDER | store::CLEANUP_PROVIDER => value
            .as_str()
            .is_some_and(|v| store::PROVIDERS.contains(&v)),
        store::TRANSCRIPTION_LANGUAGE => value
            .as_str()
            .is_some_and(store::is_supported_transcription_language),
        store::TRANSCRIPTION_MODEL
        | store::CLEANUP_MODEL
        | store::TRANSCRIPTION_DEFAULT_MODEL
        | store::CLEANUP_DEFAULT_MODEL
        | store::DEFAULT_TONE
        | store::CLEANUP_INTENSITY
        | store::MICROPHONE_DEVICE
        | "history_retention"
        | "update_dismissed_version" => value.is_string() || value.is_null(),
        store::TRANSCRIPTION_MODELS_BY_PROVIDER | store::CLEANUP_MODELS_BY_PROVIDER => {
            is_model_map(value)
        }
        store::TRANSCRIPTION_FALLBACK_MODELS | store::CLEANUP_FALLBACK_MODELS => {
            is_non_empty_string_array(value)
        }
        store::APPEARANCE_MODE => value
            .as_str()
            .is_some_and(|v| matches!(v, "system" | "light" | "dark")),
        store::CLEANUP_ENABLED
        | store::NOISE_REDUCTION
        | store::MUTE_AUDIO
        | store::APP_CONTEXT_HINT
        | store::AUTO_LEARN_ENABLED
        | store::AUTO_LEARN_EVENT_MODE
        | store::CONTEXTUAL_CAPS
        | store::AUTO_SPACING
        | store::SETUP_COMPLETE
        | store::FORCE_SETUP_ON_LAUNCH
        | store::ADVANCED_MODEL_UI
        | "autostart_enabled" => value.is_boolean(),
        store::MIC_GAIN => value.as_f64().is_some_and(|v| (1.0..=8.0).contains(&v)),
        store::APP_MAPPINGS => serde_json::from_value::<Vec<AppMapping>>(value.clone()).is_ok(),
        store::HOTKEY => value
            .as_array()
            .is_some_and(|keys| keys.len() == 2 && keys.iter().all(serde_json::Value::is_string)),
        _ => false,
    };

    if valid {
        Ok(())
    } else {
        Err(format!("Invalid or unsupported setting: {key}"))
    }
}

// ---------- API keys ----------

#[tauri::command]
pub async fn save_api_key(_app: AppHandle, provider: String, key: String) -> Result<(), String> {
    tokio::task::spawn_blocking(move || crate::data::credentials::set(&provider, &key))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn delete_api_key(_app: AppHandle, provider: String) -> Result<(), String> {
    tokio::task::spawn_blocking(move || crate::data::credentials::delete(&provider))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn get_api_key_status(_app: AppHandle) -> Result<serde_json::Value, String> {
    use crate::data::{credentials, store};
    tokio::task::spawn_blocking(move || {
        Ok(serde_json::json!({
            "groq":   credentials::has(store::GROQ),
            "openai": credentials::has(store::OPENAI),
            "google": credentials::has(store::GOOGLE),
        }))
    })
    .await
    .map_err(|e| e.to_string())?
}

// ---------- generic settings ----------

#[tauri::command]
pub async fn save_setting(
    app: AppHandle,
    key: String,
    value: serde_json::Value,
) -> Result<(), String> {
    validate_setting(&key, &value)?;
    let store = app.store("settings.json").map_err(|e| e.to_string())?;
    store.set(key.clone(), value);
    store.save().map_err(|e| e.to_string())?;

    if key == store::APPEARANCE_MODE {
        crate::apply_runtime_icons(&app, None);
    }

    Ok(())
}

#[tauri::command]
pub async fn get_setting(app: AppHandle, key: String) -> Result<Option<serde_json::Value>, String> {
    let store = app.store("settings.json").map_err(|e| e.to_string())?;
    Ok(store.get(&key))
}

#[derive(serde::Serialize)]
pub struct AllSettings {
    pub transcription_model: Option<String>,
    pub transcription_language: Option<String>,
    pub cleanup_model: Option<String>,
    pub transcription_models_by_provider: Option<serde_json::Value>,
    pub cleanup_models_by_provider: Option<serde_json::Value>,
    pub transcription_default_model: Option<String>,
    pub cleanup_default_model: Option<String>,
    pub transcription_fallback_models: Option<Vec<String>>,
    pub cleanup_fallback_models: Option<Vec<String>>,
    pub advanced_model_ui: Option<bool>,
    pub cleanup_enabled: Option<bool>,
    pub noise_reduction: Option<bool>,
    pub mute_audio: Option<bool>,
    pub autostart_enabled: Option<bool>,
    pub app_context_hint: Option<bool>,
    pub auto_learn_enabled: Option<bool>,
    pub contextual_caps_enabled: Option<bool>,
    pub auto_spacing_enabled: Option<bool>,
    pub mic_gain: Option<f64>,
    pub history_retention: Option<String>,
    pub microphone_device: Option<String>,
    pub hotkey: Option<Vec<String>>,
    pub appearance_mode: Option<String>,
}

#[derive(serde::Serialize)]
pub struct CleanupCacheStatus {
    pub entry_count: i64,
    pub is_space_constrained: bool,
    pub free_bytes: u64,
}

#[tauri::command]
pub async fn get_all_settings(app: AppHandle) -> Result<AllSettings, String> {
    let s = app.store("settings.json").map_err(|e| e.to_string())?;
    let bool_val = |key: &str| s.get(key).and_then(|v| v.as_bool());
    let str_val = |key: &str| s.get(key).and_then(|v| v.as_str().map(String::from));
    let f64_val = |key: &str| s.get(key).and_then(|v| v.as_f64());
    let json_val = |key: &str| s.get(key);
    let str_array_val = |key: &str| {
        s.get(key).and_then(|v| {
            v.as_array().map(|arr| {
                arr.iter()
                    .filter_map(|x| x.as_str().map(String::from))
                    .collect()
            })
        })
    };
    Ok(AllSettings {
        transcription_model: str_val(store::TRANSCRIPTION_MODEL),
        transcription_language: str_val(store::TRANSCRIPTION_LANGUAGE),
        cleanup_model: str_val(store::CLEANUP_MODEL),
        transcription_models_by_provider: json_val(store::TRANSCRIPTION_MODELS_BY_PROVIDER),
        cleanup_models_by_provider: json_val(store::CLEANUP_MODELS_BY_PROVIDER),
        transcription_default_model: str_val(store::TRANSCRIPTION_DEFAULT_MODEL),
        cleanup_default_model: str_val(store::CLEANUP_DEFAULT_MODEL),
        transcription_fallback_models: str_array_val(store::TRANSCRIPTION_FALLBACK_MODELS),
        cleanup_fallback_models: str_array_val(store::CLEANUP_FALLBACK_MODELS),
        advanced_model_ui: bool_val(store::ADVANCED_MODEL_UI),
        cleanup_enabled: bool_val(store::CLEANUP_ENABLED),
        noise_reduction: bool_val(store::NOISE_REDUCTION),
        mute_audio: bool_val(store::MUTE_AUDIO),
        autostart_enabled: bool_val("autostart_enabled"),
        app_context_hint: bool_val(store::APP_CONTEXT_HINT),
        auto_learn_enabled: bool_val(store::AUTO_LEARN_ENABLED),
        contextual_caps_enabled: bool_val(store::CONTEXTUAL_CAPS),
        auto_spacing_enabled: bool_val(store::AUTO_SPACING),
        mic_gain: f64_val(store::MIC_GAIN),
        history_retention: str_val("history_retention"),
        microphone_device: str_val(store::MICROPHONE_DEVICE),
        hotkey: s.get(store::HOTKEY).and_then(|v| {
            v.as_array().map(|arr| {
                arr.iter()
                    .filter_map(|x| x.as_str().map(String::from))
                    .collect()
            })
        }),
        appearance_mode: str_val(store::APPEARANCE_MODE),
    })
}

// ---------- window management ----------

#[tauri::command]
pub async fn show_main(app: AppHandle) -> Result<(), String> {
    if let Some(w) = app.get_webview_window("main") {
        #[cfg(target_os = "macos")]
        {
            let _ = crate::system::mac_app::set_regular_activation_policy();
        }
        w.show().ok();
        w.set_focus().ok();
        #[cfg(target_os = "macos")]
        {
            crate::system::mac_app::refresh_dock_icon();
        }
    }
    Ok(())
}

#[tauri::command]
pub async fn hide_main(app: AppHandle) -> Result<(), String> {
    if let Some(w) = app.get_webview_window("main") {
        #[cfg(target_os = "macos")]
        {
            let _ = crate::system::mac_app::set_accessory_activation_policy();
        }
        w.hide().ok();
    }
    Ok(())
}

// ---------- history / stats ----------

#[tauri::command]
pub async fn get_recent(app: AppHandle) -> Result<Vec<db::RecentEntry>, String> {
    let db = app.state::<DbHandle>().inner().clone();
    tokio::task::spawn_blocking(move || db::query_recent(&db).map_err(|e| e.to_string()))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn get_stats(app: AppHandle) -> Result<db::Stats, String> {
    let db = app.state::<DbHandle>().inner().clone();
    tokio::task::spawn_blocking(move || db::query_stats(&db).map_err(|e| e.to_string()))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn retry_transcription(
    app: AppHandle,
    state: tauri::State<'_, SharedState>,
) -> Result<db::RecentEntry, String> {
    pipeline::retry_transcription_impl(&app, &state)
        .await
        .map_err(|e| e.to_string())
}

#[cfg(target_os = "windows")]
fn free_bytes_for_path(path: &std::path::Path) -> Result<u64, String> {
    use std::os::windows::ffi::OsStrExt;
    use windows::core::PCWSTR;
    use windows::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;

    let mut free_bytes_available: u64 = 0;
    let mut total_bytes: u64 = 0;
    let mut total_free_bytes: u64 = 0;
    let wide: Vec<u16> = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    let result = unsafe {
        GetDiskFreeSpaceExW(
            PCWSTR(wide.as_ptr()),
            Some(&mut free_bytes_available),
            Some(&mut total_bytes),
            Some(&mut total_free_bytes),
        )
    };
    result
        .map(|_| free_bytes_available)
        .map_err(|_| "Failed to read free disk space".to_string())
}

#[cfg(target_os = "macos")]
fn free_bytes_for_path(path: &std::path::Path) -> Result<u64, String> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;

    let c_path =
        CString::new(path.as_os_str().as_bytes()).map_err(|_| "Invalid path".to_string())?;
    let mut stat: libc::statvfs = unsafe { std::mem::zeroed() };
    let rc = unsafe { libc::statvfs(c_path.as_ptr(), &mut stat) };
    if rc != 0 {
        return Err("Failed to read free disk space".to_string());
    }
    Ok(stat.f_bavail as u64 * stat.f_frsize as u64)
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
fn free_bytes_for_path(_path: &std::path::Path) -> Result<u64, String> {
    Ok(u64::MAX)
}

#[tauri::command]
pub async fn clear_cleanup_cache(app: AppHandle) -> Result<usize, String> {
    let db = app.state::<DbHandle>().inner().clone();
    tokio::task::spawn_blocking(move || db::cleanup_cache_clear_all(&db).map_err(|e| e.to_string()))
        .await
        .map_err(|e| format!("clear_cleanup_cache task panicked: {e}"))?
}

#[tauri::command]
pub async fn get_cleanup_cache_status(app: AppHandle) -> Result<CleanupCacheStatus, String> {
    let db = app.state::<DbHandle>().inner().clone();
    let app_data = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("Failed to resolve app data directory: {e}"))?;
    let (free_bytes, entry_count) = tokio::task::spawn_blocking(move || {
        let free = free_bytes_for_path(&app_data)
            .map_err(|e| format!("Failed to read free disk space: {e}"))?;
        let count = db::cleanup_cache_count(&db)
            .map_err(|e| format!("Failed to count cleanup cache entries: {e}"))?;
        Ok::<_, String>((free, count))
    })
    .await
    .map_err(|e| format!("get_cleanup_cache_status task panicked: {e}"))??;
    Ok(CleanupCacheStatus {
        entry_count,
        is_space_constrained: free_bytes < SPACE_CONSTRAINED_THRESHOLD_BYTES,
        free_bytes,
    })
}

// ---------- microphone ----------

#[tauri::command]
pub async fn get_microphones() -> Vec<String> {
    match tokio::task::spawn_blocking(audio::list_input_devices).await {
        Ok(devices) => devices,
        Err(e) => {
            log::error!("Task to get microphones panicked: {e}");
            Vec::new()
        }
    }
}

// ---------- memory ----------

#[tauri::command]
pub async fn get_memory_mb() -> u64 {
    match tokio::task::spawn_blocking(crate::system::memory::measure).await {
        Ok(v) => v,
        Err(e) => {
            log::error!("Task to get memory usage panicked: {e}");
            0
        }
    }
}

// ---------- recording control ----------

#[tauri::command]
pub async fn start_input_recording(
    app: AppHandle,
    state: tauri::State<'_, SharedState>,
) -> Result<(), String> {
    {
        let mut st = lock_state(&state)?;
        if st.session.is_some() || st.starting {
            return Err("Already recording".to_string());
        }
        st.starting = true;
    }

    let state_clone = state.inner().clone();
    let app_clone = app.clone();
    let start_result = tokio::task::spawn_blocking(move || {
        pipeline::start_recording_session_ex(
            &app_clone,
            &state_clone,
            "recording",
            false,
            None,
            true,
            false,
        )
    })
    .await;

    {
        let mut st = lock_state(&state)?;
        st.starting = false;
    }

    let start_result = start_result.map_err(|e| format!("Recording task panicked: {e}"))?;

    match start_result {
        Ok(()) => Ok(()),
        Err(e) => {
            let msg = format!("Failed to start recording: {e}");
            crate::pipeline::hide_pill(&app);
            app.emit("open-flow:error", msg.clone()).ok();
            Err(msg)
        }
    }
}

#[tauri::command]
pub async fn start_calibration_monitoring(
    app: AppHandle,
    state: tauri::State<'_, SharedState>,
) -> Result<(), String> {
    {
        let mut st = lock_state(&state)?;
        if st.session.is_some() || st.starting {
            return Err("Already recording".to_string());
        }
        st.starting = true;
    }

    let state_clone = state.inner().clone();
    let app_clone = app.clone();
    let start_result = tokio::task::spawn_blocking(move || {
        pipeline::start_recording_session_ex(
            &app_clone,
            &state_clone,
            "calibration",
            false,
            Some(1.0),
            false,
            true,
        )
    })
    .await;

    {
        let mut st = lock_state(&state)?;
        st.starting = false;
    }

    let start_result = start_result.map_err(|e| format!("Calibration task panicked: {e}"))?;
    start_result.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn stop_calibration_monitoring(
    state: tauri::State<'_, SharedState>,
) -> Result<(), String> {
    let session = {
        let mut st = lock_state(&state)?;
        st.session.take()
    };
    if let Some(s) = session {
        tauri::async_runtime::spawn_blocking(move || {
            let _ = s.stop();
        });
    }
    Ok(())
}

#[tauri::command]
pub async fn stop_and_transcribe_input(
    app: AppHandle,
    state: tauri::State<'_, SharedState>,
) -> Result<String, String> {
    pipeline::transcribe_input_only(app, state.inner().clone())
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn stop_recording(
    app: AppHandle,
    state: tauri::State<'_, SharedState>,
) -> Result<(), String> {
    crate::core::hotkey::set_handless_active(false);
    let session = {
        let mut st = lock_state(&state)?;
        st.handless = false;
        st.session.take()
    };
    if let Some(s) = session {
        let _ = s.stop();
        std::thread::spawn(crate::system::volume::unmute);
    }
    pipeline::hide_pill(&app);
    Ok(())
}

#[tauri::command]
pub async fn stop_handless_mode(
    app: AppHandle,
    state: tauri::State<'_, SharedState>,
) -> Result<(), String> {
    lock_state(&state)?.handless = false;
    tauri::async_runtime::spawn(pipeline::run_pipeline(app, state.inner().clone()));
    Ok(())
}

// ---------- app mappings ----------

#[tauri::command]
pub async fn get_installed_apps() -> Vec<InstalledApp> {
    match tokio::task::spawn_blocking(crate::system::apps::list_installed_apps).await {
        Ok(apps) => apps,
        Err(e) => {
            log::error!("Task to get installed apps panicked: {e}");
            Vec::new()
        }
    }
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
    let value = serde_json::to_value(mappings).map_err(|e| e.to_string())?;
    store.set(store::APP_MAPPINGS, value);
    store.save().map_err(|e| e.to_string())
}

// ---------- snippets ----------

#[tauri::command]
pub async fn get_snippets(app: AppHandle) -> Result<Vec<db::Snippet>, String> {
    let db = app.state::<DbHandle>().inner().clone();
    tokio::task::spawn_blocking(move || db::query_snippets(&db).map_err(|e| e.to_string()))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn create_snippet(
    app: AppHandle,
    trigger: String,
    expansion: String,
    instructions: String,
) -> Result<(), String> {
    let db = app.state::<DbHandle>().inner().clone();
    tokio::task::spawn_blocking(move || {
        db::insert_snippet(&db, &trigger, &expansion, &instructions).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn edit_snippet(
    app: AppHandle,
    id: i64,
    trigger: String,
    expansion: String,
    instructions: String,
) -> Result<(), String> {
    let db = app.state::<DbHandle>().inner().clone();
    tokio::task::spawn_blocking(move || {
        db::update_snippet(&db, id, &trigger, &expansion, &instructions).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn remove_snippet(app: AppHandle, id: i64) -> Result<(), String> {
    let db = app.state::<DbHandle>().inner().clone();
    tokio::task::spawn_blocking(move || db::delete_snippet(&db, id).map_err(|e| e.to_string()))
        .await
        .map_err(|e| e.to_string())?
}

// ---------- dictionary ----------

#[tauri::command]
pub async fn get_dictionary(app: AppHandle) -> Result<Vec<db::DictionaryEntry>, String> {
    let db = app.state::<DbHandle>().inner().clone();
    tokio::task::spawn_blocking(move || db::query_dictionary(&db).map_err(|e| e.to_string()))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn create_dictionary_entry(
    app: AppHandle,
    term: String,
    mistake: Option<String>,
) -> Result<(), String> {
    let db = app.state::<DbHandle>().inner().clone();
    tokio::task::spawn_blocking(move || {
        db::insert_dictionary_entry(&db, &term, mistake.as_deref()).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn edit_dictionary_entry(
    app: AppHandle,
    id: i64,
    term: String,
    mistake: Option<String>,
) -> Result<(), String> {
    let db = app.state::<DbHandle>().inner().clone();
    tokio::task::spawn_blocking(move || {
        db::update_dictionary_entry(&db, id, &term, mistake.as_deref()).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn remove_dictionary_entry(app: AppHandle, id: i64) -> Result<(), String> {
    let db = app.state::<DbHandle>().inner().clone();
    tokio::task::spawn_blocking(move || {
        db::delete_dictionary_entry(&db, id).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn get_auto_learn_status_summary(
    app: AppHandle,
) -> Result<db::AutoLearnStatusSummary, String> {
    let db = app.state::<DbHandle>().inner().clone();
    tokio::task::spawn_blocking(move || {
        db::get_auto_learn_status_summary(&db).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn get_recent_auto_learn_activity(
    app: AppHandle,
    limit: Option<i64>,
) -> Result<Vec<db::AutoLearnEvent>, String> {
    let db = app.state::<DbHandle>().inner().clone();
    tokio::task::spawn_blocking(move || {
        db::get_recent_auto_learn_activity(&db, limit.unwrap_or(20)).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
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
            .map_err(|e| format!("Failed to get executable path: {e}"))?
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
                RegSetValueExW(
                    hkey,
                    PCWSTR(value_name.as_ptr()),
                    None,
                    REG_SZ,
                    Some(std::slice::from_raw_parts(
                        app_path_wide.as_ptr() as *const u8,
                        (app_path_wide.len() - 1) * 2,
                    )),
                )
            } else {
                RegDeleteValueW(hkey, PCWSTR(value_name.as_ptr()))
            };

            let _ = RegCloseKey(hkey);

            if result.is_err() {
                return Err("Failed to set registry value".to_string());
            }
        }
    }

    // macOS: write/remove a LaunchAgent plist that launches the app at login.
    #[cfg(target_os = "macos")]
    {
        let label = "com.openflow.app";
        let domain = format!("gui/{}", unsafe { libc::getuid() });
        let service_target = format!("{domain}/{label}");
        let home = std::env::var("HOME").map_err(|_| "HOME not set".to_string())?;
        let dir = std::path::PathBuf::from(&home).join("Library/LaunchAgents");
        let plist_path = dir.join(format!("{label}.plist"));

        if enabled {
            let app_path = std::env::current_exe()
                .map_err(|e| format!("Failed to get executable path: {e}"))?
                .to_string_lossy()
                .to_string();
            let mut use_open = false;
            let mut target_path = app_path.clone();
            if let Some(index) = app_path.find(".app/Contents/MacOS/") {
                target_path = app_path[..index + 4].to_string();
                use_open = true;
            }

            let escaped_target_path = target_path
                .replace('&', "&amp;")
                .replace('<', "&lt;")
                .replace('>', "&gt;");
            std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
            let plist = if use_open {
                format!(
                    "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
                     <!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
                     <plist version=\"1.0\">\n\
                     <dict>\n\
                       <key>Label</key><string>{label}</string>\n\
                       <key>ProgramArguments</key><array><string>open</string><string>-g</string><string>{escaped_target_path}</string></array>\n\
                       <key>RunAtLoad</key><true/>\n\
                     </dict>\n\
                     </plist>\n"
                )
            } else {
                format!(
                    "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
                     <!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
                     <plist version=\"1.0\">\n\
                     <dict>\n\
                       <key>Label</key><string>{label}</string>\n\
                       <key>ProgramArguments</key><array><string>{escaped_target_path}</string></array>\n\
                       <key>RunAtLoad</key><true/>\n\
                     </dict>\n\
                     </plist>\n"
                )
            };
            std::fs::write(&plist_path, plist).map_err(|e| e.to_string())?;
            let _ = launchctl_bootout(&service_target);
            launchctl_bootstrap(&domain, &plist_path)?;
        } else {
            let _ = launchctl_bootout(&service_target);
            if plist_path.exists() {
                std::fs::remove_file(&plist_path).map_err(|e| e.to_string())?;
            }
        }
    }

    let store = _app.store("settings.json").map_err(|e| e.to_string())?;
    store.set("autostart_enabled", serde_json::json!(enabled));
    store.save().map_err(|e| e.to_string())
}

#[cfg(target_os = "macos")]
fn launchctl_bootstrap(domain: &str, plist_path: &std::path::Path) -> Result<(), String> {
    run_launchctl(&[
        "bootstrap",
        domain,
        plist_path.to_str().ok_or("Invalid plist path")?,
    ])
}

#[cfg(target_os = "macos")]
fn launchctl_bootout(service_target: &str) -> Result<(), String> {
    run_launchctl(&["bootout", service_target])
}

#[cfg(target_os = "macos")]
fn run_launchctl(args: &[&str]) -> Result<(), String> {
    let output = std::process::Command::new("launchctl")
        .args(args)
        .output()
        .map_err(|e| format!("Failed to run launchctl: {e}"))?;

    if output.status.success() {
        return Ok(());
    }

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let detail = if !stderr.is_empty() {
        stderr
    } else if !stdout.is_empty() {
        stdout
    } else {
        format!("exit status {}", output.status)
    };

    Err(format!("launchctl {:?} failed: {detail}", args))
}

// ---------- macOS permissions ----------

/// Whether Open Flow is trusted for the Accessibility API (needed for the global
/// hotkey, Cmd+V injection, and auto-learn). When `prompt` is true, macOS shows
/// the system permission dialog. Always true on non-macOS platforms.
#[cfg(target_os = "macos")]
#[tauri::command]
pub fn check_accessibility_permission(prompt: bool) -> bool {
    use accessibility_sys::{kAXTrustedCheckOptionPrompt, AXIsProcessTrustedWithOptions};
    use core_foundation::base::TCFType;
    use core_foundation::boolean::CFBoolean;
    use core_foundation::dictionary::CFDictionary;
    use core_foundation::string::CFString;

    unsafe {
        let key = CFString::wrap_under_get_rule(kAXTrustedCheckOptionPrompt);
        let value = CFBoolean::from(prompt);
        let options = CFDictionary::from_CFType_pairs(&[(key.as_CFType(), value.as_CFType())]);
        AXIsProcessTrustedWithOptions(options.as_concrete_TypeRef())
    }
}

#[cfg(not(target_os = "macos"))]
#[tauri::command]
pub fn check_accessibility_permission(_prompt: bool) -> bool {
    true
}

/// Opens the macOS Accessibility privacy pane so the user can grant permission.
/// No-op on other platforms.
#[tauri::command]
pub fn open_accessibility_settings() -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility")
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub fn get_accessibility_permission_status() -> String {
    #[cfg(target_os = "macos")]
    {
        if check_accessibility_permission(false) {
            "authorized".to_string()
        } else {
            "needs_permission".to_string()
        }
    }

    #[cfg(not(target_os = "macos"))]
    {
        "authorized".to_string()
    }
}

#[tauri::command]
pub fn get_microphone_permission_status() -> String {
    #[cfg(target_os = "macos")]
    {
        crate::system::mac_app::microphone_permission_status().to_string()
    }

    #[cfg(not(target_os = "macos"))]
    {
        "authorized".to_string()
    }
}

/// Opens the macOS Microphone privacy pane so the user can grant permission.
/// No-op on other platforms.
#[tauri::command]
pub fn open_microphone_settings() -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone")
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

// ---------- updates ----------

#[tauri::command]
pub async fn check_for_update() -> Result<Option<serde_json::Value>, String> {
    match crate::api::updater::check().await {
        Ok(Some(info)) => Ok(Some(serde_json::json!({
            "version": info.version,
            "downloadUrl": info.download_url,
        }))),
        Ok(None) => Ok(None),
        Err(e) => {
            log::warn!("Update check failed: {e}");
            Ok(None)
        }
    }
}

#[tauri::command]
pub async fn install_update(app: AppHandle, download_url: String) -> Result<(), String> {
    #[cfg(not(windows))]
    {
        let _ = (&app, &download_url);
        Err(
            "In-app update is only available on Windows. Download the latest release from GitHub."
                .into(),
        )
    }

    #[cfg(windows)]
    {
        use std::io::Write;
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;

        // Back up the database before touching anything.
        if let Ok(mut db_path) = app.path().app_data_dir() {
            db_path.push("openflow.db");
            if db_path.exists() {
                let _ = std::fs::copy(&db_path, db_path.with_extension("db.bak"));
            }
        }

        let bytes = crate::api::client::get()
            .get(&download_url)
            .header("User-Agent", "open-flow")
            .send()
            .await
            .and_then(|r| r.error_for_status())
            .map_err(|e| e.to_string())?
            .bytes()
            .await
            .map_err(|e| e.to_string())?;

        let installer = std::env::temp_dir().join("open-flow-update.exe");
        let mut f = std::fs::File::create(&installer).map_err(|e| e.to_string())?;
        f.write_all(&bytes).map_err(|e| e.to_string())?;
        drop(f);

        // Batch launcher: waits for this process to exit, runs the installer silently,
        // then relaunches the app. cmd.exe avoids PowerShell execution-policy issues;
        // CREATE_NO_WINDOW suppresses any console flash.
        let current_exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let script = format!(
            "@echo off\r\ntimeout /t 2 /nobreak >nul\r\n\"{}\" /S\r\nstart \"\" \"{}\"\r\n",
            installer.display(),
            current_exe.display()
        );
        let script_path = std::env::temp_dir().join("open-flow-updater.cmd");
        std::fs::write(&script_path, &script).map_err(|e| e.to_string())?;

        std::process::Command::new("cmd")
            .arg("/c")
            .arg(&script_path)
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .map_err(|e| e.to_string())?;

        // Exit immediately so the binary is free before the installer starts.
        std::process::exit(0)
    }
}

// ---------- connectivity ----------

#[tauri::command]
pub async fn check_connectivity() -> bool {
    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(3))
        .build()
    {
        Ok(c) => c,
        Err(_) => return false,
    };
    client.head("https://www.google.com").send().await.is_ok()
}

// ---------- developer logs ----------

#[tauri::command]
pub fn get_recent_logs(limit: Option<usize>) -> Vec<String> {
    crate::system::logger::recent(limit)
}

#[tauri::command]
pub async fn download_logs(app: AppHandle) -> Result<String, String> {
    tokio::task::spawn_blocking(move || crate::system::logger::export_to_downloads(&app))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn set_dev_logging_enabled(enabled: bool) {
    crate::system::logger::set_verbose(enabled);
}
