use serde::Deserialize;

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

/// Repo to check for releases.
const RELEASE_REPO: &str = "MONKE2525E/Verenu";

/// Check the configured repo for a release newer than the current version. A
/// 404 (repo has no releases yet) or other request error is treated as "no
/// release" rather than failing the whole check.
pub async fn check() -> anyhow::Result<Option<UpdateInfo>> {
    check_repo(RELEASE_REPO).await
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
    #[cfg(target_os = "macos")]
    let suffix = if cfg!(target_arch = "aarch64") { "Apple_Silicon.dmg" } else { "Intel.dmg" };
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

/// Extract the first three numeric groups from any version string, return as "major.minor.patch".
/// Handles tags like "vVerenu-0.5.0-beta", "v1.2.3", "0.5.0", etc.
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
