use super::download::{
    cleanup_failed_download_artifacts, download_model, LocalSttModelEventPayload,
};
use super::engine::{load_engine, LoadedLocalSttEngine};
use super::model::{built_in_model_manifests, manifest_by_id, LocalSttModelInfo};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter};

#[derive(Clone)]
pub struct LocalTranscriptionManager {
    pub engine: Arc<Mutex<Option<LoadedLocalSttEngine>>>,
    pub current_model_id: Arc<Mutex<Option<String>>>,
    pub last_activity_ms: Arc<AtomicU64>,
    pub is_loading: Arc<Mutex<bool>>,
    pub loading_condvar: Arc<Condvar>,
    pub shutdown_signal: Arc<AtomicBool>,
    pub recording_active: Arc<AtomicBool>,
    download_task: Arc<Mutex<Option<DownloadTaskState>>>,
}

#[derive(Clone)]
struct DownloadTaskState {
    model_id: String,
    cancel: Arc<AtomicBool>,
    completion: Arc<tokio::sync::Notify>,
}

/// Guarantees `is_loading` is cleared and waiters are woken even if
/// `load_model_inner` panics mid-load — without this, a panic during load
/// (unwind, not the release profile's panic=abort) leaves `is_loading` stuck
/// `true` forever, and every future `ensure_loaded()` call deadlocks
/// permanently waiting on a notify that will never come.
struct LoadSlotGuard {
    is_loading: Arc<Mutex<bool>>,
    loading_condvar: Arc<Condvar>,
}

impl Drop for LoadSlotGuard {
    fn drop(&mut self) {
        if let Ok(mut loading) = self.is_loading.lock() {
            *loading = false;
        }
        self.loading_condvar.notify_all();
    }
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct LocalTranscriptionState {
    pub current_model_id: Option<String>,
    pub is_loaded: bool,
    pub is_loading: bool,
    pub is_downloading: bool,
    pub downloading_model_id: Option<String>,
}

impl LocalTranscriptionManager {
    pub fn new() -> Self {
        Self {
            engine: Arc::new(Mutex::new(None)),
            current_model_id: Arc::new(Mutex::new(None)),
            last_activity_ms: Arc::new(AtomicU64::new(now_ms())),
            is_loading: Arc::new(Mutex::new(false)),
            loading_condvar: Arc::new(Condvar::new()),
            shutdown_signal: Arc::new(AtomicBool::new(false)),
            recording_active: Arc::new(AtomicBool::new(false)),
            download_task: Arc::new(Mutex::new(None)),
        }
    }

    pub fn models_root() -> PathBuf {
        crate::app_data_dir().join("models").join("stt")
    }

    /// Ensures the models directory exists. Deliberately does NOT clean up
    /// stale `.extracting` artifacts here — this is called from read paths
    /// like `list_models()`/`state()` refreshes that can run concurrently
    /// with an active extraction, and deleting that directory out from under
    /// an in-progress `tar` unpack corrupts it (confirmed via logs: rapid
    /// Cancel-button clicks during extraction, each triggering a refresh,
    /// deleted the live `.extracting` dir and crashed the unpack with
    /// "the system cannot find the file specified"). Stale-leftover cleanup
    /// from a previous crashed session happens exactly once, inside
    /// `download::download_model()`, which only runs while this manager's
    /// `download_task` guard is held — i.e. provably no concurrent extraction
    /// can be using that directory at that point.
    pub fn prepare_models_dir(&self) -> anyhow::Result<PathBuf> {
        let root = Self::models_root();
        std::fs::create_dir_all(&root)?;
        Ok(root)
    }

