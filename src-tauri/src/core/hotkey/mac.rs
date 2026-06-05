//! macOS global hold/release hotkey via a `CGEventTap`.
//!
//! Mirrors the public contract of the Windows backend (`super::win`):
//! `start`, `update_keys`, `map_code_to_vk`, `is_hotkey_available`,
//! `reset_chord_state`, `set_handless_active`.
//!
//! Design notes:
//! - A **listen-only** tap is used (`CGEventTapOptions::ListenOnly`). It observes
//!   key/modifier state but never deletes events, so it can't cause stuck
//!   modifiers or swallow shortcuts. The trade-off is that we cannot suppress the
//!   OS Fn/Globe "show emoji" action — users set System Settings → Keyboard →
//!   "Press 🌐 key to: Do Nothing". The tap requires Accessibility / Input
//!   Monitoring permission (the OS prompts on first creation).
//! - Key ids produced by `map_code_to_vk` are private to this backend: modifiers
//!   are tiny sentinel ids (1..=6) resolved against `CGEventFlags`; regular keys
//!   are `REGULAR_BASE + <macOS keycode>` and tracked via key down/up.
//! - The default chord is **Control (key1) + Fn (key2)**.

use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, AtomicPtr, AtomicU32, AtomicU64, Ordering};
use std::sync::mpsc;
use std::sync::OnceLock;
use std::time::Instant;

use core_foundation::base::TCFType;
use core_foundation::runloop::{kCFRunLoopCommonModes, CFRunLoop};
use core_foundation_sys::mach_port::CFMachPortRef;
use core_graphics::event::{
    CGEventTap, CGEventTapLocation, CGEventTapOptions, CGEventTapPlacement, CGEventTapProxy,
    CGEventType, EventField,
};
use foreign_types_shared::ForeignType;

#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGEventTapEnable(tap: CFMachPortRef, enable: bool);
    fn CGEventKeyboardGetUnicodeString(
        event: core_graphics::sys::CGEventRef,
        max_string_length: libc::c_ulong,
        actual_string_length: *mut libc::c_ulong,
        unicode_string: *mut u16,
    );
}

// --- private key id scheme -------------------------------------------------

const ID_CONTROL: u32 = 1;
const ID_SHIFT: u32 = 2;
const ID_ALT: u32 = 3;
const ID_COMMAND: u32 = 4;
const ID_FN: u32 = 5;
const ID_CAPS: u32 = 6;
const REGULAR_BASE: u32 = 0x100;

const KEY_ESCAPE: i64 = 53; // macOS virtual keycode for Escape
const KEY_TAB: i64 = 48;
const KEY_BACKSPACE: i64 = 51;
const KEY_RETURN: i64 = 36;
const KEY_FORWARD_DELETE: i64 = 117;
const KEY_HOME: i64 = 115;
const KEY_END: i64 = 119;
const KEY_PAGE_UP: i64 = 116;
const KEY_PAGE_DOWN: i64 = 121;
const KEY_LEFT: i64 = 123;
const KEY_RIGHT: i64 = 124;
const KEY_DOWN: i64 = 125;
const KEY_UP: i64 = 126;

// Keep production builds quiet unless we're actively debugging the event tap.
const HOTKEY_DEBUG: bool = false;

fn is_modifier(id: u32) -> bool {
    (ID_CONTROL..=ID_CAPS).contains(&id)
}

// CGEventFlags raw bits (kCGEventFlagMask*). Kept as raw u64 so this module
// doesn't depend on the exact associated-constant names across crate versions.
const FLAG_ALPHASHIFT: u64 = 0x0001_0000; // caps lock
const FLAG_SHIFT: u64 = 0x0002_0000;
const FLAG_CONTROL: u64 = 0x0004_0000;
const FLAG_ALTERNATE: u64 = 0x0008_0000;
const FLAG_COMMAND: u64 = 0x0010_0000;
const FLAG_SECONDARY_FN: u64 = 0x0080_0000;

fn modifier_mask(id: u32) -> u64 {
    match id {
        ID_CONTROL => FLAG_CONTROL,
        ID_SHIFT => FLAG_SHIFT,
        ID_ALT => FLAG_ALTERNATE,
        ID_COMMAND => FLAG_COMMAND,
        ID_FN => FLAG_SECONDARY_FN,
        ID_CAPS => FLAG_ALPHASHIFT,
        _ => 0,
    }
}

// --- state -----------------------------------------------------------------

static KEY1: AtomicU32 = AtomicU32::new(ID_CONTROL); // held modifier (default Ctrl)
static KEY2: AtomicU32 = AtomicU32::new(ID_FN); // trigger (default Fn)

