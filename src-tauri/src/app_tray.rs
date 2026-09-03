use std::sync::{Mutex, OnceLock};
use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::TrayIconBuilder,
    AppHandle, Emitter, Manager, Theme,
};

const TRAY_ID: &str = "verenu-tray";

pub(crate) fn setting_updates_runtime_icons(key: &str) -> bool {
    matches!(
        key,
        crate::data::store::APPEARANCE_MODE | crate::data::store::ACCENT_COLOR
    )
}

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
    let accent = resolve_icon_accent(app.handle());
    #[cfg(target_os = "windows")]
    let tray_size = windows_tray_icon_size(app.handle());
    #[cfg(not(target_os = "windows"))]
    let tray_size = 32;
    let tray_icon = runtime_tray_icon_image(icon_theme, accent, tray_size);

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
fn spawn_relaunch_and_exit_with_args(app: &AppHandle, extra_args: &[&str]) -> Result<(), String> {
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

/// Rendered icon artwork for the most recent `apply_runtime_icons` call.
/// Re-asserting icons happens on window focus, theme/DPI changes, and settings
/// writes; the tray glyph's 32x supersampled renderer costs hundreds of
/// thousands of pixel ops per call, so repeat calls with unchanged inputs
/// re-send the cached bytes instead of re-running the rasterizers.
#[derive(Clone)]
struct CachedIconArt {
    theme: IconTheme,
    accent: [u8; 4],
    tray_size: u32,
    window_rgba: Vec<u8>,
    tray_rgba: Vec<u8>,
}

static ICON_ART_CACHE: OnceLock<Mutex<Option<CachedIconArt>>> = OnceLock::new();

fn cached_icon_art(theme: IconTheme, accent: [u8; 4], tray_size: u32) -> CachedIconArt {
    if let Ok(guard) = ICON_ART_CACHE.get_or_init(|| Mutex::new(None)).lock() {
        if let Some(cached) = guard.as_ref() {
            if cached.theme == theme
                && cached.accent == accent
                && cached.tray_size == tray_size
            {
                return cached.clone();
            }
        }
    }
    let art = CachedIconArt {
        theme,
        accent,
        tray_size,
        window_rgba: runtime_icon_image(theme, accent, 128).rgba().to_vec(),
        tray_rgba: runtime_tray_icon_image(theme, accent, tray_size)
            .rgba()
            .to_vec(),
    };
    if let Ok(mut guard) = ICON_ART_CACHE.get_or_init(|| Mutex::new(None)).lock() {
        *guard = Some(art.clone());
    }
    art
}

pub(crate) fn apply_runtime_icons(app: &AppHandle, theme_hint: Option<Theme>) {
    let icon_theme = resolve_icon_theme(app, theme_hint);
    let accent = resolve_icon_accent(app);
    #[cfg(target_os = "windows")]
    let tray_size = windows_tray_icon_size(app);
    #[cfg(not(target_os = "windows"))]
    let tray_size = 32;

    // Re-send the cached artwork when the inputs are unchanged — the handles
    // are re-asserted (AppWindow can lose them), but the rasterizers,
    // notably the tray glyph's 32x supersampled renderer, are skipped.
    let art = cached_icon_art(icon_theme, accent, tray_size);

    if let Some(w) = app.get_webview_window("main") {
        if let Err(err) = w.set_icon(tauri::image::Image::new_owned(
            art.window_rgba.clone(),
            128,
            128,
        )) {
            log::warn!("Failed to update window icon: {err}");
        }
    }

    #[cfg(target_os = "macos")]
    apply_native_main_window_chrome(app, theme_hint);

    #[cfg(target_os = "windows")]
    apply_native_main_window_chrome(app, theme_hint);

    #[cfg(target_os = "macos")]
    {
        let dock_icon = runtime_icon_image(icon_theme, accent, 512);
        if !crate::system::mac_app::apply_dock_icon(dock_icon.rgba(), 512, 512) {
            log::warn!("Failed to update macOS Dock icon");
        }
    }

    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        let result = tray.set_icon_with_as_template(
            Some(tauri::image::Image::new_owned(
                art.tray_rgba.clone(),
                tray_size,
                tray_size,
            )),
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
    use windows::Win32::Foundation::COLORREF;
    use windows::Win32::Graphics::Dwm::{
        DwmSetWindowAttribute, DWMWA_CAPTION_COLOR, DWMWA_TEXT_COLOR,
    };

    if let Some(w) = app.get_webview_window("main") {
        let icon_theme = resolve_icon_theme(app, theme_hint);
        let accent = resolve_icon_accent(app);
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
                replace_taskbar_icons(hwnd, icon_theme, accent);
            }
        }
    }
}

#[cfg(target_os = "windows")]
fn colorref(r: u8, g: u8, b: u8) -> windows::Win32::Foundation::COLORREF {
    windows::Win32::Foundation::COLORREF(((b as u32) << 16) | ((g as u32) << 8) | r as u32)
}

