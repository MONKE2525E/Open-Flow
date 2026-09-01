use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct InstalledApp {
    pub name: String,
    pub exe: String,
    /// Publisher/team identity when the platform exposes one. It is kept
    /// optional because running processes and some unsigned apps do not have
    /// one.
    #[serde(default)]
    pub developer: Option<String>,
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
                apps.push(InstalledApp {
                    name,
                    exe,
                    developer: mac_app_developer(&path),
                });
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
        // Redistributables, runtimes, SDKs, drivers, updaters — installed
        // dependencies, not apps a user would ever pick for a Context.
        "redistributable",
        "runtime library",
        ".net runtime",
        ".net sdk",
        ".net desktop runtime",
        ".net core",
        "visual c++",
        "visual studio installer",
        "webview2",
        "directx",
        " driver",
        " update",
        " updater",
    ];

    if CLUTTER_NAME_PARTS.iter().any(|part| name.contains(part)) {
        return false;
    }

    !looks_like_installer_junk(&name)
}

/// Some installers register an Uninstall entry whose DisplayName embeds a
/// build hash (e.g. "Antigravitysetup Stable Ecfbad74d93962fc8ca485d93ab9..."),
/// which is meaningless to search by and often duplicates the real app entry.
/// Detected as any run of 16+ contiguous hex characters in the name.
#[cfg(windows)]
fn looks_like_installer_junk(name: &str) -> bool {
    let mut run = 0usize;
    for ch in name.chars() {
        if ch.is_ascii_hexdigit() {
            run += 1;
            if run >= 16 {
                return true;
            }
        } else {
            run = 0;
        }
    }
    false
}

#[cfg(windows)]
pub(crate) fn parse_exe_from_icon(icon: &str) -> Option<String> {
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
pub(crate) unsafe fn reg_read_string(
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
                        let publisher = reg_read_string(hsubkey, "Publisher")
                            .and_then(|value| nonempty_metadata(&value));
                        apps.push(InstalledApp {
                            name,
                            exe,
                            developer: publisher,
                        });
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
                        let app = InstalledApp {
                            name,
                            exe,
                            developer: None,
                        };
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

fn nonempty_metadata(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_string())
}

/// Returns a stable publisher/team hint where macOS exposes one. Team IDs are
/// the best signal for signed apps and remain stable across bundle/version
/// changes. Unsigned apps fall back to their bundle identifier, when present.
#[cfg(target_os = "macos")]
fn mac_app_developer(path: &std::path::Path) -> Option<String> {
    use std::process::Command;

    let signed = Command::new("/usr/bin/codesign")
        .args(["-dv", "--verbose=4"])
        .arg(path)
        .output()
        .ok()?;
    let diagnostic = String::from_utf8_lossy(&signed.stderr);
    if let Some(team) = diagnostic
        .lines()
        .find_map(|line| line.strip_prefix("TeamIdentifier="))
        .and_then(nonempty_metadata)
    {
        return Some(team);
    }

    // `plutil` handles both XML and binary Info.plist files without adding a
    // plist parser dependency to the desktop binary.
    let plist = path.join("Contents/Info.plist");
    let output = Command::new("/usr/bin/plutil")
        .args(["-extract", "CFBundleIdentifier", "raw", "-o", "-"])
        .arg(plist)
        .output()
        .ok()?;
    nonempty_metadata(&String::from_utf8_lossy(&output.stdout))
}

/// A cached inventory for the dictation path. Registry/bundle enumeration is
/// appropriate for the settings UI, but should not happen on every hotkey
/// release. A short TTL still notices nightly-app replacements promptly.
pub fn list_installed_apps_cached() -> Vec<InstalledApp> {
    use std::sync::{Mutex, OnceLock};
    use std::time::{Duration, Instant};

    struct Cache {
        at: Option<Instant>,
        apps: Vec<InstalledApp>,
    }

    static CACHE: OnceLock<Mutex<Cache>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(Cache { at: None, apps: Vec::new() }));
    let mut guard = cache.lock().expect("installed-app cache lock");
    if guard.at.map_or(true, |at| at.elapsed() >= Duration::from_secs(15)) {
        guard.apps = list_installed_apps();
        guard.at = Some(Instant::now());
    }
    guard.apps.clone()
}

