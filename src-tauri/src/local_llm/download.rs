use super::model::{LocalLlmArtifact, LocalLlmModelManifest};
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};

#[derive(Clone, Debug, serde::Serialize)]
pub struct LocalLlmDownloadProgressPayload {
    pub model_id: String,
    pub downloaded_bytes: u64,
    pub total_bytes: Option<u64>,
    pub progress: f32,
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct LocalLlmModelEventPayload {
    pub model_id: String,
    pub error: Option<String>,
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct LocalLlmVerificationProgressPayload {
    pub model_id: String,
    pub progress: f32,
}

pub(super) fn download_client() -> &'static reqwest::Client {
    static CLIENT: std::sync::OnceLock<reqwest::Client> = std::sync::OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .user_agent("Verenu/0.15.0")
            .build()
            .expect("local cleanup model download client")
    })
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
        "local-llm-model-download-progress",
        LocalLlmDownloadProgressPayload {
            model_id: model_id.to_string(),
            downloaded_bytes,
            total_bytes,
            progress,
        },
    );
}

fn ensure_not_cancelled(cancel: &AtomicBool) -> anyhow::Result<()> {
    if cancel.load(Ordering::Relaxed) {
        anyhow::bail!("download cancelled")
    }
    Ok(())
}

fn partial_file_path(root: &Path, manifest: &LocalLlmModelManifest, artifact: &LocalLlmArtifact) -> PathBuf {
    manifest
        .partial_download_path(root)
        .join(format!("{}.partial", artifact.filename))
}

fn final_file_path(root: &Path, manifest: &LocalLlmModelManifest, artifact: &LocalLlmArtifact) -> PathBuf {
    manifest.final_path(root).join(artifact.filename)
}

/// Pure SHA256-of-a-file helper, kept separate from `verify_artifact_checksum`
/// so the actual hashing/comparison logic is unit-testable without needing a
/// real `AppHandle` to drive event emission.
pub(super) fn sha256_hex(path: &Path) -> anyhow::Result<String> {
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 1024 * 128];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

/// Same hash as `sha256_hex`, but emits `local-llm-model-verification-progress`
/// as it reads so the UI can show a moving bar during the checksum pass on a
/// multi-hundred-MB weights file (which otherwise leaves the bar frozen at the
/// download's 100%). Progress is per-file; cleanup models are typically a
/// single `.gguf`, so that reads as whole-model progress.
fn sha256_hex_with_progress(
    app: &AppHandle,
    model_id: &str,
    path: &Path,
) -> anyhow::Result<String> {
    let mut file = std::fs::File::open(path)?;
    let total_bytes = file.metadata().map(|meta| meta.len()).unwrap_or(0);
    let mut hashed_bytes: u64 = 0;
    let mut last_emit = Instant::now() - Duration::from_secs(1);
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 1024 * 128];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
        hashed_bytes += read as u64;
        if total_bytes > 0 && last_emit.elapsed() >= Duration::from_millis(150) {
            let progress = (hashed_bytes as f32 / total_bytes as f32).clamp(0.0, 1.0);
            let _ = app.emit(
                "local-llm-model-verification-progress",
                LocalLlmVerificationProgressPayload {
                    model_id: model_id.to_string(),
                    progress,
                },
            );
            last_emit = Instant::now();
        }
    }
    let _ = app.emit(
        "local-llm-model-verification-progress",
        LocalLlmVerificationProgressPayload {
            model_id: model_id.to_string(),
            progress: 1.0,
        },
    );
    Ok(format!("{:x}", hasher.finalize()))
}

