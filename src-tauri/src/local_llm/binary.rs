use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};

/// Pinned llama.cpp release tag. Asset filenames below must match this
/// tag's release assets exactly — bump deliberately when upgrading.
const LLAMA_CPP_TAG: &str = "b9842";

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LlamaBackend {
    // detect_backend() only constructs these inside a `#[cfg(windows)]`
    // branch, which is stripped entirely from non-Windows builds — so on
    // macOS, dead-code analysis genuinely can't see them get constructed by
    // any reachable path, even though they're real, normal states on
    // Windows. Suppressed only off-Windows so the lint stays meaningful there.
    #[cfg_attr(not(windows), allow(dead_code))]
    Cuda,
    #[cfg_attr(not(windows), allow(dead_code))]
    Vulkan,
    // detect_backend() only constructs these inside a
    // `#[cfg(target_os = "macos")]` branch, which is stripped entirely from
    // non-macOS builds — so on Windows, dead-code analysis genuinely can't
    // see them get constructed by any reachable path, even though they're
    // real, normal states on macOS. Suppressed only off-macOS so the lint
    // stays meaningful there.
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    Metal,
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    Cpu,
}

impl LlamaBackend {
    /// Approximate combined download size, shown to the user before they
    /// confirm the one-time runtime download.
    pub fn approx_download_mb(self) -> u64 {
        match self {
            LlamaBackend::Cuda => 622,
            LlamaBackend::Vulkan => 30,
            LlamaBackend::Metal => 10,
            LlamaBackend::Cpu => 16,
        }
    }

    /// Release asset URLs for this backend, each paired with the SHA256
    /// GitHub computed for that exact asset at upload time (from the
    /// release's `digest` field via the GitHub API — not project-published,
    /// not computed locally, since that would mean downloading every
    /// multi-hundred-MB asset just to populate this table). CUDA ships its
    /// cudart DLLs as a separate archive from the main build; every other
    /// backend is one file.
    fn assets(self) -> Vec<RuntimeAsset> {
        let base = format!(
            "https://github.com/ggml-org/llama.cpp/releases/download/{LLAMA_CPP_TAG}"
        );
        match self {
            LlamaBackend::Cuda => vec![
                RuntimeAsset {
                    url: format!("{base}/llama-{LLAMA_CPP_TAG}-bin-win-cuda-12.4-x64.zip"),
                    sha256: "0bec7fac740b4baa1ce88b51ada8ec301c4319ee71931ab518dd777ba316bbbe",
                },
                RuntimeAsset {
                    url: format!("{base}/cudart-llama-bin-win-cuda-12.4-x64.zip"),
                    sha256: "8c79a9b226de4b3cacfd1f83d24f962d0773be79f1e7b75c6af4ded7e32ae1d6",
                },
            ],
            LlamaBackend::Vulkan => vec![RuntimeAsset {
                url: format!("{base}/llama-{LLAMA_CPP_TAG}-bin-win-vulkan-x64.zip"),
                sha256: "8056f5c2fd8863a9b02719db527edd3c51f16567abb26981de4292d8d797444e",
            }],
            LlamaBackend::Metal => vec![RuntimeAsset {
                url: format!("{base}/llama-{LLAMA_CPP_TAG}-bin-macos-arm64.tar.gz"),
                sha256: "c2903c14b9e0cf60a62fc85b8b8ab379267f5f849b9c6f29c8a4e21d299fa62b",
            }],
            LlamaBackend::Cpu => vec![RuntimeAsset {
                url: format!("{base}/llama-{LLAMA_CPP_TAG}-bin-macos-x64.tar.gz"),
                sha256: "ec167296de6b1e9fd6510c181b6424973515a78dac916d83aad4734b3f89bf2b",
            }],
        }
    }
}

struct RuntimeAsset {
    url: String,
    sha256: &'static str,
}

