use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};

const VK_BACK: u32 = 0x08; // Backspace
const VK_ESCAPE: u32 = 0x1B; // Escape
const VK_CTRL: u32 = 0x11; // VK_CONTROL (generic, used with modifier_held)
const VK_ALT: u32 = 0x12; // VK_MENU (generic, used with modifier_held)

// Side-specific modifier VK codes that should never trigger a history update.
// Generic codes (0x10/0x11/0x12) are omitted: the !is_injected guard already
// filters all synthetic input where those codes appear, so only side-specific
// codes reach this path from real physical key presses.
static MODIFIER_VKS: &[u32] = &[
    0xA0, 0xA1, // VK_LSHIFT, VK_RSHIFT
    0xA2, 0xA3, // VK_LCONTROL, VK_RCONTROL
    0xA4, 0xA5, // VK_LMENU, VK_RMENU
    0x5B, 0x5C, // VK_LWIN, VK_RWIN
    0x14, 0x90, 0x91, // VK_CAPITAL, VK_NUMLOCK, VK_SCROLL
];
use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows::Win32::System::SystemInformation::GetTickCount64;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, GetKeyboardLayout, GetKeyboardState, MapVirtualKeyExW, RegisterHotKey,
    ToUnicodeEx, UnregisterHotKey, HOT_KEY_MODIFIERS, MAPVK_VK_TO_VSC, MOD_ALT, MOD_CONTROL,
    MOD_SHIFT, MOD_WIN,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, DispatchMessageW, GetForegroundWindow, GetMessageW,
    GetWindowThreadProcessId, SetWindowsHookExW, TranslateMessage, HC_ACTION, KBDLLHOOKSTRUCT,
    LLKHF_INJECTED, MSG, WH_KEYBOARD_LL, WM_KEYDOWN, WM_KEYUP, WM_SYSKEYDOWN, WM_SYSKEYUP,
};

// Returns true if the given specific-side VK (or its mirror) is currently held.
// Uses generic VKs for Shift/Ctrl/Alt so either side satisfies the check.
// Win key has no generic VK, so both sides are checked explicitly.
unsafe fn modifier_held(vk: u32) -> bool {
    let held = |v: u32| -> bool { (GetAsyncKeyState(v as i32) & 0x8000u16 as i16) != 0 };
    match vk {
        160 | 161 => held(16),           // L/RShift → VK_SHIFT
        162 | 163 => held(17),           // L/RControl → VK_CONTROL
        164 | 165 => held(18),           // L/RMenu → VK_MENU
        91 | 92 => held(91) || held(92), // LWin / RWin (no generic VK_WIN)
        _ => held(vk),
    }
}

// Returns true if the hook vkCode matches the configured key, including the
// mirror side for modifiers (so LCtrl binding also matches RCtrl events).
fn vk_matches(vk: u32, key: u32) -> bool {
    match key {
        160 | 161 => vk == 160 || vk == 161,
        162 | 163 => vk == 162 || vk == 163,
        164 | 165 => vk == 164 || vk == 165,
        91 | 92 => vk == 91 || vk == 92,
        _ => vk == key,
    }
}

fn is_cursor_movement_key(vk: u32) -> bool {
    matches!(
        vk,
        0x21..=0x28 | // PgUp, PgDn, End, Home, Left, Up, Right, Down
        0x2D |        // Insert
        0x2E // Delete (forward)
    )
}