#[cfg(target_os = "windows")]
fn replace_taskbar_icons(
    hwnd: windows::Win32::Foundation::HWND,
    theme: IconTheme,
    accent: [u8; 4],
) {
    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock};
    use windows::core::{w, PCWSTR};
    use windows::Win32::Foundation::{LPARAM, WPARAM};
    use windows::Win32::UI::HiDpi::{GetDpiForWindow, GetSystemMetricsForDpi};
    use windows::Win32::UI::WindowsAndMessaging::{
        DestroyIcon, FindWindowW, GetAncestor, SendMessageW, GA_ROOTOWNER, ICON_BIG, ICON_SMALL,
        SM_CXICON, SM_CXSMICON, WM_GETICON, WM_SETICON,
    };

    #[derive(Clone)]
    struct WindowIcons {
        theme: IconTheme,
        accent: [u8; 4],
        big: isize,
        small: isize,
        path: std::path::PathBuf,
    }

    static CURRENT: OnceLock<Mutex<HashMap<isize, WindowIcons>>> = OnceLock::new();
    let taskbar_hwnd = unsafe {
        let root_owner = GetAncestor(hwnd, GA_ROOTOWNER);
        if root_owner.0.is_null() {
            hwnd
        } else {
            root_owner
        }
    };
    let key = taskbar_hwnd.0 as isize;
    // Snapshot the cached handles under a short lock hold. Rendering the
    // supersampled bitmaps, encoding the ICO, and pumping Win32 messages
    // below must not block other callers on this mutex.
    let cached = CURRENT
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .ok()
        .and_then(|current| current.get(&key).cloned());
    if let Some(icons) = cached {
        if icons.theme == theme && icons.accent == accent {
            unsafe {
                let _ = SendMessageW(
                    taskbar_hwnd,
                    WM_SETICON,
                    Some(WPARAM(ICON_BIG as usize)),
                    Some(LPARAM(icons.big)),
                );
                let _ = SendMessageW(
                    taskbar_hwnd,
                    WM_SETICON,
                    Some(WPARAM(ICON_SMALL as usize)),
                    Some(LPARAM(icons.small)),
                );
                if let Err(err) = crate::system::windows_titlebar::set_runtime_icons(
                    key,
                    icons.big,
                    icons.small,
                    &icons.path,
                ) {
                    log::warn!(
                        "Failed to restore Windows AppWindow icons for hwnd=0x{key:X}: {err}"
                    );
                }
            }
            return;
        }
    }

    // Query DPI from the real Shell_TrayWnd, not our own window's ancestor — on a
    // multi-monitor mixed-DPI setup those can disagree, and any mismatch between the
    // HICON's native size and the size Explorer actually displays it at gets a low-quality
    // legacy stretch (CreateIcon-built icons don't go through the shell's normal per-DPI
    // icon loader), which is what turned a mathematically flat shared baseline into visibly
    // uneven bars. windows_tray_icon_size() already gets this right for the tray.
    let dpi_source =
        unsafe { FindWindowW(w!("Shell_TrayWnd"), PCWSTR::null()) }.unwrap_or(taskbar_hwnd);
    let dpi = unsafe { GetDpiForWindow(dpi_source) }.max(96);
    let big_size = unsafe { GetSystemMetricsForDpi(SM_CXICON, dpi) }.max(32) as u32;
    // SM_CXSMICON is the Win10-era small-icon metric (20px at 120 DPI), but the Windows 11
    // taskbar draws app icons at 24 logical px — 30 physical at 125%. Feeding it a 20px
    // HICON made Explorer upscale 20 -> 30, and a 1.5x non-integer stretch lands some bars
    // a pixel lower or wider than their neighbours, which is the "uneven bars" this kept
    // coming back as. Render at the size the shell actually paints; anything that wants the
    // classic small icon (caption, Alt+Tab) downscales from this, which is lossless-looking.
    let shell_icon_size = (24 * dpi).div_ceil(96);
    let small_size = unsafe { GetSystemMetricsForDpi(SM_CXSMICON, dpi) }
        .max(16)
        .max(shell_icon_size as i32) as u32;
    let big_image = windows_taskbar_icon_image(theme, accent, big_size);
    let small_image = windows_micro_icon_image(theme, accent, small_size);
    let big = make_hicon(big_image.rgba(), big_size as i32);
    let small = make_hicon(small_image.rgba(), small_size as i32);
    if big == 0 || small == 0 {
        unsafe {
            if big != 0 {
                let _ = DestroyIcon(windows::Win32::UI::WindowsAndMessaging::HICON(
                    big as *mut _,
                ));
            }
            if small != 0 {
                let _ = DestroyIcon(windows::Win32::UI::WindowsAndMessaging::HICON(
                    small as *mut _,
                ));
            }
        }
        log::warn!("Failed to create Windows taskbar icons: hwnd=0x{key:X}, big={big_size}px, small={small_size}px");
        return;
    }
    // Anchor the ladder on exact integer multiples of the size the taskbar paints. The
    // shell asks for the large icon (40px here) and scales it into its 30px slot; with an
    // arbitrary ladder that is a 0.75x scale, which puts every bar on a different sub-pixel
    // phase (measured pitch 5,4,4,5). With 30/60/120 the shell's pick always divides down by
    // exactly 2 or 4, so every bar keeps the same width, pitch and baseline. 256 stays for
    // genuinely large surfaces.
    let ladder = [
        shell_icon_size,
        shell_icon_size * 2,
        shell_icon_size * 4,
        256,
    ];
    let taskbar_icon_path = match write_runtime_ico(theme, accent, &ladder) {
        Ok(path) => path,
        Err(err) => {
            unsafe {
                let _ = DestroyIcon(windows::Win32::UI::WindowsAndMessaging::HICON(
                    big as *mut _,
                ));
                let _ = DestroyIcon(windows::Win32::UI::WindowsAndMessaging::HICON(
                    small as *mut _,
                ));
            }
            log::warn!("Failed to write Windows runtime taskbar icon: {err}");
            return;
        }
    };
    crate::system::notify::refresh_windows_notification_identity(&taskbar_icon_path);
    let shell_icon_path = taskbar_icon_path.clone();
    unsafe {
        let _ = SendMessageW(
            taskbar_hwnd,
            WM_SETICON,
            Some(WPARAM(ICON_BIG as usize)),
            Some(LPARAM(big)),
        );
        let _ = SendMessageW(
            taskbar_hwnd,
            WM_SETICON,
            Some(WPARAM(ICON_SMALL as usize)),
            Some(LPARAM(small)),
        );
        if let Err(err) =
            crate::system::windows_titlebar::set_runtime_icons(key, big, small, &shell_icon_path)
        {
            log::warn!("Failed to update Windows AppWindow icons for hwnd=0x{key:X}: {err}");
        }
        let read_big = SendMessageW(
            taskbar_hwnd,
            WM_GETICON,
            Some(WPARAM(ICON_BIG as usize)),
            None,
        )
        .0;
        let read_small = SendMessageW(
            taskbar_hwnd,
            WM_GETICON,
            Some(WPARAM(ICON_SMALL as usize)),
            None,
        )
        .0;
        log::info!(
            "Windows taskbar icons applied: tauri_hwnd=0x{:X}, taskbar_hwnd=0x{key:X}, dpi={dpi}, big={big_size}px 0x{big:X}/readback=0x{:X}, small={small_size}px 0x{small:X}/readback=0x{:X}, accent=#{:02X}{:02X}{:02X}",
            hwnd.0 as isize, read_big, read_small, accent[0], accent[1], accent[2]
        );
        if read_big != big || read_small != small {
            log::warn!("Windows rejected a runtime taskbar icon handle for hwnd=0x{key:X}");
        }
    }
    // Publish the new handles under a short lock hold; the displaced
    // handles are destroyed after the lock is released.
    let old = CURRENT
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .ok()
        .and_then(|mut current| {
            current.insert(
                key,
                WindowIcons {
                    theme,
                    accent,
                    big,
                    small,
                    path: shell_icon_path.clone(),
                },
            )
        });
    if let Some(old) = old {
        unsafe {
            let _ = DestroyIcon(windows::Win32::UI::WindowsAndMessaging::HICON(
                old.big as *mut _,
            ));
            let _ = DestroyIcon(windows::Win32::UI::WindowsAndMessaging::HICON(
                old.small as *mut _,
            ));
        }
        if old.path != shell_icon_path {
            let _ = std::fs::remove_file(old.path);
        }
    }
}

