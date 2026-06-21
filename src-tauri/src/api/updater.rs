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
/// `https://github.com/<owner>/<repo>/releases/download/<tag>/<asset>` — exactly
/// six path segments. Used by the `install_update` command to reject arbitrary
/// URLs before we open/download/execute them.
///
/// Validation is done segment-by-segment rather than by prefix-matching the raw
/// path, because `Url::parse` only normalizes *literal* `..` segments — it
/// leaves percent-encoded traversal sequences (`%2e`, `%2f`, `%5c`) intact,
/// which a server or proxy could later decode into a traversal to a different
/// repo. Requiring exactly six segments, pinning the fixed ones, and rejecting
/// dot/encoded-traversal segments closes every such bypass.
pub fn is_authorized_release_asset_url(url: &str) -> bool {
    let Ok(parsed) = reqwest::Url::parse(url) else {
        return false;
    };
    if parsed.scheme() != "https" || parsed.host_str() != Some("github.com") {
        return false;
    }

    let Some(segments) = parsed.path_segments() else {
        return false;
    };
    let segments: Vec<&str> = segments.collect();
    // `/<owner>/<repo>/releases/download/<tag>/<asset>` — no more, no fewer.
    let [owner, repo, releases, download, tag, asset] = segments.as_slice() else {
        return false;
    };

    // GitHub owner/repo names are case-insensitive, so match them that way.
    let mut expected = RELEASE_REPO.split('/');
    let expected_owner = expected.next().unwrap_or_default();
    let expected_repo = expected.next().unwrap_or_default();
    if !owner.eq_ignore_ascii_case(expected_owner)
        || !repo.eq_ignore_ascii_case(expected_repo)
        || !releases.eq_ignore_ascii_case("releases")
        || !download.eq_ignore_ascii_case("download")
    {
        return false;
    }

    // The tag and asset are attacker-influenced only insofar as a crafted URL
    // could put traversal markers here; reject empties, dot segments, and
    // percent-encoded dots/slashes/backslashes.
    let is_suspicious = |s: &str| {
        let lower = s.to_ascii_lowercase();
        s.is_empty()
            || s == "."
            || s == ".."
            || lower.contains("%2e")
            || lower.contains("%2f")
            || lower.contains("%5c")
    };
    !is_suspicious(tag) && !is_suspicious(asset)
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
        // Apple Silicon can run Intel builds via Rosetta 2, so it may fall back
        // to a generic/Intel DMG.
        UpdateTarget::MacOsAppleSilicon => {
            select_macos_asset(assets, &["apple_silicon", "aarch64", "arm64"], &[])
        }
        // Intel Macs cannot run arm64 binaries, so exclude Apple Silicon DMGs
        // from the generic fallback rather than installing an unusable build.
        UpdateTarget::MacOsIntel => select_macos_asset(
            assets,
            &["intel", "x64", "x86_64"],
            &["apple_silicon", "aarch64", "arm64"],
        ),
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
    exclude_hints: &[&str],
) -> Option<(&'a GhAsset, InstallMode)> {
    find_asset_with_suffix_and_hints(assets, ".dmg", arch_hints)
        .or_else(|| {
            assets.iter().find(|asset| {
                let name = asset.name.to_ascii_lowercase();
                name.ends_with(".dmg")
                    && !exclude_hints
                        .iter()
                        .any(|hint| name.contains(&hint.to_ascii_lowercase()))
            })
        })
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
        name.ends_with(&suffix)
            && hints
                .iter()
                .any(|hint| name.contains(&hint.to_ascii_lowercase()))
    })
}

#[cfg(test)]
mod tests {
    use super::{
        find_asset_with_suffix, is_authorized_release_asset_url, is_newer, normalize_version,
        select_release_asset_for_target, GhAsset, InstallMode, UpdateTarget,
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
    fn macos_intel_does_not_fall_back_to_apple_silicon_dmg() {
        let assets = [asset("Verenu_0.15.0_Apple_Silicon.dmg")];
        assert!(
            select_release_asset_for_target(&assets, UpdateTarget::MacOsIntel).is_none(),
            "Intel must not install an Apple Silicon-only DMG"
        );
    }

    #[test]
    fn macos_apple_silicon_falls_back_to_intel_dmg() {
        // Apple Silicon can run Intel builds via Rosetta 2.
        let assets = [asset("Verenu_0.15.0_Intel.dmg")];
        let selected =
            select_release_asset_for_target(&assets, UpdateTarget::MacOsAppleSilicon)
                .expect("rosetta fallback");
        assert_eq!(selected.0.name, "Verenu_0.15.0_Intel.dmg");
    }

    #[test]
    fn authorized_url_accepts_official_release_assets() {
        assert!(is_authorized_release_asset_url(
            "https://github.com/MONKE2525E/Verenu/releases/download/v0.15.0/Verenu_0.15.0_x64-setup.exe"
        ));
        // Owner/repo casing is insignificant on GitHub.
        assert!(is_authorized_release_asset_url(
            "https://github.com/monke2525e/verenu/releases/download/v0.15.0/Verenu_0.15.0_Apple_Silicon.dmg"
        ));
    }

    #[test]
    fn authorized_url_rejects_bypasses_and_foreign_hosts() {
        let bad = [
            // Wrong host / scheme.
            "http://github.com/MONKE2525E/Verenu/releases/download/v1/a.exe",
            "https://evil.com/MONKE2525E/Verenu/releases/download/v1/a.exe",
            // Different repo.
            "https://github.com/attacker/repo/releases/download/v1/a.exe",
            // Literal dot-segment traversal.
            "https://github.com/MONKE2525E/Verenu/releases/download/../../attacker/repo/releases/download/v1/a.exe",
            // Percent-encoded dot / slash / backslash traversal.
            "https://github.com/MONKE2525E/Verenu/releases/download/%2e%2e/a.exe",
            "https://github.com/MONKE2525E/Verenu/releases/download/v1/%2fa.exe",
            "https://github.com/MONKE2525E/Verenu/releases/download/..%5c..%5cattacker%5crepo/a.exe",
            // Wrong structure (too few / too many segments).
            "https://github.com/MONKE2525E/Verenu/releases/download/v1",
            "https://github.com/MONKE2525E/Verenu/blob/main/releases/download/v1/a.exe",
        ];
        for url in bad {
            assert!(
                !is_authorized_release_asset_url(url),
                "should reject: {url}"
            );
        }
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
