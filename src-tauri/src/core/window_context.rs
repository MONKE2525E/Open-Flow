#[cfg(windows)]
use windows::Win32::Foundation::{HWND, MAX_PATH};
#[cfg(windows)]
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_QUERY_LIMITED_INFORMATION,
};
#[cfg(windows)]
use windows::Win32::UI::WindowsAndMessaging::{
    GetForegroundWindow, GetWindowTextW, GetWindowThreadProcessId,
};

#[cfg(windows)]
const BROWSER_EXES: &[(&str, &str)] = &[
    ("chrome.exe", "Google Chrome"),
    ("msedge.exe", "Microsoft Edge"),
    ("firefox.exe", "Firefox"),
    ("brave.exe", "Brave"),
    ("opera.exe", "Opera"),
    ("vivaldi.exe", "Vivaldi"),
    ("arc.exe", "Arc"),
    ("waterfox.exe", "Waterfox"),
    ("librewolf.exe", "LibreWolf"),
];

// Process name convention on macOS: "<localized name>.app" lowercased.
#[cfg(target_os = "macos")]
const BROWSER_EXES: &[(&str, &str)] = &[
    ("google chrome.app", "Google Chrome"),
    ("safari.app", "Safari"),
    ("microsoft edge.app", "Microsoft Edge"),
    ("firefox.app", "Firefox"),
    ("brave browser.app", "Brave"),
    ("arc.app", "Arc"),
];