/// Verifies a freshly-downloaded artifact's SHA256 against its manifest
/// checksum before it's renamed into the final model directory and made
/// available to llama-server to load and execute. On mismatch, the
/// corrupted `.partial` file is deleted (not left for a future attempt to
/// "resume" from — appending more bytes to already-corrupted data can never
/// fix it, so leaving it in place would make every subsequent download
/// attempt fail identically forever).
fn verify_artifact_checksum(
    app: &AppHandle,
    manifest: &LocalLlmModelManifest,
    artifact: &LocalLlmArtifact,
    partial_path: &Path,
) -> anyhow::Result<()> {
    let Some(expected) = artifact.sha256 else {
        return Ok(());
    };
    log::info!(
        "local-llm: verification begin id={} file={}",
        manifest.id,
        artifact.filename
    );
    let _ = app.emit(
        "local-llm-model-verification-started",
        LocalLlmModelEventPayload {
            model_id: manifest.id.to_string(),
            error: None,
        },
    );
    let started_at = Instant::now();
    let actual = sha256_hex_with_progress(app, manifest.id, partial_path)?;
    if actual != expected {
        log::error!(
            "local-llm: checksum mismatch id={} file={} expected={} actual={}",
            manifest.id,
            artifact.filename,
            expected,
            actual
        );
        let _ = std::fs::remove_file(partial_path);
        anyhow::bail!(
            "downloaded file failed checksum verification: {}",
            artifact.filename
        );
    }
    log::info!(
        "local-llm: verification complete id={} file={} elapsed_ms={}",
        manifest.id,
        artifact.filename,
        started_at.elapsed().as_millis()
    );
    let _ = app.emit(
        "local-llm-model-verification-completed",
        LocalLlmModelEventPayload {
            model_id: manifest.id.to_string(),
            error: None,
        },
    );
    Ok(())
}

pub fn cleanup_failed_download_artifacts(
    manifest: &LocalLlmModelManifest,
    root: &Path,
    was_cancelled: bool,
) {
    if was_cancelled {
        let _ = std::fs::remove_dir_all(manifest.partial_download_path(root));
    }
}

async fn negotiate_total_bytes(
    manifest: &LocalLlmModelManifest,
    root: &Path,
) -> anyhow::Result<Option<u64>> {
    let mut total = 0u64;
    for artifact in manifest.artifacts {
        let url = format!(
            "https://huggingface.co/{}/resolve/main/{}",
            manifest.repo_id, artifact.filename
        );
        let partial = std::fs::metadata(partial_file_path(root, manifest, artifact))
            .map(|meta| meta.len())
            .unwrap_or(0);
        let response = download_client()
            .get(&url)
            .header(reqwest::header::RANGE, format!("bytes={partial}-"))
            .send()
            .await?;
        match response.status() {
            reqwest::StatusCode::PARTIAL_CONTENT | reqwest::StatusCode::OK => {
                if let Some(len) = response.content_length() {
                    total = total.saturating_add(len + partial);
                } else {
                    return Ok(None);
                }
            }
            reqwest::StatusCode::RANGE_NOT_SATISFIABLE => {
                total = total.saturating_add(partial);
            }
            _ => {
                let _ = response.error_for_status()?;
            }
        }
    }
    Ok(Some(total))
}

