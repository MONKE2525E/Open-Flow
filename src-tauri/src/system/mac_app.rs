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
use objc2::runtime::{AnyClass, AnyObject};
use objc2::{class, msg_send};
use objc2_foundation::{NSData, NSString};
use tauri::AppHandle;

#[link(name = "AVFoundation", kind = "framework")]
extern "C" {}

#[link(name = "UserNotifications", kind = "framework")]
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

const AV_AUDIO_PERMISSION_UNDETERMINED: isize = u32::from_be_bytes(*b"undt") as isize;
const AV_AUDIO_PERMISSION_DENIED: isize = u32::from_be_bytes(*b"deny") as isize;
const AV_AUDIO_PERMISSION_GRANTED: isize = u32::from_be_bytes(*b"grnt") as isize;

/// `AVAudioApplication.recordPermission` is the modern, audio-specific API on
/// macOS 14+. Load AVFAudio dynamically so Verenu can keep its macOS 11 minimum.
fn av_audio_application_class() -> Option<&'static AnyClass> {
    static AVFAUDIO_LOADED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    let loaded = AVFAUDIO_LOADED.get_or_init(|| unsafe {
        let path = b"/System/Library/Frameworks/AVFAudio.framework/AVFAudio\0";
        !libc::dlopen(
            path.as_ptr().cast::<c_char>(),
            libc::RTLD_LAZY | libc::RTLD_LOCAL,
        )
        .is_null()
    });
    loaded
        .then(|| AnyClass::get("AVAudioApplication"))
        .flatten()
}

fn bundle_info_string(key: &str) -> Option<String> {
    autoreleasepool(|_| unsafe {
        let bundle: *mut AnyObject = msg_send![class!(NSBundle), mainBundle];
        if bundle.is_null() {
            return None;
        }
        let key = NSString::from_str(key);
        let value: *mut AnyObject = msg_send![bundle, objectForInfoDictionaryKey: &*key];
        nsstring_to_string(value)
    })
}

pub fn bundle_display_name() -> Option<String> {
    bundle_info_string("CFBundleDisplayName")
}

pub fn bundle_name() -> Option<String> {
    bundle_info_string("CFBundleName")
}

pub fn bundle_url() -> Option<String> {
    autoreleasepool(|_| unsafe {
        let bundle: *mut AnyObject = msg_send![class!(NSBundle), mainBundle];
        if bundle.is_null() {
            return None;
        }
        let url: *mut AnyObject = msg_send![bundle, bundleURL];
        if url.is_null() {
            return None;
        }
        let value: *mut AnyObject = msg_send![url, absoluteString];
        nsstring_to_string(value)
    })
}

pub fn bundle_executable_url() -> Option<String> {
    autoreleasepool(|_| unsafe {
        let bundle: *mut AnyObject = msg_send![class!(NSBundle), mainBundle];
        if bundle.is_null() {
            return None;
        }
        let url: *mut AnyObject = msg_send![bundle, executableURL];
        if url.is_null() {
            return None;
        }
        let value: *mut AnyObject = msg_send![url, absoluteString];
        nsstring_to_string(value)
    })
}

pub fn bundle_url_extension() -> Option<String> {
    autoreleasepool(|_| unsafe {
        let bundle: *mut AnyObject = msg_send![class!(NSBundle), mainBundle];
        if bundle.is_null() {
            return None;
        }
        let url: *mut AnyObject = msg_send![bundle, bundleURL];
        if url.is_null() {
            return None;
        }
        let ext: *mut AnyObject = msg_send![url, pathExtension];
        nsstring_to_string(ext)
    })
}

pub fn process_name() -> Option<String> {
    autoreleasepool(|_| unsafe {
        let process_info: *mut AnyObject = msg_send![class!(NSProcessInfo), processInfo];
        if process_info.is_null() {
            return None;
        }
        let name: *mut AnyObject = msg_send![process_info, processName];
        nsstring_to_string(name)
    })
}

/// Modern Core Audio permission status, when the API exists on this macOS.
pub fn av_audio_microphone_permission_raw() -> Option<isize> {
    let class = av_audio_application_class()?;
    autoreleasepool(|_| unsafe {
        let application: *mut AnyObject = msg_send![class, sharedInstance];
        if application.is_null() {
            return None;
        }
        let status: isize = msg_send![application, recordPermission];
        Some(status)
    })
}

