use serde::Serialize;
use std::{
    os::windows::ffi::OsStrExt,
    sync::{Mutex, OnceLock},
};
use tauri::{AppHandle, Emitter, Manager, Theme, WebviewWindow};
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

impl NativeMetrics {
    /// Fields the frontend actually mirrors (see `applyNativeTitleBarMetrics`
    /// in App.svelte: height + caption insets + scale). Window/client rects
    /// and the client origin move on every drag/resize but are unconsumed, so
    /// they must not by themselves trigger a re-emit.
    fn frontend_relevant_eq(&self, other: &Self) -> bool {
        self.titlebar_height == other.titlebar_height
            && self.left_inset == other.left_inset
            && self.right_inset == other.right_inset
            && self.dpi == other.dpi
            && self.is_maximized == other.is_maximized
            && self.extends_content == other.extends_content
    }
}

/// Last metrics payload forwarded to the frontend. Window events fire
/// per-frame during a move/resize drag; without this every frame paid for a
/// WebView2 event plus a full-document style recalc from the CSS-var writes,
/// even though height/insets/scale are position- and size-independent.
static LAST_EMITTED: OnceLock<Mutex<Option<NativeMetrics>>> = OnceLock::new();

fn emit_if_changed(window: &WebviewWindow, native: &NativeMetrics) {
    let changed = match LAST_EMITTED.get_or_init(|| Mutex::new(None)).lock() {
        Ok(mut guard) => {
            let changed = guard
                .map(|prev| !prev.frontend_relevant_eq(native))
                .unwrap_or(true);
            if changed {
                *guard = Some(*native);
            }
            changed
        }
        // A poisoned lock must never swallow a metrics update — emit and let
        // the next call re-seed the cache.
        Err(_) => true,
    };
    if changed {
        let _ = window.emit("verenu:native-titlebar-metrics", convert(*native));
    }
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
type SetRuntimeIconsFn = unsafe extern "C" fn(isize, isize, isize, *const u16) -> i32;

struct Bridge {
    _module: usize,
    enable: ConfigureFn,
    update: ConfigureFn,
    metrics: MetricsFn,
    set_runtime_icons: SetRuntimeIconsFn,
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
        set_runtime_icons: unsafe { symbol(module, b"verenu_set_runtime_icons\0")? },
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

fn dark(theme: Option<Theme>, appearance_mode: Option<&str>) -> bool {
    match appearance_mode {
        Some("light") => false,
        Some("dark") => true,
        _ => !matches!(theme, Some(Theme::Light)),
    }
}

fn resolved_dark(window: &WebviewWindow, theme: Option<Theme>) -> bool {
    dark(
        theme,
        crate::app_tray::appearance_mode(window.app_handle()).as_deref(),
    )
}

pub fn enable(window: &WebviewWindow, theme: Option<Theme>) -> Result<TitleBarMetrics, String> {
    let hwnd = window.hwnd().map_err(|e| e.to_string())?.0 as isize;
    let mut native = NativeMetrics::default();
    let hr =
        unsafe { (bridge()?.enable)(hwnd, i32::from(resolved_dark(window, theme)), &mut native) };
    if hr < 0 {
        return Err(format!(
            "AppWindowTitleBar initialization failed with HRESULT 0x{:08X}",
            hr as u32
        ));
    }
    let metrics = convert(native);
    log::info!("Windows extended title bar enabled: hwnd=0x{hwnd:X}, window={:?}, client={:?}, client_origin={:?}, height={}px, insets=({}, {}), scale={}, maximized={}", metrics.window_rect, metrics.client_rect, metrics.client_origin, metrics.height, metrics.left_inset, metrics.right_inset, metrics.scale_factor, metrics.is_maximized);
    if let Ok(mut guard) = LAST_EMITTED.get_or_init(|| Mutex::new(None)).lock() {
        *guard = Some(native);
    }
    window
        .emit("verenu:native-titlebar-metrics", &metrics)
        .map_err(|e| e.to_string())?;
    Ok(metrics)
}

pub fn refresh(window: &WebviewWindow, theme: Option<Theme>) {
    refresh_with_dark(window, resolved_dark(window, theme));
}

fn refresh_with_dark(window: &WebviewWindow, dark: bool) {
    let Ok(hwnd) = window.hwnd() else { return };
    let mut native = NativeMetrics::default();
    if let Ok(bridge) = bridge() {
        let hr = unsafe {
            (bridge.update)(
                hwnd.0 as isize,
                i32::from(dark),
                &mut native,
            )
        };
        if hr >= 0 {
            emit_if_changed(window, &native);
        }
    }
}

/// Apply the theme the frontend is actually rendering. The native window theme
/// can follow the OS while Verenu is explicitly set to the opposite appearance.
#[tauri::command]
pub fn set_native_titlebar_theme(window: WebviewWindow, dark: bool) {
    refresh_with_dark(&window, dark);
}

pub(crate) fn refresh_for_app(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        refresh(&window, window.theme().ok());
    }
}

pub fn set_runtime_icons(
    hwnd: isize,
    taskbar_icon: isize,
    titlebar_icon: isize,
    taskbar_icon_path: &std::path::Path,
) -> Result<(), String> {
    let path: Vec<u16> = taskbar_icon_path
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect();
    let hr =
        unsafe { (bridge()?.set_runtime_icons)(hwnd, taskbar_icon, titlebar_icon, path.as_ptr()) };
    if hr < 0 {
        Err(format!(
            "AppWindow icon update failed with HRESULT 0x{:08X}",
            hr as u32
        ))
    } else {
        Ok(())
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

#[cfg(test)]
mod tests {
    use super::{dark, NativeMetrics};

    #[test]
    fn explicit_appearance_overrides_system_theme() {
        assert!(!dark(Some(tauri::Theme::Dark), Some("light")));
        assert!(dark(Some(tauri::Theme::Light), Some("dark")));
    }

    #[test]
    fn system_appearance_uses_native_theme() {
        assert!(!dark(Some(tauri::Theme::Light), Some("system")));
        assert!(dark(Some(tauri::Theme::Dark), Some("system")));
        assert!(dark(None, None));
    }

    fn metrics() -> NativeMetrics {
        let mut m = NativeMetrics::default();
        m.titlebar_height = 32;
        m.left_inset = 10;
        m.right_inset = 140;
        m.dpi = 144;
        m.window_left = 100;
        m.window_top = 100;
        m.window_right = 1400;
        m.window_bottom = 900;
        m.client_screen_x = 100;
        m.client_screen_y = 132;
        m
    }

    #[test]
    fn position_and_size_changes_do_not_count_as_relevant() {
        let mut moved = metrics();
        moved.window_left += 300;
        moved.window_top += 200;
        moved.client_screen_x += 300;
        moved.client_screen_y += 200;
        // A resize also shifts the rect edges without touching the chrome.
        moved.window_right += 150;
        moved.window_bottom += 100;
        assert!(metrics().frontend_relevant_eq(&moved));
    }

    #[test]
    fn chrome_changes_count_as_relevant() {
        let base = metrics();
        for mutated in [
            NativeMetrics {
                titlebar_height: 40,
                ..base
            },
            NativeMetrics {
                left_inset: 0,
                ..base
            },
            NativeMetrics {
                right_inset: 100,
                ..base
            },
            NativeMetrics { dpi: 96, ..base },
            NativeMetrics {
                is_maximized: 1,
                ..base
            },
        ] {
            assert!(!base.frontend_relevant_eq(&mutated));
        }
    }
}
