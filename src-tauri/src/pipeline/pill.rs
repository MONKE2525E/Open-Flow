//! Floating dictation-pill window lifecycle: creation, per-state show with
//! atomic resize/reposition, and the deliberately-never-hide idle path. The
//! placement math lives in `pill_position.rs` so the monitor selection can be
//! tested without dragging the window lifecycle code along with it.

use super::SharedState;
use crate::pipeline::pill_position::PillPlacement;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager, Runtime, WebviewWindow};

/// Initial window size at creation. Kept in step with the state defaults so
/// the window is never created wider than the content it will hold — the
/// frontend re-reports the real content size as soon as it mounts.
const PILL_WIDTH_POINTS: f64 = super::DEFAULT_PILL_WIDTH_POINTS;
const PILL_HEIGHT_POINTS: f64 = super::DEFAULT_PILL_HEIGHT_POINTS;

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

/// Holds a resolved tone-profile label until the reveal that should carry it
/// actually runs. `show_pill`'s cross-monitor move animates the window into
/// place and only calls `reveal_pill` (which emits `pill-state`) once that
/// tween lands, deferred well past the moment the caller finishes its own
/// synchronous call — a profile emitted directly at the call site could land
/// either before or after that deferred `pill-state`, and the frontend
/// unconditionally clears `profileLabel` on `pill-state: recording`, so a
/// profile that beat it there got silently wiped. Queuing it here and only
/// emitting it from inside `reveal_pill`, right after `pill-state`, makes the
/// ordering correct regardless of which path a given reveal takes.
static PENDING_PILL_PROFILE: Mutex<Option<String>> = Mutex::new(None);

/// Queues a tone-profile label to ride along with whichever reveal happens
/// next, instead of emitting it immediately (see `PENDING_PILL_PROFILE`).
/// Whether the pill *should* currently hold real OS keyboard focus, per our
/// own state machine — not whether Windows actually still reports it
/// focused. `set_pill_focusable(false)` only flips `WS_EX_NOACTIVATE` back
/// on to block *future* activation; Windows does not auto-return focus to
/// whatever was focused before we took it, so `pill.is_focused()` can keep
/// reading true long after the repair UI that needed it has closed. The
/// hotkey's "is the repair pill the thing I'm dictating into right now"
/// check (app_hotkey.rs) needs this deterministic flag instead, or a stale
/// true reading there routes an unrelated dictation into the repair pill.
static PILL_WANTS_REPAIR_FOCUS: AtomicBool = AtomicBool::new(false);

pub(crate) fn pill_wants_repair_focus() -> bool {
    PILL_WANTS_REPAIR_FOCUS.load(Ordering::SeqCst)
}

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
        Ok(pill) => {
            // Keep the WebView client area transparent even when Windows
            // switches the window from click-through to interactive. Without
            // an explicit native colour, WebView2 can briefly repaint the
            // newly interactive surface as an opaque rectangle around the
            // capsule.
            pill.set_background_color(Some(tauri::utils::config::Color(0, 0, 0, 0))).ok();
            harden_pill_window(&pill);

            // The repair pill actively steals OS keyboard focus while
            // waiting on typed input (see set_pill_focusable), but never had
            // any way to notice the user giving up on it by simply clicking
            // into a different app's text box — Windows doesn't return focus
            // on its own, so the pill kept it, and a normal dictation
            // afterward silently pasted into the still-focused repair
            // textarea instead of wherever the user actually clicked. React
            // to losing OS focus the same way the X button/click-away/Escape
            // already do: abandon the repair session outright.
            //
            // Debounced, not immediate: growing the textarea's content
            // resizes the native window (see measureAndResize/set_pill_size),
            // and that resize was itself observed to raise a transient
            // Focused(false)/(true) blip with no real user action behind it —
            // acting on it immediately cancelled an in-progress complaint out
            // from under the user while they were still typing. Waiting a
            // beat and rechecking real focus catches only a genuine
            // click-away; a resize-induced blip has already resolved by then.
            let app_for_blur = app.clone();
            pill.on_window_event(move |event| {
                if !matches!(event, tauri::WindowEvent::Focused(false)) {
                    return;
                }
                if !pill_wants_repair_focus() {
                    return;
                }
                let app_for_check = app_for_blur.clone();
                tauri::async_runtime::spawn(async move {
                    tokio::time::sleep(std::time::Duration::from_millis(250)).await;
                    if !pill_wants_repair_focus() {
                        return; // resolved (applied/cancelled/expired) in the meantime
                    }
                    let Some(pill) = app_for_check.get_webview_window("pill") else {
                        return;
                    };
                    if pill.is_focused().unwrap_or(true) {
                        return; // focus came back — the blur was transient
                    }
                    let Some(state) = app_for_check.try_state::<SharedState>() else {
                        return;
                    };
                    let has_repair_session = state.inner().lock().is_ok_and(|st| st.repair.is_some());
                    if has_repair_session {
                        super::clear_repair(state.inner());
                        hide_pill(&app_for_check);
                    }
                });
            });
        }
        Err(err) => log::warn!("Failed to create dictation pill window: {err}"),
    }
}

