use crate::core::window_geometry::DesktopPoint;
use tauri::{Runtime, WebviewWindow};

const PILL_WIDTH_POINTS: f64 = 380.0;
const PILL_HEIGHT_POINTS: f64 = 44.0;
const PILL_BOTTOM_GAP_POINTS: f64 = 16.0;

#[derive(Clone, Copy, Debug, PartialEq)]
struct MonitorSnapshot {
    work_x: i32,
    work_y: i32,
    work_width: u32,
    work_height: u32,
    scale_factor: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct PillPlacement {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

impl From<&tauri::Monitor> for MonitorSnapshot {
    fn from(monitor: &tauri::Monitor) -> Self {
        let work_area = monitor.work_area();
        Self {
            work_x: work_area.position.x,
            work_y: work_area.position.y,
            work_width: work_area.size.width,
            work_height: work_area.size.height,
            scale_factor: monitor.scale_factor(),
        }
    }
}

fn round_to_physical(points: f64, scale_factor: f64) -> i32 {
    (points * scale_factor).round() as i32
}

fn placement_for_monitor(monitor: MonitorSnapshot) -> PillPlacement {
    let width = round_to_physical(PILL_WIDTH_POINTS, monitor.scale_factor);
    let height = round_to_physical(PILL_HEIGHT_POINTS, monitor.scale_factor);
    let gap = round_to_physical(PILL_BOTTOM_GAP_POINTS, monitor.scale_factor);
    let work_width = monitor.work_width as f64;
    let target_x = monitor.work_x + ((work_width - width as f64) / 2.0).round() as i32;
    let target_y = monitor.work_y + monitor.work_height as i32 - height - gap;

    PillPlacement {
        x: target_x,
        y: target_y,
        width,
        height,
    }
}

fn choose_monitor(
    target_monitor: Option<MonitorSnapshot>,
    primary_monitor: Option<MonitorSnapshot>,
) -> Option<MonitorSnapshot> {
    target_monitor.or(primary_monitor)
}

pub(super) fn resolve_pill_placement<R: Runtime>(
    pill: &WebviewWindow<R>,
    target_point: Option<DesktopPoint>,
) -> Option<PillPlacement> {
    let target_monitor = target_point.and_then(|point| {
        pill.monitor_from_point(point.x, point.y)
            .ok()
            .flatten()
            .map(|monitor| MonitorSnapshot::from(&monitor))
    });
    let primary_monitor = pill
        .primary_monitor()
        .ok()
        .flatten()
        .map(|monitor| MonitorSnapshot::from(&monitor));
    let monitor = choose_monitor(target_monitor, primary_monitor)?;

    Some(placement_for_monitor(monitor))
}

/// Reads the pill's actual on-screen geometry right now. Used by the
/// animated cross-monitor path (`pill_animation.rs`) as the tween's starting
/// point — it needs the literal current placement to interpolate from, not
/// just a changed/unchanged boolean. Only called from the Windows-only
/// animated branch in `pill.rs`'s `show_pill_msg`. Returns `None` if the
/// geometry can't be read or comes back zero-sized (e.g. very early in
/// window initialization) rather than guessing `(0, 0)` — a tween that
/// actually started from `(0, 0, 0, 0)` would visibly grow in from the
/// screen's top-left corner, which is worse than just skipping the
/// animation for that one call.
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
pub(super) fn current_placement<R: Runtime>(pill: &WebviewWindow<R>) -> Option<PillPlacement> {
    let size = pill.inner_size().ok()?;
    let pos = pill.outer_position().ok()?;
    if size.width == 0 || size.height == 0 {
        return None;
    }
    Some(PillPlacement {
        x: pos.x,
        y: pos.y,
        width: size.width as i32,
        height: size.height as i32,
    })
}

pub(super) fn dimension_changed(current: f64, desired: f64) -> bool {
    (current - desired).abs() > 1.0
}

pub(super) fn position_changed(current: i32, desired: i32) -> bool {
    current.abs_diff(desired) > 1
}

/// Moves/resizes the pill to `placement` if it isn't already there. Returns
/// `true` if a native resize or reposition was actually issued. Used as-is
/// for the synchronous same-monitor path; the animated cross-monitor path in
/// `pill_animation.rs` has its own per-frame `SetWindowPos` calls instead,
/// since every tween frame must apply unconditionally to progress the
/// animation rather than skip via this function's no-op check.
pub(super) fn apply_pill_placement<R: Runtime>(
    pill: &WebviewWindow<R>,
    placement: PillPlacement,
) -> bool {
    super::pill_animation::cancel_pending_pill_tween();

    let desired_size = (
        placement.width.max(1) as f64,
        placement.height.max(1) as f64,
    );

    let needs_resize = pill
        .inner_size()
        .map(|cur| {
            dimension_changed(cur.width as f64, desired_size.0)
                || dimension_changed(cur.height as f64, desired_size.1)
        })
        .unwrap_or(true);

    let needs_reposition = pill
        .outer_position()
        .map(|cur| position_changed(cur.x, placement.x) || position_changed(cur.y, placement.y))
        .unwrap_or(true);

    #[cfg(target_os = "windows")]
    {
        use windows::Win32::UI::WindowsAndMessaging::{
            SetWindowPos, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER,
        };

        if needs_resize || needs_reposition {
            if let Ok(hwnd) = pill.hwnd() {
                let mut flags = SWP_NOZORDER | SWP_NOACTIVATE;
                if !needs_resize {
                    flags |= SWP_NOSIZE;
                }
                if !needs_reposition {
                    flags |= SWP_NOMOVE;
                }

                unsafe {
                    let _ = SetWindowPos(
                        hwnd,
                        None,
                        placement.x,
                        placement.y,
                        placement.width,
                        placement.height,
                        flags,
                    );
                }
            }
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        if needs_resize {
            pill.set_size(tauri::LogicalSize::new(
                PILL_WIDTH_POINTS,
                PILL_HEIGHT_POINTS,
            ))
            .ok();
        }
        if needs_reposition {
            pill.set_position(tauri::PhysicalPosition::new(placement.x, placement.y))
                .ok();
        }
    }

    needs_resize || needs_reposition
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dimension_changed_respects_one_pixel_tolerance() {
        assert!(!dimension_changed(380.0, 380.9));
        assert!(dimension_changed(380.0, 475.0));
    }

    #[test]
    fn position_changed_respects_one_pixel_tolerance() {
        assert!(!position_changed(100, 101));
        assert!(position_changed(100, 250));
    }

    fn monitor(
        work_x: i32,
        work_y: i32,
        work_width: u32,
        work_height: u32,
        scale_factor: f64,
    ) -> MonitorSnapshot {
        MonitorSnapshot {
            work_x,
            work_y,
            work_width,
            work_height,
            scale_factor,
        }
    }

    #[test]
    fn prefers_target_monitor_then_primary_monitor() {
        let primary = monitor(0, 0, 1920, 1080, 1.0);
        let secondary = monitor(1920, 0, 2560, 1440, 1.5);

        assert_eq!(
            choose_monitor(Some(secondary), Some(primary)),
            Some(secondary)
        );
        assert_eq!(choose_monitor(None, Some(primary)), Some(primary));
        assert_eq!(choose_monitor(None, None), None);
    }

    #[test]
    fn centers_on_secondary_monitor_with_positive_coordinates() {
        let placement = placement_for_monitor(monitor(1920, 0, 2560, 1400, 1.0));

        assert_eq!(placement.width, 380);
        assert_eq!(placement.height, 44);
        assert_eq!(placement.x, 3010);
        assert_eq!(placement.y, 1340);
    }

    #[test]
    fn centers_on_monitor_with_negative_coordinates() {
        let placement = placement_for_monitor(monitor(-2560, 0, 2560, 1400, 1.0));

        assert_eq!(placement.x, -1470);
        assert_eq!(placement.y, 1340);
    }

    #[test]
    fn respects_retina_scaling() {
        let placement = placement_for_monitor(monitor(0, 0, 2880, 1800, 2.0));

        assert_eq!(placement.width, 760);
        assert_eq!(placement.height, 88);
        assert_eq!(placement.x, 1060);
        assert_eq!(placement.y, 1680);
    }

    #[test]
    fn uses_work_area_bottom_instead_of_full_monitor_height() {
        let placement = placement_for_monitor(monitor(0, 40, 1920, 1040, 1.0));

        assert_eq!(placement.y, 1020);
    }
}
