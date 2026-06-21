use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
struct GhRelease {
    tag_name: String,
    assets: Vec<GhAsset>,
}

#[derive(Clone, Debug, Deserialize)]
struct GhAsset {
    name: String,
    browser_download_url: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum InstallMode {
    Install,
    Download,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub version: String,
    pub download_url: String,
    pub asset_name: String,
    pub install_mode: InstallMode,
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum UpdateTarget {
    Windows,
    MacOsAppleSilicon,
    MacOsIntel,
    Unsupported,
}

/// Repo to check for releases.
const RELEASE_REPO: &str = "MONKE2525E/Verenu";

/// Returns true only for URLs that point at an official release asset for
/// [`RELEASE_REPO`]. GitHub serves release assets from
/// `https://github.com/<owner>/<repo>/releases/download/<tag>/<asset>`, so any
/// legitimate `download_url` we hand to the installer lives under this path.
/// Used by the `install_update` command to reject arbitrary URLs.
///
/// The URL is fully parsed (not string-prefix matched) so dot-segment path
/// traversal can't smuggle a different repo past the check: a raw
/// `starts_with` would accept
/// `https://github.com/MONKE2525E/Verenu/releases/download/../../attacker/repo/...`,
/// but `Url::parse` normalizes the `..` segments before we inspect the host
/// and path.
pub fn is_authorized_release_asset_url(url: &str) -> bool {
    let Ok(parsed) = reqwest::Url::parse(url) else {
        return false;
    };
    if parsed.scheme() != "https" || parsed.host_str() != Some("github.com") {
        return false;
    }
    let expected_path = format!("/{RELEASE_REPO}/releases/download/");
    parsed.path().starts_with(&expected_path)
}

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

    let Some((asset, install_mode)) =
        select_release_asset_for_target(&release.assets, current_update_target())
    else {
        log::warn!(
            "No compatible update asset found for target {:?} in release {}",
            current_update_target(),
            release.tag_name
        );
        return Ok(None);
    };

    Ok(Some(UpdateInfo {
        version: display_version,
        download_url: asset.browser_download_url.clone(),
        asset_name: asset.name.clone(),
        install_mode,
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

fn current_update_target() -> UpdateTarget {
    #[cfg(windows)]
    {
        return UpdateTarget::Windows;
    }

    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    {
        return UpdateTarget::MacOsAppleSilicon;
    }

    #[cfg(all(target_os = "macos", not(target_arch = "aarch64")))]
    {
        return UpdateTarget::MacOsIntel;
    }

    #[allow(unreachable_code)]
    UpdateTarget::Unsupported
}

fn select_release_asset_for_target(
    assets: &[GhAsset],
    target: UpdateTarget,
) -> Option<(&GhAsset, InstallMode)> {
    match target {
        UpdateTarget::Windows => select_windows_asset(assets),
        UpdateTarget::MacOsAppleSilicon => select_macos_asset(
            assets,
            &["apple_silicon", "aarch64", "arm64"],
        ),
        UpdateTarget::MacOsIntel => select_macos_asset(assets, &["intel", "x64", "x86_64"]),
        UpdateTarget::Unsupported => None,
    }
}

fn select_windows_asset(assets: &[GhAsset]) -> Option<(&GhAsset, InstallMode)> {
    find_asset_with_suffix(assets, ".exe")
        .or_else(|| find_asset_with_suffix(assets, ".msi"))
        .map(|asset| (asset, InstallMode::Install))
}

fn select_macos_asset<'a>(
    assets: &'a [GhAsset],
    arch_hints: &[&str],
) -> Option<(&'a GhAsset, InstallMode)> {
    find_asset_with_suffix_and_hints(assets, ".dmg", arch_hints)
        .or_else(|| find_asset_with_suffix(assets, ".dmg"))
        .map(|asset| (asset, InstallMode::Download))
}

fn find_asset_with_suffix<'a>(assets: &'a [GhAsset], suffix: &str) -> Option<&'a GhAsset> {
    assets.iter().find(|asset| {
        asset
            .name
            .to_ascii_lowercase()
            .ends_with(&suffix.to_ascii_lowercase())
    })
}

fn find_asset_with_suffix_and_hints<'a>(
    assets: &'a [GhAsset],
    suffix: &str,
    hints: &[&str],
) -> Option<&'a GhAsset> {
    let suffix = suffix.to_ascii_lowercase();
    assets.iter().find(|asset| {
        let name = asset.name.to_ascii_lowercase();
        name.ends_with(&suffix) && hints.iter().any(|hint| name.contains(hint))
    })
}

