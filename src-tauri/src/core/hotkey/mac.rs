//! macOS global hold/release hotkey via Carbon `RegisterEventHotKey`
//! (through the `global-hotkey` crate).
//!
//! Mirrors the public contract of the Windows backend (`super::win`):
//! `start`, `update_keys`, `map_code_to_vk`, `is_hotkey_available`,
//! `reset_chord_state`, `set_handless_active`, `begin_synthetic_paste_suppression`,
//! `caps_lock_is_on`.
//!
//! Design notes:
//! - `RegisterEventHotKey` requires **no permission** — not Accessibility, not
//!   Input Monitoring. It delivers `Pressed` / `Released` events for a single
//!   registered key combination, which is exactly what hold-to-talk needs. The
//!   trade-off vs. the old `CGEventTap` is that the hotkey can no longer be a
//!   pure modifier chord (e.g. Fn+Control): Carbon hotkeys need a real key, so
//!   the default is **⌥ Option + Space** (two adjacent bottom-row keys, no Fn
//!   gymnastics and no Spotlight conflict).
//! - Because we no longer observe every keystroke, we also no longer feed the
//!   injection-history capitalization fallback on macOS — that path now relies on
//!   the Accessibility caret read / clipboard sniff (see `core::context_probe`).
//! - Key ids produced by `map_code_to_vk` are private to this backend: modifiers
//!   are tiny sentinel ids (1..=6); regular keys are `REGULAR_BASE + <macOS
//!   keycode>`. `update_keys` resolves them into a single `global_hotkey::HotKey`.

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

use global_hotkey::hotkey::{Code, HotKey, Modifiers};
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};

#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGEventSourceFlagsState(state_id: i32) -> u64;
}

// caps-lock bit in CGEventFlags (kCGEventFlagMaskAlphaShift).
const FLAG_ALPHASHIFT: u64 = 0x0001_0000;

// --- private key id scheme -------------------------------------------------

const ID_CONTROL: u32 = 1;
const ID_SHIFT: u32 = 2;
const ID_ALT: u32 = 3;
const ID_COMMAND: u32 = 4;
const ID_FN: u32 = 5;
const ID_CAPS: u32 = 6;
const REGULAR_BASE: u32 = 0x100;

// macOS virtual keycode for Space — the trigger key of the default ⌥+Space hotkey.
const MAC_KEYCODE_SPACE: u32 = 49;

// Double-tap window for handsfree toggle, and the max hold treated as a "tap".
const HANDSFREE_DOUBLE_TAP_MS: u64 = 350;
const TAP_MAX_HOLD_MS: u64 = 250;

// --- state -----------------------------------------------------------------

// Default hotkey: ⌥ Option + Space. KEY1 is the Option modifier, KEY2 the Space key.
static KEY1: AtomicU32 = AtomicU32::new(ID_ALT);
static KEY2: AtomicU32 = AtomicU32::new(REGULAR_BASE + MAC_KEYCODE_SPACE);

static PRESS_CB: OnceLock<Box<dyn Fn() + Send + Sync>> = OnceLock::new();
static RELEASE_CB: OnceLock<Box<dyn Fn() + Send + Sync>> = OnceLock::new();
static HANDLESS_CB: OnceLock<Box<dyn Fn() + Send + Sync>> = OnceLock::new();
static CANCEL_CB: OnceLock<Box<dyn Fn() + Send + Sync>> = OnceLock::new();
static ESCAPE_CB: OnceLock<Box<dyn Fn() + Send + Sync>> = OnceLock::new();
static COPY_LAST_CB: OnceLock<Box<dyn Fn() + Send + Sync>> = OnceLock::new();

static CHORD_ACTIVE: AtomicBool = AtomicBool::new(false);
static HANDLESS_ACTIVE: AtomicBool = AtomicBool::new(false);
static ESCAPE_CANCELLED: AtomicBool = AtomicBool::new(false);
static CHORD_DOWN_MS: AtomicU64 = AtomicU64::new(0);
static HANDLESS_PENDING: AtomicBool = AtomicBool::new(false);
static HANDLESS_PENDING_MS: AtomicU64 = AtomicU64::new(0);

// 0 = not processing. Mirrors the Windows backend: set once at Stopping ->
// Processing, cleared via compare-exchange so a stale/superseded task's
// cleanup can never clobber a newer generation's flag. Used so Escape keeps
// working (cancels in-flight processing) even though no chord is held.
static PROCESSING_GENERATION: AtomicU64 = AtomicU64::new(0);

