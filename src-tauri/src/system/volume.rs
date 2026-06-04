#[cfg(target_os = "macos")]
use std::sync::atomic::{AtomicBool, Ordering};
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
        kAudioDevicePropertyMute, kAudioDevicePropertyVolumeScalar, kAudioHardwareNoError,
        kAudioHardwarePropertyDefaultOutputDevice, kAudioObjectPropertyElementMain,
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
            mElement: kAudioObjectPropertyElementMain,
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

    if let Err(err) = macos::restore(snapshot) {
        log::warn!("Failed to restore system audio: {err}");
        return;
    }

    *restore_state = MacRestoreState::Idle;
}

#[cfg(not(any(windows, target_os = "macos")))]
pub fn mute() {
    let _ = set_system_muted(true);
}

#[cfg(not(any(windows, target_os = "macos")))]
pub fn unmute() {
    let _ = set_system_muted(false);
}