/// Maps a VK code to the character it produces (US QWERTY layout).
/// Letters are always returned lowercase — case doesn't affect sentence-ender
/// or whitespace checks downstream. Returns None for keys with no stable
/// printable character (numpad, function keys, etc.); those reset history.
fn vk_to_char(vk: u32) -> Option<char> {
    if vk == 0x0D {
        return Some('\n');
    }
    if vk == 0x20 {
        return Some(' ');
    }

    unsafe {
        let mut state = [0u8; 256];
        let _ = GetKeyboardState(&mut state);
        if GetAsyncKeyState(0x10) < 0 {
            state[0x10] = 0x80;
        }
        if GetAsyncKeyState(0x11) < 0 {
            state[0x11] = 0x80;
        }
        if GetAsyncKeyState(0x12) < 0 {
            state[0x12] = 0x80;
        }

        let foreground = GetForegroundWindow();
        let thread_id = GetWindowThreadProcessId(foreground, None);
        let layout = GetKeyboardLayout(thread_id);
        let scan = MapVirtualKeyExW(vk, MAPVK_VK_TO_VSC, Some(layout));
        if scan == 0 {
            return None;
        }

        let mut buff = [0u16; 8];
        let rc = ToUnicodeEx(vk, scan, &state, &mut buff, 0, Some(layout));
        if rc < 0 {
            // Flush dead-key compose state with a neutral key so subsequent
            // translations are not polluted by stale composition state.
            let neutral_vk = 0x20u32; // VK_SPACE
            let neutral_scan = MapVirtualKeyExW(neutral_vk, MAPVK_VK_TO_VSC, Some(layout));
            for _ in 0..4 {
                if ToUnicodeEx(neutral_vk, neutral_scan, &state, &mut buff, 0, Some(layout)) >= 0 {
                    break;
                }
            }
            return None;
        }
        if rc == 0 {
            return None;
        }

        let s = String::from_utf16_lossy(&buff[..rc as usize]);
        s.chars()
            .next()
            .map(|ch| ch.to_lowercase().next().unwrap_or(ch))
    }
}

pub fn is_hotkey_available(key1: &str, key2: &str) -> bool {
    let mod_flag = match key1 {
        "ShiftLeft" | "ShiftRight" => MOD_SHIFT,
        "ControlLeft" | "ControlRight" => MOD_CONTROL,
        "AltLeft" | "AltRight" => MOD_ALT,
        "MetaLeft" | "MetaRight" => MOD_WIN,
        _ => return true,
    };

    let vk2 = map_code_to_vk(key2);
    if vk2 == 0 {
        return true;
    }

    unsafe {
        // Use a dummy ID (e.g. 0x5A8E) and no HWND.
        if RegisterHotKey(None, 0x5A8E, HOT_KEY_MODIFIERS(mod_flag.0), vk2).is_ok() {
            let _ = UnregisterHotKey(None, 0x5A8E);
            true
        } else {
            false
        }
    }
}

static KEY1: AtomicU32 = AtomicU32::new(162); // VK_LCONTROL / Ctrl
static KEY2: AtomicU32 = AtomicU32::new(91); // VK_LWIN / Windows

pub fn update_keys(k1: u32, k2: u32) {
    if k1 != 0 {
        KEY1.store(k1, Ordering::SeqCst);
    }
    if k2 != 0 {
        KEY2.store(k2, Ordering::SeqCst);
    }
    if PRESS_CB.get().is_some() {
        return;
    }
    reset_chord_state();
    HANDLESS_WAITING_KEY1_2.store(false, Ordering::SeqCst);
    HANDLESS_KEY1_TIME.store(0, Ordering::SeqCst);
}

// Clears all mid-chord state. Called from the Tokio handler after a handsfree
// stop-via-cancel so the still-open double-tap window can't accidentally start
// a fresh handsfree session on a stray second key press.
pub fn reset_chord_state() {
    CHORD_DOWN.store(false, Ordering::SeqCst);
    KEY1_WAS_CHORD.store(false, Ordering::SeqCst);
    KEY2_WAS_CHORD.store(false, Ordering::SeqCst);
    CHORD_PENDING.store(false, Ordering::SeqCst);
    CHORD_PENDING_END_MS.store(0, Ordering::SeqCst);
    CHORD_FIRST_DOWN_MS.store(0, Ordering::SeqCst);
}

pub fn set_handless_active(v: bool) {
    HANDLESS_ACTIVE.store(v, Ordering::SeqCst);
}