#[cfg(target_os = "windows")]
fn harden_pill_window<R: Runtime>(pill: &WebviewWindow<R>) {
    use windows::Win32::{
        Foundation::{GetLastError, SetLastError, WIN32_ERROR},
        Graphics::Dwm::{
            DwmSetWindowAttribute, DWMWA_BORDER_COLOR, DWMWA_COLOR_NONE,
            DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_DONOTROUND,
        },
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
        // Windows 11 draws its own accent-colored active-window border and
        // rounded-corner frame around any top-level window once it becomes
        // focused — which the repair-input state does deliberately for text
        // entry. Left alone, that native frame shows up as a pale rounded
        // outline larger than (and misaligned with) the custom-drawn card,
        // since it sits outside the transparent WebView content entirely.
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_BORDER_COLOR,
            &DWMWA_COLOR_NONE as *const _ as *const _,
            std::mem::size_of_val(&DWMWA_COLOR_NONE) as u32,
        );
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE,
            &DWMWCP_DONOTROUND as *const _ as *const _,
            std::mem::size_of_val(&DWMWCP_DONOTROUND) as u32,
        );
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

    REVEAL_GEN.fetch_add(1, Ordering::SeqCst);
    super::pill_animation::cancel_pending_pill_tween();

    // Reuse the native reveal sequence without recalculating placement. The
    // Windows window can remain logically visible while its compositor surface
    // is behind another window after click-through is changed, so a conditional
    // `show()` is not enough here. `reveal_pill` uses SW_SHOWNOACTIVATE and
    // HWND_TOPMOST, which re-presents the existing window without activating it
    // or running the placement animation.
    reveal_pill(app, &pill, state, None);
}

/// Shows the pill window in the given state, optionally carrying an error
/// message. The window is sized to whatever the frontend last reported as its
/// visible content width (see `commands::recording::set_pill_size`), so the
/// transparent click-capture zone tracks the pill rather than a fixed band —
/// in a button-bearing state like handsfree, only the capsule itself swallows
/// clicks, and everything beside it passes through. Moving to a different
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
    // Handsfree, error (Retry), and cancelled (Undo/Dismiss) all have real
    // buttons that need real cursor events.
    let has_clickable_buttons = matches!(
        state,
        "handsfree"
            | "error"
            | "cancelled"
            | "paste_failed"
            | "copied"
            | "feedback_prompt"
            | "repair_input"
            | "repair_recording"
            | "repair_processing"
            | "repair_proposal"
            | "repair_applying"
            | "repair_error"
            | "repair_done"
    );
    pill.set_ignore_cursor_events(!has_clickable_buttons).ok();
    // Re-assert every reveal, not just once at window creation: WebView2 has
    // been observed repainting its surface opaque again when the window
    // flips between click-through and interactive (exactly what toggling
    // set_ignore_cursor_events above does for every repair-flow state), which
    // showed up as whatever sits behind the pill flashing through for a
    // frame — e.g. clicking "Not good"/"Good" on the feedback card.
    pill.set_background_color(Some(tauri::utils::config::Color(0, 0, 0, 0))).ok();

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

    // Must run after the SW_SHOWNOACTIVATE show above, not before: that call
    // explicitly leaves the current foreground window untouched, which was
    // silently undoing the focus grab below when this ran first — repair_input
    // needs real keyboard focus so typed/pasted text lands in the textarea
    // instead of whatever app last had it.
    let wants_focus = matches!(state, "repair_input" | "repair_proposal" | "repair_error");
    PILL_WANTS_REPAIR_FOCUS.store(wants_focus, Ordering::SeqCst);
    set_pill_focusable(pill, wants_focus);

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

    // Must fire after pill-state (see PENDING_PILL_PROFILE) — this is the
    // one place every reveal path (immediate or animated) actually
    // converges, so it's the only point where the ordering is guaranteed.
    if let Some(profile) = PENDING_PILL_PROFILE.lock().ok().and_then(|mut slot| slot.take()) {
        pill.emit("pill-profile", profile).ok();
    }
}