/// Detect the best backend for this machine. Windows: an NVIDIA GPU (via
/// `nvidia-smi`) gets CUDA for best performance; everything else gets
/// Vulkan, which accelerates AMD/Intel/NVIDIA GPUs without needing a CUDA
/// toolkit install. macOS: Apple Silicon always gets Metal (built into the
/// standard arm64 release); Intel Macs have no realistic GPU path and get CPU.
pub fn detect_backend() -> LlamaBackend {
    #[cfg(windows)]
    {
        if nvidia_gpu_present() {
            LlamaBackend::Cuda
        } else {
            LlamaBackend::Vulkan
        }
    }
    #[cfg(target_os = "macos")]
    {
        if cfg!(target_arch = "aarch64") {
            LlamaBackend::Metal
        } else {
            LlamaBackend::Cpu
        }
    }
    // Not a supported release target (Windows/macOS only, see CLAUDE.md),
    // but this must still return something so the crate type-checks when
    // built or analyzed on other platforms (e.g. a Linux CI/dev machine).
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        LlamaBackend::Cpu
    }
}

#[cfg(windows)]
fn nvidia_gpu_present() -> bool {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x08000000;
    std::process::Command::new("nvidia-smi")
        .arg("--query-gpu=name")
        .arg("--format=csv,noheader")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .creation_flags(CREATE_NO_WINDOW)
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn runtime_binary_name() -> &'static str {
    #[cfg(windows)]
    {
        "llama-server.exe"
    }
    #[cfg(not(windows))]
    {
        "llama-server"
    }
}

pub fn runtime_root() -> PathBuf {
    super::LocalLlmManager::shared_models_root().join("bin")
}

