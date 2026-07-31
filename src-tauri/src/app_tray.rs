use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::TrayIconBuilder,
    AppHandle, Emitter, Manager, Theme,
};

const TRAY_ID: &str = "verenu-tray";

#[derive(Clone, Copy, PartialEq, Eq)]
enum IconTheme {
    Light,
    Dark,
}

#[derive(Clone, Copy)]
struct IconRect {
    x: u32,
    y: u32,
    width: u32,
    height: u32,
    radius: u32,
}

pub(crate) fn setup_tray(app: &mut tauri::App) -> tauri::Result<()> {
    let open_i = MenuItem::with_id(app, "open", "Open Verenu", true, None::<&str>)?;
    #[cfg(target_os = "macos")]
    let permissions_i =
        MenuItem::with_id(app, "permissions", "Permissions...", true, None::<&str>)?;
    let settings_i = MenuItem::with_id(app, "settings", "Settings...", true, None::<&str>)?;
    let sep = PredefinedMenuItem::separator(app)?;
    let relaunch_i = MenuItem::with_id(app, "relaunch", "Relaunch", true, None::<&str>)?;
    let quit_i = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;

    #[cfg(target_os = "macos")]
    let menu = Menu::with_items(
        app,
        &[
            &open_i,
            &permissions_i,
            &settings_i,
            &sep,
            &relaunch_i,
            &quit_i,
        ],
    )?;
    #[cfg(not(target_os = "macos"))]
    let menu = Menu::with_items(app, &[&open_i, &settings_i, &sep, &relaunch_i, &quit_i])?;

    let icon_theme = resolve_icon_theme(app.handle(), None);
    let tray_icon = runtime_tray_icon_image(icon_theme, 32);

    TrayIconBuilder::with_id(TRAY_ID)
        .icon(tray_icon)
        .icon_as_template(cfg!(target_os = "macos"))
        .menu(&menu)
        .show_menu_on_left_click(true)
        .tooltip("Verenu")
        .on_menu_event(|app, ev| match ev.id.as_ref() {
            "open" => crate::show_main_window(app),
            "permissions" => {
                crate::show_main_window(app);
                let _ = app.emit("open-flow:open-settings-section", "permissions");
            }
            "settings" => {
                crate::show_main_window(app);
                let _ = app.emit("open-flow:open-settings-section", "general");
            }
            "relaunch" => relaunch_app(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .build(app)?;

    apply_runtime_icons(app.handle(), None);

    Ok(())
}

fn relaunch_app(app: &AppHandle) {
    #[cfg(target_os = "windows")]
    {
        if let Err(err) = spawn_relaunch_and_exit(app) {
            log::error!("Failed to relaunch Verenu: {err}");
            crate::show_main_window(app);
            let _ = app.emit(
                "verenu:error",
                "Could not relaunch Verenu. Please quit and reopen the app.",
            );
        }
    }

    #[cfg(not(target_os = "windows"))]
    app.restart();
}

#[cfg(all(target_os = "windows", not(debug_assertions)))]
pub(crate) fn relaunch_for_startup_recovery(app: &AppHandle) {
    if let Err(err) = spawn_relaunch_and_exit_with_args(app, &["--startup-recovery-attempted"]) {
        log::error!("Failed to recover Verenu startup: {err}");
        app.exit(0);
    }
}

#[cfg(target_os = "windows")]
fn spawn_relaunch_and_exit(app: &AppHandle) -> Result<(), String> {
    spawn_relaunch_and_exit_with_args(app, &[])
}

#[cfg(target_os = "windows")]
fn spawn_relaunch_and_exit_with_args(
    app: &AppHandle,
    extra_args: &[&str],
) -> Result<(), String> {
    use std::os::windows::process::CommandExt;

    const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
    const DETACHED_PROCESS: u32 = 0x0000_0008;

    let exe = std::env::current_exe()
        .map_err(|err| format!("could not locate current executable: {err}"))?;
    let parent_pid = std::process::id().to_string();
    let forwarded_args = forwarded_relaunch_args();
    let mut command = std::process::Command::new(exe);
    command
        .args(forwarded_args)
        .args(extra_args)
        .arg("--relaunch-parent-pid")
        .arg(parent_pid)
        .creation_flags(CREATE_NEW_PROCESS_GROUP | DETACHED_PROCESS)
        .spawn()
        .map_err(|err| format!("could not start replacement process: {err}"))?;

    app.exit(0);
    Ok(())
}

#[cfg(target_os = "windows")]
fn forwarded_relaunch_args() -> Vec<std::ffi::OsString> {
    let mut filtered = Vec::new();
    let mut args = std::env::args_os().skip(1);

    while let Some(arg) = args.next() {
        let Some(text) = arg.to_str() else {
            filtered.push(arg);
            continue;
        };

        if text == "--relaunch-parent-pid" {
            let _ = args.next();
            continue;
        }

        if text.starts_with("--relaunch-parent-pid=") {
            continue;
        }

        filtered.push(arg);
    }

    filtered
}

pub(crate) fn apply_runtime_icons(app: &AppHandle, theme_hint: Option<Theme>) {
    let icon_theme = resolve_icon_theme(app, theme_hint);

    if let Some(w) = app.get_webview_window("main") {
        if let Err(err) = w.set_icon(runtime_icon_image(icon_theme, 128)) {
            log::warn!("Failed to update window icon: {err}");
        }
    }

    #[cfg(target_os = "macos")]
    apply_native_main_window_chrome(app, theme_hint);

    #[cfg(target_os = "windows")]
    apply_native_main_window_chrome(app, theme_hint);

    #[cfg(target_os = "macos")]
    if !crate::system::mac_app::apply_dock_icon() {
        log::warn!("Failed to update macOS Dock icon");
    }

    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        let result = tray.set_icon_with_as_template(
            Some(runtime_tray_icon_image(icon_theme, 32)),
            cfg!(target_os = "macos"),
        );
        if let Err(err) = result {
            log::warn!("Failed to update tray icon: {err}");
        }
    }
}

#[cfg(target_os = "macos")]
fn apply_native_main_window_chrome(app: &AppHandle, theme_hint: Option<Theme>) {
    if let Some(w) = app.get_webview_window("main") {
        let bg = match resolve_icon_theme(app, theme_hint) {
            IconTheme::Dark => tauri::utils::config::Color(20, 17, 14, 255),
            IconTheme::Light => tauri::utils::config::Color(249, 247, 243, 255),
        };
        w.set_decorations(true).ok();
        w.set_background_color(Some(bg)).ok();
        w.set_title("").ok();
        w.set_title_bar_style(tauri::TitleBarStyle::Transparent)
            .ok();
    }
}

#[cfg(target_os = "windows")]
fn apply_native_main_window_chrome(app: &AppHandle, theme_hint: Option<Theme>) {
    use windows::Win32::Foundation::{COLORREF, LPARAM, WPARAM};
    use windows::Win32::Graphics::Dwm::{
        DwmSetWindowAttribute, DWMWA_CAPTION_COLOR, DWMWA_TEXT_COLOR,
    };
    use windows::Win32::UI::WindowsAndMessaging::{SendMessageW, ICON_BIG, ICON_SMALL, WM_SETICON};

    if let Some(w) = app.get_webview_window("main") {
        let icon_theme = resolve_icon_theme(app, theme_hint);
        // Same --paper value as theme.css, recolored onto the native caption.
        let bg = match icon_theme {
            IconTheme::Dark => colorref(20, 17, 14),
            IconTheme::Light => colorref(249, 247, 243),
        };
        if let Ok(hwnd) = w.hwnd() {
            unsafe {
                let _ = DwmSetWindowAttribute(
                    hwnd,
                    DWMWA_CAPTION_COLOR,
                    &bg as *const _ as *const _,
                    std::mem::size_of::<COLORREF>() as u32,
                );
                // Match the title text color to the caption background instead of blanking the
                // title string. Setting an empty title hides the caption text but also blanks
                // the Taskbar/Alt+Tab label and the window's accessible name; this way the real
                // title ("Verenu", from tauri.conf.json) stays intact for those, it's just
                // visually invisible against the caption. Confirmed via screenshot this doesn't
                // affect the minimize/maximize/close glyphs — Windows colors those independently
                // based on the caption color's luminance, not DWMWA_TEXT_COLOR.
                let _ = DwmSetWindowAttribute(
                    hwnd,
                    DWMWA_TEXT_COLOR,
                    &bg as *const _ as *const _,
                    std::mem::size_of::<COLORREF>() as u32,
                );
                // Decouple the caption icon from the taskbar icon. A WS_SYSMENU window always
                // paints *something* in its caption-icon slot, and the taskbar falls back to
                // the small icon when ICON_BIG is unset — so nulling the icon either shows a
                // default glyph or blanks the taskbar. Instead, give the taskbar its own real
                // icon (ICON_BIG) and make only the caption's small icon fully transparent.
                // WM_SETICON state survives — unlike the window's extended style, which tao
                // resets after our call (so WS_EX_DLGMODALFRAME did not stick here).
                let (small, big) = cached_caption_icons(icon_theme);
                let _ = SendMessageW(
                    hwnd,
                    WM_SETICON,
                    Some(WPARAM(ICON_BIG as usize)),
                    Some(LPARAM(big)),
                );
                let _ = SendMessageW(
                    hwnd,
                    WM_SETICON,
                    Some(WPARAM(ICON_SMALL as usize)),
                    Some(LPARAM(small)),
                );
            }
        }
    }
}

#[cfg(target_os = "windows")]
fn colorref(r: u8, g: u8, b: u8) -> windows::Win32::Foundation::COLORREF {
    windows::Win32::Foundation::COLORREF(((b as u32) << 16) | ((g as u32) << 8) | r as u32)
}

/// Returns `(transparent_small_icon, real_big_icon)` as raw HICON values for the given
/// theme. The small icon is a fully transparent 16×16 used to blank the caption-icon slot
/// (theme-independent — always invisible); the big icon is the app's bar-chart logo in the
/// requested theme's colours, kept for the taskbar/Alt+Tab. Each variant is built at most
/// once and cached for the process lifetime, so there is nothing to leak; caching dark and
/// light separately (rather than one cache keyed by whichever theme resolved first) is what
/// makes the taskbar icon actually follow a later theme switch.
#[cfg(target_os = "windows")]
fn cached_caption_icons(theme: IconTheme) -> (isize, isize) {
    use std::sync::OnceLock;
    static TRANSPARENT: OnceLock<isize> = OnceLock::new();
    static DARK_REAL: OnceLock<isize> = OnceLock::new();
    static LIGHT_REAL: OnceLock<isize> = OnceLock::new();

    let transparent = *TRANSPARENT.get_or_init(|| make_transparent_hicon(16));
    let real = match theme {
        IconTheme::Dark => *DARK_REAL
            .get_or_init(|| make_hicon(runtime_icon_image(IconTheme::Dark, 256).rgba(), 256)),
        IconTheme::Light => *LIGHT_REAL
            .get_or_init(|| make_hicon(runtime_icon_image(IconTheme::Light, 256).rgba(), 256)),
    };
    (transparent, real)
}

/// Builds a fully transparent square HICON (raw handle as `isize`, `0` on failure).
/// The AND mask must be a real 1-bit-per-pixel bitmap with every bit set (= every pixel
/// transparent) and rows padded to a 16-bit boundary; the colour bits are all zero. This is
/// distinct from [`make_hicon`], whose byte-per-pixel mask only renders correctly for opaque
/// icons — reusing it here left a black square in the caption.
#[cfg(target_os = "windows")]
fn make_transparent_hicon(size: i32) -> isize {
    use windows::Win32::UI::WindowsAndMessaging::CreateIcon;
    let stride_bytes = (size as usize).div_ceil(16) * 2; // 1bpp row, padded to a WORD
    let and_mask = vec![0xFF_u8; stride_bytes * size as usize];
    let color = vec![0_u8; (size * size * 4) as usize];
    // SAFETY: CreateIcon copies both buffers, which outlive the call.
    unsafe { CreateIcon(None, size, size, 1, 32, and_mask.as_ptr(), color.as_ptr()) }
        .map(|h| h.0 as isize)
        .unwrap_or(0)
}

/// Builds an HICON from an RGBA buffer (returning the raw handle as `isize`, `0` on failure).
/// The AND mask is a proper 1-bit-per-pixel bitmap (rows padded to a `WORD` boundary, same
/// packing as [`make_transparent_hicon`]) derived from the alpha channel; the colour bits are
/// the pixels swizzled to BGRA, zeroed wherever the AND mask marks the pixel transparent so
/// Windows has nothing to XOR against the background there. `CreateIcon` always expects a
/// 1bpp AND mask regardless of the XOR mask's bit depth — an earlier byte-per-pixel version
/// of this mask was read as a packed bitfield by Windows, corrupting transparency.
#[cfg(target_os = "windows")]
fn make_hicon(rgba: &[u8], size: i32) -> isize {
    use windows::Win32::UI::WindowsAndMessaging::CreateIcon;
    let size_usize = size as usize;
    if rgba.len() < size_usize * size_usize * 4 {
        return 0;
    }
    let stride_bytes = size_usize.div_ceil(16) * 2; // 1bpp row, padded to a WORD
    let mut and_mask = vec![0_u8; stride_bytes * size_usize];
    let mut bgra = rgba.to_vec();
    for y in 0..size_usize {
        for x in 0..size_usize {
            let i = y * size_usize + x;
            if rgba[i * 4 + 3] < 128 {
                and_mask[y * stride_bytes + x / 8] |= 1 << (7 - x % 8);
                bgra[i * 4..i * 4 + 4].fill(0);
            } else {
                bgra[i * 4] = rgba[i * 4 + 2];
                bgra[i * 4 + 2] = rgba[i * 4];
            }
        }
    }
    // SAFETY: CreateIcon copies the AND/colour buffers, which outlive the call.
    unsafe { CreateIcon(None, size, size, 1, 32, and_mask.as_ptr(), bgra.as_ptr()) }
        .map(|h| h.0 as isize)
        .unwrap_or(0)
}

#[cfg(target_os = "macos")]
fn runtime_tray_icon_image(theme: IconTheme, size: u32) -> tauri::image::Image<'static> {
    let mut rgba = vec![0_u8; (size * size * 4) as usize];
    let color = runtime_tray_icon_color(theme);

    for (x, y, width, height, radius) in [
        (64, 304, 64, 96, 30),
        (144, 208, 64, 192, 30),
        (224, 112, 64, 288, 30),
        (304, 240, 64, 160, 30),
        (384, 320, 64, 80, 30),
    ] {
        draw_rounded_rect(
            &mut rgba,
            size,
            IconRect {
                x: scale(size, x),
                y: scale(size, y),
                width: scale(size, width),
                height: scale(size, height),
                radius: scale(size, radius),
            },
            color,
        );
    }

    tauri::image::Image::new_owned(rgba, size, size)
}

#[cfg(target_os = "macos")]
fn runtime_tray_icon_color(theme: IconTheme) -> [u8; 4] {
    match theme {
        IconTheme::Light => [0, 0, 0, 255],
        IconTheme::Dark => [255, 255, 255, 255],
    }
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::{runtime_tray_icon_color, IconTheme};

    #[test]
    fn tray_icon_uses_black_in_light_mode() {
        assert_eq!(runtime_tray_icon_color(IconTheme::Light), [0, 0, 0, 255]);
    }

    #[test]
    fn tray_icon_uses_white_in_dark_mode() {
        assert_eq!(
            runtime_tray_icon_color(IconTheme::Dark),
            [255, 255, 255, 255]
        );
    }
}

#[cfg(not(target_os = "macos"))]
fn runtime_tray_icon_image(theme: IconTheme, size: u32) -> tauri::image::Image<'static> {
    runtime_icon_image(theme, size)
}