static PRESS_CB: OnceLock<Box<dyn Fn() + Send + Sync>> = OnceLock::new();
static RELEASE_CB: OnceLock<Box<dyn Fn() + Send + Sync>> = OnceLock::new();
static HANDLESS_CB: OnceLock<Box<dyn Fn() + Send + Sync>> = OnceLock::new();
static CANCEL_CB: OnceLock<Box<dyn Fn() + Send + Sync>> = OnceLock::new();
static ESCAPE_CB: OnceLock<Box<dyn Fn() + Send + Sync>> = OnceLock::new();

static CHORD_ACTIVE: AtomicBool = AtomicBool::new(false);
static HANDLESS_ACTIVE: AtomicBool = AtomicBool::new(false);
static ESCAPE_CANCELLED: AtomicBool = AtomicBool::new(false);
static CHORD_DOWN_MS: AtomicU64 = AtomicU64::new(0);
static HANDLESS_PENDING: AtomicBool = AtomicBool::new(false);
static HANDLESS_PENDING_MS: AtomicU64 = AtomicU64::new(0);
static EVENT_TAP_PORT: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());

// Down-state for the (rare) case where a configured chord key is a regular key
// rather than a modifier. Modifiers are read from CGEventFlags instead.
static K1_REGULAR_DOWN: AtomicBool = AtomicBool::new(false);
static K2_REGULAR_DOWN: AtomicBool = AtomicBool::new(false);
static SYNTHETIC_PASTE_UNTIL_MS: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy)]
struct HotkeyEvent {
    etype: CGEventType,
    flags: u64,
    keycode: i64,
    text: [u16; 8],
    text_len: u8,
}

fn now_ms() -> u64 {
    static START: OnceLock<Instant> = OnceLock::new();
    START.get_or_init(Instant::now).elapsed().as_millis() as u64
}

pub fn begin_synthetic_paste_suppression(duration_ms: u64) {
    SYNTHETIC_PASTE_UNTIL_MS.store(now_ms().saturating_add(duration_ms), Ordering::SeqCst);
}

fn synthetic_paste_suppressed(now: u64) -> bool {
    SYNTHETIC_PASTE_UNTIL_MS.load(Ordering::SeqCst) > now
}

fn reenable_event_tap() {
    let tap_port = EVENT_TAP_PORT.load(Ordering::SeqCst) as CFMachPortRef;
    if tap_port.is_null() {
        log::warn!("macOS event tap disabled before a tap port was registered");
        return;
    }

    unsafe {
        CGEventTapEnable(tap_port, true);
    }
    log::info!("macOS event tap re-enabled");
}

// --- public contract -------------------------------------------------------

pub fn update_keys(k1: u32, k2: u32) {
    if k1 != 0 {
        KEY1.store(k1, Ordering::SeqCst);
    }
    if k2 != 0 {
        KEY2.store(k2, Ordering::SeqCst);
    }
    reset_chord_state();
}

pub fn reset_chord_state() {
    CHORD_ACTIVE.store(false, Ordering::SeqCst);
    ESCAPE_CANCELLED.store(false, Ordering::SeqCst);
    HANDLESS_PENDING.store(false, Ordering::SeqCst);
    HANDLESS_PENDING_MS.store(0, Ordering::SeqCst);
    CHORD_DOWN_MS.store(0, Ordering::SeqCst);
    K1_REGULAR_DOWN.store(false, Ordering::SeqCst);
    K2_REGULAR_DOWN.store(false, Ordering::SeqCst);
}

pub fn set_handless_active(v: bool) {
    HANDLESS_ACTIVE.store(v, Ordering::SeqCst);
}

/// A chord is registrable as long as the trigger key maps to a known id.
pub fn is_hotkey_available(_key1: &str, key2: &str) -> bool {
    map_code_to_vk(key2) != 0
}

/// Map a JS `KeyboardEvent.code` to this backend's private key id.
pub fn map_code_to_vk(code: &str) -> u32 {
    match code {
        "ControlLeft" | "ControlRight" => ID_CONTROL,
        "ShiftLeft" | "ShiftRight" => ID_SHIFT,
        "AltLeft" | "AltRight" => ID_ALT,
        "MetaLeft" | "MetaRight" => ID_COMMAND,
        "Fn" => ID_FN,
        "CapsLock" => ID_CAPS,
        other => js_code_to_mac_keycode(other)
            .map(|kc| REGULAR_BASE + kc)
            .unwrap_or(0),
    }
}