#[cfg(test)]
mod tests {
    use super::{
        find_asset_with_suffix, is_newer, normalize_version, select_release_asset_for_target,
        GhAsset, InstallMode, UpdateTarget,
    };

    fn asset(name: &str) -> GhAsset {
        GhAsset {
            name: name.to_string(),
            browser_download_url: format!("https://example.invalid/{name}"),
        }
    }

    #[test]
    fn normalize_version_extracts_numeric_triplet() {
        assert_eq!(normalize_version("vVerenu-0.5.0-beta"), "0.5.0");
        assert_eq!(normalize_version("v1.2"), "1.2.0");
        assert_eq!(normalize_version("release-7"), "7.0.0");
    }

    #[test]
    fn newer_version_uses_normalized_comparison() {
        assert!(is_newer("v0.15.0", "0.14.1"));
        assert!(!is_newer("v0.14.1", "0.14.1"));
        assert!(!is_newer("0.14.0", "0.14.1"));
    }

    #[test]
    fn windows_prefers_exe_then_msi() {
        let assets = [asset("Verenu_0.15.0_x64_en-US.msi"), asset("Verenu_0.15.0_x64-setup.exe")];
        let selected = select_release_asset_for_target(&assets, UpdateTarget::Windows)
            .expect("windows asset");
        assert_eq!(selected.0.name, "Verenu_0.15.0_x64-setup.exe");
        assert_eq!(selected.1, InstallMode::Install);

        let fallback_assets = [asset("Verenu_0.15.0_x64_en-US.msi")];
        let fallback = select_release_asset_for_target(&fallback_assets, UpdateTarget::Windows)
            .expect("windows msi fallback");
        assert_eq!(fallback.0.name, "Verenu_0.15.0_x64_en-US.msi");
    }

    #[test]
    fn macos_apple_silicon_prefers_matching_dmg() {
        let assets = [
            asset("Verenu_0.15.0_Intel.dmg"),
            asset("Verenu_0.15.0_Apple_Silicon.dmg"),
        ];
        let selected =
            select_release_asset_for_target(&assets, UpdateTarget::MacOsAppleSilicon)
                .expect("arm dmg");
        assert_eq!(selected.0.name, "Verenu_0.15.0_Apple_Silicon.dmg");
        assert_eq!(selected.1, InstallMode::Download);
    }

    #[test]
    fn macos_intel_prefers_matching_dmg() {
        let assets = [
            asset("Verenu_0.15.0_Apple_Silicon.dmg"),
            asset("Verenu_0.15.0_Intel.dmg"),
        ];
        let selected = select_release_asset_for_target(&assets, UpdateTarget::MacOsIntel)
            .expect("intel dmg");
        assert_eq!(selected.0.name, "Verenu_0.15.0_Intel.dmg");
    }

    #[test]
    fn macos_falls_back_to_any_dmg() {
        let assets = [asset("Verenu_0.15.0.dmg")];
        let selected = select_release_asset_for_target(&assets, UpdateTarget::MacOsIntel)
            .expect("generic dmg");
        assert_eq!(selected.0.name, "Verenu_0.15.0.dmg");
    }

    #[test]
    fn unsupported_target_skips_updates() {
        let assets = [asset("Verenu_0.15.0_x64-setup.exe")];
        assert!(select_release_asset_for_target(&assets, UpdateTarget::Unsupported).is_none());
    }

    #[test]
    fn suffix_match_is_case_insensitive() {
        let assets = [asset("Verenu_0.15.0_X64-SETUP.EXE")];
        assert_eq!(
            find_asset_with_suffix(&assets, ".exe")
                .expect("case insensitive match")
                .name,
            "Verenu_0.15.0_X64-SETUP.EXE"
        );
    }
}
