//! Small macOS helpers around the frontmost application, shared by
//! `window_context` (capture), `injection` (re-focus before paste) and
//! `auto_learn` (focus check). Uses runtime `msg_send!` against AppKit classes
//! that are already loaded into the process by Tauri/Cocoa, so no `objc2-app-kit`
//! dependency is needed.

#![cfg(target_os = "macos")]

use std::ffi::CStr;
use std::os::raw::c_char;
use std::sync::atomic::{AtomicU8, Ordering};

use objc2::rc::autoreleasepool;
use objc2::runtime::AnyObject;
use objc2::{class, msg_send};
use objc2_foundation::{NSData, NSString};
use tauri::AppHandle;

#[link(name = "AVFoundation", kind = "framework")]
extern "C" {}

// Input Monitoring (HID listen) permission. A keyboard `CGEventTap` only receives
// events from *other* applications when the app is granted Input Monitoring —
// without it the tap only sees keystrokes while the app is frontmost. This is
// distinct from Accessibility (which authorizes posting events / Cmd+V).
#[link(name = "IOKit", kind = "framework")]
extern "C" {
    fn IOHIDCheckAccess(request_type: u32) -> u32;
    fn IOHIDRequestAccess(request_type: u32) -> bool;
}

// IOHIDRequestType: kIOHIDRequestTypeListenEvent is the first variant (= 0).
const IOHID_REQUEST_TYPE_LISTEN_EVENT: u32 = 0;
// IOHIDAccessType return values from IOHIDCheckAccess.
const IOHID_ACCESS_TYPE_GRANTED: u32 = 0;
const IOHID_ACCESS_TYPE_DENIED: u32 = 1;

/// Current Input Monitoring (HID listen) permission for the app.
///
/// Returns `authorized`, `denied`, or `not_determined`.
pub fn input_monitoring_status() -> &'static str {
    match unsafe { IOHIDCheckAccess(IOHID_REQUEST_TYPE_LISTEN_EVENT) } {
        IOHID_ACCESS_TYPE_GRANTED => "authorized",
        IOHID_ACCESS_TYPE_DENIED => "denied",
        _ => "not_determined",
    }
}

/// Request Input Monitoring access. If undetermined, macOS shows the consent
/// prompt and adds the app to System Settings → Privacy & Security → Input
/// Monitoring. Returns `true` if already granted. Changes generally require an
/// app relaunch before an existing event tap sees global events.
pub fn request_input_monitoring() -> bool {
    unsafe { IOHIDRequestAccess(IOHID_REQUEST_TYPE_LISTEN_EVENT) }
}

const APP_ICON_ICNS: &[u8] = include_bytes!("../../icons/icon.icns");

const POLICY_UNKNOWN: u8 = 0;
const POLICY_ACCESSORY: u8 = 1;
const POLICY_REGULAR: u8 = 2;

static LAST_APPLIED_POLICY: AtomicU8 = AtomicU8::new(POLICY_UNKNOWN);

#[repr(transparent)]
#[derive(Clone, Copy)]
struct NSApplicationActivationPolicy(isize);

unsafe impl objc2::Encode for NSApplicationActivationPolicy {
    const ENCODING: objc2::Encoding = isize::ENCODING;
}

// CGRect-compatible structs for msg_send! return values (64-bit macOS).
// AppKit returns CGRect here; objc2 checks the exact type code at runtime, so
// we mirror the CoreGraphics encodings rather than the NS* aliases.
#[repr(C)]
#[derive(Copy, Clone, Default)]
struct MacPoint {
    x: f64,
    y: f64,
}
#[repr(C)]
#[derive(Copy, Clone, Default)]
struct MacSize {
    width: f64,
    height: f64,
}
#[repr(C)]
#[derive(Copy, Clone, Default)]
struct MacRect {
    origin: MacPoint,
    size: MacSize,
}

