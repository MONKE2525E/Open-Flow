use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::SystemInformation::GetTickCount64;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, RegisterHotKey, UnregisterHotKey,
    MOD_ALT, MOD_CONTROL, MOD_SHIFT, MOD_WIN, HOT_KEY_MODIFIERS,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, DispatchMessageW, GetMessageW, SetWindowsHookExW,
    TranslateMessage, HC_ACTION, KBDLLHOOKSTRUCT, MSG, WH_KEYBOARD_LL,
    WM_KEYDOWN, WM_KEYUP, WM_SYSKEYDOWN, WM_SYSKEYUP,
};

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

static KEY1: AtomicU32 = AtomicU32::new(164); // VK_LMENU
static KEY2: AtomicU32 = AtomicU32::new(32);  // VK_SPACE

pub fn update_keys(k1: u32, k2: u32) {
    if k1 != 0 { KEY1.store(k1, Ordering::SeqCst); }
    if k2 != 0 { KEY2.store(k2, Ordering::SeqCst); }
    // Reset all chord state so stale key events against the old binding can't
    // trigger phantom presses or missed releases after a hotkey change.
    CHORD_DOWN.store(false, Ordering::SeqCst);
    KEY1_WAS_CHORD.store(false, Ordering::SeqCst);
    CHORD_PENDING.store(false, Ordering::SeqCst);
    CHORD_PENDING_END_MS.store(0, Ordering::SeqCst);
    CHORD_FIRST_DOWN_MS.store(0, Ordering::SeqCst);
    HANDLESS_WAITING_KEY1_2.store(false, Ordering::SeqCst);
    HANDLESS_KEY1_TIME.store(0, Ordering::SeqCst);
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
        c if c.starts_with("Key") && c.len() == 4 => {
            c.as_bytes()[3] as u32
        }
        c if c.starts_with("Digit") && c.len() == 6 => {
            c.as_bytes()[5] as u32
        }
        c if c.starts_with("F") && c.len() > 1 => {
            if let Ok(n) = c[1..].parse::<u32>() {
                if n >= 1 && n <= 12 { 111 + n } else { 0 }
            } else { 0 }
        }
        c if c.starts_with("Numpad") && c.len() == 7 => {
            let b = c.as_bytes()[6];
            if b >= b'0' && b <= b'9' { 96 + (b - b'0') as u32 } else { 0 }
        }
        "NumpadMultiply" => 106,
        "NumpadAdd" => 107,
        "NumpadSubtract" => 109,
        "NumpadDecimal" => 110,
        "NumpadDivide" => 111,
        _ => 0,
    }
}

static PRESS_CB:    std::sync::OnceLock<Box<dyn Fn() + Send + Sync>> = std::sync::OnceLock::new();
static RELEASE_CB:  std::sync::OnceLock<Box<dyn Fn() + Send + Sync>> = std::sync::OnceLock::new();
static HANDLESS_CB: std::sync::OnceLock<Box<dyn Fn() + Send + Sync>> = std::sync::OnceLock::new();
static CANCEL_CB:   std::sync::OnceLock<Box<dyn Fn() + Send + Sync>> = std::sync::OnceLock::new();

