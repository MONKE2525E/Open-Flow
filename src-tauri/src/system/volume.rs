use std::sync::Mutex;

#[cfg(windows)]
use windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume;
#[cfg(windows)]
use windows::Win32::Media::Audio::{eConsole, eRender, IMMDeviceEnumerator, MMDeviceEnumerator};
#[cfg(windows)]
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CLSCTX_ALL, COINIT_MULTITHREADED,
};

static IS_MUTED: Mutex<bool> = Mutex::new(false);

#[cfg(windows)]
unsafe fn get_volume_interface() -> Result<IAudioEndpointVolume, windows::core::Error> {
    let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
    let enumerator: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
    let device = enumerator.GetDefaultAudioEndpoint(eRender, eConsole)?;
    device.Activate(CLSCTX_ALL, None)
}

#[cfg(windows)]
fn set_system_muted(muted: bool) -> Result<(), String> {
    let volume = unsafe { get_volume_interface() }
        .map_err(|e| format!("Failed to obtain audio endpoint volume: {e}"))?;
    unsafe { volume.SetMute(muted, std::ptr::null()) }
        .map_err(|e| format!("Failed to set system mute: {e}"))
}

// macOS: toggle the default output device mute via AppleScript. Runs on the
// caller's (already-spawned) thread, so the brief `osascript` invocation is fine.
#[cfg(target_os = "macos")]
fn set_system_muted(muted: bool) -> Result<(), String> {
    let value = if muted { "true" } else { "false" };
    let output = std::process::Command::new("osascript")
        .args(["-e", &format!("set volume output muted {value}")])
        .output()
        .map_err(|e| format!("Failed to run osascript: {e}"))?;

    if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr)
            .trim()
            .to_string()
            .if_empty_fallback("osascript failed to set system mute"))
    }
}

#[cfg(not(any(windows, target_os = "macos")))]
fn set_system_muted(_muted: bool) -> Result<(), String> {
    Ok(())
}

pub fn mute() {
    let mut muted = match IS_MUTED.lock() {
        Ok(guard) => guard,
        Err(poisoned) => {
            log::warn!("System mute state lock was poisoned; recovering");
            poisoned.into_inner()
        }
    };

    if *muted {
        return;
    }
    if let Err(err) = set_system_muted(true) {
        log::warn!("Failed to mute system audio: {err}");
        return;
    }
    *muted = true;
}

pub fn unmute() {
    let mut muted = match IS_MUTED.lock() {
        Ok(guard) => guard,
        Err(poisoned) => {
            log::warn!("System mute state lock was poisoned; recovering");
            poisoned.into_inner()
        }
    };

    if !*muted {
        return;
    }
    if let Err(err) = set_system_muted(false) {
        log::warn!("Failed to unmute system audio: {err}");
        return;
    }
    *muted = false;
}

trait EmptyFallback {
    fn if_empty_fallback(self, fallback: &str) -> String;
}

impl EmptyFallback for String {
    fn if_empty_fallback(self, fallback: &str) -> String {
        if self.trim().is_empty() {
            fallback.to_string()
        } else {
            self
        }
    }
}
