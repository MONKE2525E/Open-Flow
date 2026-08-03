use super::model::LocalSttModelManifest;
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};

#[derive(Clone, Debug, serde::Serialize)]
pub struct LocalSttDownloadProgressPayload {
    pub model_id: String,
    pub downloaded_bytes: u64,
    pub total_bytes: Option<u64>,
    pub progress: f32,
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct LocalSttModelEventPayload {
    pub model_id: String,
    pub error: Option<String>,
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct LocalSttExtractionProgressPayload {
    pub model_id: String,
    pub progress: f32,
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct LocalSttVerificationProgressPayload {
    pub model_id: String,
    pub progress: f32,
}

/// Reports extraction progress as the fraction of the compressed archive's
/// bytes read so far. The archive is read sequentially front-to-back by the
/// gzip/tar decoders, so this tracks wall-clock progress closely even though
/// the unpacked output is larger than the compressed input.
struct ProgressReader<'a, R> {
    inner: R,
    app: &'a AppHandle,
    model_id: &'a str,
    read_bytes: u64,
    total_bytes: u64,
    last_emit: Instant,
    last_log: Instant,
    cancel: &'a AtomicBool,
}

impl<'a, R: Read> Read for ProgressReader<'a, R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        // `tar::Archive::unpack()` is a single blocking call with no
        // cancellation hook of its own — without this check, clicking Cancel
        // mid-extraction had no effect until the unpack finished or errored
        // on its own (confirmed via logs: ~10 Cancel clicks over 8s did
        // nothing while a 650MB entry was mid-read). Checking here means
        // cancellation takes effect within one read buffer (~128KB), not just
        // between download/extraction phases.
        //
        // MUST NOT use ErrorKind::Interrupted: that kind specifically tells
        // io::copy (which tar's entry extraction uses internally) to retry
        // the read immediately rather than propagate — since this check runs
        // before any real I/O, that turned into an infinite zero-throughput
        // CPU-spin loop on cancellation (confirmed: 0 MB/s/0 Mbps in Task
        // Manager while a core pegged at 100%). ErrorKind::Other is never
        // auto-retried by std's io combinators.
        if self.cancel.load(Ordering::Relaxed) {
            return Err(std::io::Error::other("download cancelled"));
        }
        let n = self.inner.read(buf)?;
        self.read_bytes += n as u64;
        if n > 0 && self.last_emit.elapsed() >= Duration::from_millis(150) {
            let progress = if self.total_bytes == 0 {
                0.0
            } else {
                (self.read_bytes as f32 / self.total_bytes as f32).clamp(0.0, 1.0)
            };
            let _ = self.app.emit(
                "local-stt-model-extraction-progress",
                LocalSttExtractionProgressPayload {
                    model_id: self.model_id.to_string(),
                    progress,
                },
            );
            self.last_emit = Instant::now();
            if self.last_log.elapsed() >= Duration::from_secs(2) {
                log::info!(
                    "local-stt: extracting id={} progress={:.1}% read_bytes={} archive_bytes={}",
                    self.model_id,
                    progress * 100.0,
                    self.read_bytes,
                    self.total_bytes
                );
                self.last_log = Instant::now();
            }
        }
        Ok(n)
    }
}

fn emit_progress(
    app: &AppHandle,
    model_id: &str,
    downloaded_bytes: u64,
    total_bytes: Option<u64>,
) {
    let progress = total_bytes
        .map(|total| {
            if total == 0 {
                0.0
            } else {
                (downloaded_bytes as f32 / total as f32).clamp(0.0, 1.0)
            }
        })
        .unwrap_or(0.0);
    let _ = app.emit(
        "local-stt-model-download-progress",
        LocalSttDownloadProgressPayload {
            model_id: model_id.to_string(),
            downloaded_bytes,
            total_bytes,
            progress,
        },
    );
}

fn emit_model_event(app: &AppHandle, event: &str, model_id: &str, error: Option<String>) {
    let _ = app.emit(
        event,
        LocalSttModelEventPayload {
            model_id: model_id.to_string(),
            error,
        },
    );
}

