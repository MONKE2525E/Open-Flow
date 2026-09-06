//! Native target-window geometry used to choose the dictation pill's display.
//!
//! Coordinates intentionally stay in each platform's desktop coordinate space:
//! physical pixels on Windows and Core Graphics logical coordinates on macOS.
//! Those are the coordinate spaces expected by Tauri's `monitor_from_point`.

use crate::core::window_context;
use tauri::{Runtime, WebviewWindow};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DesktopPoint {
    pub x: f64,
    pub y: f64,
}

/// The native focus target used for injection plus the display anchor captured
/// at dictation start. The id is an HWND on Windows and an application PID on
/// macOS.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct WindowTarget {
    pub id: usize,
    pub display_point: Option<DesktopPoint>,
}

impl WindowTarget {
    pub fn capture_foreground() -> Self {
        Self::from_id(window_context::get_foreground_hwnd())
    }

    pub fn capture_display_only() -> Self {
        let target = Self::capture_foreground();
        Self::from_parts(0, target.display_point)
    }

    pub fn from_parts(id: usize, display_point: Option<DesktopPoint>) -> Self {
        Self { id, display_point }
    }

    pub fn from_id(id: usize) -> Self {
        Self {
            id,
            display_point: window_center(id),
        }
    }

    /// Re-read geometry for retries in case the target window moved. Retain the
    /// original point if the native window is no longer queryable.
    pub fn refreshed(self) -> Self {
        let refreshed = Self::from_id(self.id);
        Self {
            id: self.id,
            display_point: refreshed.display_point.or(self.display_point),
        }
    }
}

/// Returns the center of a Tauri webview window in the platform coordinate
/// space used by `WindowTarget::display_point`.
pub fn capture_webview_center<R: Runtime>(window: &WebviewWindow<R>) -> Option<DesktopPoint> {
    let position = window.outer_position().ok()?;
    let size = window.outer_size().ok()?;
    if size.width == 0 || size.height == 0 {
        return None;
    }

    #[cfg(target_os = "macos")]
    let scale_factor = window
        .scale_factor()
        .ok()
        .filter(|scale| *scale > 0.0)
        .unwrap_or(1.0);

    #[cfg(not(target_os = "macos"))]
    let scale_factor = 1.0;

    Some(DesktopPoint {
        x: (position.x as f64 + size.width as f64 / 2.0) / scale_factor,
        y: (position.y as f64 + size.height as f64 / 2.0) / scale_factor,
    })
}

#[cfg(windows)]
fn window_center(id: usize) -> Option<DesktopPoint> {
    use windows::Win32::Foundation::{HWND, RECT};
    use windows::Win32::UI::WindowsAndMessaging::GetWindowRect;

    if id == 0 {
        return None;
    }

    let mut rect = RECT::default();
    unsafe { GetWindowRect(HWND(id as *mut core::ffi::c_void), &mut rect) }.ok()?;
    if rect.right <= rect.left || rect.bottom <= rect.top {
        return None;
    }

    Some(DesktopPoint {
        x: (f64::from(rect.left) + f64::from(rect.right)) / 2.0,
        y: (f64::from(rect.top) + f64::from(rect.bottom)) / 2.0,
    })
}

#[cfg(target_os = "macos")]
fn window_center(id: usize) -> Option<DesktopPoint> {
    use accessibility_sys::{
        kAXErrorSuccess, kAXFocusedWindowAttribute, kAXPositionAttribute, kAXSizeAttribute,
        kAXValueTypeCGPoint, kAXValueTypeCGSize, AXUIElementCopyAttributeValue,
        AXUIElementCreateApplication, AXUIElementRef, AXUIElementSetMessagingTimeout,
        AXValueGetValue, AXValueRef,
    };
    use core_foundation::base::{CFRelease, CFTypeRef, TCFType};
    use core_foundation::string::CFString;
    use std::ffi::c_void;
    use std::ptr;

    #[repr(C)]
    #[derive(Default)]
    struct Point {
        x: f64,
        y: f64,
    }

    #[repr(C)]
    #[derive(Default)]
    struct Size {
        width: f64,
        height: f64,
    }

    unsafe fn copy_attribute(element: AXUIElementRef, name: &str) -> Option<CFTypeRef> {
        let attribute = CFString::new(name);
        let mut value: CFTypeRef = ptr::null();
        if AXUIElementCopyAttributeValue(element, attribute.as_concrete_TypeRef(), &mut value)
            != kAXErrorSuccess
            || value.is_null()
        {
            return None;
        }
        Some(value)
    }

    if id == 0 || id > i32::MAX as usize {
        return None;
    }

    unsafe {
        let application = AXUIElementCreateApplication(id as i32);
        if application.is_null() {
            return None;
        }
        let _ = AXUIElementSetMessagingTimeout(application, 0.015);

        let result = (|| {
            let window_value = copy_attribute(application, kAXFocusedWindowAttribute)?;
            let window = window_value as AXUIElementRef;
            let _ = AXUIElementSetMessagingTimeout(window, 0.015);

            let position_value = copy_attribute(window, kAXPositionAttribute);
            let size_value = copy_attribute(window, kAXSizeAttribute);

            let center = match (position_value, size_value) {
                (Some(position_value), Some(size_value)) => {
                    let mut position = Point::default();
                    let mut size = Size::default();
                    let position_ok = AXValueGetValue(
                        position_value as AXValueRef,
                        kAXValueTypeCGPoint,
                        &mut position as *mut Point as *mut c_void,
                    );
                    let size_ok = AXValueGetValue(
                        size_value as AXValueRef,
                        kAXValueTypeCGSize,
                        &mut size as *mut Size as *mut c_void,
                    );
                    if position_ok && size_ok && size.width > 0.0 && size.height > 0.0 {
                        Some(DesktopPoint {
                            x: position.x + size.width / 2.0,
                            y: position.y + size.height / 2.0,
                        })
                    } else {
                        None
                    }
                }
                _ => None,
            };

            if let Some(value) = position_value {
                CFRelease(value);
            }
            if let Some(value) = size_value {
                CFRelease(value);
            }
            CFRelease(window_value);
            center
        })();

        CFRelease(application as CFTypeRef);
        result
    }
}

#[cfg(not(any(windows, target_os = "macos")))]
fn window_center(_id: usize) -> Option<DesktopPoint> {
    None
}