/// The focus target to refocus before paste. On Windows this is the foreground
/// `HWND`; on macOS it is the frontmost application's PID (both fit in a `usize`).
pub fn get_foreground_hwnd() -> usize {
    #[cfg(windows)]
    unsafe {
        let hwnd = GetForegroundWindow();
        hwnd.0 as usize
    }
    #[cfg(target_os = "macos")]
    {
        // Avoid CGWindowListCopyWindowInfo on the hotkey/keypress path — it
        // communicates synchronously with WindowServer for all on-screen windows
        // and introduces several ms of typing latency, which can trigger the
        // macOS event tap watchdog timeout. frontmost_pid() is a single fast
        // NSWorkspace call and sufficient for both focus-target tracking and the
        // "same window?" comparisons in injection.rs (pid & 0xFFFFFFFF).
        crate::system::mac_app::frontmost_pid()
            .map(|p| p as usize)
            .unwrap_or(0)
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    0
}

/// Whether `process_name` (as returned by `get_process_name_for_hwnd`) is a
/// known browser — used to gate the address-bar domain probe so it's never
/// attempted against a non-browser foreground window.
#[cfg(any(windows, target_os = "macos"))]
pub fn is_browser_exe(process_name: &str) -> bool {
    BROWSER_EXES.iter().any(|(exe, _)| *exe == process_name)
}
#[cfg(not(any(windows, target_os = "macos")))]
pub fn is_browser_exe(_process_name: &str) -> bool {
    false
}

pub fn get_active_process_name() -> Option<String> {
    get_process_name_for_hwnd(get_foreground_hwnd())
}

#[cfg_attr(not(windows), allow(unused_variables))]
pub fn get_process_name_for_hwnd(hwnd: usize) -> Option<String> {
    #[cfg(windows)]
    unsafe {
        let hwnd = HWND(hwnd as *mut core::ffi::c_void);
        if hwnd.0.is_null() {
            return None;
        }

        let mut pid = 0;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if pid == 0 {
            return None;
        }

        let process_handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;

        let mut buffer = [0u16; MAX_PATH as usize];
        let mut size = buffer.len() as u32;

        if QueryFullProcessImageNameW(
            process_handle,
            windows::Win32::System::Threading::PROCESS_NAME_FORMAT(0),
            windows::core::PWSTR::from_raw(buffer.as_mut_ptr()),
            &mut size,
        )
        .is_ok()
        {
            let path = String::from_utf16_lossy(&buffer[..size as usize]);
            if let Some(name) = std::path::Path::new(&path)
                .file_name()
                .and_then(|n| n.to_str())
            {
                return Some(name.to_lowercase());
            }
        }
        None
    }
    #[cfg(target_os = "macos")]
    {
        // Match the AppMapping key convention: "<localized name>.app", lowercased.
        crate::system::mac_app::frontmost_app_name().map(|n| format!("{}.app", n.to_lowercase()))
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    None
}

/// Returns a compact, labeled context hint for the cleanup prompt. It reads
/// the captured target window because focus may move during processing.
pub fn get_app_context_hint(
    process_name: &str,
    target_id: usize,
    browser_domain: Option<&str>,
    context_name: Option<&str>,
) -> Option<String> {
    let mut lines = Vec::new();
    // Resolved context group (see core/context.rs). Labeled and truncated like
    // every other hint line, and omitted when empty so the prompt never carries
    // a dangling "Context:" header.
    if let Some(name) = context_name
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        lines.push(format!("Context: {}", truncate_hint_value(name, 60)));
    }
    #[cfg(any(windows, target_os = "macos"))]
    {
        let browser = BROWSER_EXES
            .iter()
            .find(|(exe, _)| *exe == process_name)
            .map(|(_, name)| *name);

        if let Some(browser_name) = browser {
            lines.push(format!("Application: {browser_name}"));
            if let Some(domain) = browser_domain
                .map(str::trim)
                .filter(|value| !value.is_empty())
            {
                lines.push(format!("Website: {}", truncate_hint_value(domain, 120)));
            }
            let title = get_window_title(target_id).unwrap_or_default();
            let page = strip_browser_suffix(&title, browser_name);
            if !page.is_empty() {
                lines.push(format!("Window title: {}", truncate_hint_value(&page, 160)));
            }
        } else {
            lines.push(format!("Application: {}", friendly_app_name(process_name)));
        }
    }

    #[cfg(not(any(windows, target_os = "macos")))]
    lines.push(format!("Application: {}", friendly_app_name(process_name)));

    (!lines.is_empty()).then(|| lines.join("\n"))
}

fn friendly_app_name(process_name: &str) -> String {
    let base = process_name
        .trim_end_matches(".exe")
        .trim_end_matches(".app");
    let normalized = base.to_ascii_lowercase();
    // Release-channel/version suffixes do not help disambiguate dictation and
    // make otherwise identical targets look like different applications.
    let stable_base = [
        " (nightly)",
        "-nightly",
        "_nightly",
        " nightly",
        " (beta)",
        "-beta",
        "_beta",
        " beta",
        " (dev)",
        "-dev",
        "_dev",
        " dev",
    ]
    .iter()
    .filter_map(|suffix| normalized.find(suffix).map(|index| (index, *suffix)))
    .min_by_key(|(index, _)| *index)
    .map(|(index, _)| &base[..index])
    .map(str::trim_end)
    .filter(|value| !value.is_empty())
    .unwrap_or(base);

    match stable_base.to_ascii_lowercase().as_str() {
        "code" => "Visual Studio Code".to_string(),
        "winword" => "Microsoft Word".to_string(),
        "excel" => "Microsoft Excel".to_string(),
        "powerpnt" => "Microsoft PowerPoint".to_string(),
        "outlook" | "olk" => "Microsoft Outlook".to_string(),
        "slack" => "Slack".to_string(),
        "discord" => "Discord".to_string(),
        "teams" | "ms-teams" => "Microsoft Teams".to_string(),
        "notepad" => "Notepad".to_string(),
        "obsidian" => "Obsidian".to_string(),
        "notion" => "Notion".to_string(),
        "t3-code" => "T3 Code".to_string(),
        _ => stable_base.to_string(),
    }
}

fn truncate_hint_value(value: &str, max_chars: usize) -> String {
    let clean = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if clean.chars().count() <= max_chars {
        return clean;
    }
    let mut shortened = clean
        .chars()
        .take(max_chars.saturating_sub(1))
        .collect::<String>();
    shortened.push('…');
    shortened
}

fn get_window_title(target_id: usize) -> Option<String> {
    #[cfg(windows)]
    unsafe {
        let hwnd = HWND(target_id as *mut core::ffi::c_void);
        if hwnd.0.is_null() {
            return None;
        }
        let mut buf = [0u16; 512];
        let len = GetWindowTextW(hwnd, &mut buf);
        if len > 0 {
            Some(String::from_utf16_lossy(&buf[..len as usize]))
        } else {
            None
        }
    }
    #[cfg(not(windows))]
    {
        let _ = target_id;
        None
    }
}

/// Strips the trailing " - Browser Name" or " — Browser Name" suffix from a
/// window title, leaving just the page/site portion.
fn strip_browser_suffix(title: &str, browser_name: &str) -> String {
    // Try both " - " and " — " as separators (Firefox uses em-dash)
    for sep in &[" - ", " — "] {
        if let Some(pos) = title.rfind(sep) {
            let tail = title[pos + sep.len()..].trim();
            // Match the first word of the browser name (e.g. "Google" matches "Google Chrome")
            let first_word = browser_name
                .split_whitespace()
                .next()
                .unwrap_or(browser_name);
            if tail.to_lowercase().starts_with(&first_word.to_lowercase()) {
                return title[..pos].trim().to_string();
            }
        }
    }
    title.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn friendly_app_names_hide_process_file_conventions() {
        assert_eq!(friendly_app_name("code.exe"), "Visual Studio Code");
        assert_eq!(friendly_app_name("winword.exe"), "Microsoft Word");
        assert_eq!(friendly_app_name("slack.app"), "Slack");
        assert_eq!(friendly_app_name("t3-code-nightly-20260830.exe"), "T3 Code");
        assert_eq!(friendly_app_name("Example (nightly).app"), "Example");
    }

    #[test]
    fn hint_values_are_single_line_and_bounded() {
        let value = format!("  issue\n{}  ", "x".repeat(200));
        let shortened = truncate_hint_value(&value, 40);
        assert_eq!(shortened.chars().count(), 40);
        assert!(!shortened.contains('\n'));
        assert!(shortened.ends_with('…'));
    }

    #[test]
    fn browser_suffix_is_removed_from_window_title() {
        assert_eq!(
            strip_browser_suffix("Pull request · GitHub - Google Chrome", "Google Chrome"),
            "Pull request · GitHub"
        );
    }
}
