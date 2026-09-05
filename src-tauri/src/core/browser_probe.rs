//! Best-effort browser address-bar domain read, used to pick a Context by
//! website when dictating into a browser tab. Only ever called when the
//! captured process is a known browser (`window_context::is_browser_exe`).
//! Any failure, timeout, or missing permission returns `None` and the caller
//! falls back to exe-only context resolution — this must never block or
//! error the dictation pipeline.

/// Strips scheme/path/query/fragment/port/userinfo from a URL or address-bar
/// string down to a bare lowercase domain, e.g.
/// "https://user@mail.google.com:443/mail/u/0?tab=rm" -> "mail.google.com".
/// Returns `None` for input that doesn't look like a URL/domain at all (pure
/// search-bar text with spaces).
pub fn extract_domain(raw: &str) -> Option<String> {
    let trimmed = raw.trim().to_lowercase();
    if trimmed.is_empty() || trimmed.contains(char::is_whitespace) {
        return None;
    }
    let without_scheme = trimmed.split("://").last().unwrap_or(&trimmed);
    let host = without_scheme
        .split(['/', '?', '#'])
        .next()
        .unwrap_or(without_scheme);
    let host = host.split('@').next_back().unwrap_or(host);
    let host = host.split(':').next().unwrap_or(host);
    if host.is_empty() || !host.contains('.') {
        return None;
    }
    Some(host.to_string())
}

#[cfg(windows)]
mod win {
    use std::time::{Duration, Instant};
    use windows::Win32::Foundation::HWND;
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER,
        COINIT_APARTMENTTHREADED,
    };
    use windows::Win32::UI::Accessibility::{
        CUIAutomation, IUIAutomation, IUIAutomationElement, IUIAutomationTreeWalker,
        IUIAutomationValuePattern, UIA_EditControlTypeId, UIA_ValuePatternId,
    };
    const BUDGET: Duration = Duration::from_millis(150);
    const MAX_DEPTH: u32 = 8;
    const MAX_VISITED: u32 = 400;

    struct ComGuard(bool);
    impl ComGuard {
        fn init() -> Self {
            let hr = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
            ComGuard(hr.is_ok())
        }
    }
    impl Drop for ComGuard {
        fn drop(&mut self) {
            if self.0 {
                unsafe { CoUninitialize() };
            }
        }
    }

    struct Budget {
        started: Instant,
        visited: u32,
    }
    impl Budget {
        fn exhausted(&self) -> bool {
            self.visited >= MAX_VISITED || self.started.elapsed() > BUDGET
        }
    }

    /// Reads a browser window's address bar text via UI Automation. Chromium's
    /// exact `addressEditBox` is preferred even when it appears after a page
    /// input in the tree; the generic Edit fallback is only used when no exact
    /// address bar was found (for Firefox and other engines).
    pub fn read_address_bar_text(window_id: usize) -> Option<String> {
        let _com = ComGuard::init();
        let automation: IUIAutomation =
            unsafe { CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER).ok()? };
        let hwnd = HWND(window_id as *mut core::ffi::c_void);
        if hwnd.0.is_null() {
            return None;
        }
        let root = unsafe { automation.ElementFromHandle(HWND(hwnd.0)).ok()? };
        let walker: IUIAutomationTreeWalker = unsafe { automation.ControlViewWalker().ok()? };

        let mut budget = Budget {
            started: Instant::now(),
            visited: 0,
        };

        let mut result = AddressBarSearch::default();
        find_address_bar(&walker, &root, 0, &mut budget, &mut result);
        result.exact.or(result.fallback)
    }

    #[derive(Default)]
    struct AddressBarSearch {
        exact: Option<String>,
        fallback: Option<String>,
    }

    fn find_address_bar(
        walker: &IUIAutomationTreeWalker,
        element: &IUIAutomationElement,
        depth: u32,
        budget: &mut Budget,
        result: &mut AddressBarSearch,
    ) {
        if depth > MAX_DEPTH || budget.exhausted() {
            return;
        }
        budget.visited += 1;

        let automation_id = unsafe { element.CurrentAutomationId() }
            .map(|v| v.to_string())
            .unwrap_or_default();
        let control_type = unsafe { element.CurrentControlType() }.ok();
        let is_edit = control_type.map(|v| v.0) == Some(UIA_EditControlTypeId.0);

        // Do not require the control type here. The automation id is the
        // browser-owned identifier and remains the strongest signal if the
        // browser changes how it exposes the value pattern.
        if automation_id.eq_ignore_ascii_case("addressEditBox") {
            if let Some(text) = read_value(element) {
                result.exact = Some(text);
                return;
            }
        }

        if is_edit && result.fallback.is_none() {
            result.fallback = read_value(element);
        }

        if let Ok(child) = unsafe { walker.GetFirstChildElement(element) } {
            let mut current = Some(child);
            while let Some(node) = current {
                if budget.exhausted() || result.exact.is_some() {
                    break;
                }
                find_address_bar(walker, &node, depth + 1, budget, result);
                current = unsafe { walker.GetNextSiblingElement(&node) }.ok();
            }
        }
    }

    fn read_value(element: &IUIAutomationElement) -> Option<String> {
        let pattern =
            unsafe { element.GetCurrentPatternAs::<IUIAutomationValuePattern>(UIA_ValuePatternId) }
                .ok()?;
        let value = unsafe { pattern.CurrentValue() }.ok()?.to_string();
        if value.trim().is_empty() {
            None
        } else {
            Some(value)
        }
    }
}

#[cfg(target_os = "macos")]
mod mac {
    /// macOS address-bar read is not yet implemented (would extend the
    /// existing AX shim in `system/macos_ax_text_marker.m` to read the known
    /// per-browser address-bar identifiers). Returns `None` so website
    /// matching silently falls back to exe-only resolution on macOS for now.
    pub fn read_address_bar_text(_window_id: usize) -> Option<String> {
        None
    }
}

#[cfg(target_os = "macos")]
use mac::read_address_bar_text as platform_read_address_bar_text;
#[cfg(windows)]
use win::read_address_bar_text as platform_read_address_bar_text;
#[cfg(not(any(windows, target_os = "macos")))]
fn platform_read_address_bar_text(_window_id: usize) -> Option<String> {
    None
}

/// Reads a browser window's address bar and returns just the domain, e.g.
/// "mail.google.com". Caller must confirm the captured process is a
/// browser (`window_context::is_browser_exe`) before calling this — it does
/// not check that itself, since the OS-level read is comparatively costly.
pub fn read_browser_domain_for_window(window_id: usize) -> Option<String> {
    let raw = platform_read_address_bar_text(window_id)?;
    extract_domain(&raw)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_domain_from_full_url() {
        assert_eq!(
            extract_domain("https://user@mail.google.com:443/mail/u/0?tab=rm#inbox"),
            Some("mail.google.com".to_string())
        );
    }

    #[test]
    fn extracts_domain_from_bare_host() {
        assert_eq!(
            extract_domain("Example.com"),
            Some("example.com".to_string())
        );
    }

    #[test]
    fn rejects_search_bar_text() {
        assert_eq!(extract_domain("how to bake bread"), None);
        assert_eq!(extract_domain(""), None);
        assert_eq!(extract_domain("localhost"), None);
    }
}