async fn download_one_artifact(
    app: &AppHandle,
    manifest: &LocalLlmModelManifest,
    artifact: &LocalLlmArtifact,
    root: &Path,
    cancel: &AtomicBool,
    aggregate_downloaded: &mut u64,
    aggregate_total: Option<u64>,
) -> anyhow::Result<()> {
    ensure_not_cancelled(cancel)?;

    let partial_path = partial_file_path(root, manifest, artifact);
    if let Some(parent) = partial_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let partial_size = std::fs::metadata(&partial_path)
        .map(|meta| meta.len())
        .unwrap_or(0);
    let url = format!(
        "https://huggingface.co/{}/resolve/main/{}",
        manifest.repo_id, artifact.filename
    );

    let probe = if partial_size > 0 {
        download_client()
            .get(&url)
            .header(reqwest::header::RANGE, format!("bytes={partial_size}-"))
            .send()
            .await?
    } else {
        download_client().get(&url).send().await?
    };

    let resumed = probe.status() == reqwest::StatusCode::PARTIAL_CONTENT;
    let already_complete = probe.status() == reqwest::StatusCode::RANGE_NOT_SATISFIABLE;
    let mut response = match probe.status() {
        reqwest::StatusCode::PARTIAL_CONTENT | reqwest::StatusCode::OK => Some(probe.error_for_status()?),
        reqwest::StatusCode::RANGE_NOT_SATISFIABLE => None,
        _ => Some(probe.error_for_status()?),
    };

    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .append(partial_size > 0 && resumed)
        .truncate(partial_size == 0 || !resumed)
        .open(&partial_path)?;

    if !resumed && partial_size > 0 {
        *aggregate_downloaded = aggregate_downloaded.saturating_sub(partial_size);
    }

    if already_complete {
        *aggregate_downloaded = aggregate_downloaded.saturating_add(partial_size);
        emit_progress(app, manifest.id, *aggregate_downloaded, aggregate_total);
        return Ok(());
    }

    let mut last_emit = Instant::now() - Duration::from_secs(1);
    let mut response = response
        .take()
        .expect("response exists when artifact is not already complete");

    while let Some(chunk) = response.chunk().await? {
        ensure_not_cancelled(cancel)?;
        file.write_all(&chunk)?;
        *aggregate_downloaded = aggregate_downloaded.saturating_add(chunk.len() as u64);
        if last_emit.elapsed() >= Duration::from_millis(150) {
            emit_progress(app, manifest.id, *aggregate_downloaded, aggregate_total);
            last_emit = Instant::now();
        }
    }
    file.flush()?;
    emit_progress(app, manifest.id, *aggregate_downloaded, aggregate_total);
    Ok(())
}

pub async fn download_model(
    app: &AppHandle,
    manifest: &LocalLlmModelManifest,
    root: &Path,
    cancel: Arc<AtomicBool>,
) -> anyhow::Result<()> {
    std::fs::create_dir_all(root)?;
    let final_dir = manifest.final_path(root);
    if final_dir.exists() {
        if final_dir.is_dir() {
            std::fs::remove_dir_all(&final_dir)?;
        } else {
            std::fs::remove_file(&final_dir)?;
        }
    }

    let total_bytes = negotiate_total_bytes(manifest, root).await?;
    let mut downloaded_bytes = manifest.partial_size(root);

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

    emit_progress(app, manifest.id, downloaded_bytes, total_bytes);

    for artifact in manifest.artifacts {
        download_one_artifact(
            app,
            manifest,
            artifact,
            root,
            &cancel,
            &mut downloaded_bytes,
            total_bytes,
        )
        .await?;
    }

    ensure_not_cancelled(&cancel)?;

    for artifact in manifest.artifacts {
        ensure_not_cancelled(&cancel)?;
        let partial_path = partial_file_path(root, manifest, artifact);
        verify_artifact_checksum(app, manifest, artifact, &partial_path)?;
    }

    let partial_dir = manifest.partial_download_path(root);
    std::fs::create_dir_all(&final_dir)?;
    for artifact in manifest.artifacts {
        let partial_path = partial_file_path(root, manifest, artifact);
        let final_path = final_file_path(root, manifest, artifact);
        if let Some(parent) = final_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::rename(&partial_path, &final_path)?;
    }
    let _ = std::fs::remove_dir_all(partial_dir);
    emit_progress(app, manifest.id, downloaded_bytes, total_bytes);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::sha256_hex;
    use std::io::Write;

    #[test]
    fn sha256_hex_matches_a_known_vector() {
        // SHA256("abc") is a standard published test vector.
        let dir = std::env::temp_dir().join(format!("verenu-sha-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("abc.txt");
        std::fs::File::create(&path).unwrap().write_all(b"abc").unwrap();

        let hash = sha256_hex(&path).unwrap();
        assert_eq!(
            hash,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn sha256_hex_differs_for_different_content() {
        let dir = std::env::temp_dir().join(format!("verenu-sha-test2-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path_a = dir.join("a.txt");
        let path_b = dir.join("b.txt");
        std::fs::File::create(&path_a).unwrap().write_all(b"hello").unwrap();
        std::fs::File::create(&path_b).unwrap().write_all(b"world").unwrap();

        assert_ne!(sha256_hex(&path_a).unwrap(), sha256_hex(&path_b).unwrap());

        let _ = std::fs::remove_dir_all(&dir);
    }
}