fn set_pill_focusable<R: Runtime>(pill: &WebviewWindow<R>, focusable: bool) {
    #[cfg(target_os = "windows")]
    {
        use windows::Win32::Graphics::Dwm::{
            DwmSetWindowAttribute, DWMWA_BORDER_COLOR, DWMWA_COLOR_NONE,
            DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_DONOTROUND,
        };
        use windows::Win32::UI::WindowsAndMessaging::{
            GetWindowLongPtrW, SetWindowLongPtrW, SetWindowPos, GWL_EXSTYLE, SWP_FRAMECHANGED,
            SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, WS_EX_NOACTIVATE,
        };
        if let Ok(hwnd) = pill.hwnd() {
            unsafe {
                let current = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
                let desired = if focusable {
                    current & !(WS_EX_NOACTIVATE.0 as isize)
                } else {
                    current | WS_EX_NOACTIVATE.0 as isize
                };
                if desired != current {
                    let _ = SetWindowLongPtrW(hwnd, GWL_EXSTYLE, desired);
                    let _ = SetWindowPos(
                        hwnd,
                        None,
                        0,
                        0,
                        0,
                        0,
                        SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
                    );
                    // SWP_FRAMECHANGED makes DWM recompute the non-client frame,
                    // which has been observed to redraw the native active-window
                    // border even though harden_pill_window() already suppressed
                    // it once at window creation — re-assert it here so becoming
                    // focusable (the repair-input path) never brings it back.
                    let _ = DwmSetWindowAttribute(
                        hwnd,
                        DWMWA_BORDER_COLOR,
                        &DWMWA_COLOR_NONE as *const _ as *const _,
                        std::mem::size_of_val(&DWMWA_COLOR_NONE) as u32,
                    );
                    let _ = DwmSetWindowAttribute(
                        hwnd,
                        DWMWA_WINDOW_CORNER_PREFERENCE,
                        &DWMWCP_DONOTROUND as *const _ as *const _,
                        std::mem::size_of_val(&DWMWCP_DONOTROUND) as u32,
                    );
                }
            }
        }
    }
    if focusable {
        // `WebviewWindow::set_focus()` alone was not reliably stealing OS
        // keyboard focus from whatever app was previously foreground — plain
        // SetFocus() only works within the calling thread's own input queue,
        // and Windows blocks a background process from calling
        // SetForegroundWindow() on its own. Attaching this thread's input
        // queue to the current foreground thread first is the standard
        // workaround, so typed/pasted text actually lands in the repair
        // textarea instead of whatever window last had focus.
        #[cfg(target_os = "windows")]
        {
            use windows::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
            use windows::Win32::UI::Input::KeyboardAndMouse::SetFocus;
            use windows::Win32::UI::WindowsAndMessaging::{
                BringWindowToTop, GetForegroundWindow, GetWindowThreadProcessId,
                SetForegroundWindow,
            };
            if let Ok(hwnd) = pill.hwnd() {
                unsafe {
                    let foreground = GetForegroundWindow();
                    let current_thread = GetCurrentThreadId();
                    let foreground_thread = GetWindowThreadProcessId(foreground, None);
                    let attached = foreground_thread != 0
                        && foreground_thread != current_thread
                        && AttachThreadInput(current_thread, foreground_thread, true).as_bool();
                    let _ = SetForegroundWindow(hwnd);
                    let _ = BringWindowToTop(hwnd);
                    let _ = SetFocus(Some(hwnd));
                    if attached {
                        let _ = AttachThreadInput(current_thread, foreground_thread, false);
                    }
                }
            }
        }
        pill.set_focus().ok();
    }
}

