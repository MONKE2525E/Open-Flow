//! Floating dictation-pill window lifecycle: creation, per-state show with
//! atomic resize/reposition, and the deliberately-never-hide idle path. The
//! placement math lives in `pill_position.rs` so the monitor selection can be
//! tested without dragging the window lifecycle code along with it.

use super::SharedState;
use crate::pipeline::pill_position::PillPlacement;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use tauri::{AppHandle, Emitter, Manager, Runtime, WebviewWindow};

const PILL_WIDTH_POINTS: f64 = 380.0;
const PILL_HEIGHT_POINTS: f64 = 44.0;

/// Guards the animated path's deferred reveal against being overtaken by a
/// newer `show_pill_msg` call. The animated cross-monitor move (see
/// `pill_animation.rs`) defers its `reveal_pill` until the ~180ms tween
/// lands; if the dictation state moves on (e.g. recording -> processing, or
/// `hide_pill`) before that tween finishes, the newer call already revealed
/// the correct state synchronously (since `next_pill_placement` returns
/// `None` once the placement is no longer stale), and the stale deferred
/// reveal must not clobber it by re-emitting the *old* state afterward.
/// Every `show_pill_msg` call claims a new generation; a deferred reveal
/// only runs if its generation is still current.
static REVEAL_GEN: AtomicU64 = AtomicU64::new(0);
/// Tracks whether the pill is currently showing a non-idle frontend state.
/// The native window itself stays visible even in idle so WebView2 doesn't
/// suspend, which makes `pill.is_visible()` a bad proxy for "the user can
/// already see the pill."
static PILL_VISUALLY_ACTIVE: AtomicBool = AtomicBool::new(false);

/// Whether the pill has ever had a real, monitor-resolved placement applied
/// in this process. `false` only for the very first `show_pill_msg` call —
/// after that, even a reveal that follows a `hide_pill` idle cycle still has
/// real (if stale) geometry on screen, so a monitor change found on that
/// reveal is still worth animating into rather than jumping. Unlike
/// `PILL_VISUALLY_ACTIVE`, this never resets back to `false`.
#[cfg(target_os = "windows")]
static PILL_PLACED_ONCE: AtomicBool = AtomicBool::new(false);

fn create_pill_if_needed(app: &AppHandle) {
    if app.get_webview_window("pill").is_some() {
        return;
    }
    match tauri::WebviewWindowBuilder::new(app, "pill", tauri::WebviewUrl::App("/pill.html".into()))
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
        .build()
    {
        Ok(pill) => harden_pill_window(&pill),
        Err(err) => log::warn!("Failed to create dictation pill window: {err}"),
    }
}

#[cfg(target_os = "windows")]
fn harden_pill_window<R: Runtime>(pill: &WebviewWindow<R>) {
    use windows::Win32::{
        Foundation::{GetLastError, SetLastError, WIN32_ERROR},
        UI::WindowsAndMessaging::{
            GetWindowLongPtrW, SetWindowLongPtrW, SetWindowPos, GWL_EXSTYLE, HWND_TOPMOST,
            SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, WS_EX_APPWINDOW,
            WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
        },
    };

    let Ok(hwnd) = pill.hwnd() else {
        return;
    };

    unsafe {
        SetLastError(WIN32_ERROR(0));
        let current = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        if current == 0 {
            let err = GetLastError();
            if err != WIN32_ERROR(0) {
                log::warn!("Failed to read pill extended window styles: {err:?}");
                return;
            }
        }
        let desired = (current | WS_EX_NOACTIVATE.0 as isize | WS_EX_TOOLWINDOW.0 as isize)
            & !(WS_EX_APPWINDOW.0 as isize);

        if desired != current {
            let _ = SetWindowLongPtrW(hwnd, GWL_EXSTYLE, desired);
        }

        let _ = SetWindowPos(
            hwnd,
            Some(HWND_TOPMOST),
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_FRAMECHANGED,
        );
    }
}

#[cfg(not(target_os = "windows"))]
fn harden_pill_window<R: Runtime>(_pill: &WebviewWindow<R>) {}

pub(crate) fn show_pill(app: &AppHandle, state: &str) {
    show_pill_msg(app, state, None);
}

/// Updates an already-visible pill without repeating the native reveal and
/// placement work. Used for in-session state changes such as recording to
/// hands-free, where re-running `show_pill` can produce a one-frame flicker.
pub(crate) fn update_pill_state(app: &AppHandle, state: &str) {
    let Some(pill) = app.get_webview_window("pill") else {
        show_pill(app, state);
        return;
    };

    // Reuse the native reveal sequence without recalculating placement. The
    // Windows window can remain logically visible while its compositor surface
    // is behind another window after click-through is changed, so a conditional
    // `show()` is not enough here. `reveal_pill` uses SW_SHOWNOACTIVATE and
    // HWND_TOPMOST, which re-presents the existing window without activating it
    // or running the placement animation.
    reveal_pill(app, &pill, state, None);
}