/// Finds a replacement for a target whose executable/name no longer exists.
/// Exact app identity is preferred, a known developer may lower the name
/// threshold, and a conflicting known developer is never accepted.
pub fn closest_installed_app<'a>(
    source_executable: &str,
    source_name: Option<&str>,
    source_developer: Option<&str>,
    apps: &'a [InstalledApp],
) -> Option<&'a InstalledApp> {
    let source_executable = source_executable.trim_start_matches("?::");
    let source_keys = [source_executable, source_name.unwrap_or("")]
        .into_iter()
        .map(app_match_key)
        .filter(|key| key.len() >= 3)
        .collect::<Vec<_>>();
    if source_keys.is_empty() {
        return None;
    }
    let source_developer = normalized_metadata(source_developer);
    apps.iter()
        .filter_map(|app| {
            let candidate_developer = normalized_metadata(app.developer.as_deref());
            if source_developer.is_some()
                && candidate_developer.is_some()
                && source_developer != candidate_developer
            {
                return None;
            }
            let candidate_keys = [app.exe.as_str(), app.name.as_str()]
                .into_iter()
                .map(app_match_key)
                .filter(|key| !key.is_empty())
                .collect::<Vec<_>>();
            let score = source_keys
                .iter()
                .flat_map(|source| candidate_keys.iter().map(move |candidate| app_match_score(source, candidate)))
                .fold(0.0_f32, f32::max);
            let same_developer = source_developer.is_some() && source_developer == candidate_developer;
            let threshold = if same_developer { 0.72 } else { 0.78 };
            // Legacy targets do not have metadata yet. A lower threshold is
            // still safe when the candidate preserves a meaningful prefix;
            // without either developer agreement or that prefix, a similar
            // score is not enough to auto-rebind a user assignment.
            let strong_prefix = source_keys.iter().any(|source| {
                candidate_keys.iter().any(|candidate| common_prefix_len(source, candidate) >= 4)
            });
            (score >= threshold && (same_developer || strong_prefix))
                .then_some((app, score, same_developer))
        })
        .max_by(|(_, left_score, left_same), (_, right_score, right_same)| {
            // Prefer developer-confirmed matches when scores are close.
            left_score
                .total_cmp(right_score)
                .then_with(|| left_same.cmp(right_same))
        })
        .map(|(app, _, _)| app)
}

fn normalized_metadata(value: Option<&str>) -> Option<String> {
    value.map(str::trim).filter(|value| !value.is_empty()).map(str::to_lowercase)
}

fn app_match_key(value: &str) -> String {
    let basename = value.rsplit(['/', '\\']).next().unwrap_or(value).to_lowercase();
    let basename = basename
        .strip_suffix(".exe")
        .or_else(|| basename.strip_suffix(".app"))
        .unwrap_or(basename.as_str());
    basename.chars().filter(|ch| ch.is_ascii_alphanumeric()).collect()
}

fn app_match_score(left: &str, right: &str) -> f32 {
    if left.is_empty() || right.is_empty() { return 0.0; }
    if left == right { return 1.0; }
    if left.contains(right) || right.contains(left) {
        return left.len().min(right.len()) as f32 / left.len().max(right.len()) as f32;
    }
    let distance = levenshtein(left.as_bytes(), right.as_bytes());
    1.0 - distance as f32 / left.len().max(right.len()) as f32
}

fn common_prefix_len(left: &str, right: &str) -> usize {
    left.chars()
        .zip(right.chars())
        .take_while(|(left, right)| left == right)
        .count()
}

fn levenshtein(left: &[u8], right: &[u8]) -> usize {
    let mut previous: Vec<usize> = (0..=right.len()).collect();
    let mut current = vec![0; right.len() + 1];
    for (i, &left_byte) in left.iter().enumerate() {
        current[0] = i + 1;
        for (j, &right_byte) in right.iter().enumerate() {
            current[j + 1] = (previous[j + 1] + 1)
                .min(current[j] + 1)
                .min(previous[j] + usize::from(left_byte != right_byte));
        }
        std::mem::swap(&mut previous, &mut current);
    }
    previous[right.len()]
}
