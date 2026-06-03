//! Small macOS helpers around the frontmost application, shared by
//! `window_context` (capture), `injection` (re-focus before paste) and
//! `auto_learn` (focus check). Uses runtime `msg_send!` against AppKit classes
//! that are already loaded into the process by Tauri/Cocoa, so no `objc2-app-kit`
//! dependency is needed.

#![cfg(target_os = "macos")]

use std::ffi::CStr;
use std::os::raw::c_char;

use objc2::rc::autoreleasepool;
use objc2::runtime::AnyObject;
use objc2::{class, msg_send};
use objc2_foundation::{NSData, NSString};

#[link(name = "AVFoundation", kind = "framework")]
extern "C" {}

const APP_ICON_PNG: &[u8] = include_bytes!("../../icons/icon.png");

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

        let data = NSData::with_bytes(APP_ICON_PNG);
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

/// Current macOS microphone permission status for the app.
///
/// Returns one of: `authorized`, `not_determined`, `denied`, `restricted`,
/// or `unknown`.
pub fn microphone_permission_status() -> &'static str {
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

// UTI for plain UTF-8 text — the value of `NSPasteboardTypeString`.
const PASTEBOARD_TYPE_STRING: &str = "public.utf8-plain-text";

/// PID of the frontmost (active) application, or `None` if unavailable.
pub fn frontmost_pid() -> Option<i32> {
    autoreleasepool(|_| unsafe {
        let workspace: *mut AnyObject = msg_send![class!(NSWorkspace), sharedWorkspace];
        if workspace.is_null() {
            return None;
        }
        let app: *mut AnyObject = msg_send![workspace, frontmostApplication];
        if app.is_null() {
            return None;
        }
        let pid: i32 = msg_send![app, processIdentifier];
        if pid <= 0 {
            None
        } else {
            Some(pid)
        }
    })
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
