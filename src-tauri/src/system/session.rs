//! OS session lifecycle helpers — detecting when the OS (not the user) is
//! tearing the app down, so exit handling can differ between "hide to tray"
//! window closes and real session shutdown.

/// True while Windows is logging off or shutting down (SM_SHUTTINGDOWN is set
/// during WM_ENDSESSION processing). The main window's CloseRequested handler
/// uses this to let the window close instead of hiding to tray — otherwise the
/// hidden window keeps the process alive through session teardown, Windows
/// force-kills it after its timeout, and the force-kill skips `RunEvent::Exit`
/// (orphaning a loaded llama-server.exe child process).
#[cfg(target_os = "windows")]
pub fn system_is_shutting_down() -> bool {
    use windows::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_SHUTTINGDOWN};
    // SAFETY: GetSystemMetrics is a process-wide read of the system metrics
    // table and is safe to call from any thread without prerequisites.
    unsafe { GetSystemMetrics(SM_SHUTTINGDOWN) != 0 }
}

/// Non-Windows builds never prevent window close to hide to tray in the same
/// way (see main.rs), so the OS session path needs no special casing.
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