unsafe impl objc2::Encode for MacPoint {
    const ENCODING: objc2::Encoding = objc2::Encoding::Struct(
        "CGPoint",
        &[objc2::Encoding::Double, objc2::Encoding::Double],
    );
}
unsafe impl objc2::Encode for MacSize {
    const ENCODING: objc2::Encoding = objc2::Encoding::Struct(
        "CGSize",
        &[objc2::Encoding::Double, objc2::Encoding::Double],
    );
}
unsafe impl objc2::Encode for MacRect {
    const ENCODING: objc2::Encoding =
        objc2::Encoding::Struct("CGRect", &[MacPoint::ENCODING, MacSize::ENCODING]);
}

/// Height of the Dock in logical points when it is positioned at the bottom of
/// the screen. Returns 0.0 if the Dock is on a side or auto-hidden, in which
/// case the caller should use its own fallback gap.
pub fn dock_height_points() -> f64 {
    autoreleasepool(|_| unsafe {
        let main: *mut AnyObject = msg_send![class!(NSScreen), mainScreen];
        if main.is_null() {
            return 0.0;
        }
        let frame: MacRect = msg_send![main, frame];
        let visible: MacRect = msg_send![main, visibleFrame];
        // NSScreen uses a bottom-left origin (Y increases upward).
        // When the Dock is at the bottom, visibleFrame.origin.y > frame.origin.y
        // by exactly the Dock height. Side-positioned or hidden Dock → no Y inset.
        (visible.origin.y - frame.origin.y).max(0.0)
    })
}

/// Apply the current bundled PNG as the macOS application icon at runtime.
/// This sidesteps Dock/LaunchServices caching during development and keeps the
/// visible Dock icon in sync with `icons/icon-source.svg`.
pub fn apply_dock_icon() -> bool {
    autoreleasepool(|_| unsafe {
        let app: *mut AnyObject = msg_send![class!(NSApplication), sharedApplication];
        if app.is_null() {
            return false;
        }

        let data = NSData::with_bytes(APP_ICON_ICNS);
        let image: *mut AnyObject = msg_send![class!(NSImage), alloc];
        let image: *mut AnyObject = msg_send![image, initWithData: &*data];
        if image.is_null() {
            return false;
        }

        let _: () = msg_send![app, setApplicationIconImage: image];
        let _: () = msg_send![image, release];
        true
    })
}

/// Switch the app to macOS accessory mode so it stays out of the Dock.
///
/// This is the default for Open Flow on macOS when the main window is hidden
/// or shown from the menu bar/tray.
pub fn set_accessory_activation_policy() -> bool {
    set_activation_policy(POLICY_ACCESSORY)
}

/// Dispatch `set_accessory_activation_policy()` onto the macOS main thread.
pub fn set_accessory_activation_policy_on_main_thread(app: &AppHandle) {
    let app = app.clone();
    let _ = app.run_on_main_thread(move || {
        let _ = set_accessory_activation_policy();
    });
}

/// Switch the app to regular mode so it appears in the Dock.
///
/// We use this while the main window is minimized.
pub fn set_regular_activation_policy() -> bool {
    let ok = set_activation_policy(POLICY_REGULAR);
    if ok {
        refresh_dock_icon();
    }
    ok
}

/// Dispatch `set_regular_activation_policy()` onto the macOS main thread.
pub fn set_regular_activation_policy_on_main_thread(app: &AppHandle) {
    let app = app.clone();
    let _ = app.run_on_main_thread(move || {
        let _ = set_regular_activation_policy();
    });
}