#[cfg(target_os = "windows")]
fn windows_tray_icon_size(app: &AppHandle) -> u32 {
    use windows::core::{w, PCWSTR};
    use windows::Win32::UI::HiDpi::{GetDpiForWindow, GetSystemMetricsForDpi};
    use windows::Win32::UI::WindowsAndMessaging::{FindWindowW, SM_CXSMICON};

    let taskbar = unsafe { FindWindowW(w!("Shell_TrayWnd"), PCWSTR::null()) }.ok();
    taskbar
        .or_else(|| {
            app.get_webview_window("main")
                .and_then(|window| window.hwnd().ok())
        })
        .map(|hwnd| {
            let dpi = unsafe { GetDpiForWindow(hwnd) }.max(96);
            unsafe { GetSystemMetricsForDpi(SM_CXSMICON, dpi) }.clamp(16, 32) as u32
        })
        .unwrap_or(16)
}

#[cfg(target_os = "windows")]
fn write_runtime_taskbar_ico(
    theme: IconTheme,
    accent: [u8; 4],
) -> Result<std::path::PathBuf, String> {
    write_runtime_ico(theme, accent, &FULL_ICO_SIZES)
}

#[cfg(target_os = "windows")]
fn write_runtime_ico(
    theme: IconTheme,
    accent: [u8; 4],
    sizes: &[u32],
) -> Result<std::path::PathBuf, String> {
    use std::hash::{Hash, Hasher};

    let ico = runtime_ico_bytes(theme, accent, sizes)?;
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    ico.hash(&mut hasher);
    let appearance = match theme {
        IconTheme::Light => "light",
        IconTheme::Dark => "dark",
    };
    let path = std::env::temp_dir().join(format!(
        "verenu-taskbar-{:02X}{:02X}{:02X}-{appearance}-{:016X}.ico",
        accent[0],
        accent[1],
        accent[2],
        hasher.finish()
    ));
    if !path.exists() {
        std::fs::write(&path, ico).map_err(|err| err.to_string())?;
    }
    Ok(path)
}

#[cfg(target_os = "windows")]
pub(crate) fn prepare_windows_shell_icon(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    let theme = resolve_icon_theme(app, None);
    let accent = resolve_icon_accent(app);
    let path = write_runtime_taskbar_ico(theme, accent)?;
    log::info!("Windows themed ICO generated: {}", path.display());
    Ok(path)
}

