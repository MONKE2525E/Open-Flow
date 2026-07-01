use super::binary::{
    ensure_llama_server_binary, LocalLlmRuntimeEventPayload, LocalLlmRuntimeInfo,
};
use super::download::{cleanup_failed_download_artifacts, download_model, LocalLlmModelEventPayload};
use super::model::{built_in_model_manifests, manifest_by_id, LocalLlmModelInfo};
use super::runtime::{request_cleanup, start_server, ManagedLocalLlmServer};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter};

#[derive(Clone)]
pub struct LocalLlmManager {
    server: Arc<Mutex<Option<ManagedLocalLlmServer>>>,
    pub current_model_id: Arc<Mutex<Option<String>>>,
    pub last_activity_ms: Arc<AtomicU64>,
    pub is_loading: Arc<Mutex<bool>>,
    pub loading_condvar: Arc<Condvar>,
    pub shutdown_signal: Arc<AtomicBool>,
    download_task: Arc<Mutex<Option<DownloadTaskState>>>,
    runtime_download: Arc<Mutex<Option<Arc<AtomicBool>>>>,
    // Counts in-flight cleanup_with_prompt calls (load + generate). There is
    // no safe way to cancel an in-flight llama-server request — killing the
    // process mid-generation just surfaces as a connection error to the
    // caller — so resource-pressure unload must wait for this to hit zero
    // rather than interrupting a request the user is actively waiting on.
    active_requests: Arc<AtomicU32>,
}

#[derive(Clone)]
struct DownloadTaskState {
    model_id: String,
    cancel: Arc<AtomicBool>,
}

struct ActiveRequestGuard<'a>(&'a AtomicU32);

impl Drop for ActiveRequestGuard<'_> {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct LocalLlmState {
    pub current_model_id: Option<String>,
    pub is_loaded: bool,
    pub is_loading: bool,
    pub is_downloading: bool,
    pub downloading_model_id: Option<String>,
    pub endpoint: Option<String>,
}

impl LocalLlmManager {
    pub fn new() -> Self {
        Self {
            server: Arc::new(Mutex::new(None)),
            current_model_id: Arc::new(Mutex::new(None)),
            last_activity_ms: Arc::new(AtomicU64::new(now_ms())),
            is_loading: Arc::new(Mutex::new(false)),
            loading_condvar: Arc::new(Condvar::new()),
            shutdown_signal: Arc::new(AtomicBool::new(false)),
            download_task: Arc::new(Mutex::new(None)),
            runtime_download: Arc::new(Mutex::new(None)),
            active_requests: Arc::new(AtomicU32::new(0)),
        }
    }

    pub fn shared_models_root() -> PathBuf {
        crate::app_data_dir().join("models")
    }

    pub fn models_root() -> PathBuf {
        Self::shared_models_root().join("cleanup")
    }

    pub fn prepare_models_dir(&self) -> anyhow::Result<PathBuf> {
        let root = Self::models_root();
        std::fs::create_dir_all(&root)?;
        Ok(root)
    }

    pub fn list_models(&self) -> anyhow::Result<Vec<LocalLlmModelInfo>> {
        let root = self.prepare_models_dir()?;
        let downloading_model_id = self
            .download_task
            .lock()
            .map_err(|_| anyhow::anyhow!("download state lock poisoned"))?
            .as_ref()
            .map(|state| state.model_id.clone());

        Ok(built_in_model_manifests()
            .into_iter()
            .map(|manifest| {
                manifest.to_info(
                    &root,
                    downloading_model_id.as_deref() == Some(manifest.id),
                )
            })
            .collect())
    }

    pub fn state(&self) -> LocalLlmState {
        let current_model_id = self
            .current_model_id
            .lock()
            .ok()
            .and_then(|guard| guard.clone());
        let server_guard = self.server.lock().ok();
        let endpoint = server_guard
            .as_ref()
            .and_then(|guard| guard.as_ref().map(|server| server.endpoint.clone()));
        let is_loaded = server_guard
            .as_ref()
            .is_some_and(|guard| guard.is_some());
        let is_loading = self.is_loading.lock().map(|guard| *guard).unwrap_or(false);
        let download_state = self.download_task.lock().ok().and_then(|guard| guard.clone());
        LocalLlmState {
            current_model_id,
            is_loaded,
            is_loading,
            is_downloading: download_state.is_some(),
            downloading_model_id: download_state.map(|state| state.model_id),
            endpoint,
        }
    }