    pub fn list_models(&self) -> anyhow::Result<Vec<LocalSttModelInfo>> {
        let root = self.prepare_models_dir()?;
        let downloading_model_id = self
            .download_task
            .lock()
            .map_err(|_| anyhow::anyhow!("download state lock poisoned"))?
            .as_ref()
            .map(|state| state.model_id.clone());
        let models: Vec<LocalSttModelInfo> = built_in_model_manifests()
            .into_iter()
            .map(|manifest| {
                manifest.to_info(&root, downloading_model_id.as_deref() == Some(manifest.id))
            })
            .collect();
        log::debug!(
            "local-stt: list_models downloading_model_id={:?} downloaded=[{}]",
            downloading_model_id,
            models
                .iter()
                .filter(|m| m.is_downloaded)
                .map(|m| m.id.as_str())
                .collect::<Vec<_>>()
                .join(",")
        );
        Ok(models)
    }

    pub fn state(&self) -> LocalTranscriptionState {
        let current_model_id = self
            .current_model_id
            .lock()
            .ok()
            .and_then(|guard| guard.clone());
        let is_loaded = current_model_id.is_some();
        let is_loading = self.is_loading.lock().map(|guard| *guard).unwrap_or(false);
        let download_state = self
            .download_task
            .lock()
            .ok()
            .and_then(|guard| guard.clone());
        let state = LocalTranscriptionState {
            current_model_id,
            is_loaded,
            is_loading,
            is_downloading: download_state.is_some(),
            downloading_model_id: download_state.map(|state| state.model_id),
        };
        log::debug!(
            "local-stt: state current_model={:?} is_loaded={} is_loading={} is_downloading={} downloading={:?}",
            state.current_model_id,
            state.is_loaded,
            state.is_loading,
            state.is_downloading,
            state.downloading_model_id
        );
        state
    }

    pub fn touch_activity(&self) {
        self.last_activity_ms.store(now_ms(), Ordering::Relaxed);
    }

    pub fn set_recording_active(&self, active: bool) {
        self.recording_active.store(active, Ordering::Relaxed);
        if active {
            self.touch_activity();
        }
    }