pub fn set_processing_generation(generation: u64) {
    PROCESSING_GENERATION.store(generation, Ordering::SeqCst);
    refresh_escape_listening();
}

pub fn clear_processing_generation(expected_generation: u64) {
    let _ = PROCESSING_GENERATION.compare_exchange(
        expected_generation,
        0,
        Ordering::SeqCst,
        Ordering::SeqCst,
    );
    refresh_escape_listening();
}

// `GlobalHotKeyManager` is `unsafe impl Send + Sync` (it guards Carbon access
// with an internal mutex), so it is safe to hold in a static and re-register
// the hotkey from `update_keys` on any thread.
static MANAGER: OnceLock<GlobalHotKeyManager> = OnceLock::new();
static CURRENT_HOTKEY: Mutex<Option<HotKey>> = Mutex::new(None);
static MAIN_HOTKEY_ID: AtomicU32 = AtomicU32::new(0);
static ESCAPE_HOTKEY: OnceLock<HotKey> = OnceLock::new();

// User-configurable "open the repair complaint box" hotkey: two modifiers
// plus one regular trigger key, default Ctrl+Alt+Z. Matches the Windows
// backend's shape (see its REPAIR_MOD1 doc comment for why a modifier-only
// combo is disallowed there) even though Carbon's RegisterEventHotKey was
// never at risk of that specific bug itself.
static REPAIR_KEY1: AtomicU32 = AtomicU32::new(ID_CONTROL);
static REPAIR_KEY2: AtomicU32 = AtomicU32::new(ID_ALT);
static REPAIR_KEY3: AtomicU32 = AtomicU32::new(REGULAR_BASE + 6); // KeyZ
static CURRENT_REPAIR_HOTKEY: Mutex<Option<HotKey>> = Mutex::new(None);
static REPAIR_HOTKEY_ID: AtomicU32 = AtomicU32::new(0);
static REPAIR_OPEN_CB: OnceLock<Box<dyn Fn() + Send + Sync>> = OnceLock::new();
// ⌥⌘C: always-registered fallback to re-copy the last dictation, in case
// paste failed in a way the pipeline's own detection missed. Unlike Escape
// this is registered permanently at startup, not just while recording.
static COPY_LAST_HOTKEY: OnceLock<HotKey> = OnceLock::new();
// `Mutex<bool>` (not an atomic) so the check and the register/unregister call are
// one critical section — `set_escape_listening` is invoked from both the hotkey
// event thread and Tauri command threads, and a lock-free swap could interleave
// such that Escape stays registered (hijacked) system-wide.
static ESCAPE_REGISTERED: Mutex<bool> = Mutex::new(false);

fn now_ms() -> u64 {
    static START: OnceLock<Instant> = OnceLock::new();
    START.get_or_init(Instant::now).elapsed().as_millis() as u64
}

// --- public contract -------------------------------------------------------

/// There's no Windows key on macOS — kept to satisfy the shared contract.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn is_win_key_down() -> bool {
    false
}

/// No-op on macOS — see `is_win_key_down`.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn force_release_win_key() {}

pub fn update_keys(k1: u32, k2: u32) {
    KEY1.store(k1, Ordering::SeqCst);
    KEY2.store(k2, Ordering::SeqCst);
    reset_chord_state();
    register_main_hotkey();
}

pub fn update_repair_keys(k1: u32, k2: u32, k3: u32) {
    REPAIR_KEY1.store(k1, Ordering::SeqCst);
    REPAIR_KEY2.store(k2, Ordering::SeqCst);
    REPAIR_KEY3.store(k3, Ordering::SeqCst);
    register_repair_hotkey();
}

pub fn reset_chord_state() {
    CHORD_ACTIVE.store(false, Ordering::SeqCst);
    ESCAPE_CANCELLED.store(false, Ordering::SeqCst);
    HANDLESS_PENDING.store(false, Ordering::SeqCst);
    HANDLESS_PENDING_MS.store(0, Ordering::SeqCst);
    CHORD_DOWN_MS.store(0, Ordering::SeqCst);
    refresh_escape_listening();
}

pub fn set_handless_active(v: bool) {
    HANDLESS_ACTIVE.store(v, Ordering::SeqCst);
    refresh_escape_listening();
}

