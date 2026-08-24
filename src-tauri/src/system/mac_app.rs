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

// The global hotkey now uses Carbon `RegisterEventHotKey` (see
// `core::hotkey::mac`), which needs no Input Monitoring permission, so the old
// IOKit HID-listen probing has been removed. The only remaining macOS
// permissions are Accessibility (Cmd+V injection / AX reads) and Microphone.

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
/// This is the default for Verenu on macOS when the main window is hidden
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
/// suppresses for a background (non-active) app - so the pill only appeared
/// while Verenu was frontmost. We instead:
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
        // Order front even though Verenu is not the active application.
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

pub fn bundle_identifier() -> Option<String> {
    autoreleasepool(|_| unsafe {
        let bundle: *mut AnyObject = msg_send![class!(NSBundle), mainBundle];
        if bundle.is_null() {
            return None;
        }
        let identifier: *mut AnyObject = msg_send![bundle, bundleIdentifier];
        nsstring_to_string(identifier)
    })
}

pub fn bundle_path() -> Option<String> {
    autoreleasepool(|_| unsafe {
        let bundle: *mut AnyObject = msg_send![class!(NSBundle), mainBundle];
        if bundle.is_null() {
            return None;
        }
        let path: *mut AnyObject = msg_send![bundle, bundlePath];
        nsstring_to_string(path)
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
/// successful capture is authoritative proof - unlike
/// `AVCaptureDevice authorizationStatusForMediaType:`, which can return a value
/// cached at first call for the lifetime of the process and never refresh after
/// the user grants access mid-session.
static MIC_VERIFIED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Record that the microphone was successfully opened for capture. Call this from
/// the audio backend once a recording stream is confirmed playing.
pub fn mark_microphone_verified() {
    MIC_VERIFIED.store(true, Ordering::SeqCst);
}

pub fn is_microphone_verified() -> bool {
    MIC_VERIFIED.load(Ordering::SeqCst)
}

/// Latched once a cross-process Accessibility (AX) read succeeds - i.e. the app
/// actually read another application's focused-element tree. Like the microphone
/// latch, an empirically successful AX call is authoritative proof the permission
/// is granted, which matters when `AXIsProcessTrusted()` reports a stale `false`
/// (most often after an ad-hoc rebuild changes the code signature the TCC grant
/// was tied to). This is a process-lifetime latch: if the user revokes access the
/// OS tears the capability down and a relaunch resets the flag, mirroring the
/// accepted behaviour of [`MIC_VERIFIED`].
static ACCESSIBILITY_VERIFIED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// Record that a cross-process AX read succeeded. Call from the AX caret probe
/// whenever the OS returned real element data (any source other than
/// "permission missing" / "unavailable").
pub fn mark_accessibility_verified() {
    ACCESSIBILITY_VERIFIED.store(true, Ordering::SeqCst);
}

pub fn is_accessibility_verified() -> bool {
    ACCESSIBILITY_VERIFIED.load(Ordering::SeqCst)
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
/// permission is undetermined. The completion handler is a no-op - callers read
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

// UTI for plain UTF-8 text - the value of `NSPasteboardTypeString`.
const PASTEBOARD_TYPE_STRING: &str = "public.utf8-plain-text";

/// PID of the frontmost (active) application, or `None` if unavailable.
///
/// Reverted to using AppKit's `NSWorkspace` because `CGWindowListCopyWindowInfo`
/// triggers a system-wide Screen Recording permission prompt on macOS 15 Sequoia,
/// which is highly undesirable for a dictation app. `NSWorkspace` is thread-safe
/// since macOS 10.6 and safe to call from background threads.
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

#[derive(Debug)]
pub struct PasteboardSnapshot {
    items: Vec<Vec<(String, Vec<u8>)>>,
}

/// Copies every pasteboard item and every advertised representation. Saving
/// only the plain-text projection destroys rich text, images, file URLs, and
/// multi-item clipboards when Verenu restores after Cmd+V.
pub fn pasteboard_snapshot() -> Option<PasteboardSnapshot> {
    autoreleasepool(|_| unsafe {
        let pb: *mut AnyObject = msg_send![class!(NSPasteboard), generalPasteboard];
        if pb.is_null() {
            return None;
        }
        let pasteboard_items: *mut AnyObject = msg_send![pb, pasteboardItems];
        if pasteboard_items.is_null() {
            return Some(PasteboardSnapshot { items: Vec::new() });
        }
        let item_count: usize = msg_send![pasteboard_items, count];
        let mut items = Vec::with_capacity(item_count);
        for item_index in 0..item_count {
            let item: *mut AnyObject = msg_send![pasteboard_items, objectAtIndex: item_index];
            let types: *mut AnyObject = msg_send![item, types];
            if types.is_null() {
                items.push(Vec::new());
                continue;
            }
            let type_count: usize = msg_send![types, count];
            let mut representations = Vec::with_capacity(type_count);
            for type_index in 0..type_count {
                let item_type: *mut AnyObject = msg_send![types, objectAtIndex: type_index];
                let Some(type_name) = nsstring_to_string(item_type) else {
                    continue;
                };
                let data: *mut AnyObject = msg_send![item, dataForType: item_type];
                if data.is_null() {
                    continue;
                }
                let length: usize = msg_send![data, length];
                let bytes: *const u8 = msg_send![data, bytes];
                let value = if length == 0 {
                    Vec::new()
                } else if bytes.is_null() {
                    continue;
                } else {
                    std::slice::from_raw_parts(bytes, length).to_vec()
                };
                representations.push((type_name, value));
            }
            items.push(representations);
        }
        Some(PasteboardSnapshot { items })
    })
}

impl PasteboardSnapshot {
    /// Restores only if nobody changed the pasteboard after Verenu wrote its
    /// temporary payload. This prevents a user copy made during a slow paste
    /// from being overwritten by the delayed restore.
    pub fn restore_if_unchanged(self, expected_change_count: isize) -> bool {
        autoreleasepool(|_| unsafe {
            let pb: *mut AnyObject = msg_send![class!(NSPasteboard), generalPasteboard];
            if pb.is_null() {
                return false;
            }
            let current_change_count: isize = msg_send![pb, changeCount];
            if current_change_count != expected_change_count {
                log::debug!(
                    "pasteboard restore skipped: expected change_count={} current={}",
                    expected_change_count,
                    current_change_count
                );
                return false;
            }

            let objects: *mut AnyObject = msg_send![class!(NSMutableArray), array];
            let was_empty = self.items.is_empty();
            for representations in self.items {
                let item: *mut AnyObject = msg_send![class!(NSPasteboardItem), alloc];
                let item: *mut AnyObject = msg_send![item, init];
                for (type_name, value) in representations {
                    let ty = NSString::from_str(&type_name);
                    let data: *mut AnyObject = if value.is_empty() {
                        msg_send![class!(NSData), data]
                    } else {
                        msg_send![
                            class!(NSData),
                            dataWithBytes: value.as_ptr(),
                            length: value.len()
                        ]
                    };
                    let _ok: bool = msg_send![item, setData: data, forType: &*ty];
                }
                let _: () = msg_send![objects, addObject: item];
                let _: () = msg_send![item, release];
            }
            let _: isize = msg_send![pb, clearContents];
            if was_empty {
                true
            } else {
                let ok: bool = msg_send![pb, writeObjects: objects];
                ok
            }
        })
    }
}

pub fn pasteboard_write_string(s: &str) -> Result<isize, isize> {
    autoreleasepool(|_| unsafe {
        let pb: *mut AnyObject = msg_send![class!(NSPasteboard), generalPasteboard];
        if pb.is_null() {
            return Err(-1);
        }
        let _: isize = msg_send![pb, clearContents];
        let value = NSString::from_str(s);
        let ty = NSString::from_str(PASTEBOARD_TYPE_STRING);
        let ok: bool = msg_send![pb, setString: &*value, forType: &*ty];
        let change_count: isize = msg_send![pb, changeCount];
        if ok {
            Ok(change_count)
        } else {
            Err(change_count)
        }
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