    pub fn download_model(&self, app: &AppHandle, model_id: &str) -> anyhow::Result<()> {
        let manifest = manifest_by_id(model_id)
            .ok_or_else(|| anyhow::anyhow!("unknown local model: {model_id}"))?;
        let url = manifest.url.ok_or_else(|| {
            anyhow::anyhow!("{} cannot be downloaded automatically", manifest.name)
        })?;
        let root = self.prepare_models_dir()?;

        // The model is already fully installed (e.g. the frontend's cached
        // state went stale after a prior download finished while nobody was
        // listening). Re-running the full download would silently overwrite
        // a working installation, so just resync the frontend instead.
        if manifest.is_downloaded(&root) {
            log::info!(
                "local-stt: download requested for already-installed model id={model_id}, skipping and resyncing state"
            );
            let _ = app.emit(
                "local-stt-model-download-complete",
                LocalSttModelEventPayload {
                    model_id: model_id.to_string(),
                    error: None,
                },
            );
            return Ok(());
        }

        let cancel = Arc::new(AtomicBool::new(false));
        let completion = Arc::new(tokio::sync::Notify::new());

        {
            let mut guard = self
                .download_task
                .lock()
                .map_err(|_| anyhow::anyhow!("download state lock poisoned"))?;
            if let Some(active) = guard.as_ref() {
                if active.model_id == model_id {
                    log::info!(
                        "local-stt: download already in progress for id={model_id}, ignoring duplicate request"
                    );
                    return Ok(());
                }
                log::warn!(
                    "local-stt: refused download id={model_id}, another download is active id={}",
                    active.model_id
                );
                anyhow::bail!("another local model download is already running")
            }
            *guard = Some(DownloadTaskState {
                model_id: model_id.to_string(),
                cancel: Arc::clone(&cancel),
                completion: Arc::clone(&completion),
            });
        }

        log::info!("local-stt: download begin id={} url={}", model_id, url);
        let started_at = Instant::now();
        let manager = self.clone();
        let app_handle = app.clone();
        tauri::async_runtime::spawn(async move {
            let result = download_model(&app_handle, &manifest, &root, Arc::clone(&cancel)).await;

            // Clear the active-download guard BEFORE emitting any terminal event.
            // The frontend's completion/failure handler immediately refreshes via
            // list_local_stt_models + get_local_transcription_state, both of which
            // derive `is_downloading` from this guard. If we emitted the event
            // first and cleared the guard second, that refresh would race the
            // clear and read is_downloading=true for an already-finished download,
            // leaving the UI stuck on "Downloading 0%" until some later refresh.
            if let Ok(mut guard) = manager.download_task.lock() {
                *guard = None;
            }
            completion.notify_waiters();
            log::info!(
                "local-stt: download task cleared active-download guard id={}",
                manifest.id
            );

            match &result {
                Ok(()) => {
                    log::info!(
                        "local-stt: download task finished id={} elapsed_ms={}",
                        manifest.id,
                        started_at.elapsed().as_millis()
                    );
                    let _ = app_handle.emit(
                        "local-stt-model-download-complete",
                        LocalSttModelEventPayload {
                            model_id: manifest.id.to_string(),
                            error: None,
                        },
                    );
                    crate::system::notify::notify_model_download_complete(
                        &app_handle,
                        manifest.name,
                    );
                }
                Err(err) => {
                    let was_cancelled = cancel.load(Ordering::Relaxed);
                    log::error!(
                        "local-stt: download task failed id={} elapsed_ms={} was_cancelled={} error={err:?}",
                        manifest.id,
                        started_at.elapsed().as_millis(),
                        was_cancelled
                    );
                    // An explicit user cancellation should leave nothing behind to
                    // resume from — the next Download click starts fully fresh.
                    // An incidental failure (network blip, disk error) keeps the
                    // partial file so the next attempt can resume instead of
                    // re-downloading from scratch.
                    cleanup_failed_download_artifacts(&manifest, &root, was_cancelled);

                    if !was_cancelled {
                        let _ = app_handle.emit(
                            "verenu:error",
                            format!("Failed to download local model: {}", err),
                        );
                    }

                    let _ = app_handle.emit(
                        "local-stt-model-download-failed",
                        LocalSttModelEventPayload {
                            model_id: manifest.id.to_string(),
                            error: Some(err.to_string()),
                        },
                    );
                }
            }
        });
        Ok(())
    }

    pub fn cancel_download(&self, model_id: Option<&str>) -> anyhow::Result<()> {
        let guard = self
            .download_task
            .lock()
            .map_err(|_| anyhow::anyhow!("download state lock poisoned"))?;
        let Some(active) = guard.as_ref() else {
            log::info!("local-stt: cancel requested id={model_id:?} but no download is active");
            return Ok(());
        };
        if model_id.is_none_or(|value| value == active.model_id) {
            log::info!(
                "local-stt: cancelling active download id={}",
                active.model_id
            );
            active.cancel.store(true, Ordering::Relaxed);
        } else {
            log::info!(
                "local-stt: cancel requested id={model_id:?} did not match active download id={}",
                active.model_id
            );
        }
        Ok(())
    }

    pub async fn delete_model(&self, app: &AppHandle, model_id: &str) -> anyhow::Result<()> {
        log::info!("local-stt: delete requested id={model_id}");
        let manifest = manifest_by_id(model_id)
            .ok_or_else(|| anyhow::anyhow!("unknown local model: {model_id}"))?;
        let root = self.prepare_models_dir()?;
        self.cancel_download(Some(model_id))?;
        let completion = self.download_task.lock().ok().and_then(|guard| {
            guard
                .as_ref()
                .filter(|task| task.model_id == model_id)
                .map(|task| Arc::clone(&task.completion))
        });
        if let Some(completion) = completion {
            loop {
                let notified = completion.notified();
                let still_active = self
                    .download_task
                    .lock()
                    .ok()
                    .and_then(|guard| guard.as_ref().map(|task| task.model_id == model_id))
                    .unwrap_or(false);
                if !still_active {
                    break;
                }
                notified.await;
            }
        }
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
            if final_path.is_dir() {
                remove_with_retry(|| std::fs::remove_dir_all(&final_path))?;
            } else {
                remove_with_retry(|| std::fs::remove_file(&final_path))?;
            }
        }
        let _ = remove_with_retry(|| std::fs::remove_file(manifest.partial_download_path(&root)));
        let _ = remove_with_retry(|| std::fs::remove_dir_all(manifest.extracting_path(&root)));
        log::info!("local-stt: delete complete id={model_id}");
        let _ = app.emit(
            "local-stt-model-deleted",
            LocalSttModelEventPayload {
                model_id: model_id.to_string(),
                error: None,
            },
        );
        Ok(())
    }

