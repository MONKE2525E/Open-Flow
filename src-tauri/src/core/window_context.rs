#[cfg(windows)]
use windows::Win32::Foundation::{HANDLE, HWND, MAX_PATH};
#[cfg(windows)]
use windows::Win32::System::Threading::{OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW};
#[cfg(windows)]
use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId, SetForegroundWindow};

/// Returns the raw HWND handle of the current foreground window as an isize.
/// isize is used so the value is Send/Sync-safe to store in shared state.
pub fn get_foreground_hwnd() -> isize {
    #[cfg(windows)]
    unsafe { GetForegroundWindow().0 as isize }
    #[cfg(not(windows))]
    0
}

/// Restores foreground focus to a previously captured HWND.
pub fn restore_foreground(hwnd: isize) {
    #[cfg(windows)]
    if hwnd != 0 {
        unsafe { SetForegroundWindow(HWND(hwnd as *mut _)).ok(); }
    }
}

pub fn get_active_process_name() -> Option<String> {
    #[cfg(windows)]
    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.0 == std::ptr::null_mut() {
            return None;
        }

        let mut pid = 0;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if pid == 0 {
            return None;
        }

        let process_handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        
        let mut buffer = [0u16; MAX_PATH as usize];
        let mut size = buffer.len() as u32;
        
        if QueryFullProcessImageNameW(process_handle, windows::Win32::System::Threading::PROCESS_NAME_FORMAT(0), windows::core::PWSTR::from_raw(buffer.as_mut_ptr()), &mut size).is_ok() {
            let path = String::from_utf16_lossy(&buffer[..size as usize]);
            if let Some(name) = std::path::Path::new(&path).file_name().and_then(|n| n.to_str()) {
                return Some(name.to_lowercase());
            }
        }
        None
    }
    #[cfg(not(windows))]
    None
}
