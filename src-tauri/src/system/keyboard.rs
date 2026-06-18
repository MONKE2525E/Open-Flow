//! Caps Lock toggle-state detection, used by the optional "Automatic caps
//! lock detection" output setting (see `pipeline.rs`).

pub fn caps_lock_is_on() -> bool {
    crate::core::hotkey::caps_lock_is_on()
}
