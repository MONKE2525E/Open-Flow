//! Floating dictation-pill window lifecycle: creation, per-state show with
//! atomic resize/reposition, and the deliberately-never-hide idle path. Pulled
//! out of pipeline.rs verbatim; see the module docs on each function for the
//! WebView2/AppKit gotchas that dictate this ordering. Not unit-tested (GUI),
//! so changes here must be validated by running the app.

use tauri::{AppHandle, Emitter, Manager};

const PILL_WIDTH_POINTS: f64 = 380.0;
const PILL_HEIGHT_POINTS: f64 = 44.0;
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
const PILL_BOTTOM_GAP_POINTS: f64 = 16.0;

fn create_pill_if_needed(app: &AppHandle) {
    if app.get_webview_window("pill").is_some() {
        return;
    }
    let _ =
        tauri::WebviewWindowBuilder::new(app, "pill", tauri::WebviewUrl::App("/pill.html".into()))
            .title("")
            .inner_size(140.0, 44.0)
            .decorations(false)
            .transparent(true)
            .always_on_top(true)
            .skip_taskbar(true)
            .visible(false)
            .resizable(false)
            .shadow(false)
            .focused(false)
            .build();
}

pub(crate) fn show_pill(app: &AppHandle, state: &str) {
    show_pill_msg(app, state, None);
}

/// Shows the pill window in the given state, optionally carrying an error
/// message. The window is always kept at the same width regardless of state
/// (room enough for the error text to expand into) so it never needs to
/// resize or reposition after its first appearance — resizing a WebView2
/// window causes a visible repaint-lag flicker even when the native resize
/// itself is atomic, so the fix is to avoid triggering one at all rather
/// than to make it faster. Handsfree's click-capture zone is therefore wider
/// than the visible pill; clicks in the empty space around it are swallowed
/// instead of passing through while handsfree is active.
fn show_pill_msg(app: &AppHandle, state: &str, message: Option<&str>) {
    create_pill_if_needed(app);
    if let Some(pill) = app.get_webview_window("pill") {
        // Click-through for passive states so nothing behind the pill is blocked.
        // Handsfree needs real cursor events for its cancel/confirm buttons.
        pill.set_ignore_cursor_events(state != "handsfree").ok();

        // Size and position while still hidden, so the window appears already
        // in place instead of flashing at its previous geometry and jumping.
        let width = PILL_WIDTH_POINTS;

        // Resize and reposition are checked independently: primary_monitor()
        // can return None on some platforms (e.g. macOS) while the window is
        // still hidden, which would otherwise skip positioning on the first
        // call and then skip it again on every later call once needs_resize
        // is false — permanently mispositioning the pill. Falling back to a
        // scale factor of 1.0 keeps the resize check meaningful even before
        // monitor info is available.
        let monitor = pill.primary_monitor().ok().flatten();
        let scale_factor = monitor.as_ref().map(|m| m.scale_factor()).unwrap_or(1.0);

        // Most transitions (recording/processing/error/idle) share this same
        // width, so skip the resize when the window is already at the target
        // size — re-issuing identical set_size calls can still make the OS
        // window manager flicker.
        let needs_resize = pill
            .inner_size()
            .map(|cur| (cur.width as f64 - width * scale_factor).abs() > 1.0)
            .unwrap_or(true);

        // Target position (physical pixels) on the current monitor, if known.
        let target_pos = monitor.as_ref().map(|m| {
            let sz = m.size();
            let sf = m.scale_factor();
            let pos = m.position();
            let target_x = pos.x + ((sz.width as f64 - width * sf) / 2.0) as i32;
            let bottom_offset_points = pill_bottom_offset_points();
            let target_y = pos.y
                + (sz.height as f64 - (PILL_HEIGHT_POINTS + bottom_offset_points) * sf) as i32;
            (target_x, target_y)
        });

        let needs_reposition = match target_pos {
            Some((target_x, target_y)) => pill
                .outer_position()
                .map(|cur| (cur.x - target_x).abs() > 1 || (cur.y - target_y).abs() > 1)
                .unwrap_or(true),
            None => false,
        };

        // On Windows, resize and reposition are merged into a single
        // SetWindowPos call so the two are atomic if both are ever needed at
        // once (e.g. a monitor/DPI change). With a constant width this only
        // actually fires on the very first show, while still hidden.
        #[cfg(target_os = "windows")]
        {
            use windows::Win32::UI::WindowsAndMessaging::{
                SetWindowPos, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER,
            };

            if needs_resize || needs_reposition {
                if let Ok(hwnd) = pill.hwnd() {
                    let cx = (width * scale_factor).round() as i32;
                    let cy = (PILL_HEIGHT_POINTS * scale_factor).round() as i32;
                    let (x, y) = target_pos.unwrap_or((0, 0));

                    let mut flags = SWP_NOZORDER | SWP_NOACTIVATE;
                    if !needs_resize {
                        flags |= SWP_NOSIZE;
                    }
                    if !needs_reposition {
                        flags |= SWP_NOMOVE;
                    }

                    unsafe {
                        let _ = SetWindowPos(hwnd, None, x, y, cx, cy, flags);
                    }
                }
            }
        }

        #[cfg(not(target_os = "windows"))]
        {
            if needs_resize {
                pill.set_size(tauri::LogicalSize::new(width, PILL_HEIGHT_POINTS))
                    .ok();
            }
            if let Some((target_x, target_y)) = target_pos {
                if needs_reposition {
                    pill.set_position(tauri::PhysicalPosition::new(target_x, target_y))
                        .ok();
                }
            }
        }

        // Show the window before emitting state so WebView2 is active when it
        // receives the event. WebView2 suspends event processing while hidden;
        // emitting into a suspended view causes the first state to be dropped or
        // overtaken by the next emit (e.g. "recording" lost, only "processing" seen).
        // SW_SHOWNOACTIVATE: appears without stealing keyboard focus from
        // whatever window the user is dictating into.
        #[cfg(target_os = "windows")]
        {
            use windows::Win32::UI::WindowsAndMessaging::{ShowWindow, SW_SHOWNOACTIVATE};
            if let Ok(hwnd) = pill.hwnd() {
                unsafe {
                    let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
                }
            }
        }
        #[cfg(not(target_os = "windows"))]
        pill.show().ok();

        // macOS: `show()` (orderFront:) is ignored for a background app, so the
        // pill only appeared when Verenu was frontmost. Force it above the
        // active app's windows without stealing focus. AppKit window calls must
        // run on the main thread — show_pill is invoked from pipeline worker
        // threads, so dispatch there (a raw msg_send off-thread raises an ObjC
        // exception and aborts the process).
        #[cfg(target_os = "macos")]
        {
            let pill_for_main = pill.clone();
            let _ = app.run_on_main_thread(move || {
                if let Ok(ns_window) = pill_for_main.ns_window() {
                    crate::system::mac_app::float_pill_window(ns_window);
                }
            });
        }

        // Emit the message before the state so the pill has the error text
        // ready before it measures and animates open.
        if let Some(msg) = message {
            pill.emit("pill-error", msg).ok();
        }
        pill.emit("pill-state", state).ok();
    }
}