fn ensure_not_cancelled(cancel: &AtomicBool) -> anyhow::Result<()> {
    if cancel.load(Ordering::Relaxed) {
        anyhow::bail!("download cancelled")
    }
    Ok(())
}

fn verify_checksum_if_present(
    app: &AppHandle,
    manifest: &LocalSttModelManifest,
    archive_path: &Path,
    cancel: &AtomicBool,
) -> anyhow::Result<()> {
    log::info!(
        "local-stt: verification begin id={} checksum_present={}",
        manifest.id,
        manifest.sha256.is_some()
    );
    let started_at = Instant::now();
    emit_model_event(
        app,
        "local-stt-model-verification-started",
        manifest.id,
        None,
    );
    if let Some(expected) = manifest.sha256 {
        let mut file = std::fs::File::open(archive_path)?;
        // Verification hashes the whole (often multi-hundred-MB) archive and
        // can take tens of seconds on a large model, so emit a real fraction
        // as we go instead of letting the bar sit frozen at the download's
        // 100%. Reuses the archive size as the denominator; the hash reads it
        // front-to-back, so bytes-hashed tracks wall-clock progress closely.
        let total_bytes = file.metadata().map(|meta| meta.len()).unwrap_or(0);
        let mut hashed_bytes: u64 = 0;
        let mut last_emit = Instant::now()
            .checked_sub(Duration::from_secs(1))
            .unwrap_or_else(Instant::now);
        let mut hasher = Sha256::new();
        let mut buffer = [0u8; 1024 * 128];
        loop {
            ensure_not_cancelled(cancel)?;
            let read = file.read(&mut buffer)?;
            if read == 0 {
                break;
            }
            hasher.update(&buffer[..read]);
            hashed_bytes += read as u64;
            if total_bytes > 0 && last_emit.elapsed() >= Duration::from_millis(150) {
                let progress = (hashed_bytes as f32 / total_bytes as f32).clamp(0.0, 1.0);
                let _ = app.emit(
                    "local-stt-model-verification-progress",
                    LocalSttVerificationProgressPayload {
                        model_id: manifest.id.to_string(),
                        progress,
                    },
                );
                last_emit = Instant::now();
            }
        }
        let _ = app.emit(
            "local-stt-model-verification-progress",
            LocalSttVerificationProgressPayload {
                model_id: manifest.id.to_string(),
                progress: 1.0,
            },
        );
        let actual = format!("{:x}", hasher.finalize());
        if actual != expected.to_lowercase() {
            log::error!(
                "local-stt: checksum mismatch id={} expected={} actual={}",
                manifest.id,
                expected,
                actual
            );
            // Otherwise the corrupted file stays on disk at its full size; a
            // retry's range-resume request would then get back a 416 (Range
            // Not Satisfiable, since there's nothing left to fetch), read
            // that as "already complete", skip straight to verification, and
            // fail this exact same check forever with no way to recover.
            let _ = std::fs::remove_file(archive_path);
            anyhow::bail!(
                "checksum mismatch for {}: expected {}, got {}",
                manifest.id,
                expected,
                actual
            );
        }
    }
    log::info!(
        "local-stt: verification complete id={} elapsed_ms={}",
        manifest.id,
        started_at.elapsed().as_millis()
    );
    emit_model_event(
        app,
        "local-stt-model-verification-completed",
        manifest.id,
        None,
    );
    Ok(())
}

/// True for macOS's AppleDouble sidecar files (`._filename`) and
/// `.DS_Store`, which `tar` faithfully extracts as ordinary top-level
/// entries. These archives are packed on macOS (confirmed via
/// `blob.handy.computer` archive listings), so every extraction gets them
/// alongside the real payload.
fn is_macos_extraction_junk(name: &std::ffi::OsStr) -> bool {
    let name = name.to_string_lossy();
    name.starts_with("._") || name == ".DS_Store"
}