    pub fn touch_activity(&self) {
        self.last_activity_ms.store(now_ms(), Ordering::Relaxed);
    }

    pub fn download_model(&self, app: &AppHandle, model_id: &str) -> anyhow::Result<()> {
        let manifest = manifest_by_id(model_id)
            .ok_or_else(|| anyhow::anyhow!("unknown local cleanup model: {model_id}"))?;
        let root = self.prepare_models_dir()?;
        if manifest.is_downloaded(&root) {
            let _ = app.emit(
                "local-llm-model-download-complete",
                LocalLlmModelEventPayload {
                    model_id: model_id.to_string(),
                    error: None,
                },
            );
            return Ok(());
        }

        let cancel = Arc::new(AtomicBool::new(false));
        {
            let mut guard = self
                .download_task
                .lock()
                .map_err(|_| anyhow::anyhow!("download state lock poisoned"))?;
            if let Some(active) = guard.as_ref() {
                if active.model_id == model_id {
                    return Ok(());
                }
                anyhow::bail!("another local cleanup model download is already running")
            }
            *guard = Some(DownloadTaskState {
                model_id: model_id.to_string(),
                cancel: Arc::clone(&cancel),
            });
        }

        let manager = self.clone();
        let app_handle = app.clone();
        tauri::async_runtime::spawn(async move {
            let started_at = Instant::now();
            let result = download_model(&app_handle, &manifest, &root, Arc::clone(&cancel)).await;
            if let Ok(mut guard) = manager.download_task.lock() {
                *guard = None;
            }

            match &result {
                Ok(()) => {
                    log::info!(
                        "local-llm: download finished id={} elapsed_ms={}",
                        manifest.id,
                        started_at.elapsed().as_millis()
                    );
                    let _ = app_handle.emit(
                        "local-llm-model-download-complete",
                        LocalLlmModelEventPayload {
                            model_id: manifest.id.to_string(),
                            error: None,
                        },
                    );
                }
                Err(err) => {
                    let was_cancelled = cancel.load(Ordering::Relaxed);
                    cleanup_failed_download_artifacts(&manifest, &root, was_cancelled);
                    let _ = app_handle.emit(
                        "local-llm-model-download-failed",
                        LocalLlmModelEventPayload {
                            model_id: manifest.id.to_string(),
                            error: Some(err.to_string()),
                        },
                    );
                }
            }
        });
        Ok(())
    }

    pub fn runtime_info(&self) -> LocalLlmRuntimeInfo {
        let is_downloading = self
            .runtime_download
            .lock()
            .map(|guard| guard.is_some())
            .unwrap_or(false);
        super::binary::runtime_info(is_downloading)
    }

    pub fn download_runtime(&self, app: &AppHandle) -> anyhow::Result<()> {
        if super::binary::is_runtime_installed(&super::binary::runtime_root()) {
            let _ = app.emit(
                "local-llm-runtime-download-complete",
                LocalLlmRuntimeEventPayload { error: None },
            );
            return Ok(());
        }

        let cancel = Arc::new(AtomicBool::new(false));
        {
            let mut guard = self
                .runtime_download
                .lock()
                .map_err(|_| anyhow::anyhow!("runtime download state lock poisoned"))?;
            if guard.is_some() {
                return Ok(());
            }
            *guard = Some(Arc::clone(&cancel));
        }

        let manager = self.clone();
        let app_handle = app.clone();
        tauri::async_runtime::spawn(async move {
            let started_at = Instant::now();
            let result = ensure_llama_server_binary(&app_handle, &cancel).await;
            if let Ok(mut guard) = manager.runtime_download.lock() {
                *guard = None;
            }

            match &result {
                Ok(_) => {
                    log::info!(
                        "local-llm: runtime download finished elapsed_ms={}",
                        started_at.elapsed().as_millis()
                    );
                    let _ = app_handle.emit(
                        "local-llm-runtime-download-complete",
                        LocalLlmRuntimeEventPayload { error: None },
                    );
                }
                Err(err) => {
                    super::binary::cleanup_failed_runtime_download(&super::binary::runtime_root());
                    let _ = app_handle.emit(
                        "local-llm-runtime-download-failed",
                        LocalLlmRuntimeEventPayload {
                            error: Some(err.to_string()),
                        },
                    );
                }
            }
        });
        Ok(())
    }

    pub fn cancel_runtime_download(&self) -> anyhow::Result<()> {
        let guard = self
            .runtime_download
            .lock()
            .map_err(|_| anyhow::anyhow!("runtime download state lock poisoned"))?;
        if let Some(cancel) = guard.as_ref() {
            cancel.store(true, Ordering::Relaxed);
        }
        Ok(())
    }