#[cfg(target_os = "windows")]
pub(crate) fn cleanup_runtime_icon_files() {
    let Ok(entries) = std::fs::read_dir(std::env::temp_dir()) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if entry
            .file_name()
            .to_string_lossy()
            .starts_with("verenu-taskbar-")
            && path.extension().is_some_and(|extension| extension == "ico")
        {
            let _ = std::fs::remove_file(path);
        }
    }
}

#[cfg(target_os = "windows")]
const FULL_ICO_SIZES: [u32; 15] = [16, 18, 20, 22, 24, 26, 28, 30, 32, 40, 48, 64, 96, 128, 256];

#[cfg(target_os = "windows")]
fn runtime_ico_bytes(theme: IconTheme, accent: [u8; 4], sizes: &[u32]) -> Result<Vec<u8>, String> {
    // Below 40px, Explorer's own large-icon proportions (windows_taskbar_icon_image) read as
    // thin and crooked — the micro renderer used by the tray is optically tuned for these
    // native sizes and keeps the shell-sourced taskbar button matching the tray glyph.
    //
    // The small sizes are deliberately dense (every even value 16-32, not just the four
    // classic ICO sizes): AppWindow.SetTaskbarIcon picks a frame from this file by whatever
    // physical pixel size the taskbar surface actually wants, which doesn't always land on
    // 16/20/24/32 — a miss forces the shell to rescale the nearest frame itself, and that
    // second, uncontrolled resize is exactly the kind of blur/ringing this file exists to
    // avoid. Every frame is cheap to render, so there's no reason not to cover the gaps.
    // Anything the shell realistically paints small gets the pixel-snapped micro glyph;
    // only genuinely large surfaces (Start menu, file properties) use the detailed artwork.
    const MICRO_CUTOFF: u32 = 48;
    let mut frames = Vec::with_capacity(sizes.len());
    for size in sizes.iter().copied() {
        let image = if size <= MICRO_CUTOFF {
            windows_micro_icon_image(theme, accent, size)
        } else {
            windows_taskbar_icon_image(theme, accent, size)
        };
        let rgba = image::RgbaImage::from_raw(size, size, image.rgba().to_vec())
            .ok_or_else(|| format!("invalid {size}px taskbar RGBA buffer"))?;
        let mut png = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(rgba)
            .write_to(&mut png, image::ImageFormat::Png)
            .map_err(|err| err.to_string())?;
        frames.push((size, png.into_inner()));
    }
    let directory_size = 6 + 16 * frames.len();
    let payload_size: usize = frames.iter().map(|(_, png)| png.len()).sum();
    let mut ico = Vec::with_capacity(directory_size + payload_size);
    ico.extend_from_slice(&[0, 0, 1, 0]);
    ico.extend_from_slice(&(frames.len() as u16).to_le_bytes());
    let dimension = |value: u32| if value >= 256 { 0 } else { value as u8 };
    let mut offset = directory_size as u32;
    for (size, png) in &frames {
        ico.push(dimension(*size));
        ico.push(dimension(*size));
        ico.extend_from_slice(&[0, 0, 1, 0, 32, 0]);
        ico.extend_from_slice(&(png.len() as u32).to_le_bytes());
        ico.extend_from_slice(&offset.to_le_bytes());
        offset += png.len() as u32;
    }
    for (_, png) in frames {
        ico.extend_from_slice(&png);
    }
    Ok(ico)
}

/// The window/Alt+Tab logo, drawn separately from [`runtime_icon_image`] so native
/// Windows roles can be rendered directly at their requested size.
///
/// Geometry is kept in step with `icons/icon-source-windows.svg`, which generates that
/// `icon.ico`, so Explorer and the taskbar show the same logo. What is locked to the tray
/// is the bar *height ratios*, not the silhouette: the tray's glyph is 1.31:1, so merely
/// scaling it up runs the short outer bars into the tile's side margins before the tall
/// middle bar fills the height, and it still reads as a small mark on a white card. The
/// gaps are tightened and the glyph grown vertically instead, yielding a near-square
/// 71.9% x 69.7% glyph with centre-y at 47.3% (the bars share a baseline, so true
/// bounding-box centring reads as low).
#[cfg(target_os = "windows")]
fn windows_taskbar_icon_image(
    theme: IconTheme,
    accent: [u8; 4],
    size: u32,
) -> tauri::image::Image<'static> {
    let mut rgba = vec![0_u8; (size * size * 4) as usize];
    let background = match theme {
        IconTheme::Light => [249, 247, 243, 255],
        IconTheme::Dark => [20, 17, 14, 255],
    };

    draw_rounded_rect(
        &mut rgba,
        size,
        IconRect {
            x: 0,
            y: 0,
            width: size,
            height: size,
            radius: scale(size, 96),
        },
        background,
    );

    // Mirrors the five <rect> bars in icons/icon-source-windows.svg on this 512 grid:
    // bar 56, gap 22, tallest 357, shared baseline y = 421, glyph spans x 72..440, y 64..421.
    for (x, y, width, height, radius) in [
        (72, 301, 56, 120, 28),
        (150, 181, 56, 240, 28),
        (228, 64, 56, 357, 28),
        (306, 215, 56, 206, 28),
        (384, 315, 56, 106, 28),
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
            accent,
        );
    }

    tauri::image::Image::new_owned(rgba, size, size)
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
fn runtime_tray_icon_image(
    theme: IconTheme,
    _accent: [u8; 4],
    size: u32,
) -> tauri::image::Image<'static> {
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

