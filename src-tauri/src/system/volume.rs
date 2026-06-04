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

// macOS: toggle the default output device mute via CoreAudio. Runs on the
// caller's (already-spawned) thread, so the brief native call is fine.
#[cfg(target_os = "macos")]
fn set_system_muted(muted: bool) -> Result<(), String> {
    use coreaudio::sys::{
        kAudioDevicePropertyMute, kAudioHardwareNoError, kAudioHardwarePropertyDefaultOutputDevice,
        kAudioObjectPropertyElementMaster, kAudioObjectPropertyScopeGlobal,
        kAudioObjectPropertyScopeOutput, kAudioObjectSystemObject, AudioDeviceID,
        AudioObjectGetPropertyData, AudioObjectPropertyAddress, AudioObjectSetPropertyData,
    };
    use std::mem;
    use std::ptr::null;

    let default_output_device = {
        let property_address = AudioObjectPropertyAddress {
            mSelector: kAudioHardwarePropertyDefaultOutputDevice,
            mScope: kAudioObjectPropertyScopeGlobal,
            mElement: kAudioObjectPropertyElementMaster,
        };

        let device_id: AudioDeviceID = 0;
        let data_size = mem::size_of::<AudioDeviceID>() as u32;
        let status = unsafe {
            AudioObjectGetPropertyData(
                kAudioObjectSystemObject,
                &property_address as *const _,
                0,
                null(),
                &data_size as *const _ as *mut _,
                &device_id as *const _ as *mut _,
            )
        };
        if status != kAudioHardwareNoError as i32 {
            return Err(format!(
                "Failed to get default output device for system mute: OSStatus {status}"
            ));
        }
        device_id
    };

    let muted_value: u32 = if muted { 1 } else { 0 };
    let property_address = AudioObjectPropertyAddress {
        mSelector: kAudioDevicePropertyMute,
        mScope: kAudioObjectPropertyScopeOutput,
        mElement: kAudioObjectPropertyElementMaster,
    };
    let data_size = mem::size_of::<u32>() as u32;
    let status = unsafe {
        AudioObjectSetPropertyData(
            default_output_device,
            &property_address as *const _,
            0,
            null(),
            data_size,
            &muted_value as *const _ as *const _,
        )
    };

    if status == kAudioHardwareNoError as i32 {
        Ok(())
    } else {
        Err(format!("Failed to set system mute: OSStatus {status}"))
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
