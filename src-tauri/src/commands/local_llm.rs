use super::*;
use tauri_plugin_shell::ShellExt;

#[tauri::command]
pub async fn list_local_llm_models(
    manager: tauri::State<'_, crate::local_llm::LocalLlmManager>,
) -> Result<Vec<crate::local_llm::LocalLlmModelInfo>, String> {
    manager.list_models().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn download_local_llm_model(
    app: AppHandle,
    manager: tauri::State<'_, crate::local_llm::LocalLlmManager>,
    model_id: String,
) -> Result<(), String> {
    manager.download_model(&app, &model_id).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn cancel_local_llm_model_download(
    manager: tauri::State<'_, crate::local_llm::LocalLlmManager>,
    model_id: Option<String>,
) -> Result<(), String> {
    manager
        .cancel_download(model_id.as_deref())
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn delete_local_llm_model(
    app: AppHandle,
    manager: tauri::State<'_, crate::local_llm::LocalLlmManager>,
    model_id: String,
) -> Result<(), String> {
    manager.delete_model(&app, &model_id).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn open_local_models_folder(app: AppHandle) -> Result<(), String> {
    let root = crate::local_llm::LocalLlmManager::shared_models_root();
    std::fs::create_dir_all(&root).map_err(|e| e.to_string())?;
    #[allow(deprecated)]
    app.shell()
        .open(root.to_string_lossy().to_string(), None)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_local_llm_state(
    manager: tauri::State<'_, crate::local_llm::LocalLlmManager>,
) -> Result<crate::local_llm::LocalLlmState, String> {
    Ok(manager.state())
}

#[tauri::command]
pub async fn get_local_llm_runtime_info(
    manager: tauri::State<'_, crate::local_llm::LocalLlmManager>,
) -> Result<crate::local_llm::LocalLlmRuntimeInfo, String> {
    Ok(manager.runtime_info())
}

#[tauri::command]
pub async fn download_local_llm_runtime(
    app: AppHandle,
    manager: tauri::State<'_, crate::local_llm::LocalLlmManager>,
) -> Result<(), String> {
    manager.download_runtime(&app).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn cancel_local_llm_runtime_download(
    manager: tauri::State<'_, crate::local_llm::LocalLlmManager>,
) -> Result<(), String> {
    manager.cancel_runtime_download().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn delete_local_llm_runtime(
    app: AppHandle,
    manager: tauri::State<'_, crate::local_llm::LocalLlmManager>,
) -> Result<(), String> {
    manager.delete_runtime(&app).map_err(|e| e.to_string())
}