/// Float the pill window above other apps' windows *without* activating Open
/// Flow. Tauri's `show()` maps to `-[NSWindow orderFront:]`, which AppKit
/// suppresses for a background (non-active) app — so the pill only appeared
/// while Open Flow was frontmost. We instead:
///   1. raise the window level above normal windows (NSStatusWindowLevel = 25),
///   2. let it appear on every Space and over full-screen apps,
///   3. order it front with `orderFrontRegardless`, which ignores active state.
///
/// `ns_window` is the `*mut NSWindow` obtained from `WebviewWindow::ns_window()`.
pub fn float_pill_window(ns_window: *mut std::ffi::c_void) {
    if ns_window.is_null() {
        return;
    }
    autoreleasepool(|_| unsafe {
        let win = ns_window as *mut AnyObject;
        // NSStatusWindowLevel keeps the pill above ordinary app windows while
        // staying below system menus.
        let level: isize = 25;
        let _: () = msg_send![win, setLevel: level];
        // NSWindowCollectionBehaviorCanJoinAllSpaces (1<<0) |
        // NSWindowCollectionBehaviorFullScreenAuxiliary (1<<8): show on the
        // active Space and over full-screen apps without switching Spaces.
        let behavior: usize = (1 << 0) | (1 << 8);
        let _: () = msg_send![win, setCollectionBehavior: behavior];
        // Order front even though Open Flow is not the active application.
        let _: () = msg_send![win, orderFrontRegardless];
    })
}

/// Ask macOS to activate the current app and bring its windows forward.
pub fn activate_current_app() -> bool {
    autoreleasepool(|_| unsafe {
        let app: *mut AnyObject = msg_send![class!(NSRunningApplication), currentApplication];
        if app.is_null() {
            return false;
        }
        // NSApplicationActivateIgnoringOtherApps = 1 << 1
        let options: usize = 1 << 1;
        let ok: bool = msg_send![app, activateWithOptions: options];
        ok
    })
}

/// Dispatch `activate_current_app()` onto the macOS main thread.
pub fn activate_current_app_on_main_thread(app: &AppHandle) {
    let app = app.clone();
    let _ = app.run_on_main_thread(move || {
        let _ = activate_current_app();
    });
}

/// Override the live process name so macOS surfaces the friendly app name
/// instead of the Rust binary name while running in dev mode.
pub fn set_process_name(display_name: &str) -> bool {
    autoreleasepool(|_| unsafe {
        let process_info: *mut AnyObject = msg_send![class!(NSProcessInfo), processInfo];
        if process_info.is_null() {
            return false;
        }

        let display_name = NSString::from_str(display_name);
        let _: () = msg_send![process_info, setProcessName: &*display_name];
        true
    })
}

fn set_activation_policy(new_policy: u8) -> bool {
    if LAST_APPLIED_POLICY.load(Ordering::Relaxed) == new_policy {
        return true;
    }

    autoreleasepool(|_| unsafe {
        let ns_app: *mut AnyObject = msg_send![class!(NSApplication), sharedApplication];
        if ns_app.is_null() {
            return false;
        }

        let policy = match new_policy {
            POLICY_ACCESSORY => NSApplicationActivationPolicy(1),
            POLICY_REGULAR => NSApplicationActivationPolicy(0),
            _ => return false,
        };

        let ok: bool = msg_send![ns_app, setActivationPolicy: policy];
        if ok {
            LAST_APPLIED_POLICY.store(new_policy, Ordering::Relaxed);
        }
        ok
    })
}

pub fn refresh_dock_icon() {
    autoreleasepool(|_| unsafe {
        let app: *mut AnyObject = msg_send![class!(NSApplication), sharedApplication];
        if app.is_null() {
            return;
        }

        let data = NSData::with_bytes(APP_ICON_ICNS);
        let image: *mut AnyObject = msg_send![class!(NSImage), alloc];
        let image: *mut AnyObject = msg_send![image, initWithData: &*data];
        if image.is_null() {
            return;
        }

        let _: () = msg_send![app, setApplicationIconImage: image];
        let dock_tile: *mut AnyObject = msg_send![app, dockTile];
        if !dock_tile.is_null() {
            let _: () = msg_send![dock_tile, display];
        }
        let _: () = msg_send![image, release];
    })
}