    pub fn delete_runtime(&self, app: &AppHandle) -> anyhow::Result<()> {
        self.cancel_runtime_download()?;
        // The runtime binary (llama-server.exe) is a real OS executable —
        // on Windows it's locked while running, so deleting its folder
        // while a model is loaded would fail or partially fail. Stop it
        // first regardless of memory policy, same as the resource-pressure
        // unload path: the user explicitly asked to remove the runtime, so
        // freeing it takes priority over "keep loaded".
        self.unload(app);
        super::binary::delete_runtime(&super::binary::runtime_root())?;
        let _ = app.emit(
            "local-llm-runtime-deleted",
            LocalLlmRuntimeEventPayload { error: None },
        );
        Ok(())
    }

    pub fn cancel_download(&self, model_id: Option<&str>) -> anyhow::Result<()> {
        let guard = self
            .download_task
            .lock()
            .map_err(|_| anyhow::anyhow!("download state lock poisoned"))?;
        let Some(active) = guard.as_ref() else {
            return Ok(());
        };
        if model_id.is_none_or(|value| value == active.model_id) {
            active.cancel.store(true, Ordering::Relaxed);
        }
        Ok(())
    }

    pub fn delete_model(&self, app: &AppHandle, model_id: &str) -> anyhow::Result<()> {
        let manifest = manifest_by_id(model_id)
            .ok_or_else(|| anyhow::anyhow!("unknown local cleanup model: {model_id}"))?;
        let root = self.prepare_models_dir()?;
        self.cancel_download(Some(model_id))?;
        if self
            .current_model_id
            .lock()
            .ok()
            .and_then(|guard| guard.clone())
            .as_deref()
            == Some(model_id)
        {
            self.unload(app);
        }

        let final_path = manifest.final_path(&root);
        if final_path.exists() {
            std::fs::remove_dir_all(&final_path)?;
        }
        let _ = std::fs::remove_dir_all(manifest.partial_download_path(&root));
        let _ = app.emit(
            "local-llm-model-deleted",
            LocalLlmModelEventPayload {
                model_id: model_id.to_string(),
                error: None,
            },
        );
        Ok(())
    }

    pub async fn cleanup_with_prompt(
        &self,
        app: &AppHandle,
        model_id: &str,
        raw_text: &str,
        prompt: &str,
        max_tokens: u32,
    ) -> anyhow::Result<String> {
        self.active_requests.fetch_add(1, Ordering::SeqCst);
        let _busy = ActiveRequestGuard(&self.active_requests);

        self.ensure_loaded(app, model_id).await?;
        self.touch_activity();
        let endpoint = self
            .server
            .lock()
            .map_err(|_| anyhow::anyhow!("local cleanup server lock poisoned"))?
            .as_ref()
            .map(|server| server.endpoint.clone())
            .ok_or_else(|| anyhow::anyhow!("local cleanup model is not loaded"))?;
        let output = request_cleanup(&endpoint, model_id, prompt, raw_text, max_tokens).await;
        self.touch_activity();
        output.map_err(|err| {
            let tail = self
                .server
                .lock()
                .ok()
                .and_then(|guard| guard.as_ref().map(|server| server.recent_log_tail()));
            match tail {
                Some(lines) if !lines.is_empty() => {
                    anyhow::anyhow!("{err}{}", super::runtime::format_log_tail(&lines))
                }
                _ => err,
            }
        })
    }

    pub fn unload_if_idle(&self, app: &AppHandle) -> anyhow::Result<()> {
        let settings = crate::data::store::settings_snapshot(app).map_err(anyhow::Error::msg)?;
        let cfg = crate::data::store::load_pipeline_config(&settings);
        let idle_limit = match cfg.local_model_memory_policy.as_str() {
            "keep_loaded" => return Ok(()),
            "unload_after_15m" => Duration::from_secs(15 * 60),
            "unload_immediately" => Duration::from_secs(0),
            _ => Duration::from_secs(5 * 60),
        };
        let last_ms = self.last_activity_ms.load(Ordering::Relaxed);
        let idle_for = Duration::from_millis(now_ms().saturating_sub(last_ms));
        if idle_for >= idle_limit {
            self.unload(app);
        }
        Ok(())
    }

