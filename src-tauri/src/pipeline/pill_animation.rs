//! Animated cross-monitor pill move: a hand-rolled `SetWindowPos` tween so a
//! cross-DPI monitor switch reads as a deliberate glide instead of an
//! instant jump. Pure interpolation math (testable without a window) is kept
//! apart from the Windows-only driver that actually touches the HWND — same
//! split as `pill_position.rs`'s placement math vs `pill.rs`'s lifecycle.

use crate::pipeline::pill_position::PillPlacement;
use std::sync::atomic::{AtomicU64, Ordering};
#[cfg(target_os = "windows")]
use tauri::{Runtime, WebviewWindow};

/// Total tween length and per-step cadence. Named so they can be retuned by
/// ear the same way `MONITOR_MOVE_SETTLE_MS`/the sound-cue delays were tuned
/// in earlier passes — adjust freely if real-hardware testing shows the
/// motion feels off.
#[cfg(target_os = "windows")]
const TWEEN_DURATION_MS: u64 = 180;
#[cfg(target_os = "windows")]
const TWEEN_STEP_MS: u64 = 12;

/// Guards against two tweens (or a tween and an instant same-monitor move)
/// racing over the same HWND. Mirrors `media::sound::START_CUE_GEN` exactly:
/// a bare static generation counter, not `SharedState`, since this is purely
/// an internal sequencing concern of "which thread may currently call
/// SetWindowPos on the pill," not application state anything else reads.
static PILL_TWEEN_GEN: AtomicU64 = AtomicU64::new(0);

