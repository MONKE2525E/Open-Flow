//! Application startup glue: readiness handshake, watchdog, data-directory
//! resolution, and relaunch/update-helper handling. Split out of main.rs so
//! main.rs stays focused on module wiring and the Tauri builder.

use std::sync::{atomic::AtomicBool, Arc};
use tauri::{AppHandle, Manager};

#[cfg(target_os = "windows")]
use std::sync::atomic::Ordering;
#[cfg(target_os = "windows")]
use std::time::Duration;

/// Startup readiness reported by each Tauri WebView after its frontend has
/// mounted successfully. A backend can be fully alive while WebView2 is
/// showing a stale connection-refused page, so backend liveness alone is not
/// enough to declare startup successful.
#[derive(Clone, Default)]
pub(crate) struct FrontendReadiness {
    pub(crate) main: Arc<AtomicBool>,
    /// Diagnostic state only. The pill is created after the main frontend has
    /// passed the startup gate, so its readiness is not a separate gate.
    pub(crate) pill: Arc<AtomicBool>,
}

#[cfg(target_os = "windows")]
impl FrontendReadiness {
    fn main_ready(&self) -> bool {
        self.main.load(Ordering::Acquire)
    }

    #[cfg(debug_assertions)]
    fn reset(&self) {
        self.main.store(false, Ordering::Release);
        self.pill.store(false, Ordering::Release);
    }
}

pub(crate) fn show_main_window(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        if w.is_minimized().unwrap_or(false) {
            w.unminimize().ok();
        }
        #[cfg(target_os = "macos")]
        {
            crate::system::mac_app::set_regular_activation_policy_on_main_thread(app);
            crate::system::mac_app::activate_current_app_on_main_thread(app);
        }
        #[cfg(target_os = "windows")]
        log::info!("Windows main HWND first show requested");
        w.show().ok();
        w.set_focus().ok();
    }
}

#[cfg(target_os = "macos")]
pub(crate) fn hide_main_window(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        crate::system::mac_app::set_accessory_activation_policy_on_main_thread(app);
        w.hide().ok();
    }
}

/// Canonical per-user data directory for Verenu, following each OS's convention.
///
/// This is the single source of truth for where Verenu stores its SQLite
/// database. Everything that touches the DB — startup `open`, the in-app
/// updater's pre-update backup, etc. — MUST derive its path from here (via
/// [`app_db_path`]). Do NOT use Tauri's `app.path().app_data_dir()` for the
/// database: that resolves against the bundle identifier and is not guaranteed
/// to equal this path, so backups would silently target a different file.
fn app_data_dir_override() -> Option<std::path::PathBuf> {
    std::env::var_os("VERENU_APP_DATA_DIR_OVERRIDE").map(std::path::PathBuf::from)
}

#[cfg(all(target_os = "windows", not(debug_assertions)))]
fn startup_recovery_was_attempted() -> bool {
    std::env::args_os().any(|arg| arg == "--startup-recovery-attempted")
}

