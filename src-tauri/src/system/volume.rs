#[cfg(target_os = "macos")]
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;

#[cfg(windows)]
use windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume;
#[cfg(windows)]
use windows::Win32::Media::Audio::{eConsole, eRender, IMMDeviceEnumerator, MMDeviceEnumerator};
#[cfg(windows)]
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CLSCTX_ALL, COINIT_MULTITHREADED,
};

#[cfg(windows)]
static IS_MUTED: Mutex<bool> = Mutex::new(false);

#[cfg(target_os = "macos")]
static RESTORE_STATE: Mutex<MacRestoreState> = Mutex::new(MacRestoreState::Idle);
#[cfg(target_os = "macos")]
static WARNED_UNSUPPORTED_AUDIO_CONTROL: AtomicBool = AtomicBool::new(false);

#[cfg(target_os = "macos")]
#[derive(Clone, Copy, Debug)]
enum MacRestoreState {
    Idle,
    Muted {
        device_id: u32,
        was_muted: bool,
    },
    Volume {
        device_id: u32,
        previous_volume: f32,
    },
}

// ---- exclusive microphone access (macOS hog mode) ----

#[cfg(target_os = "macos")]
static HOG_STATE: Mutex<HogState> = Mutex::new(HogState::Idle);
#[cfg(target_os = "macos")]
static WARNED_HOG_UNAVAILABLE: AtomicBool = AtomicBool::new(false);
#[cfg(target_os = "macos")]
static NEXT_SESSION_ID: AtomicU64 = AtomicU64::new(1);
#[cfg(target_os = "macos")]
static ACTIVE_SESSION_ID: AtomicU64 = AtomicU64::new(0);

#[cfg(target_os = "macos")]
#[derive(Clone, Copy, Debug)]
enum HogState {
    Idle,
    Hogged { device_id: u32, session_id: u64 },
}

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

#[cfg(target_os = "macos")]
mod macos {
    use coreaudio::sys::{
        kAudioDevicePropertyHogMode, kAudioDevicePropertyMute, kAudioDevicePropertyVolumeScalar,
        kAudioHardwareNoError, kAudioHardwarePropertyDefaultInputDevice,
        kAudioHardwarePropertyDefaultOutputDevice, kAudioObjectPropertyElementMaster,
        kAudioObjectPropertyScopeGlobal, kAudioObjectPropertyScopeOutput, kAudioObjectSystemObject,
        AudioDeviceID, AudioObjectGetPropertyData, AudioObjectHasProperty,
        AudioObjectIsPropertySettable, AudioObjectPropertyAddress, AudioObjectSetPropertyData,
        Boolean, OSStatus,
    };
    use std::mem;
    use std::ptr::null;

    use super::MacRestoreState;

    fn property_address(selector: u32, scope: u32) -> AudioObjectPropertyAddress {
        AudioObjectPropertyAddress {
            mSelector: selector,
            mScope: scope,
            mElement: kAudioObjectPropertyElementMaster,
        }
    }

    fn default_output_device() -> Result<AudioDeviceID, String> {
        let property_address = property_address(
            kAudioHardwarePropertyDefaultOutputDevice,
            kAudioObjectPropertyScopeGlobal,
        );
        let mut device_id: AudioDeviceID = 0;
        let mut data_size = mem::size_of::<AudioDeviceID>() as u32;
        let status = unsafe {
            AudioObjectGetPropertyData(
                kAudioObjectSystemObject,
                &property_address as *const _,
                0,
                null(),
                &mut data_size,
                &mut device_id as *mut _ as *mut _,
            )
        };
        if status == kAudioHardwareNoError as OSStatus {
            Ok(device_id)
        } else {
            Err(format!(
                "Failed to get default output device for system audio control: OSStatus {status}"
            ))
        }
    }

    fn property_is_settable(
        device_id: AudioDeviceID,
        selector: u32,
        scope: u32,
    ) -> Result<bool, String> {
        let address = property_address(selector, scope);
        let has_property =
            unsafe { AudioObjectHasProperty(device_id, &address as *const _) } != 0 as Boolean;
        if !has_property {
            return Ok(false);
        }

        let mut settable: Boolean = 0;
        let status = unsafe {
            AudioObjectIsPropertySettable(device_id, &address as *const _, &mut settable)
        };
        if status == kAudioHardwareNoError as OSStatus {
            Ok(settable != 0)
        } else {
            Err(format!(
                "Failed to inspect audio property settable state: selector={selector} status={status}"
            ))
        }
    }