    pub fn unload(&self, app: &AppHandle) {
        if let Ok(mut guard) = self.server.lock() {
            if let Some(server) = guard.as_mut() {
                let _ = server.stop();
            }
            *guard = None;
        }
        if let Ok(mut guard) = self.current_model_id.lock() {
            let had_model = guard.is_some();
            *guard = None;
            if had_model {
                let _ = app.emit(
                    "local-llm-model-state",
                    serde_json::json!({ "status": "unloaded" }),
                );
            }
        }
    }

    /// Unconditional safety unload triggered by system RAM/VRAM pressure —
    /// unlike `unload_if_idle`, this ignores the configured memory policy
    /// (including "keep loaded") since freeing memory for whatever else
    /// needs it takes priority over a passive idle preference. Skipped
    /// entirely while a cleanup request is in flight: killing the
    /// llama-server process mid-generation has no safe cancellation path,
    /// it just surfaces as a connection error to whoever is waiting on the
    /// result. The next periodic check (30s later) will retry.
    pub fn unload_for_resource_pressure(&self, app: &AppHandle) {
        if self.active_requests.load(Ordering::SeqCst) > 0 {
            log::debug!(
                "local-llm: skipping resource-pressure unload, a cleanup request is in flight"
            );
            return;
        }
        let was_loaded = self
            .current_model_id
            .lock()
            .map(|guard| guard.is_some())
            .unwrap_or(false);
        if was_loaded {
            self.unload(app);
        }
    }

    fn wait_for_load_slot(&self, model_id: &str) -> anyhow::Result<()> {
        let mut loading = self
            .is_loading
            .lock()
            .map_err(|_| anyhow::anyhow!("local cleanup loading lock poisoned"))?;
        while *loading {
            loading = self
                .loading_condvar
                .wait(loading)
                .map_err(|_| anyhow::anyhow!("local cleanup loading wait poisoned"))?;
            let current_model_id = self
                .current_model_id
                .lock()
                .map_err(|_| anyhow::anyhow!("local cleanup model id lock poisoned"))?;
            let already_loaded = self.server.lock().map(|guard| guard.is_some()).unwrap_or(false);
            if current_model_id.as_deref() == Some(model_id) && already_loaded {
                return Ok(());
            }
        }
        *loading = true;
        Ok(())
    }

    fn finish_load_slot(&self) -> anyhow::Result<()> {
        let mut loading = self
            .is_loading
            .lock()
            .map_err(|_| anyhow::anyhow!("local cleanup loading lock poisoned"))?;
        *loading = false;
        self.loading_condvar.notify_all();
        Ok(())
    }

    async fn ensure_loaded(&self, app: &AppHandle, model_id: &str) -> anyhow::Result<()> {
        {
            let current_model_id = self
                .current_model_id
                .lock()
                .map_err(|_| anyhow::anyhow!("local cleanup model id lock poisoned"))?;
            let already_loaded = self.server.lock().map(|guard| guard.is_some()).unwrap_or(false);
            if current_model_id.as_deref() == Some(model_id) && already_loaded {
                return Ok(());
            }
        }
        self.wait_for_load_slot(model_id)?;

        let load_result = self.load_model_inner(app, model_id).await;
        self.finish_load_slot()?;
        load_result
    }

    async fn load_model_inner(&self, app: &AppHandle, model_id: &str) -> anyhow::Result<()> {
        let manifest = manifest_by_id(model_id)
            .ok_or_else(|| anyhow::anyhow!("unknown local cleanup model: {model_id}"))?;
        let root = self.prepare_models_dir()?;
        if !manifest.is_downloaded(&root) {
            anyhow::bail!("Download the selected local cleanup model.")
        }

        self.unload(app);
        let _ = app.emit(
            "local-llm-model-state",
            serde_json::json!({ "status": "loading_started", "model_id": model_id }),
        );
        let server = start_server(app, &manifest, &root).await?;

        if let Ok(mut guard) = self.server.lock() {
            *guard = Some(server);
        }
        if let Ok(mut guard) = self.current_model_id.lock() {
            *guard = Some(model_id.to_string());
        }
        self.touch_activity();
        let _ = app.emit(
            "local-llm-model-state",
            serde_json::json!({ "status": "loaded", "model_id": model_id }),
        );
        Ok(())
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::LocalLlmManager;

    #[test]
    fn local_cleanup_models_root_uses_cleanup_subdir() {
        let path = LocalLlmManager::models_root();
        assert!(path.to_string_lossy().contains("models"));
        assert!(path.to_string_lossy().contains("cleanup"));
    }
}