pub fn map_code_to_vk(code: &str) -> u32 {
    match code {
        "ShiftLeft" => 160,
        "ShiftRight" => 161,
        "ControlLeft" => 162,
        "ControlRight" => 163,
        "AltLeft" => 164,
        "AltRight" => 165,
        "MetaLeft" => 91,
        "MetaRight" => 92,
        "Space" => 32,
        "Escape" => 27,
        "Enter" => 13,
        "Backspace" => 8,
        "Tab" => 9,
        "CapsLock" => 20,
        "Minus" => 189,
        "Equal" => 187,
        "BracketLeft" => 219,
        "BracketRight" => 221,
        "Backslash" => 220,
        "Semicolon" => 186,
        "Quote" => 222,
        "Comma" => 188,
        "Period" => 190,
        "Slash" => 191,
        "Backquote" => 192,
        "ArrowUp" => 38,
        "ArrowDown" => 40,
        "ArrowLeft" => 37,
        "ArrowRight" => 39,
        "Insert" => 45,
        "Delete" => 46,
        "Home" => 36,
        "End" => 35,
        "PageUp" => 33,
        "PageDown" => 34,
        c if c.starts_with("Key") && c.len() == 4 => c.as_bytes()[3] as u32,
        c if c.starts_with("Digit") && c.len() == 6 => c.as_bytes()[5] as u32,
        c if c.starts_with("F") && c.len() > 1 => {
            if let Ok(n) = c[1..].parse::<u32>() {
                if (1..=12).contains(&n) {
                    111 + n
                } else {
                    0
                }
            } else {
                0
            }
        }
        c if c.starts_with("Numpad") && c.len() == 7 => {
            let b = c.as_bytes()[6];
            if b.is_ascii_digit() {
                96 + (b - b'0') as u32
            } else {
                0
            }
        }
        "NumpadMultiply" => 106,
        "NumpadAdd" => 107,
        "NumpadSubtract" => 109,
        "NumpadDecimal" => 110,
        "NumpadDivide" => 111,
        _ => 0,
    }
}

static PRESS_CB: std::sync::OnceLock<Box<dyn Fn() + Send + Sync>> = std::sync::OnceLock::new();
static RELEASE_CB: std::sync::OnceLock<Box<dyn Fn() + Send + Sync>> = std::sync::OnceLock::new();
static HANDLESS_CB: std::sync::OnceLock<Box<dyn Fn() + Send + Sync>> = std::sync::OnceLock::new();
static CANCEL_CB: std::sync::OnceLock<Box<dyn Fn() + Send + Sync>> = std::sync::OnceLock::new();

static CHORD_DOWN: AtomicBool = AtomicBool::new(false);
static HANDLESS_ACTIVE: AtomicBool = AtomicBool::new(false);
static ESCAPE_CANCELLED: AtomicBool = AtomicBool::new(false);
static ESCAPE_KEY_DOWN: AtomicBool = AtomicBool::new(false);
static ESCAPE_CB: std::sync::OnceLock<Box<dyn Fn() + Send + Sync>> = std::sync::OnceLock::new();
static KEY1_WAS_CHORD: AtomicBool = AtomicBool::new(false);
// Set whenever key2 is captured as part of our chord. Guarantees key2-up is
// suppressed even if key1 goes up first and clears CHORD_DOWN before key2 does.
// Without this, Win key released after Ctrl reaches the OS and triggers the
// Start menu (or other Win+key shortcuts).
static KEY2_WAS_CHORD: AtomicBool = AtomicBool::new(false);
static HANDLESS_KEY1_TIME: AtomicU64 = AtomicU64::new(0);
static HANDLESS_WAITING_KEY1_2: AtomicBool = AtomicBool::new(false);
static CHORD_FIRST_DOWN_MS: AtomicU64 = AtomicU64::new(0);
static CHORD_PENDING: AtomicBool = AtomicBool::new(false);
static CHORD_PENDING_END_MS: AtomicU64 = AtomicU64::new(0);