fn remove_macos_extraction_junk(dir: &Path) -> anyhow::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        if is_macos_extraction_junk(&entry.file_name()) {
            if entry.file_type()?.is_dir() {
                std::fs::remove_dir_all(entry.path())?;
            } else {
                std::fs::remove_file(entry.path())?;
            }
        }
    }
    Ok(())
}

/// Archives commonly wrap their contents in a single top-level folder
/// (e.g. `parakeet-tdt-0.6b-v3-int8/encoder-model.int8.onnx`). Model loaders
/// expect the files directly inside the model directory, so hoist that
/// folder's contents up one level when present.
///
/// The single-entry check below only works if AppleDouble junk (see
/// `is_macos_extraction_junk`) is stripped first — the cohere archive wraps
/// its payload in one real folder plus a `._<folder>` sidecar, which without
/// this stripping counts as 2 top-level entries and silently skips
/// flattening, leaving the model files a directory level too deep for
/// `CohereModel::load` to find (observed live as a spurious "model not
/// found" error immediately after a from-scratch download completed).
fn flatten_single_nested_dir(dir: &Path) -> anyhow::Result<()> {
    remove_macos_extraction_junk(dir)?;
    let mut entries: Vec<std::fs::DirEntry> = std::fs::read_dir(dir)?.collect::<Result<_, _>>()?;
    if entries.len() != 1 {
        log::debug!(
            "local-stt: extraction layout already flat ({} top-level entries)",
            entries.len()
        );
        return Ok(());
    }
    let nested = entries.remove(0).path();
    if !nested.is_dir() {
        return Ok(());
    }
    log::info!(
        "local-stt: flattening archive-wrapped folder {:?}",
        nested.file_name().unwrap_or_default()
    );
    remove_macos_extraction_junk(&nested)?;
    for child in std::fs::read_dir(&nested)? {
        let child = child?;
        std::fs::rename(child.path(), dir.join(child.file_name()))?;
    }
    std::fs::remove_dir(&nested)?;
    Ok(())
}

fn extract_archive(
    app: &AppHandle,
    manifest: &LocalSttModelManifest,
    archive_path: &Path,
    root: &Path,
    cancel: &AtomicBool,
) -> anyhow::Result<()> {
    let extracting_dir = manifest.extracting_path(root);
    let final_dir = manifest.final_path(root);
    let started_at = Instant::now();
    let _ = std::fs::remove_dir_all(&extracting_dir);
    log::info!(
        "local-stt: extraction begin id={} archive={:?} extracting_dir={:?}",
        manifest.id,
        archive_path.file_name().unwrap_or_default(),
        extracting_dir.file_name().unwrap_or_default()
    );
    emit_model_event(
        app,
        "local-stt-model-extraction-started",
        manifest.id,
        None,
    );
    std::fs::create_dir_all(&extracting_dir)?;
    ensure_not_cancelled(cancel)?;

    {
        let file = std::fs::File::open(archive_path)?;
        let archive_len = file.metadata().map(|meta| meta.len()).unwrap_or(0);
        let reader = ProgressReader {
            inner: file,
            app,
            model_id: manifest.id,
            read_bytes: 0,
            total_bytes: archive_len,
            last_emit: Instant::now()
                .checked_sub(Duration::from_secs(1))
                .unwrap_or_else(Instant::now),
            last_log: Instant::now() - Duration::from_secs(2),
            cancel,
        };
        let decoder = flate2::read::GzDecoder::new(reader);
        let mut archive = tar::Archive::new(decoder);
        archive.unpack(&extracting_dir)?;
    }
    log::info!(
        "local-stt: tar unpack complete id={} elapsed_ms={}",
        manifest.id,
        started_at.elapsed().as_millis()
    );
    flatten_single_nested_dir(&extracting_dir)?;

    ensure_not_cancelled(cancel)?;
    if final_dir.exists() {
        if final_dir.is_dir() {
            std::fs::remove_dir_all(&final_dir)?;
        } else {
            std::fs::remove_file(&final_dir)?;
        }
    }
    std::fs::rename(&extracting_dir, &final_dir)?;
    log::info!(
        "local-stt: extraction complete id={} elapsed_ms={}",
        manifest.id,
        started_at.elapsed().as_millis()
    );
    emit_model_event(
        app,
        "local-stt-model-extraction-completed",
        manifest.id,
        None,
    );
    Ok(())
}

