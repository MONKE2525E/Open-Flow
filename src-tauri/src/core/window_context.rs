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
    // macOS app names (process name is "<localized name>.app", lowercased)
    ("google chrome.app", "Google Chrome"),
    ("safari.app", "Safari"),
    ("microsoft edge.app", "Microsoft Edge"),
    ("firefox.app", "Firefox"),
    ("brave browser.app", "Brave"),
    ("arc.app", "Arc"),
];

#[allow(dead_code)]
pub fn is_browser_process_name(process_name: &str) -> bool {
    BROWSER_EXES.iter().any(|(exe, _)| *exe == process_name)
}
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

/// Returns a human-readable context hint for the cleanup prompt, e.g.
/// "Google Chrome — GitHub · Build software better, together" or "slack.exe".
/// Returns `None` if there is no useful context to add.
pub fn get_app_context_hint(process_name: &str) -> Option<String> {
    let browser = BROWSER_EXES
        .iter()
        .find(|(exe, _)| *exe == process_name)
        .map(|(_, name)| *name);

    if let Some(browser_name) = browser {
        let title = get_active_window_title().unwrap_or_default();
        if title.is_empty() {
            return Some(browser_name.to_string());
        }
        let page = strip_browser_suffix(&title, browser_name);
        if page.is_empty() {
            return Some(browser_name.to_string());
        }
        return Some(format!("{browser_name} — {page}"));
    }

    // For non-browsers just return the process name so the model at least knows
    // which app the user is in.
    Some(process_name.to_string())
}

fn get_active_window_title() -> Option<String> {
    #[cfg(windows)]
    unsafe {
        let hwnd = GetForegroundWindow();
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
    None
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
