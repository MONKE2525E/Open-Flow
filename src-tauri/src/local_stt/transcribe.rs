use super::manager::LocalTranscriptionManager;
use std::sync::Arc;
use tauri::AppHandle;

pub async fn transcribe(
    manager: LocalTranscriptionManager,
    app: AppHandle,
    model_id: String,
    samples: Arc<Vec<f32>>,
    sample_rate: u32,
    language: String,
) -> anyhow::Result<String> {
    tokio::task::spawn_blocking(move || {
        manager.transcribe_blocking(&app, &model_id, &samples, sample_rate, &language)
    })
    .await?
}