/// macOS ANSI virtual keycodes for the common `KeyboardEvent.code` values a user
/// might rebind to. The default Fn+Control chord uses none of these.
fn js_code_to_mac_keycode(code: &str) -> Option<u32> {
    let kc = match code {
        "KeyA" => 0,
        "KeyS" => 1,
        "KeyD" => 2,
        "KeyF" => 3,
        "KeyH" => 4,
        "KeyG" => 5,
        "KeyZ" => 6,
        "KeyX" => 7,
        "KeyC" => 8,
        "KeyV" => 9,
        "KeyB" => 11,
        "KeyQ" => 12,
        "KeyW" => 13,
        "KeyE" => 14,
        "KeyR" => 15,
        "KeyY" => 16,
        "KeyT" => 17,
        "KeyO" => 31,
        "KeyU" => 32,
        "KeyI" => 34,
        "KeyP" => 35,
        "KeyL" => 37,
        "KeyJ" => 38,
        "KeyK" => 40,
        "KeyN" => 45,
        "KeyM" => 46,
        "Digit1" => 18,
        "Digit2" => 19,
        "Digit3" => 20,
        "Digit4" => 21,
        "Digit5" => 23,
        "Digit6" => 22,
        "Digit7" => 26,
        "Digit8" => 28,
        "Digit9" => 25,
        "Digit0" => 29,
        "Space" => 49,
        "Enter" => 36,
        "Tab" => 48,
        "Backquote" => 50,
        "Minus" => 27,
        "Equal" => 24,
        "BracketLeft" => 33,
        "BracketRight" => 30,
        "Backslash" => 42,
        "Semicolon" => 41,
        "Quote" => 39,
        "Comma" => 43,
        "Period" => 47,
        "Slash" => 44,
        "ArrowLeft" => 123,
        "ArrowRight" => 124,
        "ArrowDown" => 125,
        "ArrowUp" => 126,
        "F1" => 122,
        "F2" => 120,
        "F3" => 99,
        "F4" => 118,
        "F5" => 96,
        "F6" => 97,
        "F7" => 98,
        "F8" => 100,
        "F9" => 101,
        "F10" => 109,
        "F11" => 103,
        "F12" => 111,
        _ => return None,
    };
    Some(kc)
}

pub fn start<P, R, H, C, E>(
    on_press: P,
    on_release: R,
    on_handless: H,
    on_cancel: C,
    on_escape: E,
) -> Result<std::thread::JoinHandle<()>, String>
where
    P: Fn() + Send + Sync + 'static,
    R: Fn() + Send + Sync + 'static,
    H: Fn() + Send + Sync + 'static,
    C: Fn() + Send + Sync + 'static,
    E: Fn() + Send + Sync + 'static,
{
    let _ = PRESS_CB.set(Box::new(on_press));
    let _ = RELEASE_CB.set(Box::new(on_release));
    let _ = HANDLESS_CB.set(Box::new(on_handless));
    let _ = CANCEL_CB.set(Box::new(on_cancel));
    let _ = ESCAPE_CB.set(Box::new(on_escape));

    let (tx, rx) = mpsc::channel::<HotkeyEvent>();
    std::thread::spawn(move || {
        while let Ok(event) = rx.recv() {
            handle_event(event);
        }
    });

    let handle = std::thread::spawn(move || {
        // CGEventTap creation returns Err until Accessibility permission is
        // granted. Rather than failing permanently (forcing an app restart after
        // the user grants it), poll until it succeeds.
        let mut attempt: u32 = 0;
        let tap = loop {
            let result = CGEventTap::new(
                CGEventTapLocation::HID,
                CGEventTapPlacement::HeadInsertEventTap,
                CGEventTapOptions::ListenOnly,
                // core-graphics 0.24.x builds the CGEventMask internally from
                // a Vec<CGEventType>, so keep the typed list here.
                vec![
                    CGEventType::KeyDown,
                    CGEventType::KeyUp,
                    CGEventType::FlagsChanged,
                ],
                {
                    let tx = tx.clone();
                    move |_proxy: CGEventTapProxy, etype: CGEventType, event| {
                        let (flags, keycode, text, text_len) = if matches!(
                            etype,
                            CGEventType::TapDisabledByTimeout | CGEventType::TapDisabledByUserInput
                        ) {
                            (0, 0, [0u16; 8], 0)
                        } else {
                            let mut text = [0u16; 8];
                            let mut actual_len = 0u8;
                            if matches!(etype, CGEventType::KeyDown) {
                                let mut out_len = 0 as libc::c_ulong;
                                unsafe {
                                    CGEventKeyboardGetUnicodeString(
                                        event.as_ptr(),
                                        text.len() as libc::c_ulong,
                                        &mut out_len,
                                        text.as_mut_ptr(),
                                    );
                                }
                                actual_len = out_len.min(text.len() as libc::c_ulong) as u8;
                            }
                            (
                                event.get_flags().bits(),
                                event.get_integer_value_field(EventField::KEYBOARD_EVENT_KEYCODE),
                                text,
                                actual_len,
                            )
                        };
                        let _ = tx.send(HotkeyEvent {
                            etype,
                            flags,
                            keycode,
                            text,
                            text_len,
                        });
                        None
                    }
                },
            );
            match result {
                Ok(t) => break t,
                Err(()) => {
                    #[allow(clippy::manual_is_multiple_of)]
                    if attempt % 10 == 0 {
                        log::error!(
                            "CGEventTap not created — grant Open Flow Accessibility permission (System Settings → Privacy & Security → Accessibility). Retrying…"
                        );
                        if HOTKEY_DEBUG {
                            eprintln!("[hotkey] CGEventTap::new failed (attempt {attempt}) — Accessibility not granted yet");
                        }
                    }
                    attempt += 1;
                    std::thread::sleep(std::time::Duration::from_secs(2));
                }
            }
        };

        let loop_source = match tap.mach_port.create_runloop_source(0) {
            Ok(s) => s,
            Err(()) => {
                log::error!("failed to create run-loop source for event tap");
                return;
            }
        };
        let current = CFRunLoop::get_current();
        unsafe {
            current.add_source(&loop_source, kCFRunLoopCommonModes);
        }
        EVENT_TAP_PORT.store(
            tap.mach_port.as_concrete_TypeRef() as *mut c_void,
            Ordering::SeqCst,
        );
        tap.enable();
        log::info!("macOS hotkey event tap installed");
        if HOTKEY_DEBUG {
            eprintln!("[hotkey] event tap installed — listening for Fn+Control");
        }
        CFRunLoop::run_current();
    });

    Ok(handle)
}

