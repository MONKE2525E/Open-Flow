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
    let latest = release.tag_name.trim_start_matches('v');

    if !is_newer(latest, env!("CARGO_PKG_VERSION")) {
        return Ok(None);
    }

    let asset = release.assets
        .iter()
        .find(|a| a.name.ends_with(".exe"))
        .ok_or_else(|| anyhow::anyhow!("No .exe asset in release"))?;

    Ok(Some(UpdateInfo {
        version: latest.to_owned(),
        download_url: asset.browser_download_url.clone(),
    }))
}

fn is_newer(latest: &str, current: &str) -> bool {
    let parse = |s: &str| -> Vec<u32> {
        s.split('.')
            .filter_map(|p| p.parse().ok())
            .collect()
    };
    parse(latest) > parse(current)
}
