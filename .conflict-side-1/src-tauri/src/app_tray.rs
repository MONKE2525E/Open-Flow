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
        IconTheme::Dark => *DARK_REAL.get_or_init(|| {
            make_hicon(windows_taskbar_icon_image(IconTheme::Dark, 256).rgba(), 256)
        }),
        IconTheme::Light => *LIGHT_REAL.get_or_init(|| {
            make_hicon(
                windows_taskbar_icon_image(IconTheme::Light, 256).rgba(),
                256,
            )
        }),
    };
    (transparent, real)
}

/// The taskbar/Alt+Tab logo, drawn separately from [`runtime_icon_image`] (which the
/// tray uses) so the two can be scaled independently.
///
/// They were the same function, and that made the taskbar unfixable without breaking
/// the tray: the tray's glyph is tuned for a dark tile that blends into the shell,
/// where it reads as correctly inset, but the same proportions in the taskbar's cream
/// tile look small, flat, and low. Verified by capturing the live taskbar: the shell
/// renders THIS icon (via `WM_SETICON`/`ICON_BIG`), not the exe's embedded `icon.ico`,
/// because `ICON_SMALL`/`ICON_SMALL2` are deliberately transparent here.
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
fn windows_taskbar_icon_image(theme: IconTheme, size: u32) -> tauri::image::Image<'static> {
    let mut rgba = vec![0_u8; (size * size * 4) as usize];
    let background = match theme {
        IconTheme::Light => [249, 247, 243, 255],
        IconTheme::Dark => [20, 17, 14, 255],
    };
    let accent = [217, 119, 87, 255];

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

#[cfg(all(test, not(target_os = "macos")))]
mod windows_icon_tests {
    #[cfg(target_os = "windows")]
    use super::windows_taskbar_icon_image;
    use super::{runtime_tray_icon_image, IconTheme};

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
        let tray = bar_height_ratios(runtime_tray_icon_image(IconTheme::Dark, 256).rgba(), 256);
        assert_eq!(
            actual.len(),
            tray.len(),
            "{label}: expected {} bars, found {}",
            tray.len(),
            actual.len()
        );
        for (i, (a, t)) in actual.iter().zip(tray.iter()).enumerate() {
            assert!(
                (a - t).abs() <= 0.03,
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

    /// The shell draws the taskbar button of a RUNNING window from `WM_SETICON`/`ICON_BIG`,
    /// i.e. from [`windows_taskbar_icon_image`] — NOT from `icon.ico` (confirmed by capturing
    /// the live taskbar and matching its centre-y against both candidates). So this is the
    /// function whose geometry actually decides how the taskbar looks while Verenu is open,
    /// and it must stay in step with the `.ico` above, or the icon visibly changes shape the
    /// moment the app starts.
    #[cfg(target_os = "windows")]
    #[test]
    fn taskbar_hicon_matches_the_ico_geometry() {
        let size = 256_u32;
        let taskbar = windows_taskbar_icon_image(IconTheme::Light, size);
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

    /// The tray is deliberately frozen; this pins its rendered geometry so a future
    /// taskbar tweak cannot silently move it again.
    #[test]
    fn tray_geometry_is_unchanged() {
        let (w, h, cx, cy) = glyph_bounds(runtime_tray_icon_image(IconTheme::Dark, 32).rgba(), 32);
        assert!((w - 0.656).abs() < 0.02, "tray glyph width changed: {w:.3}");
        assert!(
            (h - 0.500).abs() < 0.02,
            "tray glyph height changed: {h:.3}"
        );
        assert!((cx - 0.484).abs() < 0.02, "tray centre-x changed: {cx:.3}");
        assert!((cy - 0.531).abs() < 0.02, "tray centre-y changed: {cy:.3}");
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