#[cfg(target_os = "windows")]
fn runtime_tray_icon_image(
    theme: IconTheme,
    accent: [u8; 4],
    size: u32,
) -> tauri::image::Image<'static> {
    windows_micro_icon_image(theme, accent, size)
}

// Pure software rasterization with no OS calls, so the Linux CI build (which
// exists only to run tests) also compiles it for tests: that lets the tray
// geometry tests exercise the exact artwork Windows ships.
#[cfg(any(target_os = "windows", all(test, target_os = "linux")))]
fn windows_micro_icon_image(
    theme: IconTheme,
    accent: [u8; 4],
    size: u32,
) -> tauri::image::Image<'static> {
    // Define the silhouette on the final physical-pixel grid, then render it
    // at 32x. This keeps straight edges intentional while retaining smooth caps.
    let factor = 32;
    let render_size = size * factor;
    let mut rgba = vec![0_u8; (render_size * render_size * 4) as usize];
    let background = match theme {
        IconTheme::Light => [249, 247, 243, 255],
        IconTheme::Dark => [20, 17, 14, 255],
    };
    draw_rounded_rect(
        &mut rgba,
        render_size,
        IconRect {
            x: 0,
            y: 0,
            width: render_size,
            height: render_size,
            radius: ((size * 3).div_ceil(16)).max(2) * factor,
        },
        background,
    );

    // Classic tray proportions (bar 48, gap 24, heights 88/177/263/152/78 on a
    // shared baseline at 416 of the 512 grid) — the same silhouette the tray
    // used before supersampling, so the glyph sits in the tile with real
    // margins instead of filling ~70% of it edge to edge. Bar height *ratios*
    // match the taskbar artwork either way, so the two still read as one logo.
    //
    // The horizontal metrics and the baseline are rounded to WHOLE NATIVE PIXELS first and
    // only then scaled into the supersampled grid. Deriving them as raw fractions instead
    // gives a bar 3.28px wide on a 4.91px pitch at 30px, so every bar sits on a different
    // sub-pixel phase and the box filter smears each one by a different amount — measured on
    // the real taskbar that came out as bar pitch 5,4,4,5 and gaps 2,1,1,2, i.e. the
    // "uneven bars". Snapping to integers makes every bar the same width, every gap the same
    // width, and the shared baseline land on one exact row; only the rounded caps get
    // anti-aliased, which is the part that should be smooth.
    const SOURCE_GRID: u32 = 512;
    let round_div = |value: u32, div: u32| (value + div / 2) / div;
    let bar_width = round_div(size * 48, SOURCE_GRID).max(2);
    let gap = round_div(size * 24, SOURCE_GRID).max(1);
    let glyph_width = bar_width * 5 + gap * 4;
    let left = size.saturating_sub(glyph_width) / 2;
    let max_height_px = round_div(size * 263, SOURCE_GRID).max(4);
    // Center the glyph box in the tile: baseline-anchoring left a taller top
    // margin than bottom one (6 vs 4 at 20px) and the mark read as sitting low.
    let top_px = size.saturating_sub(max_height_px) / 2;
    let baseline_px = top_px + max_height_px;
    let heights =
        [88_u32, 177, 263, 152, 78].map(|h| round_div(max_height_px * h, 263).max(1) * factor);

    let bar_width_ss = bar_width * factor;
    let gap_ss = gap * factor;
    let left_ss = left * factor;
    let baseline = baseline_px * factor;

    for (index, height) in heights.into_iter().enumerate() {
        draw_rounded_rect(
            &mut rgba,
            render_size,
            IconRect {
                x: left_ss + index as u32 * (bar_width_ss + gap_ss),
                y: baseline - height,
                width: bar_width_ss,
                height,
                radius: bar_width_ss.div_ceil(2),
            },
            accent,
        );
    }

    // Box-average downsample (mean of each factor x factor block), not Lanczos: at a 32x
    // ratio Lanczos's negative lobes ring on the bars' sharp edges, most visibly on the
    // tallest bar, turning its flat rounded cap into a crooked point. A plain average is
    // exactly what "supersample then downsample" anti-aliasing calls for and can't ring.
    let mut output = vec![0_u8; (size * size * 4) as usize];
    let samples = factor * factor;
    for oy in 0..size {
        for ox in 0..size {
            let mut sum = [0_u32; 4];
            for dy in 0..factor {
                for dx in 0..factor {
                    let idx =
                        (((oy * factor + dy) * render_size + (ox * factor + dx)) * 4) as usize;
                    for c in 0..4 {
                        sum[c] += u32::from(rgba[idx + c]);
                    }
                }
            }
            let out_idx = ((oy * size + ox) * 4) as usize;
            for c in 0..4 {
                output[out_idx + c] = (sum[c] / samples) as u8;
            }
        }
    }
    tauri::image::Image::new_owned(output, size, size)
}