fn next_pill_placement<R: Runtime>(
    app: &AppHandle,
    pill: &WebviewWindow<R>,
) -> Option<PillPlacement> {
    let (target_point, width_points, height_points, cached, stale) = {
        let state = app.try_state::<SharedState>()?;
        let guard = state.lock().ok()?;
        (
            guard.target.display_point,
            guard.pill_width_points,
            guard.pill_height_points,
            guard.pill_placement,
            guard.pill_placement_stale,
        )
    };

    if !stale && cached.is_some() {
        return None;
    }

    let resolved = super::pill_position::resolve_pill_placement(
        pill,
        target_point,
        width_points,
        height_points,
    )
    .or(cached);

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
        // Re-enable click-through: after a button-bearing state (handsfree,
        // error, cancelled, paste_failed) reveal_pill left the window
        // click-capturing. Idle is invisible, so it must never swallow clicks
        // in the pill's zone even though the pill content has disappeared.
        pill.set_ignore_cursor_events(true).ok();
        // Do not call pill.hide() - hiding the window suspends the WebView2
        // renderer. The next show_pill("recording") emit would then be lost
        // before WebView2 wakes up, causing only "processing" to appear.
        // The pill window is transparent + click-through in idle state, so
        // leaving it visible has no user-visible effect.
    }
}

/// Shows the pill's "Cancelled" state — a cancelled recording whose audio was
/// good enough to stash for the pill's Continue button (see
/// `pipeline::cancel_recording_with_resume`). Auto-dismiss is handled by the
/// frontend (`PillApp.svelte`), same as `show_error_pill`.
pub(super) fn show_cancelled_pill(app: &AppHandle) {
    show_pill_msg(app, "cancelled", None);
}

/// Shows the pill's "Paste failed" state — injection didn't land (or
/// couldn't be verified), but the finished text is safely stashed as
/// `paste_failure` for the pill's Copy button (see
/// `commands::recording::copy_paste_failure_to_clipboard`). Auto-dismiss is
/// handled by the frontend, same as `show_error_pill`.
pub(super) fn show_paste_failed_pill(app: &AppHandle) {
    if super::start_stop_sounds_enabled(app) {
        crate::media::sound::play(crate::media::sound::SoundCue::Error);
    }
    show_pill_msg(app, "paste_failed", None);
}

/// Shows the pill's "Copied" confirmation for the global copy-last-dictation
/// shortcut (Ctrl+Alt+C / ⌥⌘C) — a lightweight, button-less toast so the
/// user gets clear feedback the shortcut actually did something, even when
/// nothing is focused to receive it. Auto-dismiss (5s) is handled by the
/// frontend, same as the other transient pill states.
pub(crate) fn show_copied_pill(app: &AppHandle, msg: &str) {
    if super::start_stop_sounds_enabled(app) {
        crate::media::sound::play(crate::media::sound::SoundCue::Stop);
    }
    show_pill_msg(app, "copied", Some(msg));
}

/// Emits the current processing sub-stage to the pill window. The pill is
/// already showing `processing`; this only refines what it displays
/// ("Transcribing…" / "Cleaning…" / "Pasting…"). Payload is a bare stage id
/// string; the frontend maps it to a label, so adding a stage never requires
/// an IPC schema change.
pub(crate) fn emit_pill_stage(app: &AppHandle, stage: &str) {
    if let Some(pill) = app.get_webview_window("pill") {
        pill.emit("pill-stage", stage).ok();
    }
}

/// Emits the resolved tone profile (e.g. "casual") to the pill window so it
/// can show which style will apply to the current dictation. Emitted from the
/// pipeline itself — the frontend never re-resolves it.
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

/// A delivered dictation can still have a clipboard-phrase warning. This is
/// deliberately passive: the text already reached its destination.
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