/// Latched once a `cpal` input stream opens successfully. macOS only hands out a
/// working audio stream when the microphone permission is actually granted, so a
/// successful capture is authoritative proof — unlike
/// `AVCaptureDevice authorizationStatusForMediaType:`, which can return a value
/// cached at first call for the lifetime of the process and never refresh after
/// the user grants access mid-session.
static MIC_VERIFIED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Record that the microphone was successfully opened for capture. Call this from
/// the audio backend once a recording stream is confirmed playing.
pub fn mark_microphone_verified() {
    MIC_VERIFIED.store(true, Ordering::SeqCst);
}

/// Current macOS microphone permission status for the app.
///
/// Returns one of: `authorized`, `not_determined`, `denied`, `restricted`,
/// or `unknown`.
pub fn microphone_permission_status() -> &'static str {
    // A previously successful capture is proof the permission is granted, even if
    // the cached AV authorization status is stale.
    if MIC_VERIFIED.load(Ordering::SeqCst) {
        return "authorized";
    }
    autoreleasepool(|_| unsafe {
        let media_type = NSString::from_str("soun");
        let status: isize =
            msg_send![class!(AVCaptureDevice), authorizationStatusForMediaType: &*media_type];
        match status {
            3 => "authorized",
            0 => "not_determined",
            1 => "restricted",
            2 => "denied",
            _ => "unknown",
        }
    })
}

/// Request microphone access, showing the macOS consent prompt when the
/// permission is undetermined. The completion handler is a no-op — callers read
/// the resulting status separately via `microphone_permission_status()` once the
/// user responds. Safe to call when already authorized (no prompt is shown).
pub async fn request_microphone() -> bool {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let tx = std::sync::Mutex::new(Some(tx));
    autoreleasepool(|_| unsafe {
        let media_type = NSString::from_str("soun");
        let handler = block2::RcBlock::new(move |granted: objc2::runtime::Bool| {
            if let Ok(mut guard) = tx.lock() {
                if let Some(tx) = guard.take() {
                    let _ = tx.send(granted.as_bool());
                }
            }
        });
        let _: () = msg_send![
            class!(AVCaptureDevice),
            requestAccessForMediaType: &*media_type,
            completionHandler: &*handler
        ];
    });
    rx.await.unwrap_or(false)
}

// UTI for plain UTF-8 text — the value of `NSPasteboardTypeString`.
const PASTEBOARD_TYPE_STRING: &str = "public.utf8-plain-text";

/// PID of the frontmost (active) application, or `None` if unavailable.
///
/// Uses CoreGraphics (`CGWindowListCopyWindowInfo`) instead of AppKit so that
/// this function is safe to call from background threads (e.g. the CGEventTap
/// callback). AppKit's `NSWorkspace` APIs are strictly main-thread-only.
pub fn frontmost_pid() -> Option<i32> {
    use core_foundation::base::TCFType;
    unsafe {
        // kCGWindowListOptionOnScreenOnly (1<<0) | kCGWindowListExcludeDesktopElements (1<<1)
        let array_ref = CGWindowListCopyWindowInfo(3, 0);
        if array_ref.is_null() {
            return None;
        }
        let count = core_foundation_sys::array::CFArrayGetCount(array_ref);
        let mut result = None;
        for i in 0..count {
            let dict_ref = core_foundation_sys::array::CFArrayGetValueAtIndex(array_ref, i)
                as core_foundation_sys::dictionary::CFDictionaryRef;
            if dict_ref.is_null() {
                continue;
            }
            // Skip windows not on the normal layer (layer 0 = regular app windows).
            // The list is front-to-back, so the first layer-0 window owns the frontmost app.
            let layer_ref = core_foundation_sys::dictionary::CFDictionaryGetValue(
                dict_ref,
                kCGWindowLayer as *const std::ffi::c_void,
            );
            if layer_ref.is_null() {
                continue;
            }
            let layer_num = core_foundation::number::CFNumber::wrap_under_get_rule(
                layer_ref as core_foundation_sys::number::CFNumberRef,
            );
            if layer_num.to_i32() != Some(0) {
                continue;
            }
            let pid_ref = core_foundation_sys::dictionary::CFDictionaryGetValue(
                dict_ref,
                kCGWindowOwnerPID as *const std::ffi::c_void,
            );
            if pid_ref.is_null() {
                continue;
            }
            let pid_num = core_foundation::number::CFNumber::wrap_under_get_rule(
                pid_ref as core_foundation_sys::number::CFNumberRef,
            );
            if let Some(p) = pid_num.to_i32() {
                if p > 0 {
                    result = Some(p);
                    break;
                }
            }
        }
        core_foundation_sys::base::CFRelease(array_ref as *const std::ffi::c_void);
        result
    }
}

