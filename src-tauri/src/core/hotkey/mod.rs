//! Global hold/release hotkey.
//!
//! Both platforms expose the same public contract consumed by `main.rs` and
//! `commands`:
//!   `start(on_press, on_release, on_handless, on_cancel, on_escape)`,
//!   `update_keys`, `map_code_to_vk`, `is_hotkey_available`,
//!   `reset_chord_state`, `set_handless_active`,
//!   `begin_synthetic_paste_suppression`.
//!
//! Windows uses a `WH_KEYBOARD_LL` hook; macOS uses a `CGEventTap`. The numeric
//! key ids produced by `map_code_to_vk` are platform-private — only the matching
//! backend interprets them — so the two implementations never need to agree on a
//! shared numbering.

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
    pub fn reset_chord_state() {}
    pub fn set_handless_active(_v: bool) {}
    pub fn begin_synthetic_paste_suppression(_duration_ms: u64) {}
    pub fn map_code_to_vk(_code: &str) -> u32 {
        0
    }
    pub fn start<P, R, H, C, E>(
        _on_press: P,
        _on_release: R,
        _on_handless: H,
        _on_cancel: C,
        _on_escape: E,
    ) -> Result<std::thread::JoinHandle<()>, String>
    where
        P: Fn() + Send + Sync + 'static,
        R: Fn() + Send + Sync + 'static,
        H: Fn() + Send + Sync + 'static,
        C: Fn() + Send + Sync + 'static,
        E: Fn() + Send + Sync + 'static,
    {
        Ok(std::thread::spawn(|| {}))
    }
}
#[cfg(not(any(windows, target_os = "macos")))]
pub use noop::*;
