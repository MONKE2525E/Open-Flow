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

pub async fn check() -> anyhow::Result<Option<UpdateInfo>> {
    let url = "https://api.github.com/repos/MONKE2525E/Open-Flow/releases/latest";
    let resp = super::client::get()
        .get(url)
        .header("User-Agent", "open-flow")
        .send()
        .await?
        .error_for_status()?;

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