    fn get_u32_property(
        device_id: AudioDeviceID,
        selector: u32,
        scope: u32,
    ) -> Result<u32, String> {
        let address = property_address(selector, scope);
        let mut value: u32 = 0;
        let mut data_size = mem::size_of::<u32>() as u32;
        let status = unsafe {
            AudioObjectGetPropertyData(
                device_id,
                &address as *const _,
                0,
                null(),
                &mut data_size,
                &mut value as *mut _ as *mut _,
            )
        };
        if status == kAudioHardwareNoError as OSStatus {
            Ok(value)
        } else {
            Err(format!(
                "Failed to read audio property: selector={selector} status={status}"
            ))
        }
    }

    fn get_f32_property(
        device_id: AudioDeviceID,
        selector: u32,
        scope: u32,
    ) -> Result<f32, String> {
        let address = property_address(selector, scope);
        let mut value: f32 = 0.0;
        let mut data_size = mem::size_of::<f32>() as u32;
        let status = unsafe {
            AudioObjectGetPropertyData(
                device_id,
                &address as *const _,
                0,
                null(),
                &mut data_size,
                &mut value as *mut _ as *mut _,
            )
        };
        if status == kAudioHardwareNoError as OSStatus {
            Ok(value)
        } else {
            Err(format!(
                "Failed to read audio property: selector={selector} status={status}"
            ))
        }
    }

    fn set_u32_property(
        device_id: AudioDeviceID,
        selector: u32,
        scope: u32,
        value: u32,
    ) -> Result<(), String> {
        let address = property_address(selector, scope);
        let data_size = mem::size_of::<u32>() as u32;
        let status = unsafe {
            AudioObjectSetPropertyData(
                device_id,
                &address as *const _,
                0,
                null(),
                data_size,
                &value as *const _ as *const _,
            )
        };
        if status == kAudioHardwareNoError as OSStatus {
            Ok(())
        } else {
            Err(format!(
                "Failed to set audio property: selector={selector} status={status}"
            ))
        }
    }

    fn set_f32_property(
        device_id: AudioDeviceID,
        selector: u32,
        scope: u32,
        value: f32,
    ) -> Result<(), String> {
        let address = property_address(selector, scope);
        let data_size = mem::size_of::<f32>() as u32;
        let status = unsafe {
            AudioObjectSetPropertyData(
                device_id,
                &address as *const _,
                0,
                null(),
                data_size,
                &value as *const _ as *const _,
            )
        };
        if status == kAudioHardwareNoError as OSStatus {
            Ok(())
        } else {
            Err(format!(
                "Failed to set audio property: selector={selector} status={status}"
            ))
        }
    }

    pub fn snapshot_and_mute() -> Result<Option<MacRestoreState>, String> {
        let device_id = default_output_device()?;

        if property_is_settable(
            device_id,
            kAudioDevicePropertyMute,
            kAudioObjectPropertyScopeOutput,
        )? {
            let was_muted = get_u32_property(
                device_id,
                kAudioDevicePropertyMute,
                kAudioObjectPropertyScopeOutput,
            )? != 0;
            set_u32_property(
                device_id,
                kAudioDevicePropertyMute,
                kAudioObjectPropertyScopeOutput,
                1,
            )?;
            return Ok(Some(MacRestoreState::Muted {
                device_id,
                was_muted,
            }));
        }

        if property_is_settable(
            device_id,
            kAudioDevicePropertyVolumeScalar,
            kAudioObjectPropertyScopeOutput,
        )? {
            let previous_volume = get_f32_property(
                device_id,
                kAudioDevicePropertyVolumeScalar,
                kAudioObjectPropertyScopeOutput,
            )?;
            set_f32_property(
                device_id,
                kAudioDevicePropertyVolumeScalar,
                kAudioObjectPropertyScopeOutput,
                0.0,
            )?;
            return Ok(Some(MacRestoreState::Volume {
                device_id,
                previous_volume,
            }));
        }

        Ok(None)
    }

    pub fn restore(previous: MacRestoreState) -> Result<(), String> {
        match previous {
            MacRestoreState::Idle => Ok(()),
            MacRestoreState::Muted {
                device_id,
                was_muted,
            } => set_u32_property(
                device_id,
                kAudioDevicePropertyMute,
                kAudioObjectPropertyScopeOutput,
                if was_muted { 1 } else { 0 },
            ),
            MacRestoreState::Volume {
                device_id,
                previous_volume,
            } => set_f32_property(
                device_id,
                kAudioDevicePropertyVolumeScalar,
                kAudioObjectPropertyScopeOutput,
                previous_volume,
            ),
        }
    }

