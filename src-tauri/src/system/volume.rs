use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::thread::sleep;
use std::time::Duration;
use windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume;
use windows::Win32::Media::Audio::{eConsole, eRender, IMMDeviceEnumerator, MMDeviceEnumerator};
use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_ALL, COINIT_MULTITHREADED};

static IS_MUTED: AtomicBool = AtomicBool::new(false);
static SAVED_VOLUME: AtomicU32 = AtomicU32::new(0);

unsafe fn get_volume_interface() -> Result<IAudioEndpointVolume, windows::core::Error> {
    // Try to initialize COM on this thread. We ignore errors because it may already be initialized.
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
        if let Ok(current_level) = unsafe { volume.GetMasterVolumeLevelScalar() } {
            // Save current volume
            SAVED_VOLUME.store(current_level.to_bits(), Ordering::SeqCst);

            // Smooth fade to 0
            let steps = 10;
            for i in 0..=steps {
                let level = current_level * (1.0 - (i as f32 / steps as f32));
                let _ = unsafe { volume.SetMasterVolumeLevelScalar(level, std::ptr::null()) };
                sleep(Duration::from_millis(15));
            }
        }
    }
}

pub fn unmute() {
    if !IS_MUTED.swap(false, Ordering::SeqCst) {
        return;
    }

    if let Ok(volume) = unsafe { get_volume_interface() } {
        let target = f32::from_bits(SAVED_VOLUME.load(Ordering::SeqCst));
        let current_level = unsafe { volume.GetMasterVolumeLevelScalar() }.unwrap_or(0.0);

        // Smooth fade up to target
        let steps = 10;
        for i in 0..=steps {
            let p = i as f32 / steps as f32;
            let level = current_level + (target - current_level) * p;
            let _ = unsafe { volume.SetMasterVolumeLevelScalar(level, std::ptr::null()) };
            sleep(Duration::from_millis(15));
        }
        
        // Ensure it's exactly the target at the end
        let _ = unsafe { volume.SetMasterVolumeLevelScalar(target, std::ptr::null()) };
    }
}