/// One interpolated frame of the tween: physical-pixel geometry to apply via
/// `SetWindowPos` at a given point in the animation.
#[derive(Clone, Copy, Debug)]
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
pub(super) struct TweenFrame {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

/// Monotonic ease-out curve, no overshoot. The bouncy
/// `cubic-bezier(0.34, 1.56, 0.64, 1)` used elsewhere in PillApp.svelte's CSS
/// would overshoot past 1.0 partway through, which is fine for a CSS element
/// growing past its own final size but wrong for a native window move —
/// `placement_for_monitor` sits the pill flush against the work-area's
/// bottom edge, so overshoot here would briefly push it past the edge of the
/// visible work area.
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
fn ease_out_cubic(t: f64) -> f64 {
    let u = 1.0 - t;
    1.0 - u * u * u
}

#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
fn lerp_i32(a: i32, b: i32, t: f64) -> i32 {
    (a as f64 + (b - a) as f64 * t).round() as i32
}

/// Builds the tween's frame sequence from `from` to `to`. Frame 0 (`from`) is
/// intentionally not included — the window is already showing that geometry
/// — and the last frame is guaranteed to equal `to` exactly (since
/// `ease_out_cubic(1.0) == 1.0`), so the tween always lands precisely on the
/// already-verified-correct placement math in `pill_position.rs`, with no
/// separate "snap to final" step needed. All four axes share one eased
/// progress value per frame so the pill glides rather than wobbling (e.g.
/// width finishing before position does).
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
pub(super) fn build_tween_frames(
    from: PillPlacement,
    to: PillPlacement,
    step_count: u32,
) -> Vec<TweenFrame> {
    let step_count = step_count.max(1);
    (1..=step_count)
        .map(|i| {
            let t = i as f64 / step_count as f64;
            let eased = ease_out_cubic(t);
            TweenFrame {
                x: lerp_i32(from.x, to.x, eased),
                y: lerp_i32(from.y, to.y, eased),
                width: lerp_i32(from.width, to.width, eased),
                height: lerp_i32(from.height, to.height, eased),
            }
        })
        .collect()
}

/// Bumps the generation counter so any in-flight tween bails out on its next
/// frame instead of fighting an instant same-monitor move over the window.
/// Cheap enough to call unconditionally regardless of whether a tween is
/// actually running — mirrors `media::sound::cancel_pending_start()`.
pub(super) fn cancel_pending_pill_tween() {
    PILL_TWEEN_GEN.fetch_add(1, Ordering::SeqCst);
}

/// Starts an animated move/resize from `from` to `to` on Tauri's async
/// runtime and returns immediately. `on_complete` runs once the tween lands
/// exactly on `to` — never if a newer tween or instant move superseded this
/// one first. Win32 `SetWindowPos` can be called from any thread (thread
/// affinity governs who *receives* a window's messages, not who can issue
/// commands to it), and this codebase already calls it from non-main
/// threads today (`pill_position::apply_pill_placement`). Uses
/// `tauri::async_runtime::spawn` + `tokio::time::sleep` rather than a raw OS
/// thread, matching the same generation-counter-guarded delayed-action shape
/// already used in `media::sound::play_start_delayed_then`.
#[cfg(target_os = "windows")]
pub(super) fn animate_pill_placement<R: Runtime>(
    pill: &WebviewWindow<R>,
    from: PillPlacement,
    to: PillPlacement,
    on_complete: impl FnOnce() + Send + 'static,
) {
    use windows::Win32::UI::WindowsAndMessaging::{SetWindowPos, SWP_NOACTIVATE, SWP_NOZORDER};

    let generation = PILL_TWEEN_GEN
        .fetch_add(1, Ordering::SeqCst)
        .wrapping_add(1);
    let pill = pill.clone();
    let step_count = (TWEEN_DURATION_MS / TWEEN_STEP_MS).max(1) as u32;
    let frames = build_tween_frames(from, to, step_count);

    tauri::async_runtime::spawn(async move {
        let mut remaining = frames.into_iter().peekable();
        while let Some(frame) = remaining.next() {
            if PILL_TWEEN_GEN.load(Ordering::SeqCst) != generation {
                return; // superseded — cede the window to the newer move.
            }
            // Re-checked every frame (not cached) so a window closed/destroyed
            // mid-tween is caught immediately instead of sleeping through the
            // remaining frames for nothing.
            let Ok(hwnd) = pill.hwnd() else {
                return;
            };
            unsafe {
                let _ = SetWindowPos(
                    hwnd,
                    None,
                    frame.x,
                    frame.y,
                    frame.width,
                    frame.height,
                    SWP_NOZORDER | SWP_NOACTIVATE,
                );
            }
            // Skip the sleep after the last frame — nothing left to wait for
            // before revealing, so don't add a pointless ~12ms of latency.
            if remaining.peek().is_some() {
                tokio::time::sleep(std::time::Duration::from_millis(TWEEN_STEP_MS)).await;
            }
        }

        if PILL_TWEEN_GEN.load(Ordering::SeqCst) == generation {
            on_complete();
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn placement(x: i32, y: i32, width: i32, height: i32) -> PillPlacement {
        PillPlacement {
            x,
            y,
            width,
            height,
        }
    }

    #[test]
    fn ease_out_cubic_is_monotonic_and_bounded() {
        assert_eq!(ease_out_cubic(0.0), 0.0);
        assert_eq!(ease_out_cubic(1.0), 1.0);
        let samples: Vec<f64> = (0..=10).map(|i| ease_out_cubic(i as f64 / 10.0)).collect();
        for pair in samples.windows(2) {
            assert!(pair[1] >= pair[0], "ease_out_cubic must be non-decreasing");
        }
    }

    #[test]
    fn build_tween_frames_lands_exactly_on_target() {
        let from = placement(0, 1000, 380, 44);
        let to = placement(1920, 1340, 475, 55);
        let frames = build_tween_frames(from, to, 15);
        let last = frames.last().expect("at least one frame");
        assert_eq!(
            (last.x, last.y, last.width, last.height),
            (to.x, to.y, to.width, to.height)
        );
    }

    #[test]
    fn build_tween_frames_returns_step_count_frames() {
        let from = placement(0, 0, 380, 44);
        let to = placement(1920, 0, 475, 55);
        assert_eq!(build_tween_frames(from, to, 15).len(), 15);
    }

    fn assert_axis_monotonic_toward_target(from: i32, to: i32, values: &[i32]) {
        let lo = from.min(to);
        let hi = from.max(to);
        let mut prev = from;
        for &v in values {
            assert!(v >= lo && v <= hi, "{v} escaped range [{lo},{hi}]");
            if to >= from {
                assert!(
                    v >= prev,
                    "expected non-decreasing toward target, got {v} after {prev}"
                );
            } else {
                assert!(
                    v <= prev,
                    "expected non-increasing toward target, got {v} after {prev}"
                );
            }
            prev = v;
        }
    }

    #[test]
    fn build_tween_frames_is_monotonic_per_axis_with_no_overshoot() {
        // Cover both directions: growing+moving right (A -> higher-DPI B) and
        // shrinking+moving left (B -> A), since a real setup crosses both ways.
        for (from, to) in [
            (placement(0, 1000, 380, 44), placement(1920, 1340, 475, 55)),
            (placement(1920, 1340, 475, 55), placement(0, 1000, 380, 44)),
        ] {
            let frames = build_tween_frames(from, to, 15);
            let xs: Vec<i32> = frames.iter().map(|f| f.x).collect();
            let ys: Vec<i32> = frames.iter().map(|f| f.y).collect();
            let widths: Vec<i32> = frames.iter().map(|f| f.width).collect();
            let heights: Vec<i32> = frames.iter().map(|f| f.height).collect();
            assert_axis_monotonic_toward_target(from.x, to.x, &xs);
            assert_axis_monotonic_toward_target(from.y, to.y, &ys);
            assert_axis_monotonic_toward_target(from.width, to.width, &widths);
            assert_axis_monotonic_toward_target(from.height, to.height, &heights);
        }
    }

    #[test]
    fn cancel_pending_pill_tween_bumps_generation() {
        // Verifies only the atomic mechanics; real thread-cancellation
        // behavior needs a live window and can't be unit tested here.
        let before = PILL_TWEEN_GEN.load(Ordering::SeqCst);
        cancel_pending_pill_tween();
        cancel_pending_pill_tween();
        assert!(PILL_TWEEN_GEN.load(Ordering::SeqCst) >= before + 2);
    }
}
