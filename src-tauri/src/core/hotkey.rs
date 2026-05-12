/// Low-level Windows keyboard hook for Alt+Space hold-to-talk and
/// Space+Alt-double-click handless-mode toggle.
///
/// `RegisterHotKey` (used by tauri-plugin-global-shortcut) only fires on
/// press, never release — useless for hold-to-talk. We use WH_KEYBOARD_LL
/// instead, which gives us both WM_KEYDOWN and WM_KEYUP.
///
/// Alt key suppression: when Alt+Space is detected we also suppress the
/// matching Alt-up. Without this, apps receive a bare Alt-down / Alt-up
/// pair which Windows interprets as "activate menu bar", blurring text fields.
///
/// Handless gesture: Space held → Alt pressed twice within 300ms fires the
/// handless callback. Both Alt presses are suppressed to prevent menu activation.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows::Win32::System::SystemInformation::GetTickCount64;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, VK_LMENU, VK_RMENU, VK_SPACE,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, DispatchMessageW, GetMessageW, SetWindowsHookExW,
    TranslateMessage, HC_ACTION, KBDLLHOOKSTRUCT, MSG, WH_KEYBOARD_LL,
    WM_KEYDOWN, WM_KEYUP, WM_SYSKEYDOWN, WM_SYSKEYUP,
};

static PRESS_CB:   std::sync::OnceLock<Box<dyn Fn() + Send + Sync>> = std::sync::OnceLock::new();
static RELEASE_CB: std::sync::OnceLock<Box<dyn Fn() + Send + Sync>> = std::sync::OnceLock::new();
static HANDLESS_CB: std::sync::OnceLock<Box<dyn Fn() + Send + Sync>> = std::sync::OnceLock::new();

/// True while Space is held down as part of an Alt+Space chord we captured.
static ALT_SPACE_DOWN: AtomicBool = AtomicBool::new(false);

/// Set when an Alt+Space chord is first detected; cleared when Alt is released.
/// Used to suppress the Alt-up that would otherwise trigger menu-mode in apps.
static ALT_WAS_CHORD: AtomicBool = AtomicBool::new(false);

/// Tick count (GetTickCount64) of the first Alt-down while Space was held.
/// 0 means no pending first press.
static HANDLESS_ALT1_TIME: AtomicU64 = AtomicU64::new(0);

/// True while waiting for the second Alt-down to complete the handless double-click.
static HANDLESS_WAITING_ALT2: AtomicBool = AtomicBool::new(false);

unsafe extern "system" fn hook_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        let kb = &*(lparam.0 as *const KBDLLHOOKSTRUCT);
        let msg = wparam.0 as u32;
        let vk = kb.vkCode;

        let is_space = vk == VK_SPACE.0 as u32;
        let is_alt   = vk == VK_LMENU.0 as u32 || vk == VK_RMENU.0 as u32;
        let is_down  = msg == WM_KEYDOWN || msg == WM_SYSKEYDOWN;
        let is_up    = msg == WM_KEYUP   || msg == WM_SYSKEYUP;

        if is_space && is_down {
            let alt_held = (GetAsyncKeyState(VK_LMENU.0 as i32) & 0x8000u16 as i16) != 0
                        || (GetAsyncKeyState(VK_RMENU.0 as i32) & 0x8000u16 as i16) != 0;
            if alt_held {
                if !ALT_SPACE_DOWN.swap(true, Ordering::SeqCst) {
                    ALT_WAS_CHORD.store(true, Ordering::SeqCst);
                    if let Some(cb) = PRESS_CB.get() { cb(); }
                }
                return LRESULT(1);
            }
        }

        if is_space && is_up {
            if ALT_SPACE_DOWN.swap(false, Ordering::SeqCst) {
                if let Some(cb) = RELEASE_CB.get() { cb(); }
                return LRESULT(1);
            }
        }

        // Handless gesture: Alt pressed while Space is held (but not as part of
        // an existing Alt+Space chord). Two Alt presses within 300ms toggles
        // handless mode. Both presses are suppressed to prevent menu activation.
        if is_alt && is_down {
            let space_held = (GetAsyncKeyState(VK_SPACE.0 as i32) & 0x8000u16 as i16) != 0;
            if space_held && !ALT_SPACE_DOWN.load(Ordering::SeqCst) {
                let now = GetTickCount64();
                if HANDLESS_WAITING_ALT2.load(Ordering::SeqCst) {
                    let t1 = HANDLESS_ALT1_TIME.load(Ordering::SeqCst);
                    HANDLESS_WAITING_ALT2.store(false, Ordering::SeqCst);
                    HANDLESS_ALT1_TIME.store(0, Ordering::SeqCst);
                    if now.saturating_sub(t1) <= 300 {
                        if let Some(cb) = HANDLESS_CB.get() { cb(); }
                    }
                } else {
                    HANDLESS_ALT1_TIME.store(now, Ordering::SeqCst);
                    HANDLESS_WAITING_ALT2.store(true, Ordering::SeqCst);
                }
                return LRESULT(1);
            }
        }

        if is_alt && is_up {
            // Reset the handless double-click window when Space is no longer held.
            let space_held = (GetAsyncKeyState(VK_SPACE.0 as i32) & 0x8000u16 as i16) != 0;
            if !space_held {
                HANDLESS_WAITING_ALT2.store(false, Ordering::SeqCst);
                HANDLESS_ALT1_TIME.store(0, Ordering::SeqCst);
            }

            // Alt released while Space was still held — end the chord.
            if ALT_SPACE_DOWN.swap(false, Ordering::SeqCst) {
                if let Some(cb) = RELEASE_CB.get() { cb(); }
            }
            // Suppress the Alt-up if it was part of our chord.
            // Without this, apps see a bare Alt-down/Alt-up pair and activate
            // their menu bar, blurring whatever text field had focus.
            if ALT_WAS_CHORD.swap(false, Ordering::SeqCst) {
                return LRESULT(1);
            }
        }
    }

    CallNextHookEx(None, code, wparam, lparam)
}

/// Spawn the hook on its own thread with a Win32 message loop.
pub fn start<P, R, H>(on_press: P, on_release: R, on_handless: H) -> std::thread::JoinHandle<()>
where
    P: Fn() + Send + Sync + 'static,
    R: Fn() + Send + Sync + 'static,
    H: Fn() + Send + Sync + 'static,
{
    PRESS_CB.set(Box::new(on_press)).ok();
    RELEASE_CB.set(Box::new(on_release)).ok();
    HANDLESS_CB.set(Box::new(on_handless)).ok();

    std::thread::spawn(|| unsafe {
        let hook = SetWindowsHookExW(WH_KEYBOARD_LL, Some(hook_proc), None, 0)
            .expect("SetWindowsHookExW failed");

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }

        windows::Win32::UI::WindowsAndMessaging::UnhookWindowsHookEx(hook).ok();
    })
}
