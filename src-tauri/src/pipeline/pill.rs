//! Floating dictation-pill window lifecycle: creation, per-state show with
//! atomic resize/reposition, and the deliberately-never-hide idle path. The
//! placement math lives in `pill_position.rs` so the monitor selection can be
//! tested without dragging the window lifecycle code along with it.

use super::SharedState;
use crate::pipeline::pill_position::PillPlacement;
use tauri::{AppHandle, Emitter, Manager, Runtime, WebviewWindow};

const PILL_WIDTH_POINTS: f64 = 380.0;
const PILL_HEIGHT_POINTS: f64 = 44.0;

fn create_pill_if_needed(app: &AppHandle) {
    if app.get_webview_window("pill").is_some() {
        return;
    }
    let _ =
        tauri::WebviewWindowBuilder::new(app, "pill", tauri::WebviewUrl::App("/pill.html".into()))
            .title("")
            .inner_size(PILL_WIDTH_POINTS, PILL_HEIGHT_POINTS)
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
/// (room enough for the error text to expand into), so within a single
/// monitor it never needs to resize or reposition after its first
/// appearance. Handsfree's click-capture zone is therefore wider than the
/// visible pill; clicks in the empty space around it are swallowed instead
/// of passing through while handsfree is active. Moving to a monitor with a
/// different scale factor still resizes it — on Windows that resize is
/// animated (see `pill_animation.rs`) instead of jumping instantly, since an
/// instant cross-DPI resize on this always-visible window made WebView2
/// visibly stutter recreating its swap chain.
fn show_pill_msg(app: &AppHandle, state: &str, message: Option<&str>) {
    create_pill_if_needed(app);
    let Some(pill) = app.get_webview_window("pill") else {
        return;
    };

    let Some(placement) = next_pill_placement(app, &pill) else {
        reveal_pill(app, &pill, state, message);
        return;
    };

    #[cfg(target_os = "windows")]
    if let Some(current) = super::pill_position::current_placement(&pill) {
        let needs_animated_move =
            super::pill_position::dimension_changed(current.width as f64, placement.width as f64)
                || super::pill_position::dimension_changed(
                    current.height as f64,
                    placement.height as f64,
                );

        if needs_animated_move {
            let app = app.clone();
            let state = state.to_string();
            let message = message.map(str::to_string);
            super::pill_animation::animate_pill_placement(&pill, current, placement, move || {
                if let Some(pill) = app.get_webview_window("pill") {
                    reveal_pill(&app, &pill, &state, message.as_deref());
                }
            });
            return;
        }
    }

    super::pill_position::apply_pill_placement(&pill, placement);
    reveal_pill(app, &pill, state, message);
}

/// The non-placement part of showing the pill: click-through flag, bringing
/// it to the front without stealing focus, and emitting the state (plus
/// optional error message) the frontend reacts to. Shared by both the
/// synchronous same-monitor path and the animated cross-monitor path in
/// `show_pill_msg` — the animated path just defers this until its tween
/// lands.
fn reveal_pill(_app: &AppHandle, pill: &WebviewWindow, state: &str, message: Option<&str>) {
    // Click-through for passive states so nothing behind the pill is blocked.
    // Handsfree needs real cursor events for its cancel/confirm buttons.
    pill.set_ignore_cursor_events(state != "handsfree").ok();

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
    // run on the main thread - show_pill is invoked from pipeline worker
    // threads, so dispatch there (a raw msg_send off-thread raises an ObjC
    // exception and aborts the process).
    #[cfg(target_os = "macos")]
    {
        let pill_for_main = pill.clone();
        let _ = _app.run_on_main_thread(move || {
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

fn next_pill_placement<R: Runtime>(
    app: &AppHandle,
    pill: &WebviewWindow<R>,
) -> Option<PillPlacement> {
    let (target_point, cached, stale) = {
        let state = app.try_state::<SharedState>()?;
        let guard = state.lock().ok()?;
        (
            guard.target.display_point,
            guard.pill_placement,
            guard.pill_placement_stale,
        )
    };

    if !stale && cached.is_some() {
        return None;
    }

    let resolved = super::pill_position::resolve_pill_placement(pill, target_point).or(cached);

    if let Some(placement) = resolved {
        if stale || cached != Some(placement) {
            if let Some(state) = app.try_state::<SharedState>() {
                if let Ok(mut guard) = state.lock() {
                    guard.pill_placement = Some(placement);
                    guard.pill_placement_stale = false;
                }
            }
        }
    }

    resolved
}

pub(crate) fn hide_pill(app: &AppHandle) {
    if let Some(pill) = app.get_webview_window("pill") {
        pill.emit("pill-state", "idle").ok();
        // Do not call pill.hide() - hiding the window suspends the WebView2
        // renderer. The next show_pill("recording") emit would then be lost
        // before WebView2 wakes up, causing only "processing" to appear.
        // The pill window is transparent + click-through in idle state, so
        // leaving it visible has no user-visible effect.
    }
}

pub(super) async fn show_error_pill(app: &AppHandle, msg: &str) {
    log::error!("pipeline error: {msg}");
    app.emit("verenu:error", msg).ok();
    if super::start_stop_sounds_enabled(app) {
        crate::media::sound::play(crate::media::sound::SoundCue::Error);
    }
    // Auto-hide is handled by the frontend (PillApp.svelte), which can check
    // its own state before reverting to idle, avoiding a race where a new
    // recording session's pill gets hidden by this error's timeout.
    show_pill_msg(app, "error", Some(msg));
}

/// Shows the pill in error state for a quality-gate rejection without
/// focusing the main window or blocking the pipeline task.
pub(super) fn reject_with_pill(app: &AppHandle, msg: &str) {
    app.emit("verenu:error", msg).ok();
    // A quality-gate rejection (too short / too quiet) is an error from the
    // user's point of view, so play the error cue here too — not just on API
    // failures in show_error_pill.
    if super::start_stop_sounds_enabled(app) {
        crate::media::sound::play(crate::media::sound::SoundCue::Error);
    }
    // Auto-hide is handled by the frontend (PillApp.svelte), matching
    // show_error_pill's clean implementation.
    show_pill_msg(app, "error", Some(msg));
}
