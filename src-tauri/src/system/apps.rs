use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct InstalledApp {
    pub name: String,
    pub exe: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AppMapping {
    pub exe: String,
    pub profile: String,
    #[serde(default)]
    pub name: String,
    /// Per-app override for `cleanup_intensity`. `None` (or empty) falls back
    /// to the global setting.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cleanup_intensity: Option<String>,
}

/// Combines registry-discovered and currently-running apps into a single deduplicated list.
pub fn list_installed_apps() -> Vec<InstalledApp> {
    #[cfg(not(any(windows, target_os = "macos")))]
    return vec![];

    // macOS: enumerate `.app` bundles in the standard Applications folders. The
    // `exe` key is "<bundle name>.app" lowercased, matching the foreground app
    // name produced by `window_context::get_active_process_name` so AppMappings
    // resolve correctly.
    #[cfg(target_os = "macos")]
    {
        use std::collections::HashSet;

        let mut dirs = vec![
            std::path::PathBuf::from("/Applications"),
            std::path::PathBuf::from("/Applications/Utilities"),
            std::path::PathBuf::from("/System/Applications"),
            std::path::PathBuf::from("/System/Applications/Utilities"),
        ];
        if let Ok(home) = std::env::var("HOME") {
            dirs.push(std::path::PathBuf::from(home).join("Applications"));
        }

        let mut apps: Vec<InstalledApp> = Vec::new();
        for dir in dirs {
            let Ok(entries) = std::fs::read_dir(&dir) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) != Some("app") {
                    continue;
                }
                let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
                    continue;
                };
                let name = stem.to_string();
                let exe = format!("{}.app", name.to_lowercase());
                apps.push(InstalledApp { name, exe });
            }
        }

        let mut seen = HashSet::new();
        apps.retain(|a| seen.insert(a.exe.clone()));
        apps.sort_by_key(|app| app.name.to_lowercase());
        apps
    }

    #[cfg(windows)]
    {
        use windows::Win32::System::Registry::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};
        let mut apps: Vec<InstalledApp> = Vec::new();
        for (root, path) in [
            (
                HKEY_LOCAL_MACHINE,
                "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Uninstall",
            ),
            (
                HKEY_LOCAL_MACHINE,
                "SOFTWARE\\WOW6432Node\\Microsoft\\Windows\\CurrentVersion\\Uninstall",
            ),
            (
                HKEY_CURRENT_USER,
                "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Uninstall",
            ),
        ] {
            apps.extend(scan_uninstall(root, path));
        }
        {
            let registry_exes: std::collections::HashSet<&str> =
                apps.iter().map(|a| a.exe.as_str()).collect();
            let to_add: Vec<_> = get_running_processes()
                .into_iter()
                .filter(|p| !registry_exes.contains(p.exe.as_str()))
                .collect();
            apps.extend(to_add);
        }
        let mut seen = std::collections::HashSet::new();
        apps.retain(|a| seen.insert(a.exe.clone()));
        apps.retain(is_user_facing_app);
        apps.sort_by_key(|app| app.name.to_lowercase());
        apps
    }
}

#[cfg(windows)]
fn friendly_app_name(exe: &str, name: &str) -> String {
    if let Some(name) = known_app_name(exe) {
        return name.to_string();
    }

    let trimmed = name.trim();
    if !trimmed.is_empty() && trimmed.to_lowercase() != exe.trim_end_matches(".exe") {
        return trimmed.to_string();
    }

    let spaced = exe
        .trim_end_matches(".exe")
        .chars()
        .map(|ch| if ch == '_' || ch == '-' { ' ' } else { ch })
        .collect::<String>();
    title_case(&spaced)
}

#[cfg(windows)]
fn known_app_name(exe: &str) -> Option<&'static str> {
    match exe.to_lowercase().as_str() {
        "chrome.exe" => Some("Google Chrome"),
        "msedge.exe" => Some("Microsoft Edge"),
        "firefox.exe" => Some("Firefox"),
        "brave.exe" => Some("Brave"),
        "code.exe" => Some("Visual Studio Code"),
        "notion.exe" => Some("Notion"),
        "slack.exe" => Some("Slack"),
        "discord.exe" => Some("Discord"),
        "teams.exe" | "ms-teams.exe" => Some("Microsoft Teams"),
        "outlook.exe" => Some("Outlook"),
        "winword.exe" => Some("Microsoft Word"),
        "excel.exe" => Some("Microsoft Excel"),
        "powerpnt.exe" => Some("Microsoft PowerPoint"),
        "onenote.exe" => Some("OneNote"),
        "robloxplayerinstaller.exe" => Some("Roblox Player Installer"),
        "robloxplayerbeta.exe" => Some("Roblox"),
        _ => None,
    }
}

#[cfg(windows)]
fn title_case(value: &str) -> String {
    value
        .split_whitespace()
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => format!("{}{}", first.to_uppercase(), chars.as_str().to_lowercase()),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(windows)]
fn is_user_facing_app(app: &InstalledApp) -> bool {
    let exe = app.exe.to_lowercase();
    let name = app.name.to_lowercase();

    const CLUTTER_EXES: &[&str] = &[
        "aggregatorhost.exe",
        "applicationframehost.exe",
        "audiodg.exe",
        "backgroundtaskhost.exe",
        "conhost.exe",
        "csrss.exe",
        "ctfmon.exe",
        "dllhost.exe",
        "dwm.exe",
        "fontdrvhost.exe",
        "lsass.exe",
        "verenu.exe",
        "registry",
        "runtimebroker.exe",
        "searchhost.exe",
        "securityhealthservice.exe",
        "securityhealthsystray.exe",
        "services.exe",
        "shellexperiencehost.exe",
        "sihost.exe",
        "smartscreen.exe",
        "smss.exe",
        "startmenuexperiencehost.exe",
        "svchost.exe",
        "system",
        "system idle process",
        "taskhostw.exe",
        "textinputhost.exe",
        "widgetservice.exe",
        "widgets.exe",
        "wininit.exe",
        "winlogon.exe",
        "winstore.app.exe",
    ];

    if CLUTTER_EXES.contains(&exe.as_str()) {
        return false;
    }

    const CLUTTER_NAME_PARTS: &[&str] = &[
        "microsoft edge update",
        "microsoft update",
        "search host",
        "service host",
        "shell experience host",
        "start menu experience host",
        "windows input experience",
        "windows security notification",
    ];

    !CLUTTER_NAME_PARTS.iter().any(|part| name.contains(part))
}