pub fn av_audio_microphone_permission_status() -> Option<&'static str> {
    av_audio_microphone_permission_raw().map(|status| match status {
        AV_AUDIO_PERMISSION_GRANTED => "authorized",
        AV_AUDIO_PERMISSION_UNDETERMINED => "not_determined",
        AV_AUDIO_PERMISSION_DENIED => "denied",
        _ => "unknown",
    })
}

/// Legacy AVFoundation status retained for macOS 11-13 compatibility and
/// diagnostics when the two Apple frameworks disagree.
pub fn av_capture_microphone_permission_raw() -> isize {
    autoreleasepool(|_| unsafe {
        let media_type = NSString::from_str("soun");
        let status: isize =
            msg_send![class!(AVCaptureDevice), authorizationStatusForMediaType: &*media_type];
        status
    })
}

pub fn av_capture_microphone_permission_status() -> &'static str {
    match av_capture_microphone_permission_raw() {
        3 => "authorized",
        0 => "not_determined",
        1 => "restricted",
        2 => "denied",
        _ => "unknown",
    }
}

/// Current macOS microphone permission status for the app.
///
/// Returns one of: `authorized`, `not_determined`, `denied`, `restricted`,
/// or `unknown`.
pub fn microphone_permission_status() -> &'static str {
    // Runtime verification on macOS 26 showed AVCaptureDevice remaining
    // `notDetermined` while System Settings was enabled and the current
    // audio-specific API returned `grnt`. Use exactly one source by OS API
    // availability: AVAudioApplication on macOS 14+, AVCaptureDevice only on
    // older systems where AVAudioApplication does not exist.
    av_audio_microphone_permission_status().unwrap_or_else(av_capture_microphone_permission_status)
}

/// Request microphone access, showing the macOS consent prompt when the
/// permission is undetermined. Fails after a bounded wait if AVFoundation never
/// calls its completion handler, rather than leaving the permissions UI stuck.
/// Safe to call when already authorized (no prompt is shown).
pub async fn request_microphone() -> Result<bool, String> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let tx = std::sync::Mutex::new(Some(tx));
    autoreleasepool(|_| unsafe {
        let handler = block2::RcBlock::new(move |granted: objc2::runtime::Bool| {
            let granted = granted.as_bool();
            if let Ok(mut guard) = tx.lock() {
                if let Some(tx) = guard.take() {
                    let _ = tx.send(granted);
                }
            }
        });
        // Keep the request paired with the same AVCaptureDevice API used for
        // the authoritative UI status. The AVAudioApplication value is logged
        // after completion as a diagnostic only.
        let media_type = NSString::from_str("soun");
        let _: () = msg_send![
            class!(AVCaptureDevice),
            requestAccessForMediaType: &*media_type,
            completionHandler: &*handler
        ];
    });
    tokio::time::timeout(std::time::Duration::from_secs(60), rx)
        .await
        .map_err(|_| {
            "macOS did not answer the microphone permission request within 60 seconds.".to_string()
        })?
        .map_err(|_| "macOS closed the microphone permission request unexpectedly.".to_string())
}

/// Starts the AVCaptureDevice request on AppKit's main thread. Calling this
/// directly from a Tauri async command can produce an immediate `false`
/// callback while TCC remains `notDetermined` and no system sheet appears.
pub async fn request_microphone_on_main_thread(app: &AppHandle) -> Result<bool, String> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    app.run_on_main_thread(move || {
        let _ = activate_current_app();
        let tx = std::sync::Mutex::new(Some(tx));
        autoreleasepool(|_| unsafe {
            let handler = block2::RcBlock::new(move |granted: objc2::runtime::Bool| {
                if let Ok(mut guard) = tx.lock() {
                    if let Some(tx) = guard.take() {
                        let _ = tx.send(granted.as_bool());
                    }
                }
            });
            let media_type = NSString::from_str("soun");
            let _: () = msg_send![
                class!(AVCaptureDevice),
                requestAccessForMediaType: &*media_type,
                completionHandler: &*handler
            ];
        });
    })
    .map_err(|error| format!("Could not dispatch microphone request to main thread: {error}"))?;

    tokio::time::timeout(std::time::Duration::from_secs(60), rx)
        .await
        .map_err(|_| {
            "macOS did not answer the microphone permission request within 60 seconds.".to_string()
        })?
        .map_err(|_| "macOS closed the microphone permission request unexpectedly.".to_string())
}

