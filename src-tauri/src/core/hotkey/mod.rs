//! Global hold/release hotkey.
//!
//! Both platforms expose the same public contract consumed by `main.rs` and
//! `commands`:
//!   `start(on_press, on_release, on_handless, on_cancel, on_escape, on_copy_last)`,
//!   `update_keys`, `map_code_to_vk`, `is_hotkey_available`,
//!   `reset_chord_state`, `set_handless_active`,
//!   `is_win_key_down`,
//!   `force_release_win_key`.
//!
//! Windows uses a `WH_KEYBOARD_LL` hook; macOS uses Carbon `RegisterEventHotKey`
//! via the `global-hotkey` crate. The numeric
//! key ids produced by `map_code_to_vk` are platform-private — only the matching
//! backend interprets them — so the two implementations never need to agree on a
//! shared numbering.

/// Whether `code` (a JS `KeyboardEvent.code`) is a key the backend can map to a
/// real key on either supported platform. Platform-independent: mirrors the
/// union of codes accepted by the Windows and macOS `map_code_to_vk`
/// implementations, so a hotkey that validates here will register on at least
/// one of them.
///
/// Used by the generic `save_setting`/`import_data` path, which must not let a
/// hand-edited or imported settings.json silently disable the dictation hotkey
/// by storing a code no backend recognizes (e.g. `["Foo", "Bar"]` — the startup
/// hook would then map both to VK 0 and never fire).
pub fn is_known_key_code(code: &str) -> bool {
    match code {
        "" => false,
        "ShiftLeft" | "ShiftRight" | "ControlLeft" | "ControlRight" => true,
        "AltLeft" | "AltRight" | "MetaLeft" | "MetaRight" | "Fn" => true,
        "Space" | "Escape" | "Enter" | "Backspace" | "Tab" | "CapsLock" => true,
        "Minus" | "Equal" | "BracketLeft" | "BracketRight" | "Backslash" => true,
        "Semicolon" | "Quote" | "Comma" | "Period" | "Slash" | "Backquote" => true,
        "ArrowUp" | "ArrowDown" | "ArrowLeft" | "ArrowRight" => true,
        "Insert" | "Delete" | "Home" | "End" | "PageUp" | "PageDown" => true,
        _ if code.starts_with("Key")
            && code.len() == 4
            && code.as_bytes()[3].is_ascii_uppercase() =>
        {
            true
        }
        _ if code.starts_with("Digit")
            && code.len() == 6
            && code.as_bytes()[5].is_ascii_digit() =>
        {
            true
        }
        _ if code.starts_with('F') && code.len() > 1 => code[1..]
            .parse::<u32>()
            .is_ok_and(|n| (1..=12).contains(&n)),
        _ => false,
    }
}

#[cfg(test)]
mod known_key_code_tests {
    use super::is_known_key_code;

    #[test]
    fn recognizes_supported_hotkey_codes() {
        for code in [
            "ControlLeft",
            "MetaLeft",
            "AltLeft",
            "ShiftLeft",
            "Space",
            "Fn",
            "F5",
            "KeyA",
            "Digit7",
        ] {
            assert!(is_known_key_code(code), "{code} should be known");
        }
    }

    #[test]
    fn rejects_unknown_and_malformed_codes() {
        for code in ["Foo", "Bar", "Control", "Key", "F13", "Digit10", "", "meta"] {
            assert!(!is_known_key_code(code), "{code:?} should be rejected");
        }
    }
}

#[cfg(windows)]
mod win;
#[cfg(windows)]
pub use win::*;

#[cfg(target_os = "macos")]
mod mac;
#[cfg(target_os = "macos")]
pub use mac::*;

// Fallback for any other platform (e.g. Linux CI): inert no-op shims so the
// crate still builds. Mirrors the public contract above.
#[cfg(not(any(windows, target_os = "macos")))]
mod noop {
    pub fn is_hotkey_available(_key1: &str, _key2: &str) -> bool {
        true
    }
    pub fn update_keys(_k1: u32, _k2: u32) {}
    pub fn update_repair_keys(_k1: u32, _k2: u32, _k3: u32) {}
    pub fn reset_chord_state() {}
    pub fn set_handless_active(_v: bool) {}
    pub fn begin_synthetic_paste_suppression(_duration_ms: u64) {}
    pub fn set_processing_generation(_generation: u64) {}
    pub fn clear_processing_generation(_expected_generation: u64) {}
    pub fn is_win_key_down() -> bool {
        false
    }
    pub fn force_release_win_key() {}
    pub fn caps_lock_is_on() -> bool {
        false
    }
    pub fn map_code_to_vk(_code: &str) -> u32 {
        0
    }
    #[allow(clippy::too_many_arguments)]
    pub fn start<P, R, H, C, E, L, O>(
        _on_press: P,
        _on_release: R,
        _on_handless: H,
        _on_cancel: C,
        _on_escape: E,
        _on_copy_last: L,
        _on_repair_open: O,
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
        Ok(std::thread::spawn(|| {}))
    }
}
#[cfg(not(any(windows, target_os = "macos")))]
pub use noop::*;