// --- event handling --------------------------------------------------------

fn keycode_is_navigation_or_reset(keycode: i64) -> bool {
    matches!(
        keycode,
        KEY_TAB
            | KEY_FORWARD_DELETE
            | KEY_ESCAPE
            | KEY_HOME
            | KEY_END
            | KEY_PAGE_UP
            | KEY_PAGE_DOWN
            | KEY_LEFT
            | KEY_RIGHT
            | KEY_DOWN
            | KEY_UP
    )
}

fn shortcut_modifier_held(flags: u64) -> bool {
    flags & (FLAG_CONTROL | FLAG_ALTERNATE | FLAG_COMMAND) != 0
}

fn event_text(event: &HotkeyEvent) -> String {
    String::from_utf16_lossy(&event.text[..event.text_len as usize])
}

fn update_injection_history_for_event(event: &HotkeyEvent, now: u64) {
    if !matches!(event.etype, CGEventType::KeyDown) || synthetic_paste_suppressed(now) {
        return;
    }

    let keycode = event.keycode;
    let flags = event.flags;

    if keycode == KEY_BACKSPACE {
        if shortcut_modifier_held(flags) {
            crate::core::injection::reset_injection_history();
        } else {
            let hwnd = crate::core::window_context::get_foreground_hwnd();
            crate::core::injection::backspace_injection_history(hwnd);
        }
        return;
    }

    if keycode == KEY_RETURN {
        if flags & FLAG_SHIFT != 0 {
            let hwnd = crate::core::window_context::get_foreground_hwnd();
            if hwnd != 0 {
                crate::core::injection::append_or_reset_injection_history(hwnd, '\n');
            }
        } else {
            crate::core::injection::reset_injection_history();
        }
        return;
    }

    if keycode_is_navigation_or_reset(keycode) || shortcut_modifier_held(flags) {
        crate::core::injection::reset_injection_history();
        return;
    }

    let text = event_text(event);
    let mut appended = false;
    let hwnd = crate::core::window_context::get_foreground_hwnd();
    if hwnd == 0 {
        crate::core::injection::reset_injection_history();
        return;
    }
    for ch in text.chars().filter(|ch| !ch.is_control() || *ch == '\n' || *ch == '\r') {
        crate::core::injection::append_or_reset_injection_history(hwnd, ch);
        appended = true;
    }

    if !appended && !text.is_empty() {
        crate::core::injection::reset_injection_history();
    }
}

