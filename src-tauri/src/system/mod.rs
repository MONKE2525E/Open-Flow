pub mod apps;
pub mod connectivity;
pub mod logger;
#[cfg(target_os = "macos")]
pub mod mac_app;
pub mod media_control;
pub mod memory;
pub mod notify;
pub mod number_parser;
pub mod platform;
pub mod text;
pub mod volume;
#[cfg(target_os = "windows")]
pub mod windows_titlebar;
#[cfg(not(target_os = "windows"))]
pub mod windows_titlebar {
    #[tauri::command]
    pub fn get_native_titlebar_metrics() -> Result<(), String> {
        Err("native title bar metrics are only available on Windows".to_owned())
    }
}