static CHORD_DOWN: AtomicBool = AtomicBool::new(false);
static KEY1_WAS_CHORD: AtomicBool = AtomicBool::new(false);
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

        let is_key2 = vk == k2;
        let is_key1 = vk == k1;
        let is_down = msg == WM_KEYDOWN || msg == WM_SYSKEYDOWN;
        let is_up   = msg == WM_KEYUP   || msg == WM_SYSKEYUP;

        if is_key2 && is_down {
            let k1_held = (GetAsyncKeyState(k1 as i32) & 0x8000u16 as i16) != 0;
            if k1_held {
                if CHORD_PENDING.load(Ordering::SeqCst) {
                    let t1  = CHORD_PENDING_END_MS.load(Ordering::SeqCst);
                    let now = GetTickCount64();
                    CHORD_PENDING.store(false, Ordering::SeqCst);
                    CHORD_PENDING_END_MS.store(0, Ordering::SeqCst);
                    if now.saturating_sub(t1) <= 300 {
                        if let Some(cb) = HANDLESS_CB.get() { cb(); }
                    } else {
                        CHORD_DOWN.store(true, Ordering::SeqCst);
                        KEY1_WAS_CHORD.store(true, Ordering::SeqCst);
                        CHORD_FIRST_DOWN_MS.store(GetTickCount64(), Ordering::SeqCst);
                        if let Some(cb) = PRESS_CB.get() { cb(); }
                    }
                } else if !CHORD_DOWN.swap(true, Ordering::SeqCst) {
                    KEY1_WAS_CHORD.store(true, Ordering::SeqCst);
                    CHORD_FIRST_DOWN_MS.store(GetTickCount64(), Ordering::SeqCst);
                    if let Some(cb) = PRESS_CB.get() { cb(); }
                }
                return LRESULT(1);
            }
        }

        if is_key2 && is_up {
            if CHORD_DOWN.swap(false, Ordering::SeqCst) {
                let now = GetTickCount64();
                let t0  = CHORD_FIRST_DOWN_MS.load(Ordering::SeqCst);
                let held_ms = now.saturating_sub(t0);
                let k1_still_held = (GetAsyncKeyState(k1 as i32) & 0x8000u16 as i16) != 0;
                
                if held_ms < 200 && k1_still_held {
                    CHORD_PENDING.store(true, Ordering::SeqCst);
                    CHORD_PENDING_END_MS.store(now, Ordering::SeqCst);
                    if let Some(cb) = CANCEL_CB.get() { cb(); }
                } else {
                    if let Some(cb) = RELEASE_CB.get() { cb(); }
                }
                return LRESULT(1);
            }
        }

        if is_key1 && is_down {
            let k2_held = (GetAsyncKeyState(k2 as i32) & 0x8000u16 as i16) != 0;
            if k2_held && !CHORD_DOWN.load(Ordering::SeqCst) {
                let now = GetTickCount64();
                if HANDLESS_WAITING_KEY1_2.load(Ordering::SeqCst) {
                    let t1 = HANDLESS_KEY1_TIME.load(Ordering::SeqCst);
                    HANDLESS_WAITING_KEY1_2.store(false, Ordering::SeqCst);
                    HANDLESS_KEY1_TIME.store(0, Ordering::SeqCst);
                    if now.saturating_sub(t1) <= 300 {
                        if let Some(cb) = HANDLESS_CB.get() { cb(); }
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

            let k2_held = (GetAsyncKeyState(k2 as i32) & 0x8000u16 as i16) != 0;
            if !k2_held {
                HANDLESS_WAITING_KEY1_2.store(false, Ordering::SeqCst);
                HANDLESS_KEY1_TIME.store(0, Ordering::SeqCst);
            }

            if CHORD_DOWN.swap(false, Ordering::SeqCst) {
                if let Some(cb) = RELEASE_CB.get() { cb(); }
            }
            if KEY1_WAS_CHORD.swap(false, Ordering::SeqCst) {
                let is_menu_trigger = k1 == 164 || k1 == 165 || k1 == 18 || k1 == 91 || k1 == 92;
                if is_menu_trigger {
                    return LRESULT(1);
                }
            }
        }
    }

    CallNextHookEx(None, code, wparam, lparam)
}

pub fn start<P, R, H, C>(on_press: P, on_release: R, on_handless: H, on_cancel: C)
    -> Result<std::thread::JoinHandle<()>, String>
where
    P: Fn() + Send + Sync + 'static,
    R: Fn() + Send + Sync + 'static,
    H: Fn() + Send + Sync + 'static,
    C: Fn() + Send + Sync + 'static,
{
    PRESS_CB.set(Box::new(on_press)).ok();
    RELEASE_CB.set(Box::new(on_release)).ok();
    HANDLESS_CB.set(Box::new(on_handless)).ok();
    CANCEL_CB.set(Box::new(on_cancel)).ok();

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
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }

        windows::Win32::UI::WindowsAndMessaging::UnhookWindowsHookEx(hook).ok();
    });

    Ok(handle)
}