/// Localized display name of the frontmost application (e.g. "Google Chrome").
pub fn frontmost_app_name() -> Option<String> {
    autoreleasepool(|_| unsafe {
        let workspace: *mut AnyObject = msg_send![class!(NSWorkspace), sharedWorkspace];
        if workspace.is_null() {
            return None;
        }
        let app: *mut AnyObject = msg_send![workspace, frontmostApplication];
        if app.is_null() {
            return None;
        }
        let name: *mut AnyObject = msg_send![app, localizedName];
        nsstring_to_string(name)
    })
}

/// Bring the application owning `pid` to the foreground, so a subsequent
/// synthetic Cmd+V lands in the window the user was dictating into.
pub fn activate_pid(pid: i32) -> bool {
    autoreleasepool(|_| unsafe {
        let app: *mut AnyObject = msg_send![
            class!(NSRunningApplication),
            runningApplicationWithProcessIdentifier: pid
        ];
        if app.is_null() {
            return false;
        }
        // NSApplicationActivateIgnoringOtherApps = 1 << 1
        let options: usize = 1 << 1;
        let ok: bool = msg_send![app, activateWithOptions: options];
        ok
    })
}

/// Returns true if the general pasteboard contains non-text or rich-text formats
/// that would be lost if we cleared and wrote back only plain text.
pub fn pasteboard_has_non_text_formats() -> bool {
    autoreleasepool(|_| unsafe {
        let pb: *mut AnyObject = msg_send![class!(NSPasteboard), generalPasteboard];
        if pb.is_null() {
            return false;
        }
        let types: *mut AnyObject = msg_send![pb, types];
        if types.is_null() {
            return false;
        }
        let count: usize = msg_send![types, count];
        for i in 0..count {
            let item_type: *mut AnyObject = msg_send![types, objectAtIndex: i];
            if let Some(type_str) = nsstring_to_string(item_type) {
                let type_lower = type_str.to_lowercase();
                if type_lower.contains("html")
                    || type_lower.contains("rtf")
                    || type_lower.contains("image")
                    || type_lower.contains("pdf")
                    || type_lower.contains("file-url")
                    || type_lower == "public.tiff"
                    || type_lower == "public.png"
                    || type_lower == "public.jpeg"
                    || type_lower == "public.url"
                    || type_lower == "com.apple.webarchive"
                {
                    return true;
                }
            }
        }
        false
    })
}

/// Current plain-text contents of the general pasteboard, if any.
pub fn pasteboard_get_string() -> Option<String> {
    autoreleasepool(|_| unsafe {
        let pb: *mut AnyObject = msg_send![class!(NSPasteboard), generalPasteboard];
        if pb.is_null() {
            return None;
        }
        let ty = NSString::from_str(PASTEBOARD_TYPE_STRING);
        let s: *mut AnyObject = msg_send![pb, stringForType: &*ty];
        nsstring_to_string(s)
    })
}

/// Replace the general pasteboard with `s` as plain text.
pub fn pasteboard_set_string(s: &str) {
    autoreleasepool(|_| unsafe {
        let pb: *mut AnyObject = msg_send![class!(NSPasteboard), generalPasteboard];
        if pb.is_null() {
            return;
        }
        let _: isize = msg_send![pb, clearContents];
        let value = NSString::from_str(s);
        let ty = NSString::from_str(PASTEBOARD_TYPE_STRING);
        let _ok: bool = msg_send![pb, setString: &*value, forType: &*ty];
    })
}