/// Carbon global-hotkey events are generated by the registered shortcut only;
/// synthetic paste keystrokes are not observed by this backend.
pub fn begin_synthetic_paste_suppression(_duration_ms: u64) {}

/// Current Caps Lock toggle state, queried synchronously from the OS.
pub fn caps_lock_is_on() -> bool {
    const COMBINED_SESSION_STATE: i32 = 0; // kCGEventSourceStateCombinedSessionState
    unsafe { (CGEventSourceFlagsState(COMBINED_SESSION_STATE) & FLAG_ALPHASHIFT) != 0 }
}

/// A hotkey is registrable as long as the (modifiers, key) pair resolves to a
/// real key — Carbon hotkeys cannot be modifier-only.
pub fn is_hotkey_available(key1: &str, key2: &str) -> bool {
    build_hotkey(map_code_to_vk(key1), map_code_to_vk(key2)).is_some()
}

pub fn is_repair_hotkey_available(key1: &str, key2: &str, key3: &str) -> bool {
    build_hotkey_from(&[
        map_code_to_vk(key1),
        map_code_to_vk(key2),
        map_code_to_vk(key3),
    ])
    .is_some()
}

/// Map a JS `KeyboardEvent.code` to this backend's private key id.
pub fn map_code_to_vk(code: &str) -> u32 {
    match code {
        "" => 0,
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
/// might rebind to. The default F5 hotkey resolves through here (F5 → 96).
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
        "Escape" => 53,
        "Backspace" => 51,
        "Delete" => 117,
        "Enter" => 36,
        "Tab" => 48,
        "Home" => 115,
        "End" => 119,
        "PageUp" => 116,
        "PageDown" => 121,
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

/// Reverse of `js_code_to_mac_keycode`: macOS virtual keycode → `global-hotkey`
/// `Code`. Used by `build_hotkey` to register the trigger key.
fn mac_keycode_to_code(kc: u32) -> Option<Code> {
    let code = match kc {
        0 => Code::KeyA,
        1 => Code::KeyS,
        2 => Code::KeyD,
        3 => Code::KeyF,
        4 => Code::KeyH,
        5 => Code::KeyG,
        6 => Code::KeyZ,
        7 => Code::KeyX,
        8 => Code::KeyC,
        9 => Code::KeyV,
        11 => Code::KeyB,
        12 => Code::KeyQ,
        13 => Code::KeyW,
        14 => Code::KeyE,
        15 => Code::KeyR,
        16 => Code::KeyY,
        17 => Code::KeyT,
        31 => Code::KeyO,
        32 => Code::KeyU,
        34 => Code::KeyI,
        35 => Code::KeyP,
        37 => Code::KeyL,
        38 => Code::KeyJ,
        40 => Code::KeyK,
        45 => Code::KeyN,
        46 => Code::KeyM,
        18 => Code::Digit1,
        19 => Code::Digit2,
        20 => Code::Digit3,
        21 => Code::Digit4,
        23 => Code::Digit5,
        22 => Code::Digit6,
        26 => Code::Digit7,
        28 => Code::Digit8,
        25 => Code::Digit9,
        29 => Code::Digit0,
        49 => Code::Space,
        53 => Code::Escape,
        51 => Code::Backspace,
        117 => Code::Delete,
        36 => Code::Enter,
        48 => Code::Tab,
        115 => Code::Home,
        119 => Code::End,
        116 => Code::PageUp,
        121 => Code::PageDown,
        50 => Code::Backquote,
        27 => Code::Minus,
        24 => Code::Equal,
        33 => Code::BracketLeft,
        30 => Code::BracketRight,
        42 => Code::Backslash,
        41 => Code::Semicolon,
        39 => Code::Quote,
        43 => Code::Comma,
        47 => Code::Period,
        44 => Code::Slash,
        123 => Code::ArrowLeft,
        124 => Code::ArrowRight,
        125 => Code::ArrowDown,
        126 => Code::ArrowUp,
        122 => Code::F1,
        120 => Code::F2,
        99 => Code::F3,
        118 => Code::F4,
        96 => Code::F5,
        97 => Code::F6,
        98 => Code::F7,
        100 => Code::F8,
        101 => Code::F9,
        109 => Code::F10,
        103 => Code::F11,
        111 => Code::F12,
        _ => return None,
    };
    Some(code)
}

fn modifier_id_to_mods(id: u32) -> Option<Modifiers> {
    match id {
        ID_CONTROL => Some(Modifiers::CONTROL),
        ID_SHIFT => Some(Modifiers::SHIFT),
        ID_ALT => Some(Modifiers::ALT),
        ID_COMMAND => Some(Modifiers::META),
        // Fn / Caps Lock can't be Carbon hotkey modifiers — ignore them.
        ID_FN | ID_CAPS => Some(Modifiers::empty()),
        _ => None,
    }
}

/// Resolve the two private key ids into a single registrable `HotKey`, or `None`
/// if there is no real trigger key (e.g. a modifier-only combination).
fn build_hotkey(k1: u32, k2: u32) -> Option<HotKey> {
    build_hotkey_from(&[k1, k2])
}

/// Same as `build_hotkey` but over an arbitrary number of ids — used by the
/// repair-open hotkey, which is 2 modifiers + 1 trigger key. Already rejects
/// a modifier-only combination (returns `None` since `code` stays unset), so
/// unlike the Windows backend's low-level key hook, Carbon's
/// `RegisterEventHotKey` was never at risk of intercepting a bare modifier
/// press on its own — it only ever fires on the complete registered combo.
fn build_hotkey_from(ids: &[u32]) -> Option<HotKey> {
    let mut mods = Modifiers::empty();
    let mut code: Option<Code> = None;
    for id in ids.iter().copied() {
        if id == 0 {
            continue;
        }
        if let Some(m) = modifier_id_to_mods(id) {
            mods |= m;
        } else if id >= REGULAR_BASE {
            if let Some(c) = mac_keycode_to_code(id - REGULAR_BASE) {
                code = Some(c);
            }
        }
    }
    let trigger = code?;
    let mods = if mods.is_empty() { None } else { Some(mods) };
    // A modifier-less single key would be consumed system-wide by
    // RegisterEventHotKey, hijacking normal typing of that key everywhere. Only
    // function keys (which don't produce text) are safe as a bare hotkey.
    if mods.is_none() && !is_function_key(trigger) {
        return None;
    }
    Some(HotKey::new(mods, trigger))
}

fn is_function_key(code: Code) -> bool {
    matches!(
        code,
        Code::F1
            | Code::F2
            | Code::F3
            | Code::F4
            | Code::F5
            | Code::F6
            | Code::F7
            | Code::F8
            | Code::F9
            | Code::F10
            | Code::F11
            | Code::F12
    )
}

// --- registration ----------------------------------------------------------

fn register_main_hotkey() {
    let Some(mgr) = MANAGER.get() else {
        return;
    };
    let hk = match build_hotkey(KEY1.load(Ordering::SeqCst), KEY2.load(Ordering::SeqCst)) {
        Some(hk) => hk,
        None => {
            // Fall back to ⌥+Space so the app always has a working hotkey rather
            // than silently registering nothing (e.g. a stale modifier-only setting).
            log::warn!("hotkey: configured keys are not registrable — falling back to ⌥+Space");
            HotKey::new(Some(Modifiers::ALT), Code::Space)
        }
    };
    if let Ok(mut cur) = CURRENT_HOTKEY.lock() {
        if let Some(prev) = cur.take() {
            let _ = mgr.unregister(prev);
        }
        match mgr.register(hk) {
            Ok(()) => {
                *cur = Some(hk);
                MAIN_HOTKEY_ID.store(hk.id(), Ordering::SeqCst);
                log::info!("hotkey: registered global hotkey id={}", hk.id());
            }
            Err(e) => {
                MAIN_HOTKEY_ID.store(0, Ordering::SeqCst);
                log::error!("hotkey: failed to register global hotkey: {e}");
            }
        }
    }
}

fn register_repair_hotkey() {
    let Some(mgr) = MANAGER.get() else {
        return;
    };
    let hk = build_hotkey_from(&[
        REPAIR_KEY1.load(Ordering::SeqCst),
        REPAIR_KEY2.load(Ordering::SeqCst),
        REPAIR_KEY3.load(Ordering::SeqCst),
    ]);
    let Ok(mut cur) = CURRENT_REPAIR_HOTKEY.lock() else {
        return;
    };
    if let Some(prev) = cur.take() {
        let _ = mgr.unregister(prev);
        REPAIR_HOTKEY_ID.store(0, Ordering::SeqCst);
    }
    let Some(hk) = hk else {
        // Unconfigured or unregistrable — leave nothing registered rather
        // than falling back to a combo the user never chose.
        return;
    };
    match mgr.register(hk) {
        Ok(()) => {
            *cur = Some(hk);
            REPAIR_HOTKEY_ID.store(hk.id(), Ordering::SeqCst);
        }
        Err(e) => log::warn!("hotkey: failed to register repair-open hotkey: {e}"),
    }
}

fn register_copy_last_hotkey() {
    let Some(mgr) = MANAGER.get() else {
        return;
    };
    let hk = *COPY_LAST_HOTKEY
        .get_or_init(|| HotKey::new(Some(Modifiers::ALT | Modifiers::META), Code::KeyC));
    if let Err(e) = mgr.register(hk) {
        log::warn!("hotkey: failed to register copy-last-dictation hotkey: {e}");
    }
}

/// Register/unregister a plain-Escape hotkey for the duration of an active
/// recording so the user can cancel mid-dictation. We keep it transient because
/// a registered hotkey is consumed system-wide — we don't want to swallow Escape
/// everywhere, only while the user is holding the dictation key.
fn set_escape_listening(on: bool) {
    let Some(mgr) = MANAGER.get() else {
        return;
    };
    let esc = *ESCAPE_HOTKEY.get_or_init(|| HotKey::new(None, Code::Escape));
    let Ok(mut registered) = ESCAPE_REGISTERED.lock() else {
        return;
    };
    if on && !*registered {
        if mgr.register(esc).is_ok() {
            *registered = true;
        }
    } else if !on && *registered {
        let _ = mgr.unregister(esc);
        *registered = false;
    }
}

fn refresh_escape_listening() {
    // Keep Escape transient to an active recording or in-flight processing
    // only. Leaving it registered while the app is idle would hijack Escape
    // system-wide.
    let processing = PROCESSING_GENERATION.load(Ordering::SeqCst) != 0;
    set_escape_listening(
        CHORD_ACTIVE.load(Ordering::SeqCst) || HANDLESS_ACTIVE.load(Ordering::SeqCst) || processing,
    );
}

// --- event handling --------------------------------------------------------

fn handle_hotkey_event(ev: GlobalHotKeyEvent) {
    if ESCAPE_HOTKEY.get().is_some_and(|h| h.id() == ev.id) {
        if matches!(ev.state, HotKeyState::Pressed) {
            on_escape_pressed();
        }
        return;
    }
    if COPY_LAST_HOTKEY.get().is_some_and(|h| h.id() == ev.id) {
        if matches!(ev.state, HotKeyState::Pressed) {
            if let Some(cb) = COPY_LAST_CB.get() {
                cb();
            }
        }
        return;
    }
    if REPAIR_HOTKEY_ID.load(Ordering::SeqCst) != 0 && ev.id == REPAIR_HOTKEY_ID.load(Ordering::SeqCst) {
        if matches!(ev.state, HotKeyState::Pressed) {
            if let Some(cb) = REPAIR_OPEN_CB.get() {
                cb();
            }
        }
        return;
    }
    if ev.id != MAIN_HOTKEY_ID.load(Ordering::SeqCst) {
        return;
    }
    match ev.state {
        HotKeyState::Pressed => on_main_pressed(),
        HotKeyState::Released => on_main_released(),
    }
}

fn on_main_pressed() {
    // Carbon delivers a single Pressed per physical press, but guard against any
    // duplicate before a Released arrives.
    if CHORD_ACTIVE.load(Ordering::SeqCst) {
        return;
    }
    let now = now_ms();

    // Double-tap within the window toggles handsfree mode.
    if HANDLESS_PENDING.swap(false, Ordering::SeqCst)
        && now.saturating_sub(HANDLESS_PENDING_MS.load(Ordering::SeqCst)) <= HANDSFREE_DOUBLE_TAP_MS
    {
        if let Some(cb) = HANDLESS_CB.get() {
            cb();
        }
        return;
    }

    CHORD_ACTIVE.store(true, Ordering::SeqCst);
    CHORD_DOWN_MS.store(now, Ordering::SeqCst);
    ESCAPE_CANCELLED.store(false, Ordering::SeqCst);
    refresh_escape_listening();
    if let Some(cb) = PRESS_CB.get() {
        cb();
    }
}

fn on_main_released() {
    if !CHORD_ACTIVE.swap(false, Ordering::SeqCst) {
        return;
    }
    let now = now_ms();
    let held_ms = now.saturating_sub(CHORD_DOWN_MS.load(Ordering::SeqCst));
    refresh_escape_listening();

    if ESCAPE_CANCELLED.swap(false, Ordering::SeqCst) {
        // Escape already cancelled this session — emit nothing further.
    } else if held_ms < TAP_MAX_HOLD_MS {
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

fn on_escape_pressed() {
    if CHORD_ACTIVE.load(Ordering::SeqCst) {
        ESCAPE_CANCELLED.store(true, Ordering::SeqCst);
    }
    if let Some(cb) = ESCAPE_CB.get() {
        cb();
    }
}

// --- start -----------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
pub fn start<P, R, H, C, E, L, O>(
    on_press: P,
    on_release: R,
    on_handless: H,
    on_cancel: C,
    on_escape: E,
    on_copy_last: L,
    on_repair_open: O,
) -> Result<std::thread::JoinHandle<()>, String>
where
    P: Fn() + Send + Sync + 'static,
    R: Fn() + Send + Sync + 'static,
    H: Fn() + Send + Sync + 'static,
    C: Fn() + Send + Sync + 'static,
    E: Fn() + Send + Sync + 'static,
    L: Fn() + Send + Sync + 'static,
    O: Fn() + Send + Sync + 'static,
{
    if MANAGER.get().is_some() {
        log::warn!("hotkey: global hotkey manager already initialized");
        return Ok(std::thread::spawn(|| {}));
    }

    let _ = PRESS_CB.set(Box::new(on_press));
    let _ = RELEASE_CB.set(Box::new(on_release));
    let _ = HANDLESS_CB.set(Box::new(on_handless));
    let _ = CANCEL_CB.set(Box::new(on_cancel));
    let _ = ESCAPE_CB.set(Box::new(on_escape));
    let _ = COPY_LAST_CB.set(Box::new(on_copy_last));
    let _ = REPAIR_OPEN_CB.set(Box::new(on_repair_open));

    // Created here (on the main thread, from Tauri `setup`) because the crate
    // installs its Carbon event handler on the application event target, which
    // is serviced by the main run loop Tauri already runs.
    let manager = GlobalHotKeyManager::new()
        .map_err(|e| format!("failed to create global hotkey manager: {e}"))?;
    if MANAGER.set(manager).is_err() {
        log::warn!("hotkey: global hotkey manager already initialized");
        return Ok(std::thread::spawn(|| {}));
    }
    register_main_hotkey();
    register_copy_last_hotkey();
    register_repair_hotkey();

    // Drain hotkey events on a background thread; the receiver is a process-wide
    // channel fed by the Carbon handler, so it is safe to poll off-thread.
    let handle = std::thread::spawn(move || {
        let rx = GlobalHotKeyEvent::receiver();
        while let Ok(ev) = rx.recv() {
            handle_hotkey_event(ev);
        }
    });
    log::info!("macOS global hotkey installed (RegisterEventHotKey)");

    Ok(handle)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hk(code1: &str, code2: &str) -> Option<HotKey> {
        build_hotkey(map_code_to_vk(code1), map_code_to_vk(code2))
    }

    #[test]
    fn modifier_less_single_key_is_rejected_unless_function_key() {
        // A bare printable key would be consumed system-wide — reject it.
        assert!(hk("KeyA", "").is_none());
        assert!(hk("Space", "").is_none());
        // Function keys don't produce text, so they're allowed bare.
        assert!(hk("F5", "").is_some());
        assert!(hk("F12", "").is_some());
    }

    #[test]
    fn modifier_plus_key_and_modifier_only_combinations() {
        // The default ⌥+Space and other modifier+key combos are valid.
        assert!(hk("AltLeft", "Space").is_some());
        assert!(hk("ControlLeft", "KeyA").is_some());
        assert!(hk("AltLeft", "Escape").is_some());
        assert!(hk("AltLeft", "Backspace").is_some());
        assert!(hk("AltLeft", "PageDown").is_some());
        // A modifier-only chord (e.g. the legacy Fn+Control) is not registrable.
        assert!(hk("ControlLeft", "Fn").is_none());
        assert!(hk("", "").is_none());
    }
}
