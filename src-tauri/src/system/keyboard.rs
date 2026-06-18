//! Caps Lock toggle-state detection, used by the optional "Automatic caps
//! lock detection" output setting (see `pipeline.rs`).

#[cfg(windows)]
pub fn caps_lock_is_on() -> bool {
    use windows::Win32::UI::Input::KeyboardAndMouse::GetKeyState;
    const VK_CAPITAL: i32 = 0x14;
    // Caps Lock is a toggle key; the low-order bit of GetKeyState reflects
    // its persistent toggle state regardless of which thread/message-pump
    // calls it (only the high/pressed bit is queue-dependent).
    unsafe { (GetKeyState(VK_CAPITAL) & 0x0001) != 0 }
}

#[cfg(not(windows))]
pub fn caps_lock_is_on() -> bool {
    crate::core::hotkey::caps_lock_is_on()
}