#[cfg(all(debug_assertions, target_os = "windows"))]
async fn wait_for_dev_frontend() -> bool {
    let Ok(client) = reqwest::Client::builder()
        .connect_timeout(Duration::from_millis(500))
        .timeout(Duration::from_secs(1))
        .build()
    else {
        return false;
    };

    let deadline = tokio::time::Instant::now() + Duration::from_secs(8);
    while tokio::time::Instant::now() < deadline {
        if let Ok(response) = client.get("http://127.0.0.1:1420/").send().await {
            if response.status().is_success() {
                return true;
            }
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    false
}

#[cfg(target_os = "windows")]
pub(crate) fn start_frontend_watchdog(app: &AppHandle, readiness: FrontendReadiness) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let reveal_windows = || {
            // The main window is the startup gate. Create the pill only after
            // the main UI is healthy because a hidden WebView2 renderer can
            // suspend before it reports its own readiness.
            show_main_window(&app);
            if !crate::pipeline::failover::offer_restored_capture_pill(&app) {
                crate::pipeline::show_pill(&app, "idle");
            }
        };

        // WebView2 normally mounts in well under a second. This grace period
        // leaves room for a cold Vite/WebView2 start without delaying normal
        // startup or flashing a recovery window.
        let deadline = tokio::time::Instant::now() + Duration::from_secs(12);
        while tokio::time::Instant::now() < deadline {
            if readiness.main_ready() {
                log::debug!("startup handshake completed");
                reveal_windows();
                return;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }

        if readiness.main_ready() {
            reveal_windows();
            return;
        }

        // In development, the WebView can fail its first navigation while
        // Vite is restarting or optimizing dependencies. Reload the existing
        // windows once the server is reachable before restarting the entire
        // process. This also avoids creating an orphaned app if the parent
        // `tauri dev` session has already stopped its Vite child.
        #[cfg(debug_assertions)]
        if wait_for_dev_frontend().await {
            readiness.reset();
            if let Some(window) = app.get_webview_window("main") {
                window.reload().ok();
            }

            let retry_deadline = tokio::time::Instant::now() + Duration::from_secs(8);
            while tokio::time::Instant::now() < retry_deadline {
                if readiness.main_ready() {
                    reveal_windows();
                    return;
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        }

        #[cfg(debug_assertions)]
        log::error!(
            "startup handshake did not complete in development; keeping the dev process alive for diagnosis"
        );
        reveal_windows();

        #[cfg(not(debug_assertions))]
        {
            if startup_recovery_was_attempted() {
                log::error!(
                    "startup handshake did not complete after the automatic recovery attempt; leaving the app running for diagnosis"
                );
                reveal_windows();
                return;
            }

            log::warn!(
                "startup handshake timed out: main_ready={} pill_ready={}; relaunching once",
                readiness.main.load(Ordering::Acquire),
                readiness.pill.load(Ordering::Acquire)
            );
            crate::app_tray::relaunch_for_startup_recovery(&app);
        }
    });
}

#[cfg(not(target_os = "windows"))]
pub(crate) fn start_frontend_watchdog(app: &AppHandle, _readiness: FrontendReadiness) {
    if !crate::pipeline::failover::offer_restored_capture_pill(app) {
        crate::pipeline::show_pill(app, "idle");
    }
    show_main_window(app);
}

#[cfg(windows)]
pub(crate) fn app_data_dir() -> std::path::PathBuf {
    if let Some(path) = app_data_dir_override() {
        return path;
    }
    std::env::var("APPDATA")
        .map(|p| std::path::PathBuf::from(p).join("Verenu"))
        .unwrap_or_else(|_| std::path::PathBuf::from("."))
}

#[cfg(target_os = "macos")]
pub(crate) fn app_data_dir() -> std::path::PathBuf {
    if let Some(path) = app_data_dir_override() {
        return path;
    }
    std::env::var("HOME")
        .map(|h| std::path::PathBuf::from(h).join("Library/Application Support/Verenu"))
        .unwrap_or_else(|_| std::path::PathBuf::from("."))
}

#[cfg(not(any(windows, target_os = "macos")))]
pub(crate) fn app_data_dir() -> std::path::PathBuf {
    if let Some(path) = app_data_dir_override() {
        return path;
    }
    std::env::var("HOME")
        .map(|h| std::path::PathBuf::from(h).join(".config/Verenu"))
        .unwrap_or_else(|_| std::path::PathBuf::from("."))
}

/// Canonical path to the SQLite database file. Use this everywhere.
pub(crate) fn app_db_path() -> std::path::PathBuf {
    app_data_dir().join("verenu.db")
}

/// Reports a fatal startup error where a packaged user can see it before
/// aborting. Debug builds reach stderr via the panic; Windows release builds
/// have no console (`windows_subsystem = "windows"`), so the error also goes
/// to a message box — otherwise an unwritable data directory or a
/// non-quarantinable database fails the app with no visible reason. Only the
/// message is shown, never full user-local paths.
pub(crate) fn fatal_startup_error(message: &str) -> ! {
    log::error!("{message}");
    #[cfg(target_os = "windows")]
    {
        use windows::core::PCWSTR;
        use windows::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONERROR, MB_OK};

        let mut text: Vec<u16> = format!("{message}\n\nVerenu must close.")
            .encode_utf16()
            .collect();
        text.push(0);
        let mut title: Vec<u16> = "Verenu — startup failed".encode_utf16().collect();
        title.push(0);
        unsafe {
            MessageBoxW(
                None,
                PCWSTR(text.as_ptr()),
                PCWSTR(title.as_ptr()),
                MB_OK | MB_ICONERROR,
            );
        }
    }
    panic!("{message}");
}

#[cfg(target_os = "windows")]
pub(crate) fn cleanup_update_helper_if_requested() {
    let Some(helper) = std::env::args_os().find_map(|arg| {
        let text = arg.to_string_lossy();
        text.strip_prefix("--cleanup-update-helper=")
            .map(std::path::PathBuf::from)
    }) else {
        return;
    };

    // The helper has just spawned this process and is exiting. Retry briefly so
    // Windows has released the helper image before removing its temp copy.
    for _ in 0..20 {
        match std::fs::remove_file(&helper) {
            Ok(()) => return,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return,
            Err(_) => std::thread::sleep(std::time::Duration::from_millis(100)),
        }
    }
}

#[cfg(target_os = "windows")]
pub(crate) fn wait_for_relaunch_parent_exit() {
    use windows::Win32::Foundation::{
        CloseHandle, ERROR_INVALID_PARAMETER, WAIT_OBJECT_0, WAIT_TIMEOUT,
    };
    use windows::Win32::System::Threading::{
        OpenProcess, WaitForSingleObject, PROCESS_SYNCHRONIZE,
    };

    let Some(parent_pid) = relaunch_parent_pid() else {
        return;
    };

    unsafe {
        let handle = match OpenProcess(PROCESS_SYNCHRONIZE, false, parent_pid) {
            Ok(handle) => handle,
            Err(err) => {
                if err.code() != windows::core::HRESULT::from_win32(ERROR_INVALID_PARAMETER.0) {
                    early_startup_warn(&format!(
                        "Relaunch requested but could not open parent process {parent_pid}: {err}"
                    ));
                }
                return;
            }
        };

        // The old instance may take longer than 5 s to exit (slow model
        // unload during RunEvent::Exit). Waiting on the same handle in slices
        // up to a 15 s total budget keeps this bounded while still refusing to
        // open the database while the parent holds it: two live connections to
        // the same SQLite file would let the single-instance plugin hand off
        // to the dying parent and leave the user with no running app.
        let deadline = std::time::Instant::now() + Duration::from_secs(15);
        let wait_result = loop {
            let result = WaitForSingleObject(handle, 5_000);
            // WAIT_TIMEOUT means the parent is still alive: keep waiting up to
            // the deadline. Any other result is terminal (signaled, or a
            // WAIT_FAILED that would otherwise busy-spin), so leave the loop.
            if result != WAIT_TIMEOUT || std::time::Instant::now() >= deadline {
                break result;
            }
        };
        if wait_result != WAIT_OBJECT_0 {
            early_startup_warn(&format!(
                "Relaunch waited for parent process {parent_pid} but got result {}",
                wait_result.0
            ));
        }

        let _ = CloseHandle(handle);
    }
}

#[cfg(target_os = "windows")]
fn relaunch_parent_pid() -> Option<u32> {
    let mut args = std::env::args_os().skip(1);

    while let Some(arg) = args.next() {
        let Some(text) = arg.to_str() else {
            continue;
        };

        if let Some(value) = text.strip_prefix("--relaunch-parent-pid=") {
            if let Ok(pid) = value.parse::<u32>() {
                return Some(pid);
            }
            early_startup_warn(&format!(
                "Ignoring invalid relaunch parent pid argument: {value}"
            ));
            return None;
        }

        if text == "--relaunch-parent-pid" {
            let Some(value) = args.next() else {
                early_startup_warn("Ignoring missing relaunch parent pid value");
                return None;
            };

            match value.to_string_lossy().parse::<u32>() {
                Ok(pid) => return Some(pid),
                Err(_) => {
                    early_startup_warn(&format!(
                        "Ignoring invalid relaunch parent pid argument: {}",
                        value.to_string_lossy()
                    ));
                    return None;
                }
            }
        }
    }

    None
}

#[cfg(target_os = "windows")]
fn early_startup_warn(message: &str) {
    use windows::core::PCWSTR;
    use windows::Win32::System::Diagnostics::Debug::OutputDebugStringW;

    eprintln!("WARN: {message}");

    let mut wide: Vec<u16> = format!("WARN: {message}").encode_utf16().collect();
    wide.push(0);

    unsafe {
        OutputDebugStringW(PCWSTR(wide.as_ptr()));
    }
}
