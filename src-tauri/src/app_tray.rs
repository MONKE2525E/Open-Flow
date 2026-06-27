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
    let permissions_i =
        MenuItem::with_id(app, "permissions", "Permissions...", true, None::<&str>)?;
    let settings_i = MenuItem::with_id(app, "settings", "Settings...", true, None::<&str>)?;
    let sep = PredefinedMenuItem::separator(app)?;
    let relaunch_i = MenuItem::with_id(app, "relaunch", "Relaunch", true, None::<&str>)?;
    let quit_i = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
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
            "relaunch" => app.restart(),
            "quit" => app.exit(0),
            _ => {}
        })
        .build(app)?;

    apply_runtime_icons(app.handle(), None);

    Ok(())
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
pub(crate) fn apply_native_main_window_chrome(app: &AppHandle, theme_hint: Option<Theme>) {
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
        (76, 302, 56, 98, 28),
        (152, 204, 56, 196, 28),
        (228, 120, 56, 280, 28),
        (304, 246, 56, 154, 28),
        (380, 330, 56, 70, 28),
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