/// macOS 14+ native record-permission request. AVCaptureDevice remains the
/// authoritative status query; this API is used only to present/complete the
/// consent transaction on current macOS releases.
pub async fn request_audio_application_on_main_thread(app: &AppHandle) -> Result<bool, String> {
    let class = av_audio_application_class()
        .ok_or_else(|| "AVAudioApplication is unavailable on this macOS version".to_string())?;
    let (tx, rx) = tokio::sync::oneshot::channel();
    app.run_on_main_thread(move || {
        let _ = activate_current_app();
        let tx = std::sync::Mutex::new(Some(tx));
        autoreleasepool(|_| unsafe {
            let handler = block2::RcBlock::new(move |granted: objc2::runtime::Bool| {
                if let Ok(mut guard) = tx.lock() {
                    if let Some(tx) = guard.take() {
                        let _ = tx.send(granted.as_bool());
                    }
                }
            });
            let _: () = msg_send![
                class,
                requestRecordPermissionWithCompletionHandler: &*handler
            ];
        });
    })
    .map_err(|error| format!("Could not dispatch microphone request to main thread: {error}"))?;

    tokio::time::timeout(std::time::Duration::from_secs(60), rx)
        .await
        .map_err(|_| {
            "macOS did not answer the microphone permission request within 60 seconds.".to_string()
        })?
        .map_err(|_| "macOS closed the microphone permission request unexpectedly.".to_string())
}

/// Documented AVFoundation prompt path for a still-undetermined status:
/// creating an AVCaptureDeviceInput automatically presents the consent UI.
/// The input result is deliberately ignored; authorizationStatus remains the
/// only permission truth and is polled for the user's decision.
pub async fn request_microphone_via_device_input(app: &AppHandle) -> Result<bool, String> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    app.run_on_main_thread(move || {
        let _ = set_regular_activation_policy();
        let _ = activate_current_app();
        autoreleasepool(|_| unsafe {
            let media_type = NSString::from_str("soun");
            let device: *mut AnyObject = msg_send![
                class!(AVCaptureDevice),
                defaultDeviceWithMediaType: &*media_type
            ];
            if device.is_null() {
                let _ = tx.send(Err("No audio capture device is available".to_string()));
                return;
            }
            let mut error: *mut AnyObject = std::ptr::null_mut();
            let _: *mut AnyObject = msg_send![
                class!(AVCaptureDeviceInput),
                deviceInputWithDevice: device,
                error: &mut error
            ];
            let _ = tx.send(Ok(()));
        });
    })
    .map_err(|error| format!("Could not dispatch microphone input request: {error}"))?;
    rx.await
        .map_err(|_| "Microphone input request task closed unexpectedly".to_string())??;

    for _ in 0..240 {
        let status = microphone_permission_status();
        if status != "not_determined" {
            return Ok(status == "authorized");
        }
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    }
    Err("macOS did not produce a microphone authorization decision within 60 seconds".to_string())
}

/// Returns the raw values from UNUserNotificationCenter. This is deliberately
/// separate from the Tauri notification plugin's boolean helper: the
/// Permissions page needs Apple's authorization and presentation settings.
pub async fn notification_settings() -> Result<[i64; 6], String> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let tx = std::sync::Mutex::new(Some(tx));
    autoreleasepool(|_| unsafe {
        let center: *mut AnyObject =
            msg_send![class!(UNUserNotificationCenter), currentNotificationCenter];
        if center.is_null() {
            if let Ok(mut guard) = tx.lock() {
                if let Some(tx) = guard.take() {
                    let _ = tx.send(Err("UNUserNotificationCenter was unavailable".to_string()));
                }
            }
            return;
        }
        let handler = block2::RcBlock::new(move |settings: *mut AnyObject| {
            if settings.is_null() {
                if let Ok(mut guard) = tx.lock() {
                    if let Some(tx) = guard.take() {
                        let _ = tx.send(Err("UNNotificationSettings was null".to_string()));
                    }
                }
                return;
            }
            let values = [
                msg_send![settings, authorizationStatus],
                msg_send![settings, alertSetting],
                msg_send![settings, soundSetting],
                msg_send![settings, badgeSetting],
                msg_send![settings, notificationCenterSetting],
                msg_send![settings, lockScreenSetting],
            ];
            if let Ok(mut guard) = tx.lock() {
                if let Some(tx) = guard.take() {
                    let _ = tx.send(Ok(values));
                }
            }
        });
        let _: () = msg_send![center, getNotificationSettingsWithCompletionHandler: &*handler];
    });
    match tokio::time::timeout(std::time::Duration::from_secs(5), rx).await {
        Ok(Ok(result)) => result,
        Ok(Err(_)) => Err("Notification settings callback dropped".to_string()),
        Err(_) => Err("Timed out querying notification settings".to_string()),
    }
}

