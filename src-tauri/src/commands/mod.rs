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
        | store::HISTORY_RETENTION
        | store::UPDATE_DISMISSED_VERSION => value.is_string() || value.is_null(),
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
        | store::AUTOSTART_ENABLED => value.is_boolean(),
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
pub async fn save_api_key(app: AppHandle, provider: String, key: String) -> Result<(), String> {
    tokio::task::spawn_blocking(move || crate::data::credentials::save(&app, &provider, &key))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn delete_api_key(app: AppHandle, provider: String) -> Result<(), String> {
    tokio::task::spawn_blocking(move || crate::data::credentials::delete_saved(&app, &provider))
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
    pub transcription_provider: Option<String>,
    pub transcription_model: Option<String>,
    pub transcription_language: Option<String>,
    pub cleanup_provider: Option<String>,
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
    pub update_dismissed_version: Option<String>,
    pub hotkey: Option<Vec<String>>,
    pub appearance_mode: Option<String>,
}

#[derive(serde::Serialize)]
pub struct CleanupCacheStatus {
    pub entry_count: i64,
    pub is_space_constrained: bool,
    pub free_bytes: u64,
}

// ---------- import / export ----------

// Stats are included in the backup for informational reference only; they derive
// from transcription history which is not backed up and cannot be restored.
#[derive(serde::Serialize, serde::Deserialize, Default)]
pub struct ExportStats {
    pub total_words: i64,
    pub avg_wpm: f64,
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct ExportDictionaryEntry {
    pub term: String,
    pub mistake: Option<String>,
    pub auto_learned: bool,
    pub confidence_tier: String,
    pub correction_count: i64,
    pub created_at: String,
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct ExportSnippet {
    pub trigger: String,
    pub expansion: String,
    pub instructions: String,
    pub created_at: String,
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct ExportPayload {
    pub version: String,
    pub app_version: String,
    pub exported_at: String,
    #[serde(default, skip_deserializing)]
    pub stats: ExportStats,
    pub settings: serde_json::Value,
    #[serde(default)]
    pub dictionary: Vec<ExportDictionaryEntry>,
    #[serde(default)]
    pub snippets: Vec<ExportSnippet>,
}

#[derive(serde::Serialize)]
pub struct ImportSummary {
    pub settings_applied: usize,
    pub settings_skipped: usize,
    pub dictionary_inserted: usize,
    pub dictionary_skipped: usize,
    pub dictionary_already_existed: usize,
    pub snippets_inserted: usize,
    pub snippets_skipped: usize,
    pub snippets_already_existed: usize,
}

// Keys intentionally absent from this list (validated by validate_setting but never exported):
//   MIC_GAIN            — device-specific calibration
//   MICROPHONE_DEVICE   — device-specific hardware identifier
//   SETUP_COMPLETE / FORCE_SETUP_ON_LAUNCH — one-time setup flags
//   UPDATE_DISMISSED_VERSION — transient UI state
// When adding a new setting to validate_setting, decide here whether it should also be exported.
const EXPORTABLE_SETTINGS: &[&str] = &[
    store::TRANSCRIPTION_PROVIDER,
    store::TRANSCRIPTION_MODEL,
    store::TRANSCRIPTION_LANGUAGE,
    store::TRANSCRIPTION_DEFAULT_MODEL,
    store::TRANSCRIPTION_MODELS_BY_PROVIDER,
    store::TRANSCRIPTION_FALLBACK_MODELS,
    store::CLEANUP_PROVIDER,
    store::CLEANUP_MODEL,
    store::CLEANUP_DEFAULT_MODEL,
    store::CLEANUP_MODELS_BY_PROVIDER,
    store::CLEANUP_FALLBACK_MODELS,
    store::CLEANUP_ENABLED,
    store::CLEANUP_INTENSITY,
    store::DEFAULT_TONE,
    store::APPEARANCE_MODE,
    store::ADVANCED_MODEL_UI,
    store::HOTKEY,
    store::HISTORY_RETENTION,
    store::NOISE_REDUCTION,
    store::MUTE_AUDIO,
    store::APP_CONTEXT_HINT,
    store::AUTO_LEARN_ENABLED,
    store::AUTO_LEARN_EVENT_MODE,
    store::CONTEXTUAL_CAPS,
    store::AUTO_SPACING,
    store::AUTOSTART_ENABLED,
    store::APP_MAPPINGS,
];

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
        transcription_provider: str_val(store::TRANSCRIPTION_PROVIDER),
        transcription_model: str_val(store::TRANSCRIPTION_MODEL),
        transcription_language: str_val(store::TRANSCRIPTION_LANGUAGE),
        cleanup_provider: str_val(store::CLEANUP_PROVIDER),
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
        autostart_enabled: bool_val(store::AUTOSTART_ENABLED),
        app_context_hint: bool_val(store::APP_CONTEXT_HINT),
        auto_learn_enabled: bool_val(store::AUTO_LEARN_ENABLED),
        contextual_caps_enabled: bool_val(store::CONTEXTUAL_CAPS),
        auto_spacing_enabled: bool_val(store::AUTO_SPACING),
        mic_gain: f64_val(store::MIC_GAIN),
        history_retention: str_val(store::HISTORY_RETENTION),
        microphone_device: str_val(store::MICROPHONE_DEVICE),
        update_dismissed_version: str_val(store::UPDATE_DISMISSED_VERSION),
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
            crate::system::mac_app::set_regular_activation_policy_on_main_thread(&app);
            crate::system::mac_app::activate_current_app_on_main_thread(&app);
        }
        w.show().ok();
        w.set_focus().ok();
    }
    Ok(())
}

#[tauri::command]
pub async fn hide_main(app: AppHandle) -> Result<(), String> {
    if let Some(w) = app.get_webview_window("main") {
        #[cfg(target_os = "macos")]
        {
            crate::system::mac_app::set_accessory_activation_policy_on_main_thread(&app);
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
            app.emit("verenu:error", msg.clone()).ok();
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
    start_result.map_err(|e| {
        let msg = e.to_string();
        let msg_lower = msg.to_lowercase();
        // Surface a user-readable message for the most common production failure:
        // microphone permission denied by TCC or missing audio entitlement.

        // OSStatus codes for permission-related errors. See: https://www.osstatus.com/
        const AUDIO_HARDWARE_ILLEGAL_OPERATION_ERROR: &str = "1852797029"; // 'op??' (kAudioHardwareIllegalOperationError)
        const AUDIO_HARDWARE_PERMISSION_DENIED: &str = "1853319013"; // 'nuoe' (kAudioHardwarePermissionDenied)

        if msg_lower.contains("permissiondenied")
            || msg_lower.contains("permission denied")
            || msg.contains(AUDIO_HARDWARE_ILLEGAL_OPERATION_ERROR)
            || msg.contains(AUDIO_HARDWARE_PERMISSION_DENIED)
            || msg_lower.contains("noue") // 'noue' (No User Consent)
            || msg_lower.contains("nuoe")
            || msg_lower.contains("6e6f7565") // 'noue' hex
            || msg_lower.contains("6e756f65") // 'nuoe' hex
            || msg_lower.contains("access denied")
        {
            "Microphone access denied — grant Verenu permission in System Settings → Privacy & Security → Microphone, then try again.".to_string()
        } else {
            msg
        }
    })
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
    tokio::task::spawn_blocking(move || {
        let rows = db::query_snippets(&db).map_err(|e| e.to_string())?;
        if crate::system::logger::is_verbose() {
            log::info!("snippets:get count={}", rows.len());
        }
        Ok(rows)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn create_snippet(
    app: AppHandle,
    trigger: String,
    expansion: String,
    instructions: String,
) -> Result<db::CreatedRecordMeta, String> {
    let db = app.state::<DbHandle>().inner().clone();
    tokio::task::spawn_blocking(move || {
        log::info!(
            "snippets:create trigger_chars={} expansion_chars={} instructions_chars={}",
            trigger.chars().count(),
            expansion.chars().count(),
            instructions.chars().count()
        );
        let created = db::insert_snippet_returning(&db, &trigger, &expansion, &instructions)
            .map_err(|e| {
                log::warn!("snippets:create failed: {e}");
                e.to_string()
            })?;
        log::info!("snippets:create ok id={}", created.id);
        Ok(created)
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
) -> Result<db::CreatedRecordMeta, String> {
    let db = app.state::<DbHandle>().inner().clone();
    tokio::task::spawn_blocking(move || {
        log::info!(
            "dictionary:create term_chars={} mistake_chars={}",
            term.chars().count(),
            mistake.as_deref().map_or(0, |m| m.chars().count())
        );
        db::insert_dictionary_entry_returning(&db, &term, mistake.as_deref()).map_err(|e| {
            log::warn!("dictionary:create failed: {e}");
            e.to_string()
        })
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
    // Debug builds go stale every time they're rebuilt and (on Windows) lack the
    // `windows_subsystem = "windows"` attribute, so registering one for OS startup
    // leaves a stale, console-attached (Windows) or broken (macOS LaunchAgent)
    // binary wired into the user's login items. Bail out before the setting is
    // persisted so the UI doesn't show autostart as enabled when nothing was
    // actually registered.
    if enabled && cfg!(debug_assertions) {
        let app_path = std::env::current_exe()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|_| "<unknown>".to_string());
        log::warn!(
            "set_autostart: refusing to register debug build at {app_path}; \
             enable autostart from an installed release build instead"
        );
        return Err(
            "Autostart registration is disabled in debug builds. Enable it from an installed release build instead.".to_string(),
        );
    }

    #[cfg(target_os = "windows")]
    set_windows_autostart(enabled)?;

    #[cfg(target_os = "macos")]
    set_macos_autostart(&_app, enabled)?;

    let store = _app.store("settings.json").map_err(|e| e.to_string())?;
    store.set(store::AUTOSTART_ENABLED, serde_json::json!(enabled));
    store.save().map_err(|e| e.to_string())
}

#[cfg(target_os = "windows")]
fn set_windows_autostart(enabled: bool) -> Result<(), String> {
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
    let value_name: Vec<u16> = std::ffi::OsStr::new("Verenu")
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
                    app_path_wide.len() * 2,
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

    Ok(())
}

/// TRANSITION(verenu): on Windows, the autostart Run-key entry was previously
/// registered under the value name "OpenFlow", pointing at the old binary. On
/// startup, remove that stale entry and (if autostart is enabled) re-register
/// under "Verenu" with the current exe path. Remove once all users are on
/// >=0.12.1. See Agent-Skills/Verenu_Transition_Cleanup.md
#[cfg(target_os = "windows")]
pub fn migrate_legacy_autostart(autostart_enabled: bool) {
    use std::os::windows::ffi::OsStrExt;
    use windows::core::PCWSTR;
    use windows::Win32::System::Registry::{
        RegCloseKey, RegDeleteValueW, RegOpenKeyExW, RegQueryValueExW, HKEY, HKEY_CURRENT_USER,
        KEY_READ, KEY_WRITE,
    };

    let subkey: Vec<u16> =
        std::ffi::OsStr::new("Software\\Microsoft\\Windows\\CurrentVersion\\Run")
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
    let legacy_value: Vec<u16> = std::ffi::OsStr::new("OpenFlow")
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    let removed = unsafe {
        let mut hkey = HKEY::default();
        if RegOpenKeyExW(
            HKEY_CURRENT_USER,
            PCWSTR(subkey.as_ptr()),
            None,
            KEY_READ | KEY_WRITE,
            std::ptr::addr_of_mut!(hkey),
        )
        .is_err()
        {
            return;
        }

        let exists =
            RegQueryValueExW(hkey, PCWSTR(legacy_value.as_ptr()), None, None, None, None)
                .is_ok();
        let removed = exists && RegDeleteValueW(hkey, PCWSTR(legacy_value.as_ptr())).is_ok();

        let _ = RegCloseKey(hkey);
        removed
    };

    if !removed {
        return;
    }

    log::info!("TRANSITION(verenu): removed legacy OpenFlow autostart entry");
    if autostart_enabled {
        match set_windows_autostart(true) {
            Ok(()) => log::info!("TRANSITION(verenu): re-registered autostart under Verenu"),
            Err(e) => log::warn!(
                "TRANSITION(verenu): failed to re-register autostart under Verenu: {e}"
            ),
        }
    }
}

// macOS: write/remove a LaunchAgent plist that launches the app at login.
#[cfg(target_os = "macos")]
fn set_macos_autostart(app: &AppHandle, enabled: bool) -> Result<(), String> {
    let label = "com.verenu.app";
    let domain = format!("gui/{}", unsafe { libc::getuid() });
    let service_target = format!("{domain}/{label}");
    let home = app
        .path()
        .home_dir()
        .map_err(|e| format!("Failed to get home directory: {e}"))?;
    let dir = home.join("Library/LaunchAgents");
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

    Ok(())
}

/// TRANSITION(verenu): on macOS, the autostart LaunchAgent was previously
/// registered under the label "com.openflow.app". On startup, bootout and
/// remove that stale plist and (if autostart is enabled) re-register under
/// "com.verenu.app". Remove once all users are on >=0.12.1. See
/// Agent-Skills/Verenu_Transition_Cleanup.md
#[cfg(target_os = "macos")]
pub fn migrate_legacy_launch_agent(app: &AppHandle, autostart_enabled: bool) {
    let legacy_label = "com.openflow.app";
    let domain = format!("gui/{}", unsafe { libc::getuid() });
    let legacy_service_target = format!("{domain}/{legacy_label}");
    let Ok(home) = app.path().home_dir() else {
        return;
    };
    let legacy_plist_path = home
        .join("Library/LaunchAgents")
        .join(format!("{legacy_label}.plist"));

    if !legacy_plist_path.exists() {
        return;
    }

    let _ = launchctl_bootout(&legacy_service_target);
    match std::fs::remove_file(&legacy_plist_path) {
        Ok(()) => log::info!("TRANSITION(verenu): removed legacy com.openflow.app LaunchAgent"),
        Err(e) => {
            log::warn!("TRANSITION(verenu): failed to remove legacy LaunchAgent plist: {e}")
        }
    }

    if autostart_enabled {
        match set_macos_autostart(app, true) {
            Ok(()) => log::info!(
                "TRANSITION(verenu): re-registered LaunchAgent under com.verenu.app"
            ),
            Err(e) => log::warn!(
                "TRANSITION(verenu): failed to re-register LaunchAgent under com.verenu.app: {e}"
            ),
        }
    }
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

/// Whether Verenu is trusted for the Accessibility API (needed for the global
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

/// Returns `true` once the macOS CGEventTap has been successfully created and
/// enabled. Useful as a permission signal when `AXIsProcessTrustedWithOptions`
/// returns a stale cached result after the user grants Accessibility access.
/// Always returns `true` on non-macOS platforms.
#[tauri::command]
pub fn is_hotkey_tap_active() -> bool {
    #[cfg(target_os = "macos")]
    {
        crate::core::hotkey::is_tap_active()
    }
    #[cfg(not(target_os = "macos"))]
    {
        true
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

/// Triggers the macOS microphone consent prompt when access is undetermined,
/// then returns the resulting status. Lets the permissions UI request the mic
/// directly instead of waiting for the first recording. No-op off macOS.
#[tauri::command]
pub async fn request_microphone_permission() -> String {
    #[cfg(target_os = "macos")]
    {
        let _ = crate::system::mac_app::request_microphone().await;
        crate::system::mac_app::microphone_permission_status().to_string()
    }
    #[cfg(not(target_os = "macos"))]
    {
        "authorized".to_string()
    }
}

/// Relaunches the app. macOS caches Input Monitoring (and other TCC) decisions
/// for the life of the process, and the global event tap only picks up a newly
/// granted Input Monitoring permission after a restart — so a one-click relaunch
/// is the reliable way to apply permission changes.
#[tauri::command]
pub fn restart_app(handle: tauri::AppHandle) {
    handle.restart();
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

/// Current Input Monitoring permission status (`authorized`, `denied`, or
/// `not_determined`). Required for the global keyboard tap to see keystrokes
/// while other apps are frontmost. Always `authorized` on non-macOS platforms.
#[tauri::command]
pub fn get_input_monitoring_permission_status() -> String {
    #[cfg(target_os = "macos")]
    {
        // If the hotkey has fired from another app, the tap is receiving global
        // input — trust that over IOHIDCheckAccess, which caches a stale value
        // for the life of the process and lies after the user grants access.
        if crate::core::hotkey::has_seen_global_input() {
            return "authorized".to_string();
        }
        crate::system::mac_app::input_monitoring_status().to_string()
    }
    #[cfg(not(target_os = "macos"))]
    {
        "authorized".to_string()
    }
}

/// Requests Input Monitoring access, showing the macOS consent prompt when the
/// permission is undetermined. Returns the resulting status. No-op elsewhere.
#[tauri::command]
pub fn request_input_monitoring_permission() -> String {
    #[cfg(target_os = "macos")]
    {
        let _ = crate::system::mac_app::request_input_monitoring();
        crate::system::mac_app::input_monitoring_status().to_string()
    }
    #[cfg(not(target_os = "macos"))]
    {
        "authorized".to_string()
    }
}

/// Opens the macOS Input Monitoring privacy pane so the user can grant access.
/// No-op on other platforms.
#[tauri::command]
pub fn open_input_monitoring_settings() -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open")
            .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_ListenEvent")
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Reads the stored API key for `provider` from the system credential store to check
/// whether the app has been granted Keychain access. On macOS this triggers the native
/// Keychain dialog if the app hasn't been granted "Always Allow" yet.
/// Returns "authorized" | "not_configured" | "denied".
#[tauri::command]
#[cfg_attr(not(target_os = "macos"), allow(unused_variables))]
pub async fn check_keychain_access(provider: String) -> String {
    #[cfg(target_os = "macos")]
    {
        match tokio::task::spawn_blocking(move || {
            crate::data::credentials::read_for_status(&provider)
        })
        .await
        {
            Ok(Ok(true)) => "authorized".to_string(),
            Ok(Ok(false)) => "not_configured".to_string(),
            _ => "denied".to_string(),
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        "authorized".to_string()
    }
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
#[allow(deprecated)]
pub async fn install_update(app: AppHandle, download_url: String) -> Result<(), String> {
    #[cfg(not(windows))]
    {
        use tauri_plugin_shell::ShellExt;
        app.shell()
            .open(&download_url, None)
            .map_err(|e| format!("Failed to open download link: {e}"))?;
        Ok(())
    }

    #[cfg(windows)]
    {
        use std::io::Write;
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;

        // Back up the database before touching anything.
        if let Ok(mut db_path) = app.path().app_data_dir() {
            db_path.push("verenu.db");
            if db_path.exists() {
                let _ = std::fs::copy(&db_path, db_path.with_extension("db.bak"));
            }
        }

        let bytes = crate::api::client::get()
            .get(&download_url)
            .header("User-Agent", "verenu")
            .send()
            .await
            .and_then(|r| r.error_for_status())
            .map_err(|e| e.to_string())?
            .bytes()
            .await
            .map_err(|e| e.to_string())?;

        let installer = std::env::temp_dir().join("verenu-update.exe");
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
        let script_path = std::env::temp_dir().join("verenu-updater.cmd");
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

// ---------- about / source link ----------

/// TRANSITION(verenu): returns which repo to display/link as "Source" on the
/// About page — checks "MONKE2525E/Verenu" first, falling back to
/// "MONKE2525E/Open-Flow" if it 404s. Remove once all users are on >=0.12.1
/// and hardcode "MONKE2525E/Verenu" in AboutSection.svelte instead.
/// See Agent-Skills/Verenu_Transition_Cleanup.md
#[tauri::command]
pub async fn get_source_repo() -> String {
    crate::api::updater::resolve_source_repo().await
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
pub fn log_frontend(level: String, message: String) {
    match level.as_str() {
        "warn" => log::warn!("fe: {message}"),
        "error" => log::error!("fe: {message}"),
        _ => log::info!("fe: {message}"),
    }
}

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

#[tauri::command]
pub async fn export_data(
    app: AppHandle,
    db: tauri::State<'_, crate::DbHandle>,
) -> Result<String, String> {
    let db = db.inner().clone();
    tokio::task::spawn_blocking(move || {
        let store = app.store("settings.json").map_err(|e| e.to_string())?;
        let mut settings_map = serde_json::Map::new();
        for &key in EXPORTABLE_SETTINGS {
            if let Some(value) = store.get(key) {
                settings_map.insert(key.to_string(), value);
            }
        }

        let stats = db::query_stats(&db).map_err(|e| e.to_string())?;
        let dictionary = db::query_dictionary(&db).map_err(|e| e.to_string())?;
        let snippets = db::query_snippets(&db).map_err(|e| e.to_string())?;

        let now = chrono::Local::now();
        let payload = ExportPayload {
            version: "1".to_string(),
            app_version: env!("CARGO_PKG_VERSION").to_string(),
            exported_at: now.to_rfc3339(),
            stats: ExportStats {
                total_words: stats.total_words,
                avg_wpm: stats.avg_wpm,
            },
            settings: serde_json::Value::Object(settings_map),
            dictionary: dictionary
                .into_iter()
                .map(|e| ExportDictionaryEntry {
                    term: e.term,
                    mistake: e.mistake,
                    auto_learned: e.auto_learned,
                    confidence_tier: e.confidence_tier,
                    correction_count: e.correction_count,
                    created_at: e.created_at,
                })
                .collect(),
            snippets: snippets
                .into_iter()
                .map(|s| ExportSnippet {
                    trigger: s.trigger,
                    expansion: s.expansion,
                    instructions: s.instructions,
                    created_at: s.created_at,
                })
                .collect(),
        };

        let json = serde_json::to_string_pretty(&payload)
            .map_err(|e| format!("Serialization failed: {e}"))?;

        let downloads = app
            .path()
            .download_dir()
            .map_err(|e| format!("Failed to resolve Downloads directory: {e}"))?;
        std::fs::create_dir_all(&downloads)
            .map_err(|e| format!("Failed to create Downloads path: {e}"))?;
        let path = downloads.join(format!("verenu-backup-{}.json", now.format("%Y%m%d-%H%M%S")));
        std::fs::write(&path, json)
            .map_err(|e| format!("Failed to write backup file: {e}"))?;

        log::info!("export_data: wrote {}", path.display());
        Ok(path.display().to_string())
    })
    .await
    .map_err(|e| format!("export_data task panicked: {e}"))?
}

#[tauri::command]
pub async fn import_data(
    app: AppHandle,
    db: tauri::State<'_, crate::DbHandle>,
    json: String,
) -> Result<ImportSummary, String> {
    let db = db.inner().clone();
    tokio::task::spawn_blocking(move || {
        let payload: ExportPayload = serde_json::from_str(&json)
            .map_err(|e| format!("Invalid backup file: {e}"))?;

        if payload.version != "1" {
            return Err(format!(
                "Unsupported backup version '{}'. Only version '1' is supported.",
                payload.version
            ));
        }

        let store = app.store("settings.json").map_err(|e| e.to_string())?;
        let mut settings_applied = 0usize;
        let mut settings_skipped = 0usize;
        let mut appearance_mode_applied = false;

        if !payload.settings.is_object() {
            log::warn!("import_data: 'settings' field is not a JSON object — skipping settings restore");
        }
        if let Some(obj) = payload.settings.as_object() {
            for (key, value) in obj {
                if !EXPORTABLE_SETTINGS.contains(&key.as_str()) {
                    settings_skipped += 1;
                    continue;
                }
                match validate_setting(key, value) {
                    Ok(()) => {
                        store.set(key.clone(), value.clone());
                        if key == store::APPEARANCE_MODE {
                            appearance_mode_applied = true;
                        }
                        settings_applied += 1;
                    }
                    Err(e) => {
                        log::warn!("import_data: skipping invalid setting '{key}': {e}");
                        settings_skipped += 1;
                    }
                }
            }
            store.save().map_err(|e| e.to_string())?;
        }

        if appearance_mode_applied {
            crate::apply_runtime_icons(&app, None);
        }

        let mut dictionary_inserted = 0usize;
        let mut dictionary_skipped = 0usize;
        let mut dictionary_already_existed = 0usize;
        for entry in &payload.dictionary {
            if entry.term.trim().is_empty() {
                dictionary_skipped += 1;
                continue;
            }
            match db::insert_dictionary_entry_from_backup(&db, &entry.term, entry.mistake.as_deref(), entry.auto_learned, &entry.confidence_tier, entry.correction_count) {
                Ok(()) => dictionary_inserted += 1,
                Err(e) => {
                    let msg = e.to_string();
                    if msg.contains("UNIQUE constraint failed") {
                        dictionary_already_existed += 1;
                    } else {
                        log::warn!("import_data: dictionary insert error for '{}': {msg}", entry.term);
                        dictionary_skipped += 1;
                    }
                }
            }
        }

        let mut snippets_inserted = 0usize;
        let mut snippets_skipped = 0usize;
        let mut snippets_already_existed = 0usize;
        for snippet in &payload.snippets {
            if snippet.trigger.trim().is_empty() || snippet.expansion.trim().is_empty() {
                snippets_skipped += 1;
                continue;
            }
            match db::insert_snippet_returning(&db, &snippet.trigger, &snippet.expansion, &snippet.instructions) {
                Ok(_) => snippets_inserted += 1,
                Err(e) => {
                    let msg = e.to_string();
                    if msg.contains("UNIQUE constraint failed") {
                        snippets_already_existed += 1;
                    } else {
                        log::warn!("import_data: snippet insert error for '{}': {msg}", snippet.trigger);
                        snippets_skipped += 1;
                    }
                }
            }
        }

        log::info!(
            "import_data: settings={}/skip={} dict={}/skip={}/existed={} snip={}/skip={}/existed={}",
            settings_applied, settings_skipped,
            dictionary_inserted, dictionary_skipped, dictionary_already_existed,
            snippets_inserted, snippets_skipped, snippets_already_existed,
        );

        Ok(ImportSummary {
            settings_applied,
            settings_skipped,
            dictionary_inserted,
            dictionary_skipped,
            dictionary_already_existed,
            snippets_inserted,
            snippets_skipped,
            snippets_already_existed,
        })
    })
    .await
    .map_err(|e| format!("import_data task panicked: {e}"))?
}

#[cfg(test)]
mod tests {
    use super::validate_setting;
    use serde_json::json;

    #[test]
    fn validate_setting_rejects_unknown_keys() {
        let err = validate_setting("not_a_setting", &json!(true)).expect_err("unknown key");
        assert!(err.contains("Invalid or unsupported setting"));
    }

    #[test]
    fn validate_setting_accepts_provider_model_maps() {
        let value = json!({
            "groq": ["whisper-large-v3-turbo"],
            "openai": ["gpt-4o-transcribe"],
            "google": ["gemini-3.5-flash"]
        });
        assert!(
            validate_setting(crate::data::store::TRANSCRIPTION_MODELS_BY_PROVIDER, &value).is_ok()
        );
    }

    #[test]
    fn validate_setting_rejects_empty_fallback_entries() {
        let value = json!(["groq/whisper-large-v3-turbo", ""]);
        let err = validate_setting(crate::data::store::TRANSCRIPTION_FALLBACK_MODELS, &value)
            .expect_err("empty fallback should fail");
        assert!(err.contains("Invalid or unsupported setting"));
    }

    #[test]
    fn validate_setting_rejects_invalid_language_codes() {
        let err = validate_setting(crate::data::store::TRANSCRIPTION_LANGUAGE, &json!("xx"))
            .expect_err("invalid language should fail");
        assert!(err.contains("Invalid or unsupported setting"));
    }

    #[test]
    fn validate_setting_requires_two_hotkey_parts() {
        let err = validate_setting(crate::data::store::HOTKEY, &json!(["ControlLeft"]))
            .expect_err("single hotkey part should fail");
        assert!(err.contains("Invalid or unsupported setting"));
    }
}