fn resolve_icon_theme(app: &AppHandle, theme_hint: Option<Theme>) -> IconTheme {
    match appearance_mode(app).as_deref() {
        Some("dark") => IconTheme::Dark,
        Some("light") => IconTheme::Light,
        _ => match theme_hint.or_else(|| {
            app.get_webview_window("main")
                .and_then(|window| window.theme().ok())
        }) {
            Some(Theme::Dark) => IconTheme::Dark,
            _ => IconTheme::Light,
        },
    }
}

pub(crate) fn appearance_mode(app: &AppHandle) -> Option<String> {
    crate::data::store::settings_handle(app)
        .ok()
        .and_then(|settings| settings.get(crate::data::store::APPEARANCE_MODE))
        .and_then(|value| value.as_str().map(String::from))
}

fn runtime_icon_image(theme: IconTheme, size: u32) -> tauri::image::Image<'static> {
    let mut rgba = vec![0_u8; (size * size * 4) as usize];
    let background = match theme {
        IconTheme::Light => [249, 247, 243, 255],
        IconTheme::Dark => [20, 17, 14, 255],
    };
    let accent = [217, 119, 87, 255];

    #[cfg(target_os = "macos")]
    let background_rect = IconRect {
        x: scale(size, 64),
        y: scale(size, 64),
        width: scale(size, 384),
        height: scale(size, 384),
        radius: scale(size, 76),
    };

    #[cfg(not(target_os = "macos"))]
    let background_rect = IconRect {
        x: 0,
        y: 0,
        width: size,
        height: size,
        radius: scale(size, 96),
    };

    draw_rounded_rect(&mut rgba, size, background_rect, background);

    #[cfg(target_os = "macos")]
    let bar_rects = [
        (129, 290, 38, 70, 19),
        (183, 220, 38, 140, 19),
        (237, 152, 38, 208, 19),
        (291, 240, 38, 120, 19),
        (345, 298, 38, 62, 19),
    ];

    #[cfg(not(target_os = "macos"))]
    let bar_rects = [
        (88, 328, 48, 88, 24),
        (160, 239, 48, 177, 24),
        (232, 153, 48, 263, 24),
        (304, 264, 48, 152, 24),
        (376, 338, 48, 78, 24),
    ];

    for (x, y, width, height, radius) in bar_rects {
        draw_rounded_rect(
            &mut rgba,
            size,
            IconRect {
                x: scale(size, x),
                y: scale(size, y),
                width: scale(size, width),
                height: scale(size, height),
                radius: scale(size, radius),
            },
            accent,
        );
    }

    tauri::image::Image::new_owned(rgba, size, size)
}