pub fn is_runtime_installed(root: &Path) -> bool {
    root.join(runtime_binary_name()).is_file()
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct LocalLlmRuntimeInfo {
    pub installed: bool,
    pub is_downloading: bool,
    pub backend: LlamaBackend,
    pub approx_download_mb: u64,
}

pub fn runtime_info(is_downloading: bool) -> LocalLlmRuntimeInfo {
    let backend = detect_backend();
    LocalLlmRuntimeInfo {
        installed: is_runtime_installed(&runtime_root()),
        is_downloading,
        backend,
        approx_download_mb: backend.approx_download_mb(),
    }
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct LocalLlmRuntimeDownloadProgressPayload {
    pub downloaded_bytes: u64,
    pub total_bytes: Option<u64>,
    pub progress: f32,
    pub stage: &'static str,
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct LocalLlmRuntimeEventPayload {
    pub error: Option<String>,
}

fn emit_progress(app: &AppHandle, downloaded: u64, total: Option<u64>, stage: &'static str) {
    let progress = total
        .map(|total| {
            if total == 0 {
                0.0
            } else {
                (downloaded as f32 / total as f32).clamp(0.0, 1.0)
            }
        })
        .unwrap_or(0.0);
    let _ = app.emit(
        "local-llm-runtime-download-progress",
        LocalLlmRuntimeDownloadProgressPayload {
            downloaded_bytes: downloaded,
            total_bytes: total,
            progress,
            stage,
        },
    );
}

fn ensure_not_cancelled(cancel: &AtomicBool) -> anyhow::Result<()> {
    if cancel.load(Ordering::Relaxed) {
        anyhow::bail!("download cancelled")
    }
    Ok(())
}

async fn download_to_file(
    app: &AppHandle,
    url: &str,
    dest: &Path,
    cancel: &AtomicBool,
    aggregate_downloaded: &mut u64,
    aggregate_total: Option<u64>,
) -> anyhow::Result<()> {
    ensure_not_cancelled(cancel)?;
    let response = super::download::download_client()
        .get(url)
        .send()
        .await?
        .error_for_status()?;

    // Async file I/O (rather than std::fs + sync write_all) so each chunk
    // write can't block the Tokio executor worker thread during a
    // multi-hundred-MB runtime archive download.
    use tokio::io::AsyncWriteExt;
    let mut file = tokio::fs::File::create(dest).await?;
    let mut response = response;
    let mut last_emit = Instant::now() - Duration::from_secs(1);
    while let Some(chunk) = response.chunk().await? {
        ensure_not_cancelled(cancel)?;
        file.write_all(&chunk).await?;
        *aggregate_downloaded += chunk.len() as u64;
        if last_emit.elapsed() >= Duration::from_millis(150) {
            emit_progress(app, *aggregate_downloaded, aggregate_total, "downloading");
            last_emit = Instant::now();
        }
    }
    file.flush().await?;
    Ok(())
}

#[cfg(windows)]
fn extract_archive(archive_path: &Path, dest: &Path) -> anyhow::Result<()> {
    let file = std::fs::File::open(archive_path)?;
    let mut archive = zip::ZipArchive::new(file)?;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        if entry.is_dir() {
            continue;
        }
        let Some(name) = entry.enclosed_name() else {
            continue;
        };
        let Some(file_name) = name.file_name() else {
            continue;
        };
        let out_path = dest.join(file_name);
        let mut out_file = std::fs::File::create(&out_path)?;
        std::io::copy(&mut entry, &mut out_file)?;
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn extract_archive(archive_path: &Path, dest: &Path) -> anyhow::Result<()> {
    let file = std::fs::File::open(archive_path)?;
    let decoder = flate2::read::GzDecoder::new(file);
    let mut archive = tar::Archive::new(decoder);
    for entry in archive.entries()? {
        let mut entry = entry?;
        let entry_type = entry.header().entry_type();

        if entry_type.is_symlink() {
            let path = entry.path()?.into_owned();
            let Some(file_name) = path.file_name() else {
                continue;
            };
            let out_path = dest.join(file_name);
            if let Some(target) = entry.link_name()? {
                let Some(target_file_name) = target.file_name() else {
                    continue;
                };
                if out_path.exists() {
                    let _ = std::fs::remove_file(&out_path);
                }
                #[cfg(unix)]
                {
                    std::os::unix::fs::symlink(target_file_name, &out_path)?;
                }
            }
            continue;
        }

        if !entry_type.is_file() {
            continue;
        }
        let path = entry.path()?.into_owned();
        let Some(file_name) = path.file_name() else {
            continue;
        };
        let out_path = dest.join(file_name);
        let mut out_file = std::fs::File::create(&out_path)?;
        std::io::copy(&mut entry, &mut out_file)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Ok(mode) = entry.header().mode() {
                let _ = std::fs::set_permissions(&out_path, std::fs::Permissions::from_mode(mode));
            }
        }
    }
    Ok(())
}

pub async fn ensure_llama_server_binary(
    app: &AppHandle,
    cancel: &AtomicBool,
) -> anyhow::Result<PathBuf> {
    let root = runtime_root();
    let binary_path = root.join(runtime_binary_name());

    #[cfg(target_os = "macos")]
    if binary_path.is_file() && !root.join("libllama-common.0.dylib").exists() {
        log::warn!("local-llm: local runtime dynamic library libllama-common.0.dylib not found, forcing repair re-download");
        let _ = std::fs::remove_dir_all(&root);
    }

    if binary_path.is_file() {
        return Ok(binary_path);
    }

    std::fs::create_dir_all(&root)?;
    let backend = detect_backend();
    let assets = backend.assets();

    let tmp_dir = root.join(".download-tmp");
    std::fs::create_dir_all(&tmp_dir)?;

    let mut downloaded = 0u64;
    for (idx, asset) in assets.iter().enumerate() {
        ensure_not_cancelled(cancel)?;
        let archive_path = tmp_dir.join(format!("asset-{idx}.tmp"));
        download_to_file(app, &asset.url, &archive_path, cancel, &mut downloaded, None).await?;

        // Verify before extracting — an unverified archive must never reach
        // the extractor, since the binary inside it (llama-server.exe) gets
        // executed later. A mismatch here means either a corrupted download
        // or a tampered/substituted asset; either way, the .tmp dir (and
        // anything already extracted from a prior asset in this loop) is
        // discarded so no partially-verified install can be used.
        // Hashing (up to ~600MB) and extraction are heavy synchronous CPU/disk
        // work — run them off the Tokio worker thread so they can't stall the
        // async executor (audio capture, hotkey handling, etc.) for seconds
        // at a time.
        let hash_path = archive_path.clone();
        let actual = tokio::task::spawn_blocking(move || super::download::sha256_hex(&hash_path)).await??;
        if actual != asset.sha256 {
            log::error!(
                "local-llm: runtime asset checksum mismatch url={} expected={} actual={}",
                asset.url,
                asset.sha256,
                actual
            );
            let _ = std::fs::remove_dir_all(&tmp_dir);
            let _ = std::fs::remove_dir_all(&root);
            anyhow::bail!("downloaded runtime asset failed checksum verification: {}", asset.url);
        }

        emit_progress(app, downloaded, None, "extracting");
        let extract_archive_path = archive_path.clone();
        let extract_root = root.clone();
        tokio::task::spawn_blocking(move || extract_archive(&extract_archive_path, &extract_root)).await??;
        let _ = std::fs::remove_file(&archive_path);
    }
    let _ = std::fs::remove_dir_all(&tmp_dir);

    if !binary_path.is_file() {
        anyhow::bail!(
            "downloaded {} runtime archive(s) but {} was not present afterward",
            assets.len(),
            runtime_binary_name()
        );
    }

    Ok(binary_path)
}

/// Removes any partial download/extraction leftovers after a cancelled or
/// failed runtime download. There is no resume support for these archives
/// (unlike the resumable GGUF model downloads), so a stale partial is
/// useless — always clean it up rather than leaving it on disk.
pub fn cleanup_failed_runtime_download(root: &Path) {
    let _ = std::fs::remove_dir_all(root.join(".download-tmp"));
}

/// Removes the installed runtime binary and any DLLs that shipped alongside
/// it, for users who want to reclaim the disk space. Safe to call even if
/// nothing is installed.
pub fn delete_runtime(root: &Path) -> anyhow::Result<()> {
    if root.is_dir() {
        std::fs::remove_dir_all(root)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::LlamaBackend;

    #[test]
    fn cuda_ships_main_build_plus_separate_cudart_bundle() {
        let assets = LlamaBackend::Cuda.assets();
        assert_eq!(assets.len(), 2, "CUDA needs the main build and the cudart DLL bundle");
        assert!(assets[0].url.contains("bin-win-cuda"));
        assert!(assets[1].url.contains("cudart-llama-bin-win-cuda"));
    }

    #[test]
    fn vulkan_metal_cpu_ship_a_single_asset() {
        for backend in [LlamaBackend::Vulkan, LlamaBackend::Metal, LlamaBackend::Cpu] {
            let assets = backend.assets();
            assert_eq!(assets.len(), 1, "{backend:?} should ship exactly one archive");
        }
    }

    #[test]
    fn every_backend_targets_the_right_platform_asset() {
        assert!(LlamaBackend::Cuda.assets()[0].url.contains("win-cuda"));
        assert!(LlamaBackend::Vulkan.assets()[0].url.contains("win-vulkan"));
        assert!(LlamaBackend::Metal.assets()[0].url.contains("macos-arm64"));
        assert!(LlamaBackend::Cpu.assets()[0].url.contains("macos-x64"));
    }

    #[test]
    fn all_asset_urls_use_the_same_pinned_release_tag() {
        for backend in [
            LlamaBackend::Cuda,
            LlamaBackend::Vulkan,
            LlamaBackend::Metal,
            LlamaBackend::Cpu,
        ] {
            for asset in backend.assets() {
                assert!(
                    asset.url.contains(super::LLAMA_CPP_TAG),
                    "url {} missing pinned tag {}",
                    asset.url,
                    super::LLAMA_CPP_TAG
                );
            }
        }
    }

    #[test]
    fn every_runtime_asset_has_a_well_formed_sha256() {
        // The runtime binary gets executed (llama-server.exe), so every
        // asset must carry a checksum verified before extraction — and it
        // must actually look like a SHA256 hex digest, not a typo, since a
        // malformed value would make every download fail verification
        // forever.
        for backend in [
            LlamaBackend::Cuda,
            LlamaBackend::Vulkan,
            LlamaBackend::Metal,
            LlamaBackend::Cpu,
        ] {
            for asset in backend.assets() {
                assert_eq!(
                    asset.sha256.len(),
                    64,
                    "{} sha256 is {} chars, expected 64",
                    asset.url,
                    asset.sha256.len()
                );
                assert!(
                    asset.sha256.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()),
                    "{} sha256 is not lowercase hex: {}",
                    asset.url,
                    asset.sha256
                );
            }
        }
    }

    #[test]
    fn cuda_is_the_heaviest_download_estimate() {
        assert!(LlamaBackend::Cuda.approx_download_mb() > LlamaBackend::Vulkan.approx_download_mb());
        assert!(LlamaBackend::Vulkan.approx_download_mb() > LlamaBackend::Metal.approx_download_mb());
    }
}