/// Convert an `NSString*` (as `AnyObject*`) to a Rust `String` via `-UTF8String`.
unsafe fn nsstring_to_string(ns: *mut AnyObject) -> Option<String> {
    if ns.is_null() {
        return None;
    }
    let utf8: *const c_char = msg_send![ns, UTF8String];
    if utf8.is_null() {
        return None;
    }
    Some(CStr::from_ptr(utf8).to_string_lossy().into_owned())
}

#[allow(dead_code)]
#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGWindowListCopyWindowInfo(
        option: u32,
        relativeToWindow: u32,
    ) -> core_foundation_sys::array::CFArrayRef;
    static kCGWindowNumber: core_foundation_sys::string::CFStringRef;
    static kCGWindowOwnerPID: core_foundation_sys::string::CFStringRef;
    static kCGWindowLayer: core_foundation_sys::string::CFStringRef;
}

/// Returns the frontmost window ID and the frontmost application PID.
#[allow(dead_code)]
pub fn get_active_window_id_and_pid() -> (u32, i32) {
    use core_foundation::base::TCFType;

    let target_pid = frontmost_pid().unwrap_or(0);
    if target_pid <= 0 {
        return (0, 0);
    }

    unsafe {
        // kCGWindowListOptionOnScreenOnly = (1 << 0)
        // kCGWindowListOptionExcludeDesktopElements = (1 << 1)
        let array_ref = CGWindowListCopyWindowInfo(3, 0);
        if array_ref.is_null() {
            return (0, target_pid);
        }

        let count = core_foundation_sys::array::CFArrayGetCount(array_ref);
        let mut found_window_id = None;

        for i in 0..count {
            let dict_ref = core_foundation_sys::array::CFArrayGetValueAtIndex(array_ref, i)
                as core_foundation_sys::dictionary::CFDictionaryRef;
            if dict_ref.is_null() {
                continue;
            }

            // Get owner PID
            let pid_ref = core_foundation_sys::dictionary::CFDictionaryGetValue(
                dict_ref,
                kCGWindowOwnerPID as *const std::ffi::c_void,
            );
            if pid_ref.is_null() {
                continue;
            }
            let pid_num = core_foundation::number::CFNumber::wrap_under_get_rule(
                pid_ref as core_foundation_sys::number::CFNumberRef,
            );
            let pid = match pid_num.to_i32() {
                Some(p) => p,
                None => continue,
            };

            if pid != target_pid {
                continue;
            }

            // Get Layer
            let layer_ref = core_foundation_sys::dictionary::CFDictionaryGetValue(
                dict_ref,
                kCGWindowLayer as *const std::ffi::c_void,
            );
            if layer_ref.is_null() {
                continue;
            }
            let layer_num = core_foundation::number::CFNumber::wrap_under_get_rule(
                layer_ref as core_foundation_sys::number::CFNumberRef,
            );
            let layer = match layer_num.to_i32() {
                Some(l) => l,
                None => continue,
            };

            // We only care about normal window layer (0)
            if layer != 0 {
                continue;
            }

            // Get Window Number
            let win_num_ref = core_foundation_sys::dictionary::CFDictionaryGetValue(
                dict_ref,
                kCGWindowNumber as *const std::ffi::c_void,
            );
            if win_num_ref.is_null() {
                continue;
            }
            let win_num = core_foundation::number::CFNumber::wrap_under_get_rule(
                win_num_ref as core_foundation_sys::number::CFNumberRef,
            );
            if let Some(w) = win_num.to_i64() {
                found_window_id = Some(w as u32);
                break;
            }
        }

        // Release the array returned by Copy function
        core_foundation_sys::base::CFRelease(array_ref as *const std::ffi::c_void);

        (found_window_id.unwrap_or(0), target_pid)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_active_window_id_and_pid() {
        let (win_id, pid) = get_active_window_id_and_pid();
        println!("Active window ID: {}, PID: {}", win_id, pid);
    }
}
