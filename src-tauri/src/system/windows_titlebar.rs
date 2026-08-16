use serde::Serialize;
use std::{os::windows::ffi::OsStrExt, sync::OnceLock};
use tauri::{Emitter, Theme, WebviewWindow};
use windows::{
    core::PCSTR,
    Win32::{
        Foundation::HMODULE,
        System::LibraryLoader::{GetProcAddress, LoadLibraryW},
    },
};

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct NativeMetrics {
    titlebar_height: i32,
    left_inset: i32,
    right_inset: i32,
    dpi: u32,
    window_left: i32,
    window_top: i32,
    window_right: i32,
    window_bottom: i32,
    client_left: i32,
    client_top: i32,
    client_right: i32,
    client_bottom: i32,
    client_screen_x: i32,
    client_screen_y: i32,
    is_maximized: i32,
    extends_content: i32,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TitleBarMetrics {
    height: f64,
    left_inset: f64,
    right_inset: f64,
    scale_factor: f64,
    window_rect: [i32; 4],
    client_rect: [i32; 4],
    client_origin: [i32; 2],
    is_maximized: bool,
    extends_content: bool,
}

type ConfigureFn = unsafe extern "C" fn(isize, i32, *mut NativeMetrics) -> i32;
type MetricsFn = unsafe extern "C" fn(isize, *mut NativeMetrics) -> i32;

struct Bridge {
    _module: usize,
    enable: ConfigureFn,
    update: ConfigureFn,
    metrics: MetricsFn,
}

unsafe impl Send for Bridge {}
unsafe impl Sync for Bridge {}

static BRIDGE: OnceLock<Result<Bridge, String>> = OnceLock::new();

fn bridge() -> Result<&'static Bridge, String> {
    BRIDGE
        .get_or_init(load_bridge)
        .as_ref()
        .map_err(Clone::clone)
}

fn load_bridge() -> Result<Bridge, String> {
    let path = std::env::current_exe()
        .map_err(|e| e.to_string())?
        .with_file_name("Verenu.WindowsChrome.dll");
    let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    let module = unsafe { LoadLibraryW(windows::core::PCWSTR(wide.as_ptr())) }
        .map_err(|e| format!("could not load {}: {e}", path.display()))?;
    unsafe fn symbol<T: Copy>(module: HMODULE, name: &'static [u8]) -> Result<T, String> {
        let proc = GetProcAddress(module, PCSTR(name.as_ptr())).ok_or_else(|| {
            format!(
                "missing native title bar export {}",
                String::from_utf8_lossy(&name[..name.len() - 1])
            )
        })?;
        Ok(std::mem::transmute_copy::<
            unsafe extern "system" fn() -> isize,
            T,
        >(&proc))
    }
    Ok(Bridge {
        _module: module.0 as usize,
        enable: unsafe { symbol(module, b"verenu_enable_extended_titlebar\0")? },
        update: unsafe { symbol(module, b"verenu_update_extended_titlebar\0")? },
        metrics: unsafe { symbol(module, b"verenu_get_extended_titlebar_metrics\0")? },
    })
}

fn convert(native: NativeMetrics) -> TitleBarMetrics {
    let scale = f64::from(native.dpi.max(96)) / 96.0;
    TitleBarMetrics {
        height: f64::from(native.titlebar_height) / scale,
        left_inset: f64::from(native.left_inset) / scale,
        right_inset: f64::from(native.right_inset) / scale,
        scale_factor: scale,
        window_rect: [
            native.window_left,
            native.window_top,
            native.window_right,
            native.window_bottom,
        ],
        client_rect: [
            native.client_left,
            native.client_top,
            native.client_right,
            native.client_bottom,
        ],
        client_origin: [native.client_screen_x, native.client_screen_y],
        is_maximized: native.is_maximized != 0,
        extends_content: native.extends_content != 0,
    }
}

fn dark(theme: Option<Theme>) -> bool {
    !matches!(theme, Some(Theme::Light))
}

pub fn enable(window: &WebviewWindow, theme: Option<Theme>) -> Result<TitleBarMetrics, String> {
    let hwnd = window.hwnd().map_err(|e| e.to_string())?.0 as isize;
    let mut native = NativeMetrics::default();
    let hr = unsafe { (bridge()?.enable)(hwnd, i32::from(dark(theme)), &mut native) };
    if hr < 0 {
        return Err(format!(
            "AppWindowTitleBar initialization failed with HRESULT 0x{:08X}",
            hr as u32
        ));
    }
    let metrics = convert(native);
    log::info!("Windows extended title bar enabled: hwnd=0x{hwnd:X}, window={:?}, client={:?}, client_origin={:?}, height={}px, insets=({}, {}), scale={}, maximized={}", metrics.window_rect, metrics.client_rect, metrics.client_origin, metrics.height, metrics.left_inset, metrics.right_inset, metrics.scale_factor, metrics.is_maximized);
    window
        .emit("verenu:native-titlebar-metrics", &metrics)
        .map_err(|e| e.to_string())?;
    Ok(metrics)
}

pub fn refresh(window: &WebviewWindow, theme: Option<Theme>) {
    let Ok(hwnd) = window.hwnd() else { return };
    let mut native = NativeMetrics::default();
    if let Ok(bridge) = bridge() {
        let hr = unsafe { (bridge.update)(hwnd.0 as isize, i32::from(dark(theme)), &mut native) };
        if hr >= 0 {
            let _ = window.emit("verenu:native-titlebar-metrics", convert(native));
        }
    }
}

#[tauri::command]
pub fn get_native_titlebar_metrics(window: WebviewWindow) -> Result<TitleBarMetrics, String> {
    let hwnd = window.hwnd().map_err(|e| e.to_string())?;
    let mut native = NativeMetrics::default();
    let hr = unsafe { (bridge()?.metrics)(hwnd.0 as isize, &mut native) };
    if hr < 0 {
        Err(format!(
            "title bar metrics failed with HRESULT 0x{:08X}",
            hr as u32
        ))
    } else {
        Ok(convert(native))
    }
}
