use serde::Deserialize;
use std::sync::OnceLock;

#[derive(Deserialize)]
struct GhRelease {
    tag_name: String,
    assets: Vec<GhAsset>,
}

#[derive(Deserialize)]
struct GhAsset {
    name: String,
    browser_download_url: String,
}

pub struct UpdateInfo {
    pub version: String,
    pub download_url: String,
}

/// Repos to check for releases. The new Verenu repo takes priority once it has
/// releases; the old Open-Flow repo is kept as a fallback during the rename.
///
/// TRANSITION(verenu): drop the "MONKE2525E/Open-Flow" entry once all users are
/// on >=0.12.1. See Agent-Skills/Verenu_Transition_Cleanup.md
const RELEASE_REPOS: &[&str] = &["MONKE2525E/Verenu", "MONKE2525E/Open-Flow"];

/// Check all configured repos for a release newer than the current version,
/// returning the highest version found. A 404 (repo has no releases yet, or
/// doesn't exist) or other request error from a single repo is treated as "no
/// release" rather than failing the whole check.
pub async fn check() -> anyhow::Result<Option<UpdateInfo>> {
    let mut best: Option<UpdateInfo> = None;

    for repo in RELEASE_REPOS {
        match check_repo(repo).await {
            Ok(Some(info)) => {
                let is_better = match &best {
                    Some(current_best) => is_newer(&info.version, &current_best.version),
                    None => true,
                };
                if is_better {
                    best = Some(info);
                }
            }
            Ok(None) => {}
            Err(e) => log::warn!("Update check against {repo} failed: {e}"),
        }
    }

    Ok(best)
}

async fn check_repo(repo: &str) -> anyhow::Result<Option<UpdateInfo>> {
    let url = format!("https://api.github.com/repos/{repo}/releases/latest");
    let resp = super::client::get()
        .get(&url)
        .header("User-Agent", "verenu")
        .send()
        .await?;

    if resp.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(None);
    }
    let resp = resp.error_for_status()?;

    let release: GhRelease = resp.json().await?;
    let display_version = normalize_version(&release.tag_name);

    if !is_newer(&display_version, env!("CARGO_PKG_VERSION")) {
        return Ok(None);
    }

    #[cfg(windows)]
    let suffix = ".exe";
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    let suffix = "Apple_Silicon.dmg";
    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
    let suffix = "Intel.dmg";
    #[cfg(not(any(windows, target_os = "macos")))]
    let suffix = ".tar.gz";

    let asset = release
        .assets
        .iter()
        .find(|a| a.name.ends_with(suffix))
        .ok_or_else(|| anyhow::anyhow!("No matching update asset ({suffix}) in release"))?;

    Ok(Some(UpdateInfo {
        version: display_version,
        download_url: asset.browser_download_url.clone(),
    }))
}

/// Repos to check, in priority order, for the About page "Source" link.
///
/// TRANSITION(verenu): once the GitHub repo rename to Verenu is confirmed
/// stable and all users are on >=0.12.1, hardcode "MONKE2525E/Verenu" in
/// AboutSection.svelte, remove this constant, `resolve_source_repo()`, and
/// the `get_source_repo` command. See Agent-Skills/Verenu_Transition_Cleanup.md
const SOURCE_REPO_CANDIDATES: &[&str] = &["MONKE2525E/Verenu", "MONKE2525E/Open-Flow"];

/// Cache for `resolve_source_repo()` so the GitHub API is only queried once
/// per app run, regardless of how many times the About page mounts.
static RESOLVED_SOURCE_REPO: OnceLock<String> = OnceLock::new();

/// Resolve which repo to display/link as the project's "Source" by checking
/// each candidate in order and returning the first that doesn't 404. Falls
/// back to the last candidate (the current default) if every check fails.
/// The result is cached for the lifetime of the process.
pub async fn resolve_source_repo() -> String {
    if let Some(repo) = RESOLVED_SOURCE_REPO.get() {
        return repo.clone();
    }

    let resolved = resolve_source_repo_uncached().await;
    let _ = RESOLVED_SOURCE_REPO.set(resolved.clone());
    resolved
}

async fn resolve_source_repo_uncached() -> String {
    for repo in SOURCE_REPO_CANDIDATES {
        let url = format!("https://api.github.com/repos/{repo}");
        let exists = super::client::get()
            .get(&url)
            .header("User-Agent", "verenu")
            .send()
            .await
            .map(|resp| resp.status() != reqwest::StatusCode::NOT_FOUND)
            .unwrap_or(false);
        if exists {
            return (*repo).to_string();
        }
    }
    SOURCE_REPO_CANDIDATES.last().unwrap().to_string()
}

/// Extract the first three numeric groups from any version string, return as "major.minor.patch".
/// Handles tags like "vOpen-Flow-0.5.0-beta", "v1.2.3", "0.5.0", etc.
fn normalize_version(tag: &str) -> String {
    let parts: Vec<u32> = tag
        .split(|c: char| !c.is_ascii_digit())
        .filter_map(|s| if s.is_empty() { None } else { s.parse().ok() })
        .take(3)
        .collect();

    match parts.as_slice() {
        [a, b, c] => format!("{a}.{b}.{c}"),
        [a, b] => format!("{a}.{b}.0"),
        [a] => format!("{a}.0.0"),
        _ => tag.trim_start_matches('v').to_owned(),
    }
}

fn is_newer(latest: &str, current: &str) -> bool {
    version_tuple(&normalize_version(latest)) > version_tuple(&normalize_version(current))
}

fn version_tuple(version: &str) -> (u64, u64, u64) {
    let mut parts = version.split('.').filter_map(|p| p.parse::<u64>().ok());
    (
        parts.next().unwrap_or(0),
        parts.next().unwrap_or(0),
        parts.next().unwrap_or(0),
    )
}