/// Shows the pill window in the given state, optionally carrying an error
/// message. The window is always kept at the same width regardless of state
/// (room enough for the error text to expand into), so within a single
/// monitor it never needs to resize or reposition after its first
/// appearance. Handsfree's click-capture zone is therefore wider than the
/// visible pill; clicks in the empty space around it are swallowed instead
/// of passing through while handsfree is active. Moving to a different
/// monitor, whether or not its scale factor differs, animates the move on
/// Windows (see `pill_animation.rs`) instead of jumping instantly, since an
/// instant cross-monitor move on this window either visibly snapped
/// (same-DPI repositions) or made WebView2 stutter recreating its swap
/// chain (cross-DPI resizes) - the latter is what showed up as a clipped
/// pill on the first dictation after a monitor change, since `hide_pill`
/// resets `PILL_VISUALLY_ACTIVE` between dictations even though the window
/// keeps its stale geometry the whole time it's idle. Animates whenever the
/// pill has been placed at least once before (`PILL_PLACED_ONCE`) and the
/// resolved placement actually differs from where it currently sits - that
/// covers both a monitor change mid-session and one only discovered on the
/// next reveal after an idle cycle. Only the very first reveal of the whole
/// process skips the animation, since nothing has been shown yet for it to
/// glide from.
fn show_pill_msg(app: &AppHandle, state: &str, message: Option<&str>) {
    create_pill_if_needed(app);
    let Some(pill) = app.get_webview_window("pill") else {
        return;
    };

    let generation = REVEAL_GEN.fetch_add(1, Ordering::SeqCst).wrapping_add(1);
    #[cfg(not(target_os = "windows"))]
    let _ = generation; // only the Windows animated path below reads this.

    let Some(placement) = next_pill_placement(app, &pill) else {
        reveal_pill(app, &pill, state, message);
        return;
    };

    // Marks "the pill has a real placement now" as soon as we have one to
    // apply, independent of whether `current_placement()` below succeeds.
    // Gating this swap on that `if let` instead (as an earlier version did)
    // meant a `None` read on the very first call - e.g. WebView2 not yet
    // settled right after `create_pill_if_needed` - left the flag `false`
    // forever, so the *next* reveal would also skip animating, thinking
    // *it* was the first ever placement.
    #[cfg(target_os = "windows")]
    let already_placed = PILL_PLACED_ONCE.swap(true, Ordering::SeqCst);

    #[cfg(target_os = "windows")]
    if let Some(current) = super::pill_position::current_placement(&pill) {
        let needs_animated_move = super::pill_position::should_animate_cross_monitor_move(
            already_placed,
            current,
            placement,
        );

        // Temporary diagnostic for issue #161.
        if crate::system::logger::is_verbose() {
            log::debug!(
                "pill show_pill_msg: state={state} already_placed={already_placed} current={current:?} target={placement:?} animate={needs_animated_move}"
            );
        }

        if needs_animated_move {
            let app = app.clone();
            let state = state.to_string();
            let message = message.map(str::to_string);
            super::pill_animation::animate_pill_placement(&pill, current, placement, move || {
                if REVEAL_GEN.load(Ordering::SeqCst) != generation {
                    return; // a newer show_pill_msg call already revealed the real state.
                }
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
fn reveal_pill(app: &AppHandle, pill: &WebviewWindow, state: &str, message: Option<&str>) {
    #[cfg(not(target_os = "macos"))]
    let _ = app; // only the macOS float-above-foreground-app step below reads this.
    PILL_VISUALLY_ACTIVE.store(true, Ordering::SeqCst);

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
        use windows::Win32::UI::WindowsAndMessaging::{
            SetWindowPos, ShowWindow, HWND_TOPMOST, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
            SW_SHOWNOACTIVATE,
        };
        if let Ok(hwnd) = pill.hwnd() {
            unsafe {
                let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
                let _ = SetWindowPos(
                    hwnd,
                    Some(HWND_TOPMOST),
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
                );
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
        // Invalidate any in-flight animated move's deferred reveal - without
        // this, a tween started by an earlier show_pill_msg call could land
        // after this "idle" and re-emit its own (now stale) state, reverting
        // the pill right back to looking like it's recording/processing.
        // Also stop the tween itself from continuing to move the window.
        PILL_VISUALLY_ACTIVE.store(false, Ordering::SeqCst);
        REVEAL_GEN.fetch_add(1, Ordering::SeqCst);
        super::pill_animation::cancel_pending_pill_tween();

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