#[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
fn runtime_tray_icon_image(
    theme: IconTheme,
    accent: [u8; 4],
    size: u32,
) -> tauri::image::Image<'static> {
    // Linux builds exist only for CI: under test, render through the same
    // micro renderer the Windows tray uses so the geometry tests validate
    // real artwork instead of the large-icon fallback.
    #[cfg(test)]
    {
        windows_micro_icon_image(theme, accent, size)
    }
    #[cfg(not(test))]
    {
        runtime_icon_image(theme, accent, size)
    }
}

#[cfg(all(test, not(target_os = "macos")))]
mod windows_icon_tests {
    #[cfg(target_os = "windows")]
    use super::{runtime_ico_bytes, windows_taskbar_icon_image, FULL_ICO_SIZES};
    use super::{runtime_tray_icon_image, IconTheme, DEFAULT_ICON_ACCENT};

    /// Normalized accent-glyph bounds of an RGBA buffer, as fractions of the image:
    /// `(width, height, centre_x, centre_y)`.
    fn glyph_bounds(rgba: &[u8], size: u32) -> (f64, f64, f64, f64) {
        let (mut l, mut t, mut r, mut b) = (size, size, 0_u32, 0_u32);
        for y in 0..size {
            for x in 0..size {
                let i = ((y * size + x) * 4) as usize;
                let (red, green, blue, alpha) = (
                    rgba[i] as i32,
                    rgba[i + 1] as i32,
                    rgba[i + 2] as i32,
                    rgba[i + 3],
                );
                // #d97757 is far redder than either tile colour; the tolerance
                // lets antialiased edges in the .ico count too.
                let _ = green;
                if alpha > 128 && red > 140 && red - blue > 50 {
                    l = l.min(x);
                    t = t.min(y);
                    r = r.max(x + 1);
                    b = b.max(y + 1);
                }
            }
        }
        assert!(r > l && b > t, "no accent pixels found");
        let s = f64::from(size);
        (
            f64::from(r - l) / s,
            f64::from(b - t) / s,
            (f64::from(l + r) / 2.0) / s,
            (f64::from(t + b) / 2.0) / s,
        )
    }

    /// Per-bar heights of the accent glyph, normalized so the tallest bar is 1.0.
    ///
    /// This — not the glyph's bounding-box aspect — is what makes the taskbar mark
    /// read as the same logo as the tray. Bar width, gap and overall scale are
    /// deliberately different between the two.
    ///
    /// Columns are grouped into contiguous runs of accent pixels; each run is one bar.
    /// Only meaningful at 256px, where the bars are ~28px wide with ~11px gaps and so
    /// cannot merge — at 32px a gap is one pixel and antialiasing bridges it.
    fn bar_height_ratios(rgba: &[u8], size: u32) -> Vec<f64> {
        let column_height = |x: u32| -> u32 {
            let (mut top, mut bottom) = (size, 0_u32);
            for y in 0..size {
                let i = ((y * size + x) * 4) as usize;
                let (red, blue, alpha) = (rgba[i] as i32, rgba[i + 2] as i32, rgba[i + 3]);
                if alpha > 128 && red > 140 && red - blue > 50 {
                    top = top.min(y);
                    bottom = bottom.max(y + 1);
                }
            }
            bottom.saturating_sub(top)
        };

        let mut bars = Vec::new();
        let mut run: Option<u32> = None;
        for x in 0..size {
            match (column_height(x), run) {
                (0, Some(max)) => {
                    bars.push(max);
                    run = None;
                }
                (0, None) => {}
                (h, Some(max)) => run = Some(max.max(h)),
                (h, None) => run = Some(h),
            }
        }
        if let Some(max) = run {
            bars.push(max);
        }
        let tallest = f64::from(*bars.iter().max().expect("no accent bars found"));
        bars.into_iter().map(|h| f64::from(h) / tallest).collect()
    }

    /// Asserts a glyph's bar height ratios match the tray's, sampled at 256px so the
    /// bars are resolvable. Everything else about the taskbar glyph — bar width, gap,
    /// overall scale — is allowed to differ.
    fn assert_matches_tray_ratios(label: &str, actual: &[f64]) {
        let tray = bar_height_ratios(
            runtime_tray_icon_image(IconTheme::Dark, DEFAULT_ICON_ACCENT, 256).rgba(),
            256,
        );
        assert_eq!(
            actual.len(),
            tray.len(),
            "{label}: expected {} bars, found {}",
            tray.len(),
            actual.len()
        );
        for (i, (a, t)) in actual.iter().zip(tray.iter()).enumerate() {
            assert!(
                (a - t).abs() <= 0.05,
                "{label}: bar {i} height ratio {a:.3} drifted from the tray's {t:.3}"
            );
        }
    }

    /// Pulls one frame out of `icon.ico` by its pixel size. Frames are PNG-encoded
    /// by `scripts/generate-icons.ps1`, so this also proves the file really holds
    /// what we think it does rather than trusting the generator.
    fn ico_frame(bytes: &[u8], want: u32) -> Vec<u8> {
        let count = u16::from_le_bytes([bytes[4], bytes[5]]) as usize;
        for i in 0..count {
            let e = 6 + 16 * i;
            let size = if bytes[e] == 0 {
                256
            } else {
                u32::from(bytes[e])
            };
            if size != want {
                continue;
            }
            let len = u32::from_le_bytes([bytes[e + 8], bytes[e + 9], bytes[e + 10], bytes[e + 11]])
                as usize;
            let off =
                u32::from_le_bytes([bytes[e + 12], bytes[e + 13], bytes[e + 14], bytes[e + 15]])
                    as usize;
            let data = &bytes[off..off + len];
            assert_eq!(
                &data[0..4],
                &[0x89, b'P', b'N', b'G'],
                "ico frame {want} is not PNG-encoded"
            );
            return data.to_vec();
        }
        panic!("icon.ico has no {want}x{want} frame");
    }

