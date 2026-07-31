//! Thin wrapper over tauri-plugin-notification for the few app-level system
//! notifications Verenu raises (currently: a local model finished downloading).
//! Notifications are best-effort — a denied/unavailable permission must never
//! turn into a pipeline or download error, so every failure is swallowed and
//! logged at debug.

use tauri::AppHandle;
use tauri_plugin_notification::NotificationExt;

/// Raises a "download complete" system notification. Fires regardless of window
/// focus/visibility (that's the point — the user may have tabbed away during a
/// multi-GB download), so it's issued from the backend rather than the WebView,
/// whose renderer can be suspended while the main window is hidden.
pub fn notify_model_download_complete(app: &AppHandle, model_name: &str) {
    let result = app
        .notification()
        .builder()
        .title("Model ready")
        .body(format!(
            "{model_name} finished downloading and is ready to use."
        ))
        .show();
    if let Err(err) = result {
        log::debug!("notify: model download-complete notification failed: {err}");
    }
}