    pub fn transcribe_blocking(
        &self,
        app: &AppHandle,
        model_id: &str,
        samples: &[f32],
        sample_rate: u32,
        language: &str,
    ) -> anyhow::Result<String> {
        self.ensure_loaded(app, model_id)?;
        self.touch_activity();
        let mut guard = self
            .engine
            .lock()
            .map_err(|_| anyhow::anyhow!("local engine lock poisoned"))?;
        let engine = guard
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("local model is not loaded"))?;
        let text = engine.transcribe(samples, sample_rate, language)?;
        self.touch_activity();
        Ok(text)
    }

    pub fn unload_if_idle(&self, app: &AppHandle) -> anyhow::Result<()> {
        if self.recording_active.load(Ordering::Relaxed) {
            return Ok(());
        }
        // Skip entirely once nothing is loaded — this is polled every 30s for
        // the app's whole lifetime (see main.rs), so without this check, the
        // "unloading idle model" log below fired every single tick forever
        // once idle crossed the threshold, even long after the one real
        // unload had already happened (observed live: 10+ straight minutes of
        // identical log lines, `unload()` itself was already a safe no-op via
        // its own `had_model` check, but this caller had no equivalent guard).
        let is_loaded = self
            .current_model_id
            .lock()
            .map(|guard| guard.is_some())
            .unwrap_or(false);
        if !is_loaded {
            return Ok(());
        }
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
            log::info!(
                "local-stt: unloading idle model policy={} idle_for_ms={}",
                cfg.local_model_memory_policy,
                idle_for.as_millis()
            );
            self.unload(app);
        }
        Ok(())
    }

    pub fn unload(&self, app: &AppHandle) {
        if let Ok(mut guard) = self.engine.lock() {
            *guard = None;
        }
        if let Ok(mut guard) = self.current_model_id.lock() {
            let had_model = guard.is_some();
            let unloaded_id = guard.clone();
            *guard = None;
            if had_model {
                log::info!("local-stt: model unloaded id={unloaded_id:?}");
                let _ = app.emit(
                    "local-stt-model-state",
                    serde_json::json!({ "status": "unloaded" }),
                );
            }
        }
    }

    /// Unconditional safety unload triggered by system RAM/VRAM pressure —
    /// unlike `unload_if_idle`, this ignores the configured memory policy
    /// (including "keep loaded") since freeing memory for whatever else
    /// needs it takes priority over a passive idle preference. Still skips
    /// while a recording is actively in progress.
    pub fn unload_for_resource_pressure(&self, app: &AppHandle) {
        if self.recording_active.load(Ordering::Relaxed) {
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

    fn ensure_loaded(&self, app: &AppHandle, model_id: &str) -> anyhow::Result<()> {
        if self.is_model_loaded(model_id)? {
            return Ok(());
        }

        let mut loading = self
            .is_loading
            .lock()
            .map_err(|_| anyhow::anyhow!("local loading lock poisoned"))?;
        while *loading {
            loading = self
                .loading_condvar
                .wait(loading)
                .map_err(|_| anyhow::anyhow!("local loading wait poisoned"))?;
        }
        // Re-check after the wait loop (not inside it) — if `*loading` was
        // already false the moment this fn acquired the lock (a concurrent
        // load finished between the top-of-fn check and here), the loop body
        // above never runs, so this is the only place that recheck happens.
        // Without it, that race unconditionally falls through to a redundant
        // reload of a model another caller just finished loading.
        if self.is_model_loaded(model_id)? {
            return Ok(());
        }
        *loading = true;
        drop(loading);

        let _slot_guard = LoadSlotGuard {
            is_loading: self.is_loading.clone(),
            loading_condvar: self.loading_condvar.clone(),
        };
        self.load_model_inner(app, model_id)
    }

    /// Lock the engine first, matching `unload` and `load_model_inner`.
    /// This prevents a model load and the periodic idle reaper from waiting
    /// forever on each other's state lock.
    fn is_model_loaded(&self, model_id: &str) -> anyhow::Result<bool> {
        let engine = self
            .engine
            .lock()
            .map_err(|_| anyhow::anyhow!("local engine lock poisoned"))?;
        let current_model_id = self
            .current_model_id
            .lock()
            .map_err(|_| anyhow::anyhow!("local model id lock poisoned"))?;
        Ok(engine.is_some() && current_model_id.as_deref() == Some(model_id))
    }

    fn load_model_inner(&self, app: &AppHandle, model_id: &str) -> anyhow::Result<()> {
        let manifest = manifest_by_id(model_id)
            .ok_or_else(|| anyhow::anyhow!("unknown local model: {model_id}"))?;
        let root = self.prepare_models_dir()?;
        if !manifest.is_downloaded(&root) {
            log::warn!("local-stt: load requested but model not downloaded id={model_id}");
            anyhow::bail!("Download the selected local model.")
        }
        log::info!(
            "local-stt: loading model id={model_id} engine={:?}",
            manifest.engine_type
        );
        let started_at = Instant::now();
        let _ = app.emit(
            "local-stt-model-state",
            serde_json::json!({ "status": "loading_started", "model_id": model_id }),
        );
        let engine = load_engine(&manifest, &manifest.final_path(&root));
        match engine {
            Ok(engine) => {
                if let Ok(mut guard) = self.engine.lock() {
                    *guard = Some(engine);
                }
                if let Ok(mut guard) = self.current_model_id.lock() {
                    *guard = Some(model_id.to_string());
                }
                self.touch_activity();
                log::info!(
                    "local-stt: model loaded id={model_id} elapsed_ms={}",
                    started_at.elapsed().as_millis()
                );
                let _ = app.emit(
                    "local-stt-model-state",
                    serde_json::json!({ "status": "loading_completed", "model_id": model_id }),
                );
                Ok(())
            }
            Err(err) => {
                log::error!(
                    "local-stt: model load failed id={model_id} elapsed_ms={} error={err:?}",
                    started_at.elapsed().as_millis()
                );
                let _ = app.emit(
                    "local-stt-model-state",
                    serde_json::json!({
                        "status": "loading_failed",
                        "model_id": model_id,
                        "error": err.to_string()
                    }),
                );
                Err(err)
            }
        }
    }
}

/// Retries a filesystem removal briefly before giving up. `delete_model`
/// only signals cancellation to the background download/extraction task via
/// an atomic flag (no handle to await its actual exit), so a task that's
/// mid-write when deletion is requested may still hold open file handles for
/// a moment — on Windows especially, that turns straight into an
/// access-denied error. A short retry window covers the gap between the
/// cancel flag being seen and the task actually unwinding and dropping its
/// handles.
fn remove_with_retry(mut remove: impl FnMut() -> std::io::Result<()>) -> std::io::Result<()> {
    let mut last_err = None;
    for attempt in 0..10 {
        if attempt > 0 {
            std::thread::sleep(Duration::from_millis(100));
        }
        match remove() {
            Ok(()) => return Ok(()),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(err) => last_err = Some(err),
        }
    }
    Err(last_err.expect("loop runs at least once"))
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_millis() as u64)
        .unwrap_or(0)
}