fn handle_event(event: HotkeyEvent) {
    let etype = event.etype;
    // The system can disable a tap during lag or timeout; re-enable so the
    // global hotkey does not stop permanently.
    if matches!(
        etype,
        CGEventType::TapDisabledByTimeout | CGEventType::TapDisabledByUserInput
    ) {
        log::warn!("macOS event tap disabled by system ({etype:?})");
        reenable_event_tap();
        return;
    }

    let k1 = KEY1.load(Ordering::Relaxed);
    let k2 = KEY2.load(Ordering::Relaxed);
    let flags = event.flags;
    let keycode = event.keycode;
    let is_down = matches!(etype, CGEventType::KeyDown);
    let is_up = matches!(etype, CGEventType::KeyUp);

    // Track down-state for regular (non-modifier) configured keys.
    if k1 >= REGULAR_BASE {
        let kc = (k1 - REGULAR_BASE) as i64;
        if is_down && keycode == kc {
            K1_REGULAR_DOWN.store(true, Ordering::SeqCst);
        } else if is_up && keycode == kc {
            K1_REGULAR_DOWN.store(false, Ordering::SeqCst);
        }
    }
    if k2 >= REGULAR_BASE {
        let kc = (k2 - REGULAR_BASE) as i64;
        if is_down && keycode == kc {
            K2_REGULAR_DOWN.store(true, Ordering::SeqCst);
        } else if is_up && keycode == kc {
            K2_REGULAR_DOWN.store(false, Ordering::SeqCst);
        }
    }

    let held = |id: u32, regular_down: &AtomicBool| -> bool {
        if is_modifier(id) {
            flags & modifier_mask(id) != 0
        } else {
            regular_down.load(Ordering::SeqCst)
        }
    };
    let held1 = held(k1, &K1_REGULAR_DOWN);
    let held2 = held(k2, &K2_REGULAR_DOWN);
    let both = held1 && held2;

    if HOTKEY_DEBUG && matches!(etype, CGEventType::FlagsChanged) {
        eprintln!(
            "[hotkey] flagsChanged flags={flags:#010x} keycode={keycode} k1={k1} k2={k2} held1={held1} held2={held2} both={both}"
        );
    }

    // Escape cancels an in-flight recording / handsfree session.
    if is_down
        && keycode == KEY_ESCAPE
        && (CHORD_ACTIVE.load(Ordering::SeqCst) || HANDLESS_ACTIVE.load(Ordering::SeqCst))
    {
        if CHORD_ACTIVE.load(Ordering::SeqCst) {
            ESCAPE_CANCELLED.store(true, Ordering::SeqCst);
        }
        if let Some(cb) = ESCAPE_CB.get() {
            cb();
        }
        return;
    }

    let active = CHORD_ACTIVE.load(Ordering::SeqCst);
    let now = now_ms();

    if matches!(etype, CGEventType::KeyDown) {
        update_injection_history_for_event(&event, now);
    }

    if both && !active {
        // Double-tap of the chord within the window toggles handsfree mode.
        if HANDLESS_PENDING.swap(false, Ordering::SeqCst)
            && now.saturating_sub(HANDLESS_PENDING_MS.load(Ordering::SeqCst)) <= 350
        {
            if let Some(cb) = HANDLESS_CB.get() {
                cb();
            }
            return;
        }
        CHORD_ACTIVE.store(true, Ordering::SeqCst);
        CHORD_DOWN_MS.store(now, Ordering::SeqCst);
        ESCAPE_CANCELLED.store(false, Ordering::SeqCst);
        if HOTKEY_DEBUG {
            eprintln!("[hotkey] PRESS (chord engaged) → start recording");
        }
        if let Some(cb) = PRESS_CB.get() {
            cb();
        }
    } else if active && !both {
        if HOTKEY_DEBUG {
            eprintln!("[hotkey] RELEASE (chord released)");
        }
        CHORD_ACTIVE.store(false, Ordering::SeqCst);
        let held_ms = now.saturating_sub(CHORD_DOWN_MS.load(Ordering::SeqCst));
        if ESCAPE_CANCELLED.swap(false, Ordering::SeqCst) {
            // Escape already cancelled this session — emit nothing further.
        } else if held_ms < 250 {
            // Quick tap → cancel and arm the double-tap (handsfree) window.
            HANDLESS_PENDING.store(true, Ordering::SeqCst);
            HANDLESS_PENDING_MS.store(now, Ordering::SeqCst);
            if let Some(cb) = CANCEL_CB.get() {
                cb();
            }
        } else if let Some(cb) = RELEASE_CB.get() {
            cb();
        }
    }
}