unsafe extern "system" fn hook_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        let kb = &*(lparam.0 as *const KBDLLHOOKSTRUCT);
        let msg = wparam.0 as u32;
        let vk = kb.vkCode;

        let k1 = KEY1.load(Ordering::Relaxed);
        let k2 = KEY2.load(Ordering::Relaxed);

        let is_key2 = vk_matches(vk, k2);
        let is_key1 = vk_matches(vk, k1);
        let is_down = msg == WM_KEYDOWN || msg == WM_SYSKEYDOWN;
        let is_up = msg == WM_KEYUP || msg == WM_SYSKEYUP;

        if is_key2 && is_down {
            let k1_held = modifier_held(k1);
            if k1_held {
                KEY2_WAS_CHORD.store(true, Ordering::SeqCst);
                if CHORD_PENDING.load(Ordering::SeqCst) {
                    let t1 = CHORD_PENDING_END_MS.load(Ordering::SeqCst);
                    let now = GetTickCount64();
                    CHORD_PENDING.store(false, Ordering::SeqCst);
                    CHORD_PENDING_END_MS.store(0, Ordering::SeqCst);
                    if now.saturating_sub(t1) <= 300 {
                        if let Some(cb) = HANDLESS_CB.get() {
                            cb();
                        }
                    } else {
                        CHORD_DOWN.store(true, Ordering::SeqCst);
                        KEY1_WAS_CHORD.store(true, Ordering::SeqCst);
                        CHORD_FIRST_DOWN_MS.store(GetTickCount64(), Ordering::SeqCst);
                        if let Some(cb) = PRESS_CB.get() {
                            cb();
                        }
                    }
                } else if !CHORD_DOWN.swap(true, Ordering::SeqCst) {
                    KEY1_WAS_CHORD.store(true, Ordering::SeqCst);
                    CHORD_FIRST_DOWN_MS.store(GetTickCount64(), Ordering::SeqCst);
                    if let Some(cb) = PRESS_CB.get() {
                        cb();
                    }
                }
                return LRESULT(1);
            }
        }

        if is_key2 && is_up {
            // KEY2_WAS_CHORD guarantees we suppress key2-up even if key1 went up
            // first and cleared CHORD_DOWN — otherwise a bare Win-up reaches the OS
            // and triggers the Start menu / Win shortcuts.
            let key2_was_chord = KEY2_WAS_CHORD.swap(false, Ordering::SeqCst);
            if CHORD_DOWN.swap(false, Ordering::SeqCst) {
                if ESCAPE_CANCELLED.swap(false, Ordering::SeqCst) {
                    return LRESULT(1);
                }
                let now = GetTickCount64();
                let t0 = CHORD_FIRST_DOWN_MS.load(Ordering::SeqCst);
                let held_ms = now.saturating_sub(t0);
                let k1_still_held = modifier_held(k1);

                if held_ms < 200 && k1_still_held {
                    CHORD_PENDING.store(true, Ordering::SeqCst);
                    CHORD_PENDING_END_MS.store(now, Ordering::SeqCst);
                    if let Some(cb) = CANCEL_CB.get() {
                        cb();
                    }
                } else if let Some(cb) = RELEASE_CB.get() {
                    cb();
                }
                return LRESULT(1);
            }
            if key2_was_chord {
                return LRESULT(1);
            }
        }

        if is_key1 && is_down {
            let k2_held = modifier_held(k2);
            if k2_held && !CHORD_DOWN.load(Ordering::SeqCst) {
                let now = GetTickCount64();
                if HANDLESS_WAITING_KEY1_2.load(Ordering::SeqCst) {
                    let t1 = HANDLESS_KEY1_TIME.load(Ordering::SeqCst);
                    HANDLESS_WAITING_KEY1_2.store(false, Ordering::SeqCst);
                    HANDLESS_KEY1_TIME.store(0, Ordering::SeqCst);
                    if now.saturating_sub(t1) <= 300 {
                        if let Some(cb) = HANDLESS_CB.get() {
                            cb();
                        }
                    }
                } else {
                    HANDLESS_KEY1_TIME.store(now, Ordering::SeqCst);
                    HANDLESS_WAITING_KEY1_2.store(true, Ordering::SeqCst);
                }
                return LRESULT(1);
            }
        }

        if is_key1 && is_up {
            CHORD_PENDING.store(false, Ordering::SeqCst);
            CHORD_PENDING_END_MS.store(0, Ordering::SeqCst);

            let k2_held = modifier_held(k2);
            if !k2_held {
                HANDLESS_WAITING_KEY1_2.store(false, Ordering::SeqCst);
                HANDLESS_KEY1_TIME.store(0, Ordering::SeqCst);
            }

            if CHORD_DOWN.swap(false, Ordering::SeqCst)
                && !ESCAPE_CANCELLED.swap(false, Ordering::SeqCst)
            {
                if let Some(cb) = RELEASE_CB.get() {
                    cb();
                }
            }
            if KEY1_WAS_CHORD.swap(false, Ordering::SeqCst) {
                let is_menu_trigger = k1 == 164 || k1 == 165 || k1 == 18 || k1 == 91 || k1 == 92;
                if is_menu_trigger {
                    return LRESULT(1);
                }
            }
        }

        if vk == VK_ESCAPE {
            if is_down
                && (CHORD_DOWN.load(Ordering::SeqCst) || HANDLESS_ACTIVE.load(Ordering::SeqCst))
            {
                if CHORD_DOWN.load(Ordering::SeqCst) {
                    ESCAPE_CANCELLED.store(true, Ordering::SeqCst);
                }
                ESCAPE_KEY_DOWN.store(true, Ordering::SeqCst);
                if let Some(cb) = ESCAPE_CB.get() {
                    cb();
                }
                return LRESULT(1);
            }
            if is_up && ESCAPE_KEY_DOWN.swap(false, Ordering::SeqCst) {
                return LRESULT(1);
            }
        }

        // Update injection history for real user keystrokes only.
        // Synthetic events (LLKHF_INJECTED) are skipped — this prevents our own
        // Ctrl+V paste and any app-generated keyboard events from corrupting the
        // context tracking that drives auto-spacing and contextual capitalisation.
        let is_injected = (kb.flags.0 & LLKHF_INJECTED.0) != 0;
        if !is_injected && is_down && !MODIFIER_VKS.contains(&vk) {
            if vk == VK_BACK {
                // Ctrl+Backspace and Alt+Backspace both delete a whole word —
                // unknown char count, so reset entirely. Plain Backspace pops
                // just the last character to keep context accurate.
                if unsafe { modifier_held(VK_CTRL) || modifier_held(VK_ALT) } {
                    crate::core::injection::reset_injection_history();
                } else {
                    crate::core::injection::backspace_injection_history();
                }
            } else if is_cursor_movement_key(vk) {
                crate::core::injection::reset_injection_history();
            } else if unsafe { modifier_held(VK_CTRL) || modifier_held(VK_ALT) } {
                // Keyboard shortcut (Ctrl+Z, Ctrl+A, etc.) — context unknown.
                crate::core::injection::reset_injection_history();
            } else {
                if let Some(ch) = vk_to_char(vk) {
                    let hwnd = unsafe { GetForegroundWindow().0 as usize };
                    crate::core::injection::append_or_reset_injection_history(hwnd, ch);
                } else {
                    crate::core::injection::reset_injection_history();
                }
            }
        }
    }

    CallNextHookEx(None, code, wparam, lparam)
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

    // Verify the hook can be installed before spawning the thread so the caller
    // gets a synchronous error instead of a silent panic on a background thread.
    unsafe {
        let probe = SetWindowsHookExW(WH_KEYBOARD_LL, Some(hook_proc), None, 0)
            .map_err(|e| format!("Failed to install keyboard hook: {e}"))?;
        windows::Win32::UI::WindowsAndMessaging::UnhookWindowsHookEx(probe).ok();
    }

    let handle = std::thread::spawn(|| unsafe {
        let hook = match SetWindowsHookExW(WH_KEYBOARD_LL, Some(hook_proc), None, 0) {
            Ok(h) => h,
            Err(e) => {
                log::error!("SetWindowsHookExW failed on hook thread: {e}");
                return;
            }
        };

        let mut msg = MSG::default();
        loop {
            let status = GetMessageW(&mut msg, None, 0, 0).0;
            if status == -1 {
                log::error!("GetMessageW failed in hotkey hook thread");
                break;
            }
            if status == 0 {
                break;
            }
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }

        windows::Win32::UI::WindowsAndMessaging::UnhookWindowsHookEx(hook).ok();
    });

    Ok(handle)
}
