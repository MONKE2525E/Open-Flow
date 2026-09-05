//! OS session lifecycle helpers.

/// True while Windows is logging off or shutting down. This lets the close
/// handler exit instead of hiding to tray and keeping the process alive.
#[cfg(target_os = "windows")]
pub fn system_is_shutting_down() -> bool {
    use windows::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_SHUTTINGDOWN};
    unsafe { GetSystemMetrics(SM_SHUTTINGDOWN) != 0 }
}

#[cfg(not(target_os = "windows"))]
pub fn system_is_shutting_down() -> bool {
    false
}

#[cfg(test)]
mod tests {
    #[test]
    fn fallback_platform_never_reports_shutdown() {
        assert!(!super::system_is_shutting_down());
    }
}