    pub(super) fn default_input_device() -> Result<AudioDeviceID, String> {
        let property_address = property_address(
            kAudioHardwarePropertyDefaultInputDevice,
            kAudioObjectPropertyScopeGlobal,
        );
        let mut device_id: AudioDeviceID = 0;
        let mut data_size = mem::size_of::<AudioDeviceID>() as u32;
        let status = unsafe {
            AudioObjectGetPropertyData(
                kAudioObjectSystemObject,
                &property_address as *const _,
                0,
                null(),
                &mut data_size,
                &mut device_id as *mut _ as *mut _,
            )
        };
        if status == kAudioHardwareNoError as OSStatus {
            Ok(device_id)
        } else {
            Err(format!(
                "Failed to get default input device for exclusive mic access: OSStatus {status}"
            ))
        }
    }

    fn get_i32_property(
        device_id: AudioDeviceID,
        selector: u32,
        scope: u32,
    ) -> Result<i32, String> {
        let address = property_address(selector, scope);
        let mut value: i32 = 0;
        let mut data_size = mem::size_of::<i32>() as u32;
        let status = unsafe {
            AudioObjectGetPropertyData(
                device_id,
                &address as *const _,
                0,
                null(),
                &mut data_size,
                &mut value as *mut _ as *mut _,
            )
        };
        if status == kAudioHardwareNoError as OSStatus {
            Ok(value)
        } else {
            Err(format!(
                "Failed to read audio property: selector={selector} status={status}"
            ))
        }
    }

    fn set_i32_property(
        device_id: AudioDeviceID,
        selector: u32,
        scope: u32,
        value: i32,
    ) -> Result<(), String> {
        let address = property_address(selector, scope);
        let data_size = mem::size_of::<i32>() as u32;
        let status = unsafe {
            AudioObjectSetPropertyData(
                device_id,
                &address as *const _,
                0,
                null(),
                data_size,
                &value as *const _ as *const _,
            )
        };
        if status == kAudioHardwareNoError as OSStatus {
            Ok(())
        } else {
            Err(format!(
                "Failed to set audio property: selector={selector} status={status}"
            ))
        }
    }

    /// Acquires exclusive (hog-mode) ownership of the default input device for
    /// this process. Returns the device id on success, `None` if hog mode isn't
    /// settable or another process already owns the device (nothing to restore).
    pub fn snapshot_and_hog() -> Result<Option<AudioDeviceID>, String> {
        let device_id = default_input_device()?;

        if !property_is_settable(
            device_id,
            kAudioDevicePropertyHogMode,
            kAudioObjectPropertyScopeGlobal,
        )? {
            return Ok(None);
        }

        // Hog mode owner is a pid_t; pass our pid to acquire, -1 to release.
        let our_pid = std::process::id() as i32;
        set_i32_property(
            device_id,
            kAudioDevicePropertyHogMode,
            kAudioObjectPropertyScopeGlobal,
            our_pid,
        )?;

        let owner = get_i32_property(
            device_id,
            kAudioDevicePropertyHogMode,
            kAudioObjectPropertyScopeGlobal,
        )?;
        if owner == our_pid {
            Ok(Some(device_id))
        } else {
            // The HAL declined or another process holds the device — we own
            // nothing, so there is nothing to release later.
            Ok(None)
        }
    }

    pub fn release_hog(device_id: AudioDeviceID) -> Result<(), String> {
        set_i32_property(
            device_id,
            kAudioDevicePropertyHogMode,
            kAudioObjectPropertyScopeGlobal,
            -1,
        )
    }
}

#[cfg(not(any(windows, target_os = "macos")))]
fn set_system_muted(_muted: bool) -> Result<(), String> {
    Ok(())
}

#[cfg(windows)]
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

#[cfg(windows)]
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

#[cfg(target_os = "macos")]
pub fn mute() {
    let mut restore_state = match RESTORE_STATE.lock() {
        Ok(guard) => guard,
        Err(poisoned) => {
            log::warn!("System audio restore state lock was poisoned; recovering");
            poisoned.into_inner()
        }
    };

    if !matches!(*restore_state, MacRestoreState::Idle) {
        return;
    }

    match macos::snapshot_and_mute() {
        Ok(Some(snapshot)) => {
            *restore_state = snapshot;
        }
        Ok(None) => {
            if !WARNED_UNSUPPORTED_AUDIO_CONTROL.swap(true, Ordering::Relaxed) {
                log::warn!(
                    "Default macOS output device does not expose a writable mute or master volume property; auto-mute is unavailable for this device"
                );
            }
        }
        Err(err) => {
            log::warn!("Failed to mute system audio: {err}");
        }
    }
}

