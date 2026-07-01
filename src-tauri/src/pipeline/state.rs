use super::*;
use crate::core::window_geometry::WindowTarget;
use crate::pipeline::pill_position::PillPlacement;

pub(super) const RETRY_WINDOW: std::time::Duration = std::time::Duration::from_secs(600);
// ---------- shared state ----------

pub struct AppState {
    pub session: Option<audio::RecordingSession>,
    pub exclusive_mic_session_id: Option<u64>,
    pub starting: bool,
    pub handless: bool,
    pub target: WindowTarget,
    pub pill_placement: Option<PillPlacement>,
    pub pill_placement_stale: bool,
    pub retry_capture: Option<RetryCapture>,
}

pub type SharedState = Arc<Mutex<AppState>>;

#[derive(Clone)]
pub struct RetryCapture {
    pub wav: bytes::Bytes,
    pub captured_at: std::time::Instant,
    pub duration_ms: u64,
    pub target: WindowTarget,
    pub process_name: String,
    pub profile: String,
    pub app_context: Option<String>,
    pub caps_lock_on: bool,
}

pub(super) fn lock_state(state: &SharedState) -> anyhow::Result<MutexGuard<'_, AppState>> {
    state
        .lock()
        .map_err(|_| anyhow::anyhow!("Recording state lock was poisoned"))
}

pub(super) fn emit_pipeline_failed(app: &AppHandle) {
    app.emit(
        "verenu:pipeline-failed",
        Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true),
    )
    .ok();
}

/// Returns true if our own process currently owns the foreground window.
/// Catches the case where the user opened the Verenu main window while
/// transcribing — if we tried to Ctrl+V / Cmd+V in that state the paste would
/// land in our own WebView and silently disappear.
pub(super) fn foreground_is_own_process() -> bool {
    #[cfg(windows)]
    unsafe {
        use windows::Win32::UI::WindowsAndMessaging::{
            GetForegroundWindow, GetWindowThreadProcessId,
        };
        let hwnd = GetForegroundWindow();
        if hwnd.0.is_null() {
            return false;
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        pid == std::process::id()
    }
    #[cfg(not(windows))]
    {
        false
    }
}

/// Returns true if `hwnd` belongs to our own process.
/// Catches the case where recording was started while Verenu itself had focus.
#[cfg_attr(not(windows), allow(unused_variables))]
pub(super) fn hwnd_is_own_process(hwnd: usize) -> bool {
    if hwnd == 0 {
        return false;
    }
    #[cfg(windows)]
    unsafe {
        use windows::Win32::Foundation::HWND;
        use windows::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId;
        let mut pid = 0u32;
        GetWindowThreadProcessId(HWND(hwnd as *mut _), Some(&mut pid));
        pid == std::process::id()
    }
    #[cfg(not(windows))]
    {
        false
    }
}