    /// Windows draws two independently-produced logos for this app: the tray uses
    /// [`runtime_tray_icon_image`], while Explorer and the pinned-but-not-running
    /// taskbar button render the exe's embedded `icon.ico`. Nothing links them, and
    /// they have drifted twice.
    ///
    /// This checks the FINAL raster, not source coordinates — so it also proves
    /// `scripts/generate-icons.ps1` was actually re-run after the SVG changed.
    ///
    /// The taskbar glyph is deliberately NOT a scaled copy of the tray's. The tray sits
    /// on a dark tile that blends into the shell; this cream tile reads as a hard edge,
    /// and a tray-proportioned glyph looked stranded on a white card. Scaling the tray up
    /// does not fix that either — its 1.31:1 silhouette runs the short outer bars into the
    /// side margins before the tall middle bar fills the height. What must hold is that
    /// the bar height RATIOS are unchanged and the glyph is optically centred.
    #[test]
    fn taskbar_ico_is_centered_and_keeps_tray_bar_ratios() {
        let ico = include_bytes!("../icons/icon.ico");
        let decode = |want: u32| {
            let decoded =
                image::load_from_memory_with_format(&ico_frame(ico, want), image::ImageFormat::Png)
                    .expect("decode icon.ico frame")
                    .to_rgba8();
            assert_eq!((decoded.width(), decoded.height()), (want, want));
            decoded
        };

        // Ratios are measured at 256px: at 32px a gap is one pixel and antialiasing
        // bridges the bars into one blob.
        let large = decode(256);
        assert_matches_tray_ratios("icon.ico", &bar_height_ratios(large.as_raw(), 256));

        // Size envelope and centring are checked on the 32px frame, the one the shell
        // actually picks at normal DPI, so rounding there cannot hide a regression.
        let small = decode(32);
        let (w, h, cx, cy) = glyph_bounds(small.as_raw(), 32);
        assert!(
            (0.69..=0.76).contains(&w),
            "taskbar glyph width {w:.3} outside 0.69..0.76"
        );
        assert!(
            (0.66..=0.73).contains(&h),
            "taskbar glyph height {h:.3} outside 0.66..0.73"
        );
        assert!(
            (cx - 0.5).abs() <= 0.02,
            "taskbar glyph is not horizontally centred: cx {cx:.3}"
        );
        // The bars share a baseline, so visual mass sits low; centre-or-slightly-high
        // reads as balanced, centre-or-low does not.
        assert!(
            (0.44..=0.50).contains(&cy),
            "taskbar glyph centre-y {cy:.3} should sit just above centre (0.44..0.50)"
        );
    }