#[cfg(target_os = "macos")]
pub fn unmute() {
    let mut restore_state = match RESTORE_STATE.lock() {
        Ok(guard) => guard,
        Err(poisoned) => {
            log::warn!("System audio restore state lock was poisoned; recovering");
            poisoned.into_inner()
        }
    };

    let snapshot = *restore_state;
    if matches!(snapshot, MacRestoreState::Idle) {
        return;
    }

    *restore_state = MacRestoreState::Idle;

    if let Err(err) = macos::restore(snapshot) {
        log::warn!("Failed to restore system audio: {err}");
    }
}

/// Reserves the microphone exclusively for this process (macOS hog mode), so no
/// other app can capture it while dictating. Safe to call repeatedly — a no-op
/// if exclusive access is already held.
#[cfg(target_os = "macos")]
pub fn register_session() -> u64 {
    let session_id = NEXT_SESSION_ID.fetch_add(1, Ordering::SeqCst);
    ACTIVE_SESSION_ID.store(session_id, Ordering::SeqCst);
    session_id
}

#[cfg(target_os = "macos")]
pub fn hog_mic(session_id: u64) {
    if ACTIVE_SESSION_ID.load(Ordering::SeqCst) != session_id {
        return;
    }

    let state = match HOG_STATE.lock() {
        Ok(guard) => guard,
        Err(poisoned) => {
            log::warn!("Exclusive mic state lock was poisoned; recovering");
            poisoned.into_inner()
        }
    };

    let mut state = state;
    if let HogState::Hogged { device_id, .. } = *state {
        match macos::default_input_device() {
            Ok(default_device) if default_device == device_id => {
                if ACTIVE_SESSION_ID.load(Ordering::SeqCst) == session_id {
                    *state = HogState::Hogged {
                        device_id,
                        session_id,
                    };
                }
                return;
            }
            _ => {
                let _ = macos::release_hog(device_id);
                *state = HogState::Idle;
            }
        }
    }

    let result = macos::snapshot_and_hog();

    if ACTIVE_SESSION_ID.load(Ordering::SeqCst) != session_id {
        if let Ok(Some(device_id)) = result {
            let _ = macos::release_hog(device_id);
        }
        return;
    }

    if !matches!(*state, HogState::Idle) {
        if let Ok(Some(device_id)) = result {
            let _ = macos::release_hog(device_id);
        }
        return;
    }

    match result {
        Ok(Some(device_id)) => {
            *state = HogState::Hogged {
                device_id,
                session_id,
            };
        }
        Ok(None) => {
            if !WARNED_HOG_UNAVAILABLE.swap(true, Ordering::Relaxed) {
                log::warn!(
                    "Exclusive microphone access is unavailable for the default input device (not settable or already owned by another process)"
                );
            }
        }
        Err(err) => {
            log::warn!("Failed to acquire exclusive microphone access: {err}");
        }
    }
}

/// Releases exclusive microphone access taken by [`hog_mic`]. No-op if not held.
#[cfg(target_os = "macos")]
pub fn release_mic(session_id: u64) {
    let _ = ACTIVE_SESSION_ID.compare_exchange(session_id, 0, Ordering::SeqCst, Ordering::SeqCst);

    let mut state = match HOG_STATE.lock() {
        Ok(guard) => guard,
        Err(poisoned) => {
            log::warn!("Exclusive mic state lock was poisoned; recovering");
            poisoned.into_inner()
        }
    };

    let device_id = match *state {
        HogState::Idle => return,
        HogState::Hogged {
            device_id,
            session_id: owner_session_id,
        } if owner_session_id == session_id => device_id,
        HogState::Hogged { .. } => return,
    };

    *state = HogState::Idle;

    if let Err(err) = macos::release_hog(device_id) {
        log::warn!("Failed to release exclusive microphone access: {err}");
    }
}

#[cfg(not(any(windows, target_os = "macos")))]
pub fn mute() {
    let _ = set_system_muted(true);
}

#[cfg(not(any(windows, target_os = "macos")))]
pub fn unmute() {
    let _ = set_system_muted(false);
}

/// Exclusive microphone access is macOS-only; no-op everywhere else.
#[cfg(not(target_os = "macos"))]
pub fn register_session() -> u64 {
    0
}

#[cfg(not(target_os = "macos"))]
pub fn hog_mic(_session_id: u64) {}

#[cfg(not(target_os = "macos"))]
pub fn release_mic(_session_id: u64) {}