pub fn cleanup_incomplete_artifacts(root: &Path) {
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
            continue;
        };
        if name.ends_with(".extracting") {
            let _ = std::fs::remove_dir_all(&path);
        }
    }
}

fn download_client() -> &'static reqwest::Client {
    static CLIENT: std::sync::OnceLock<reqwest::Client> = std::sync::OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .user_agent("Verenu/0.15.0")
            .build()
            .expect("local model download client")
    })
}

pub async fn download_model(
    app: &AppHandle,
    manifest: &LocalSttModelManifest,
    root: &Path,
    cancel: Arc<AtomicBool>,
) -> anyhow::Result<()> {
    let overall_started_at = Instant::now();
    let url = manifest
        .url
        .ok_or_else(|| anyhow::anyhow!("{} does not have a download URL", manifest.name))?;
    std::fs::create_dir_all(root)?;
    cleanup_incomplete_artifacts(root);

    let partial_path = manifest.partial_download_path(root);
    let mut partial_size = std::fs::metadata(&partial_path)
        .map(|meta| meta.len())
        .unwrap_or(0);
    if partial_size > 0 {
        log::info!(
            "local-stt: found existing partial download id={} partial_bytes={}",
            manifest.id,
            partial_size
        );
    }

    let mut already_complete = false;
    let mut resumed = false;
    let mut response: Option<reqwest::Response> = None;

    if partial_size > 0 {
        let probe = download_client()
            .get(url)
            .header(reqwest::header::RANGE, format!("bytes={partial_size}-"))
            .send()
            .await?;
        match probe.status() {
            reqwest::StatusCode::PARTIAL_CONTENT => {
                resumed = true;
                response = Some(probe);
            }
            reqwest::StatusCode::RANGE_NOT_SATISFIABLE => {
                // Per RFC 7233, 416 here means partial_size is already >= the
                // remote resource's full length — there's nothing left to
                // range over. This happens when a prior attempt finished the
                // byte-download but never got to delete the .partial file
                // (e.g. crashed or got stuck mid-extraction) — confirmed via
                // logs: requesting `bytes=478517071-` on a 478517071-byte
                // resource correctly 416s, and the old code treated that as
                // a hard failure instead of "already fully downloaded."
                log::info!(
                    "local-stt: partial download id={} already covers the full resource (416), skipping byte download",
                    manifest.id
                );
                already_complete = true;
            }
            reqwest::StatusCode::OK => {
                log::warn!(
                    "local-stt: server did not honor range resume id={}, restarting from 0",
                    manifest.id
                );
                let _ = std::fs::remove_file(&partial_path);
                partial_size = 0;
                response = Some(probe);
            }
            _ => {
                response = Some(probe.error_for_status()?);
            }
        }
    } else {
        response = Some(download_client().get(url).send().await?.error_for_status()?);
    }

    let total_bytes = if already_complete {
        Some(partial_size)
    } else {
        response
            .as_ref()
            .and_then(|r| r.content_length())
            .map(|value| value + partial_size)
    };
    log::info!(
        "local-stt: download negotiated id={} resumed={} already_complete={} start_bytes={} total_bytes={:?}",
        manifest.id,
        resumed,
        already_complete,
        partial_size,
        total_bytes
    );

    let mut downloaded_bytes = partial_size;

    if let Some(total) = total_bytes {
        let required_additional = total.saturating_sub(downloaded_bytes);
        if required_additional > 0 {
            if let Ok(free_bytes) = crate::system::memory::free_bytes_for_path(root) {
                if free_bytes < required_additional {
                    let required_mb = required_additional / (1024 * 1024);
                    let free_mb = free_bytes / (1024 * 1024);
                    anyhow::bail!(
                        "Not enough disk space to download model. Required: {} MB, Available: {} MB",
                        required_mb,
                        free_mb
                    );
                }
            }
        }
    }

    let mut last_emit = Instant::now()
        .checked_sub(Duration::from_secs(1))
        .unwrap_or_else(Instant::now);
    let mut last_log = Instant::now() - Duration::from_secs(2);
    emit_progress(app, manifest.id, downloaded_bytes, total_bytes);

    if !already_complete {
        let mut response = response.expect("response is set whenever already_complete is false");
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(partial_size > 0)
            .write(true)
            .truncate(partial_size == 0)
            .open(&partial_path)?;

        loop {
            ensure_not_cancelled(&cancel)?;
            let Some(chunk) = response.chunk().await? else {
                break;
            };
            file.write_all(&chunk)?;
            downloaded_bytes += chunk.len() as u64;
            if last_emit.elapsed() >= Duration::from_millis(150) {
                emit_progress(app, manifest.id, downloaded_bytes, total_bytes);
                last_emit = Instant::now();
                if last_log.elapsed() >= Duration::from_secs(2) {
                    let pct = total_bytes
                        .map(|total| (downloaded_bytes as f64 / total as f64) * 100.0)
                        .unwrap_or(0.0);
                    log::info!(
                        "local-stt: downloading id={} progress={:.1}% downloaded_bytes={} total_bytes={:?} elapsed_ms={}",
                        manifest.id,
                        pct,
                        downloaded_bytes,
                        total_bytes,
                        overall_started_at.elapsed().as_millis()
                    );
                    last_log = Instant::now();
                }
            }
        }
        file.flush()?;
        ensure_not_cancelled(&cancel)?;
    }
    emit_progress(app, manifest.id, downloaded_bytes, total_bytes);
    log::info!(
        "local-stt: download bytes complete id={} downloaded_bytes={} elapsed_ms={}",
        manifest.id,
        downloaded_bytes,
        overall_started_at.elapsed().as_millis()
    );

    // Checksum hashing (hundreds of MB) and archive extraction are heavy
    // synchronous CPU/disk work; running them on the Tokio worker thread
    // (this fn is invoked via `tauri::async_runtime::spawn`) would block
    // other async IPC/tasks for as long as they take. spawn_blocking hands
    // them a dedicated thread-pool thread instead.
    {
        let app = app.clone();
        let manifest = manifest.clone();
        let partial_path = partial_path.clone();
        let cancel = cancel.clone();
        tokio::task::spawn_blocking(move || verify_checksum_if_present(&app, &manifest, &partial_path, &cancel))
            .await??;
    }

    if manifest.is_directory {
        {
            let app = app.clone();
            let manifest = manifest.clone();
            let partial_path = partial_path.clone();
            let root = root.to_path_buf();
            let cancel = cancel.clone();
            tokio::task::spawn_blocking(move || extract_archive(&app, &manifest, &partial_path, &root, &cancel))
                .await??;
        }
        let _ = std::fs::remove_file(&partial_path);
    } else {
        let final_path = manifest.final_path(root);
        if final_path.exists() {
            let _ = std::fs::remove_file(&final_path);
        }
        std::fs::rename(&partial_path, &final_path)?;
    }

    // NOTE: the `local-stt-model-download-complete` event is intentionally NOT
    // emitted here. The spawning task in manager.rs emits it *after* clearing
    // the active-download guard, so the frontend's refresh sees is_downloading
    // already false. Emitting it here would resurrect the stuck "Downloading 0%"
    // race (guard still Some at this point).
    log::info!(
        "local-stt: download_model complete id={} total_elapsed_ms={}",
        manifest.id,
        overall_started_at.elapsed().as_millis()
    );
    Ok(())
}