    /// The native window and hover-preview icons use this geometry independently of
    /// Explorer's registered application-group icon.
    #[cfg(target_os = "windows")]
    #[test]
    fn taskbar_hicon_matches_the_ico_geometry() {
        let size = 256_u32;
        let taskbar = windows_taskbar_icon_image(IconTheme::Light, DEFAULT_ICON_ACCENT, size);
        let (w, h, cx, cy) = glyph_bounds(taskbar.rgba(), size);

        assert_matches_tray_ratios("taskbar hicon", &bar_height_ratios(taskbar.rgba(), size));

        // Deliberately taller and squarer than the tray, which is 65.6% x 50.0%.
        assert!(
            (0.70..=0.74).contains(&w),
            "taskbar hicon width {w:.3} outside 0.70..0.74"
        );
        assert!(
            (0.68..=0.72).contains(&h),
            "taskbar hicon height {h:.3} outside 0.68..0.72"
        );
        assert!(
            (cx - 0.5).abs() <= 0.02,
            "taskbar hicon not horizontally centred: cx {cx:.3}"
        );
        assert!(
            (0.44..=0.50).contains(&cy),
            "taskbar hicon centre-y {cy:.3} sits too low (was 0.555 when shared with the tray)"
        );
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn runtime_ico_contains_all_native_windows_sizes() {
        let ico = runtime_ico_bytes(IconTheme::Dark, DEFAULT_ICON_ACCENT, &FULL_ICO_SIZES).unwrap();
        assert_eq!(u16::from_le_bytes([ico[4], ico[5]]), 15);
        let sizes: Vec<u16> = (0..15)
            .map(|index| {
                let encoded = ico[6 + index * 16];
                if encoded == 0 {
                    256
                } else {
                    u16::from(encoded)
                }
            })
            .collect();
        assert_eq!(
            sizes,
            [16, 18, 20, 22, 24, 26, 28, 30, 32, 40, 48, 64, 96, 128, 256]
        );
    }

    /// The Windows micro glyph must occupy the tiny native surface instead of
    /// collapsing into one-pixel sticks — while keeping the classic tray
    /// envelope (roughly 2:1 tile-to-glyph margins, not edge to edge).
    /// Runs on Linux CI too: the fallback delegates to this same micro
    /// renderer under cfg(test), so both platforms validate identical bytes.
    #[test]
    fn tray_geometry_is_optically_sized() {
        let (w, h, cx, cy) = glyph_bounds(
            runtime_tray_icon_image(IconTheme::Dark, DEFAULT_ICON_ACCENT, 20).rgba(),
            20,
        );
        assert!((0.64..=0.76).contains(&w), "micro glyph width: {w:.3}");
        assert!((0.46..=0.54).contains(&h), "micro glyph height: {h:.3}");
        assert!((cx - 0.5).abs() <= 0.03, "micro centre-x: {cx:.3}");
        assert!((0.48..=0.52).contains(&cy), "micro centre-y: {cy:.3}");
    }
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

const DEFAULT_ICON_ACCENT: [u8; 4] = [217, 119, 87, 255];

fn parse_icon_accent(value: Option<&str>) -> [u8; 4] {
    let Some(hex) = value.and_then(|value| value.strip_prefix('#')) else {
        return DEFAULT_ICON_ACCENT;
    };
    if hex.len() != 6 || !hex.is_ascii() {
        return DEFAULT_ICON_ACCENT;
    }
    let parse = |range| u8::from_str_radix(&hex[range], 16).ok();
    match (parse(0..2), parse(2..4), parse(4..6)) {
        (Some(r), Some(g), Some(b)) => [r, g, b, 255],
        _ => DEFAULT_ICON_ACCENT,
    }
}

fn resolve_icon_accent(app: &AppHandle) -> [u8; 4] {
    let value = crate::data::store::settings_handle(app)
        .ok()
        .and_then(|settings| settings.get(crate::data::store::ACCENT_COLOR));
    parse_icon_accent(value.as_ref().and_then(serde_json::Value::as_str))
}

#[cfg(test)]
mod icon_accent_tests {
    use super::{
        cached_icon_art, parse_icon_accent, runtime_icon_image, setting_updates_runtime_icons,
        IconTheme, DEFAULT_ICON_ACCENT,
    };

    fn pixel(image: &tauri::image::Image<'_>, size: u32, x: u32, y: u32) -> [u8; 4] {
        let offset = ((y * size + x) * 4) as usize;
        image.rgba()[offset..offset + 4].try_into().unwrap()
    }

    #[test]
    fn accent_defaults_to_verenu_orange() {
        assert_eq!(parse_icon_accent(None), DEFAULT_ICON_ACCENT);
        assert_eq!(parse_icon_accent(Some("invalid")), DEFAULT_ICON_ACCENT);
    }

    #[test]
    fn accent_accepts_six_digit_hex_in_either_case() {
        assert_eq!(parse_icon_accent(Some("#5B8CFF")), [91, 140, 255, 255]);
        assert_eq!(parse_icon_accent(Some("#a04fd8")), [160, 79, 216, 255]);
    }

    #[test]
    fn runtime_icon_keeps_theme_background_and_uses_custom_accent() {
        let accent = [91, 140, 255, 255];
        let light = runtime_icon_image(IconTheme::Light, accent, 128);
        let dark = runtime_icon_image(IconTheme::Dark, accent, 128);

        assert_eq!(pixel(&light, 128, 64, 24), [249, 247, 243, 255]);
        assert_eq!(pixel(&dark, 128, 64, 24), [20, 17, 14, 255]);
        assert_eq!(pixel(&light, 128, 64, 55), accent);
        assert_eq!(pixel(&dark, 128, 64, 55), accent);
    }

    #[test]
    fn appearance_and_accent_settings_refresh_runtime_icons() {
        assert!(setting_updates_runtime_icons("appearance_mode"));
        assert!(setting_updates_runtime_icons("accent_color"));
        assert!(!setting_updates_runtime_icons("default_tone"));
    }

    #[test]
    fn icon_art_cache_reuses_bytes_for_unchanged_inputs() {
        let first = cached_icon_art(IconTheme::Dark, DEFAULT_ICON_ACCENT, 20);
        let second = cached_icon_art(IconTheme::Dark, DEFAULT_ICON_ACCENT, 20);
        assert_eq!(first.window_rgba, second.window_rgba);
        assert_eq!(first.tray_rgba, second.tray_rgba);
        // A different tray size (e.g. after a DPI change) must re-render at
        // the new size, not serve the old size's bytes.
        let other_size = cached_icon_art(IconTheme::Dark, DEFAULT_ICON_ACCENT, 24);
        assert_eq!(other_size.tray_rgba.len(), 24 * 24 * 4);
        assert_eq!(other_size.window_rgba, first.window_rgba);
    }
}

pub(crate) fn appearance_mode(app: &AppHandle) -> Option<String> {
    crate::data::store::settings_handle(app)
        .ok()
        .and_then(|settings| settings.get(crate::data::store::APPEARANCE_MODE))
        .and_then(|value| value.as_str().map(String::from))
}

fn runtime_icon_image(
    theme: IconTheme,
    accent: [u8; 4],
    size: u32,
) -> tauri::image::Image<'static> {
    let mut rgba = vec![0_u8; (size * size * 4) as usize];
    let background = match theme {
        IconTheme::Light => [249, 247, 243, 255],
        IconTheme::Dark => [20, 17, 14, 255],
    };

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
    if value == 0 {
        0
    } else {
        ((value * size) / 512).max(1)
    }
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