fn pill_bottom_offset_points() -> f64 {
    #[cfg(target_os = "macos")]
    {
        let dock_height = crate::system::mac_app::dock_height_points();
        dock_height + PILL_BOTTOM_GAP_POINTS
    }

    #[cfg(not(target_os = "macos"))]
    {
        64.0
    }
}

pub(crate) fn hide_pill(app: &AppHandle) {
    if let Some(pill) = app.get_webview_window("pill") {
        pill.emit("pill-state", "idle").ok();
        // Do not call pill.hide() — hiding the window suspends the WebView2
        // renderer. The next show_pill("recording") emit would then be lost
        // before WebView2 wakes up, causing only "processing" to appear.
        // The pill window is transparent + click-through in idle state, so
        // leaving it visible has no user-visible effect.
    }
}

pub(super) async fn show_error_pill(app: &AppHandle, msg: &str) {
    log::error!("pipeline error: {msg}");
    app.emit("verenu:error", msg).ok();
    // Auto-hide is handled by the frontend (PillApp.svelte), which can check
    // its own state before reverting to idle, avoiding a race where a new
    // recording session's pill gets hidden by this error's timeout.
    show_pill_msg(app, "error", Some(msg));
}

/// Shows the pill in error state for a quality-gate rejection without
/// focusing the main window or blocking the pipeline task.
pub(super) fn reject_with_pill(app: &AppHandle, msg: &str) {
    app.emit("verenu:error", msg).ok();
    // Auto-hide is handled by the frontend (PillApp.svelte), matching
    // show_error_pill's clean implementation.
    show_pill_msg(app, "error", Some(msg));
}
