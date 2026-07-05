use super::*;
use tauri_plugin_shell::ShellExt;

#[tauri::command]
pub async fn list_local_stt_models(
    app: AppHandle,
    manager: tauri::State<'_, crate::local_stt::LocalTranscriptionManager>,
) -> Result<Vec<crate::local_stt::LocalSttModelInfo>, String> {
    let _ = app;
    manager.list_models().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn download_local_stt_model(
    app: AppHandle,
    manager: tauri::State<'_, crate::local_stt::LocalTranscriptionManager>,
    model_id: String,
) -> Result<(), String> {
    if crate::system::platform::is_macos_intel() {
        return Err(LOCAL_MODELS_UNAVAILABLE_ON_MACOS_INTEL.to_string());
    }
    manager.download_model(&app, &model_id).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn cancel_local_stt_model_download(
    manager: tauri::State<'_, crate::local_stt::LocalTranscriptionManager>,
    model_id: Option<String>,
) -> Result<(), String> {
    manager
        .cancel_download(model_id.as_deref())
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn delete_local_stt_model(
    app: AppHandle,
    manager: tauri::State<'_, crate::local_stt::LocalTranscriptionManager>,
    model_id: String,
) -> Result<(), String> {
    manager.delete_model(&app, &model_id).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn open_local_stt_models_folder(app: AppHandle) -> Result<(), String> {
    let root = crate::local_llm::LocalLlmManager::shared_models_root();
    std::fs::create_dir_all(&root).map_err(|e| e.to_string())?;
    #[allow(deprecated)]
    app.shell()
        .open(root.to_string_lossy().to_string(), None)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_local_transcription_state(
    manager: tauri::State<'_, crate::local_stt::LocalTranscriptionManager>,
) -> Result<crate::local_stt::LocalTranscriptionState, String> {
    Ok(manager.state())
}