fn scale(size: u32, value: u32) -> u32 {
    ((value * size) / 512).max(1)
}

fn draw_rounded_rect(rgba: &mut [u8], canvas_size: u32, rect: IconRect, color: [u8; 4]) {
    let right = rect.x.saturating_add(rect.width).min(canvas_size);
    let bottom = rect.y.saturating_add(rect.height).min(canvas_size);
    let radius = rect.radius.min(rect.width / 2).min(rect.height / 2) as i32;

    for py in rect.y..bottom {
        for px in rect.x..right {
            if is_inside_rounded_rect(
                px as i32,
                py as i32,
                rect.x as i32,
                rect.y as i32,
                right as i32,
                bottom as i32,
                radius,
            ) {
                let idx = ((py * canvas_size + px) * 4) as usize;
                rgba[idx..idx + 4].copy_from_slice(&color);
            }
        }
    }
}

fn is_inside_rounded_rect(
    px: i32,
    py: i32,
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
    radius: i32,
) -> bool {
    if radius <= 0 {
        return true;
    }

    let cx = if px < left + radius {
        left + radius
    } else if px >= right - radius {
        right - radius - 1
    } else {
        px
    };
    let cy = if py < top + radius {
        top + radius
    } else if py >= bottom - radius {
        bottom - radius - 1
    } else {
        py
    };

    let dx = px - cx;
    let dy = py - cy;
    dx * dx + dy * dy <= radius * radius
}