/// Cleans up after a download/extraction that didn't finish. `discard_partial`
/// should be `true` for an explicit user cancellation — the user asked Cancel
/// to mean "stop and throw away what was downloaded," not "pause for resume"
/// — and `false` for an incidental failure (network blip, disk error), where
/// keeping the `.partial` file lets the next attempt resume instead of
/// re-downloading from scratch.
pub fn cleanup_failed_download_artifacts(
    manifest: &LocalSttModelManifest,
    root: &Path,
    discard_partial: bool,
) {
    let _ = std::fs::remove_dir_all(manifest.extracting_path(root));
    if discard_partial {
        let _ = std::fs::remove_file(manifest.partial_download_path(root));
    }
}

#[cfg(test)]
mod tests {
    use super::flatten_single_nested_dir;
    use std::fs;

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("verenu-local-stt-test-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn flattens_archive_wrapped_in_single_top_level_folder() {
        let dir = temp_dir("flatten-nested");
        let nested = dir.join("parakeet-tdt-0.6b-v3-int8");
        fs::create_dir_all(&nested).unwrap();
        fs::write(nested.join("encoder-model.int8.onnx"), b"fake").unwrap();
        fs::write(nested.join("vocab.txt"), b"fake").unwrap();

        flatten_single_nested_dir(&dir).unwrap();

        assert!(dir.join("encoder-model.int8.onnx").is_file());
        assert!(dir.join("vocab.txt").is_file());
        assert!(!nested.exists());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn leaves_already_flat_archives_untouched() {
        let dir = temp_dir("flatten-flat");
        fs::write(dir.join("encoder-model.int8.onnx"), b"fake").unwrap();
        fs::write(dir.join("vocab.txt"), b"fake").unwrap();

        flatten_single_nested_dir(&dir).unwrap();

        assert!(dir.join("encoder-model.int8.onnx").is_file());
        assert!(dir.join("vocab.txt").is_file());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn flattens_a_wrapped_folder_even_with_a_sibling_appledouble_file() {
        // The exact live bug: a real single wrapper folder plus a `._<name>`
        // AppleDouble sidecar makes 2 top-level entries, which used to make
        // the single-entry check bail without flattening.
        let dir = temp_dir("flatten-nested-with-junk");
        let nested = dir.join("cohere-int8");
        fs::create_dir_all(&nested).unwrap();
        fs::write(nested.join("cohere-encoder.int8.onnx"), b"fake").unwrap();
        fs::write(nested.join("tokens.txt"), b"fake").unwrap();
        fs::write(dir.join("._cohere-int8"), b"junk").unwrap();

        flatten_single_nested_dir(&dir).unwrap();

        assert!(dir.join("cohere-encoder.int8.onnx").is_file());
        assert!(dir.join("tokens.txt").is_file());
        assert!(!dir.join("._cohere-int8").exists());
        assert!(!nested.exists());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn drops_appledouble_siblings_inside_the_flattened_folder() {
        let dir = temp_dir("flatten-nested-inner-junk");
        let nested = dir.join("cohere-int8");
        fs::create_dir_all(&nested).unwrap();
        fs::write(nested.join("cohere-encoder.int8.onnx"), b"fake").unwrap();
        fs::write(nested.join("._cohere-encoder.int8.onnx"), b"junk").unwrap();

        flatten_single_nested_dir(&dir).unwrap();

        assert!(dir.join("cohere-encoder.int8.onnx").is_file());
        assert!(!dir.join("._cohere-encoder.int8.onnx").exists());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn drops_top_level_appledouble_junk_in_an_already_flat_archive() {
        let dir = temp_dir("flatten-flat-with-junk");
        fs::write(dir.join("encoder-model.int8.onnx"), b"fake").unwrap();
        fs::write(dir.join("._encoder-model.int8.onnx"), b"junk").unwrap();
        fs::write(dir.join(".DS_Store"), b"junk").unwrap();

        flatten_single_nested_dir(&dir).unwrap();

        assert!(dir.join("encoder-model.int8.onnx").is_file());
        assert!(!dir.join("._encoder-model.int8.onnx").exists());
        assert!(!dir.join(".DS_Store").exists());

        let _ = fs::remove_dir_all(&dir);
    }
}
