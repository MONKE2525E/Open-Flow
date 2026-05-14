use std::sync::atomic::{AtomicBool, Ordering};
use windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume;
use windows::Win32::Media::Audio::{eConsole, eRender, IMMDeviceEnumerator, MMDeviceEnumerator};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CLSCTX_ALL, COINIT_MULTITHREADED,
};

static IS_MUTED: AtomicBool = AtomicBool::new(false);

unsafe fn get_volume_interface() -> Result<IAudioEndpointVolume, windows::core::Error> {
    let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
    let enumerator: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
    let device = enumerator.GetDefaultAudioEndpoint(eRender, eConsole)?;
    device.Activate(CLSCTX_ALL, None)
}

pub fn mute() {
    if IS_MUTED.swap(true, Ordering::SeqCst) {
        return;
    }
    if let Ok(volume) = unsafe { get_volume_interface() } {
        let _ = unsafe { volume.SetMute(true, std::ptr::null()) };
    }
}

pub fn unmute() {
    if !IS_MUTED.swap(false, Ordering::SeqCst) {
        return;
    }
    if let Ok(volume) = unsafe { get_volume_interface() } {
        let _ = unsafe { volume.SetMute(false, std::ptr::null()) };
    }
}