#[cfg(windows)]
fn parse_exe_from_icon(icon: &str) -> Option<String> {
    let s = icon.trim().trim_matches('"');
    let path_part = s.split(',').next()?.trim().trim_matches('"');
    if path_part.to_lowercase().ends_with(".exe") {
        std::path::Path::new(path_part)
            .file_name()?
            .to_str()
            .map(|s| s.to_lowercase())
    } else {
        None
    }
}

#[cfg(windows)]
unsafe fn reg_read_string(
    hkey: windows::Win32::System::Registry::HKEY,
    value: &str,
) -> Option<String> {
    use windows::core::PCWSTR;
    use windows::Win32::System::Registry::{RegGetValueW, RRF_RT_REG_SZ};
    let value_wide: Vec<u16> = value.encode_utf16().chain(std::iter::once(0)).collect();
    let mut size: u32 = 1024 * 2;
    let mut buf = vec![0u16; 1024];
    RegGetValueW(
        hkey,
        PCWSTR::null(),
        PCWSTR::from_raw(value_wide.as_ptr()),
        RRF_RT_REG_SZ,
        None,
        Some(buf.as_mut_ptr() as *mut _),
        Some(&mut size),
    )
    .ok()
    .ok()?;
    let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    Some(String::from_utf16_lossy(&buf[..end]))
}

#[cfg(windows)]
fn scan_uninstall(root: windows::Win32::System::Registry::HKEY, path: &str) -> Vec<InstalledApp> {
    use windows::core::{PCWSTR, PWSTR};
    use windows::Win32::System::Registry::{RegCloseKey, RegEnumKeyExW, RegOpenKeyExW, KEY_READ};

    let mut apps = Vec::new();
    let path_wide: Vec<u16> = path.encode_utf16().chain(std::iter::once(0)).collect();
    let mut hbase = windows::Win32::System::Registry::HKEY::default();

    unsafe {
        if RegOpenKeyExW(
            root,
            PCWSTR::from_raw(path_wide.as_ptr()),
            None,
            KEY_READ,
            &mut hbase,
        )
        .is_err()
        {
            return apps;
        }

        let mut index = 0u32;
        loop {
            let mut name_buf = [0u16; 256];
            let mut name_len = 255u32;
            let r = RegEnumKeyExW(
                hbase,
                index,
                Some(PWSTR::from_raw(name_buf.as_mut_ptr())),
                &mut name_len,
                None,
                Some(PWSTR::null()),
                None,
                None,
            );
            if r.is_err() {
                break;
            }
            let subkey_name = String::from_utf16_lossy(&name_buf[..name_len as usize]);
            let subkey_wide: Vec<u16> = subkey_name
                .encode_utf16()
                .chain(std::iter::once(0))
                .collect();
            let mut hsubkey = windows::Win32::System::Registry::HKEY::default();
            if RegOpenKeyExW(
                hbase,
                PCWSTR::from_raw(subkey_wide.as_ptr()),
                None,
                KEY_READ,
                &mut hsubkey,
            )
            .is_ok()
            {
                let display_name = reg_read_string(hsubkey, "DisplayName");
                let display_icon = reg_read_string(hsubkey, "DisplayIcon");
                if let (Some(name), Some(icon)) = (display_name, display_icon) {
                    if let Some(exe) = parse_exe_from_icon(&icon) {
                        let name = friendly_app_name(&exe, &name);
                        apps.push(InstalledApp { name, exe });
                    }
                }
                let _ = RegCloseKey(hsubkey).ok();
            }
            index += 1;
        }
        let _ = RegCloseKey(hbase).ok();
    }
    apps
}

#[cfg(windows)]
fn get_running_processes() -> Vec<InstalledApp> {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
        TH32CS_SNAPPROCESS,
    };
    let mut apps = Vec::new();
    unsafe {
        if let Ok(snap) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) {
            let mut entry: PROCESSENTRY32W = std::mem::zeroed();
            entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
            if Process32FirstW(snap, &mut entry).is_ok() {
                loop {
                    let raw = &entry.szExeFile;
                    let end = raw.iter().position(|&c| c == 0).unwrap_or(raw.len());
                    let exe = String::from_utf16_lossy(&raw[..end]).to_lowercase();
                    if !exe.is_empty() && exe != "system idle process" && !exe.starts_with('[') {
                        let name =
                            friendly_app_name(&exe, exe.strip_suffix(".exe").unwrap_or(&exe));
                        let app = InstalledApp { name, exe };
                        if is_user_facing_app(&app) {
                            apps.push(app);
                        }
                    }
                    if Process32NextW(snap, &mut entry).is_err() {
                        break;
                    }
                }
            }
            CloseHandle(snap).ok();
        }
    }
    apps.sort_by(|a, b| a.exe.cmp(&b.exe));
    apps.dedup_by(|a, b| a.exe == b.exe);
    apps
}