pub async fn request_notifications() -> Result<(), String> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let tx = std::sync::Mutex::new(Some(tx));
    autoreleasepool(|_| unsafe {
        let center: *mut AnyObject =
            msg_send![class!(UNUserNotificationCenter), currentNotificationCenter];
        if center.is_null() {
            if let Ok(mut guard) = tx.lock() {
                if let Some(tx) = guard.take() {
                    let _ = tx.send(Err("UNUserNotificationCenter was unavailable".to_string()));
                }
            }
            return;
        }
        // alert | sound | badge; request is only called from an explicit UI action.
        let handler = block2::RcBlock::new(
            move |_granted: objc2::runtime::Bool, _error: *mut AnyObject| {
                if let Ok(mut guard) = tx.lock() {
                    if let Some(tx) = guard.take() {
                        let _ = tx.send(());
                    }
                }
            },
        );
        let _: () =
            msg_send![center, requestAuthorizationWithOptions: 7usize, completionHandler: &*handler];
    });
    tokio::time::timeout(std::time::Duration::from_secs(30), rx)
        .await
        .map_err(|_| "Timed out requesting notification authorization".to_string())?
        .map_err(|_| "Notification authorization callback dropped".to_string())
}

pub async fn request_notifications_on_main_thread(app: &AppHandle) -> Result<(), String> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    app.run_on_main_thread(move || {
        let tx = std::sync::Mutex::new(Some(tx));
        autoreleasepool(|_| unsafe {
            let center: *mut AnyObject =
                msg_send![class!(UNUserNotificationCenter), currentNotificationCenter];
            if center.is_null() {
                if let Some(tx) = tx.lock().ok().and_then(|mut guard| guard.take()) {
                    let _ = tx.send(Err("UNUserNotificationCenter was unavailable".to_string()));
                }
                return;
            }
            let handler = block2::RcBlock::new(
                move |_granted: objc2::runtime::Bool, _error: *mut AnyObject| {
                    if let Ok(mut guard) = tx.lock() {
                        if let Some(tx) = guard.take() {
                            let _ = tx.send(());
                        }
                    }
                },
            );
            let _: () = msg_send![
                center,
                requestAuthorizationWithOptions: 7usize,
                completionHandler: &*handler
            ];
        });
    })
    .map_err(|error| format!("Could not dispatch notification request to main thread: {error}"))?;

    tokio::time::timeout(std::time::Duration::from_secs(60), rx)
        .await
        .map_err(|_| "Timed out requesting notification authorization".to_string())?
        .map_err(|_| "Notification authorization callback dropped".to_string())
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

/// Takes a conservative plain-text snapshot of the general pasteboard.
///
/// Do not eagerly call `dataForType:` for every advertised representation.
/// Clipboard owners may advertise lazily generated formats and block that
/// call indefinitely (observed with a live owner while dictation sat at
/// "Pasting…" for five minutes). The caller also applies a wall-clock bound.
/// Rich-only clipboards are deliberately not restored rather than risking a
/// permanent paste hang.
pub fn pasteboard_snapshot() -> Option<PasteboardSnapshot> {
    autoreleasepool(|_| unsafe {
        let pb: *mut AnyObject = msg_send![class!(NSPasteboard), generalPasteboard];
        if pb.is_null() {
            return None;
        }
        let ty = NSString::from_str(PASTEBOARD_TYPE_STRING);
        let value: *mut AnyObject = msg_send![pb, stringForType: &*ty];
        let text = nsstring_to_string(value)?;
        log::info!(
            "pasteboard snapshot: captured safe plain-text representation bytes={}",
            text.len()
        );
        Some(PasteboardSnapshot {
            items: vec![vec![(
                PASTEBOARD_TYPE_STRING.to_string(),
                text.into_bytes(),
            )]],
        })
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
