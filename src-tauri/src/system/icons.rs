#![allow(dead_code)]

//! Native app-icon extraction, disk-cached as PNG data URIs. Best-effort
//! throughout: any resolution/extraction failure returns `None` so callers
//! (the Contexts UI) fall back to a colored-initial badge — this is a
//! cosmetic feature and must never surface an error or block a command.

use std::path::PathBuf;

#[cfg(windows)]
pub fn get_icon_data_uri(app: &tauri::AppHandle, exe: &str) -> Option<String> {
    let exe = exe.trim().to_lowercase();
    let cache_path = cache_file_path(app, &exe)?;
    if let Ok(bytes) = std::fs::read(&cache_path) {
        return png_bytes_to_data_uri(&bytes);
    }
    let full_path = win::resolve_exe_full_path(&exe)?;
    let png = win::extract_icon_png(&full_path)?;
    if let Some(parent) = cache_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(&cache_path, &png);
    png_bytes_to_data_uri(&png)
}

#[cfg(target_os = "macos")]
pub fn get_icon_data_uri(app: &tauri::AppHandle, exe: &str) -> Option<String> {
    let exe = exe.trim().to_lowercase();
    let cache_path = cache_file_path(app, &exe)?;
    if let Ok(bytes) = std::fs::read(&cache_path) {
        return png_bytes_to_data_uri(&bytes);
    }
    let bundle_path = mac::resolve_app_bundle_path(&exe)?;
    let png = mac::extract_icon_png(&bundle_path)?;
    if let Some(parent) = cache_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(&cache_path, &png);
    png_bytes_to_data_uri(&png)
}

#[cfg(not(any(windows, target_os = "macos")))]
pub fn get_icon_data_uri(_app: &tauri::AppHandle, _exe: &str) -> Option<String> {
    None
}

/// Encodes PNG bytes as a data URI, rejecting anything that isn't a PNG.
/// Guards against truncated cache files (e.g. an interrupted write): serving
/// those would render a permanently broken image in the webview instead of
/// the fallback glyph.
fn png_bytes_to_data_uri(bytes: &[u8]) -> Option<String> {
    use base64::Engine;
    if bytes.len() < 8 || !bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return None;
    }
    Some(format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    ))
}

fn cache_file_path(app: &tauri::AppHandle, exe: &str) -> Option<PathBuf> {
    use tauri::Manager;
    let dir = app.path().app_data_dir().ok()?.join("icon-cache");
    // FNV-1a: stable, dependency-free filename hash — not a security context.
    let mut hash: u64 = 0xcbf29ce484222325;
    for b in exe.as_bytes() {
        hash ^= *b as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    Some(dir.join(format!("{hash:016x}.png")))
}

#[cfg(windows)]
mod win {
    #![allow(unknown_lints, clippy::chunks_exact_to_as_chunks)]
    use crate::system::apps::{parse_exe_from_icon, reg_read_string};
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::Graphics::Gdi::{
        CreateCompatibleDC, DeleteDC, DeleteObject, GetDIBits, GetObjectW, BITMAP, BITMAPINFO,
        BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, HGDIOBJ,
    };
    use windows::Win32::System::Registry::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};
    use windows::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_QUERY_LIMITED_INFORMATION,
    };
    use windows::Win32::UI::Shell::ExtractIconExW;
    use windows::Win32::UI::WindowsAndMessaging::{DestroyIcon, GetIconInfo, ICONINFO};

    /// Resolves a bare exe filename (e.g. "chrome.exe") to a full path,
    /// checking the registry Uninstall entries first (works for installed,
    /// non-running apps) and falling back to a currently-running process
    /// with that name.
    pub fn resolve_exe_full_path(exe: &str) -> Option<String> {
        for (root, path) in [
            (
                HKEY_LOCAL_MACHINE,
                "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Uninstall",
            ),
            (
                HKEY_LOCAL_MACHINE,
                "SOFTWARE\\WOW6432Node\\Microsoft\\Windows\\CurrentVersion\\Uninstall",
            ),
            (
                HKEY_CURRENT_USER,
                "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Uninstall",
            ),
        ] {
            if let Some(full_path) = find_registry_icon_path(root, path, exe) {
                return Some(full_path);
            }
        }
        resolve_running_process_path(exe)
    }

    fn find_registry_icon_path(
        root: windows::Win32::System::Registry::HKEY,
        path: &str,
        exe: &str,
    ) -> Option<String> {
        use windows::core::{PCWSTR, PWSTR};
        use windows::Win32::System::Registry::{
            RegCloseKey, RegEnumKeyExW, RegOpenKeyExW, KEY_READ,
        };

        let path_wide: Vec<u16> = path.encode_utf16().chain(std::iter::once(0)).collect();
        let mut hbase = windows::Win32::System::Registry::HKEY::default();
        unsafe {
            if RegOpenKeyExW(
                root,
                PCWSTR::from_raw(path_wide.as_ptr()),
                None,
                KEY_READ,
                &mut hbase,
            )
            .is_err()
            {
                return None;
            }

            let mut index = 0u32;
            let found = loop {
                let mut name_buf = [0u16; 256];
                let mut name_len = 255u32;
                let r = RegEnumKeyExW(
                    hbase,
                    index,
                    Some(PWSTR::from_raw(name_buf.as_mut_ptr())),
                    &mut name_len,
                    None,
                    Some(PWSTR::null()),
                    None,
                    None,
                );
                if r.is_err() {
                    break None;
                }
                let subkey_name = String::from_utf16_lossy(&name_buf[..name_len as usize]);
                let subkey_wide: Vec<u16> = subkey_name
                    .encode_utf16()
                    .chain(std::iter::once(0))
                    .collect();
                let mut hsubkey = windows::Win32::System::Registry::HKEY::default();
                let mut matched = None;
                if RegOpenKeyExW(
                    hbase,
                    PCWSTR::from_raw(subkey_wide.as_ptr()),
                    None,
                    KEY_READ,
                    &mut hsubkey,
                )
                .is_ok()
                {
                    if let Some(icon) = reg_read_string(hsubkey, "DisplayIcon") {
                        if parse_exe_from_icon(&icon).as_deref() == Some(exe) {
                            matched = icon
                                .trim()
                                .trim_matches('"')
                                .split(',')
                                .next()
                                .map(|s| s.trim().trim_matches('"').to_string());
                        }
                    }
                    let _ = RegCloseKey(hsubkey).ok();
                }
                if matched.is_some() {
                    break matched;
                }
                index += 1;
            };
            let _ = RegCloseKey(hbase).ok();
            found
        }
    }

    fn resolve_running_process_path(exe: &str) -> Option<String> {
        use windows::Win32::System::Diagnostics::ToolHelp::{
            CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
            TH32CS_SNAPPROCESS,
        };
        unsafe {
            let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0).ok()?;
            let mut entry: PROCESSENTRY32W = std::mem::zeroed();
            entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
            let mut pid = None;
            if Process32FirstW(snap, &mut entry).is_ok() {
                loop {
                    let raw = &entry.szExeFile;
                    let end = raw.iter().position(|&c| c == 0).unwrap_or(raw.len());
                    let name = String::from_utf16_lossy(&raw[..end]).to_lowercase();
                    if name == exe {
                        pid = Some(entry.th32ProcessID);
                        break;
                    }
                    if Process32NextW(snap, &mut entry).is_err() {
                        break;
                    }
                }
            }
            let _ = CloseHandle(snap);
            let pid = pid?;
            let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
            let mut buffer = [0u16; 260];
            let mut size = buffer.len() as u32;
            let ok = QueryFullProcessImageNameW(
                handle,
                windows::Win32::System::Threading::PROCESS_NAME_FORMAT(0),
                windows::core::PWSTR::from_raw(buffer.as_mut_ptr()),
                &mut size,
            )
            .is_ok();
            let _ = CloseHandle(handle);
            if !ok {
                return None;
            }
            Some(String::from_utf16_lossy(&buffer[..size as usize]))
        }
    }

    /// Extracts the large icon from `full_path` (index 0) and encodes it as
    /// PNG bytes.
    pub fn extract_icon_png(full_path: &str) -> Option<Vec<u8>> {
        let path_wide: Vec<u16> = full_path.encode_utf16().chain(std::iter::once(0)).collect();
        unsafe {
            let mut large_icons = [windows::Win32::UI::WindowsAndMessaging::HICON::default(); 1];
            let extracted = ExtractIconExW(
                windows::core::PCWSTR::from_raw(path_wide.as_ptr()),
                0,
                Some(large_icons.as_mut_ptr()),
                None,
                1,
            );
            if extracted == 0 || large_icons[0].is_invalid() {
                return None;
            }
            let icon = large_icons[0];
            let png = icon_to_png(icon);
            let _ = DestroyIcon(icon);
            png
        }
    }

    unsafe fn icon_to_png(icon: windows::Win32::UI::WindowsAndMessaging::HICON) -> Option<Vec<u8>> {
        let mut info = ICONINFO::default();
        GetIconInfo(icon, &mut info).ok()?;

        let result = (|| {
            let mut bmp = BITMAP::default();
            let bytes = GetObjectW(
                HGDIOBJ(info.hbmColor.0),
                std::mem::size_of::<BITMAP>() as i32,
                Some(&mut bmp as *mut _ as *mut _),
            );
            if bytes == 0 {
                return None;
            }
            let width = bmp.bmWidth;
            let height = bmp.bmHeight;
            if width <= 0 || height <= 0 {
                return None;
            }

            let dc = CreateCompatibleDC(None);
            if dc.is_invalid() {
                return None;
            }
            let mut bmi = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: width,
                    biHeight: -height, // negative = top-down DIB
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB.0,
                    ..Default::default()
                },
                ..Default::default()
            };
            let mut pixels = vec![0u8; (width as usize) * (height as usize) * 4];
            let scanlines = GetDIBits(
                dc,
                info.hbmColor,
                0,
                height as u32,
                Some(pixels.as_mut_ptr() as *mut _),
                &mut bmi,
                DIB_RGB_COLORS,
            );
            let _ = DeleteDC(dc);
            if scanlines == 0 {
                return None;
            }

            // GetDIBits returns BGRA; PNG/image crate wants RGBA.
            for px in pixels.chunks_exact_mut(4) {
                px.swap(0, 2);
            }

            let image = image::RgbaImage::from_raw(width as u32, height as u32, pixels)?;
            let mut png = Vec::new();
            image::DynamicImage::ImageRgba8(image)
                .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
                .ok()?;
            Some(png)
        })();

        let _ = DeleteObject(HGDIOBJ(info.hbmMask.0));
        if !info.hbmColor.is_invalid() {
            let _ = DeleteObject(HGDIOBJ(info.hbmColor.0));
        }
        result
    }
}

#[cfg(target_os = "macos")]
mod mac {
    use objc2::rc::autoreleasepool;
    use objc2::runtime::AnyObject;
    use objc2::{class, msg_send, Encode, Encoding};
    use objc2_foundation::NSString;

    /// `CGSize` (two `CGFloat`s). Not exposed by the `objc2-foundation`
    /// feature set already enabled for this crate, so encode it by hand —
    /// same approach `mac_app.rs` uses for `NSApplicationActivationPolicy`.
    #[repr(C)]
    #[derive(Clone, Copy)]
    struct CGSize {
        width: f64,
        height: f64,
    }
    unsafe impl Encode for CGSize {
        const ENCODING: Encoding = Encoding::Struct("CGSize", &[f64::ENCODING, f64::ENCODING]);
    }

    /// Requested render size before rasterizing to PNG. NSWorkspace icons
    /// carry multiple representations; setting the image's size selects which
    /// one `TIFFRepresentation` rasterizes from.
    const ICON_SIZE: f64 = 128.0;

    /// NSBitmapImageFileTypePNG. `NSBitmapImageFileType` is `NSUInteger` in
    /// the modern SDK — objc2 enforces the signedness at the msg_send call
    /// site, so this must stay unsigned or every extraction throws.
    const NS_BITMAP_IMAGE_FILE_TYPE_PNG: usize = 4;

    /// Resolves a lowercased `"<name>.app"` target (the format
    /// `system::apps::list_installed_apps` produces on macOS) to the bundle's
    /// full path by scanning the same standard Applications directories.
    pub fn resolve_app_bundle_path(exe: &str) -> Option<String> {
        let mut dirs = vec![
            std::path::PathBuf::from("/Applications"),
            std::path::PathBuf::from("/Applications/Utilities"),
            std::path::PathBuf::from("/System/Applications"),
            std::path::PathBuf::from("/System/Applications/Utilities"),
        ];
        if let Ok(home) = std::env::var("HOME") {
            dirs.push(std::path::PathBuf::from(home).join("Applications"));
        }
        for dir in dirs {
            let Ok(entries) = std::fs::read_dir(&dir) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) != Some("app") {
                    continue;
                }
                let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
                    continue;
                };
                let stem = stem.to_lowercase();
                if stem == exe || format!("{stem}.app") == exe {
                    return path.to_str().map(str::to_string);
                }
            }
        }
        None
    }

    /// Extracts the app's icon at `bundle_path` via `NSWorkspace`, encodes it
    /// as PNG, and downsizes it to a sane on-disk/wire size.
    ///
    /// `-[NSImage setSize:]` does not constrain what `-TIFFRepresentation`
    /// rasterizes at — every icon comes back as the bundle's native 1024×1024
    /// representation regardless (measured: a 1024×1024 PNG runs 1.5–2.5MB).
    /// Sent as a data URI to the frontend for every row in the app picker,
    /// that made the picker unusable — dozens of multi-megabyte IPC payloads
    /// in flight at once, appearing to the user as icons that simply never
    /// load. Resizing here, before caching or returning, is the fix.
    pub fn extract_icon_png(bundle_path: &str) -> Option<Vec<u8>> {
        let raw = extract_icon_png_native(bundle_path)?;
        let resized = image::load_from_memory(&raw)
            .ok()?
            .resize(128, 128, image::imageops::FilterType::Lanczos3);
        let mut png = Vec::new();
        resized
            .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .ok()?;
        Some(png)
    }

    fn extract_icon_png_native(bundle_path: &str) -> Option<Vec<u8>> {
        autoreleasepool(|_| unsafe {
            let workspace: *mut AnyObject = msg_send![class!(NSWorkspace), sharedWorkspace];
            if workspace.is_null() {
                return None;
            }
            let path = NSString::from_str(bundle_path);
            let icon: *mut AnyObject = msg_send![workspace, iconForFile: &*path];
            if icon.is_null() {
                return None;
            }
            let _: () = msg_send![
                icon,
                setSize: CGSize { width: ICON_SIZE, height: ICON_SIZE }
            ];
            let tiff: *mut AnyObject = msg_send![icon, TIFFRepresentation];
            if tiff.is_null() {
                return None;
            }
            let bitmap: *mut AnyObject =
                msg_send![class!(NSBitmapImageRep), imageRepWithData: tiff];
            if bitmap.is_null() {
                return None;
            }
            let empty_props: *mut AnyObject = msg_send![class!(NSDictionary), dictionary];
            let png_data: *mut AnyObject = msg_send![
                bitmap,
                representationUsingType: NS_BITMAP_IMAGE_FILE_TYPE_PNG,
                properties: empty_props
            ];
            if png_data.is_null() {
                return None;
            }
            let length: usize = msg_send![png_data, length];
            if length == 0 {
                return None;
            }
            // `-bytes` is declared `const void *` in the SDK ('^v'); objc2
            // enforces the exact encoding, so `*const u8` ('*', a C string)
            // is rejected even though the bit pattern is identical.
            let ptr: *const std::ffi::c_void = msg_send![png_data, bytes];
            if ptr.is_null() {
                return None;
            }
            Some(std::slice::from_raw_parts(ptr.cast::<u8>(), length).to_vec())
        })
    }
}

// ---------- website favicons ----------

/// Fetched favicons are disk-cached alongside app icons. `None` is cached as a
/// zero-byte marker so a site with no reachable icon is not re-fetched on every
/// render — the frontend shows its globe fallback instead. Cache entries are
/// keyed by normalized hostname, so `https://mail.google.com/u/0` and
/// `mail.google.com` resolve to the same file.
const FAVICON_MAX_BYTES: usize = 256 * 1024;

/// Reduces a pasted URL, origin, or bare host to a lowercase hostname.
/// Mirrors `normalize_domain` in `data/db/contexts.rs`, minus the length
/// validation — this path is cosmetic and never rejects input.
pub fn normalize_favicon_host(input: &str) -> Option<String> {
    let trimmed = input.trim().to_lowercase();
    if trimmed.is_empty() {
        return None;
    }
    // Strip only the leading scheme. split("://").last() would latch onto a
    // nested URL in the query (e.g. "...?redirect=https://other.com").
    let without_scheme = match trimmed.split_once("://") {
        Some((_, rest)) => rest,
        None => &trimmed,
    };
    let host = without_scheme
        .split(['/', '?', '#'])
        .next()
        .unwrap_or(without_scheme);
    let host = host.split('@').next_back().unwrap_or(host);
    let host = host.split(':').next().unwrap_or(host);
    let host = host.trim_matches('.');
    if host.is_empty() || !host.contains('.') {
        return None;
    }
    Some(host.to_string())
}

/// Returns a `data:image/...;base64,...` URI for `domain`'s favicon, or `None`
/// when it can't be resolved. Never errors: this is decoration.
pub async fn get_site_icon_data_uri(app: &tauri::AppHandle, domain: &str) -> Option<String> {
    let host = normalize_favicon_host(domain)?;
    let cache_path = favicon_cache_path(app, &host)?;
    if let Ok(bytes) = std::fs::read(&cache_path) {
        // Zero bytes is the "we tried, there is nothing" marker.
        return if bytes.is_empty() {
            None
        } else {
            png_bytes_to_data_uri(&bytes)
        };
    }

    let (bytes, definitive) = fetch_favicon(&host).await;
    if let Some(parent) = cache_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    match bytes {
        // Validate before caching: a captive portal or proxy can hand back
        // HTML/ICO garbage, and a cached non-PNG file would lock that domain
        // out of retries forever.
        Some(bytes) => match png_bytes_to_data_uri(&bytes) {
            Some(uri) => {
                let _ = std::fs::write(&cache_path, &bytes);
                Some(uri)
            }
            None => None,
        },
        // Only definitive misses get the 0-byte negative marker. Transient
        // failures (offline, timeouts) stay unwritten so the next lookup
        // retries instead of being poisoned forever.
        None if definitive => {
            let _ = std::fs::write(&cache_path, []);
            None
        }
        None => None,
    }
}

/// Google's favicon service is used rather than guessing `/favicon.ico`,
/// because most sites declare their icon in HTML rather than serving it from a
/// predictable path. Only the bare hostname leaves the machine, and only once
/// per site — the result is cached on disk from then on.
///
/// Returns the PNG bytes plus whether the outcome is definitive: `false`
/// marks a transient failure that must not be negative-cached.
async fn fetch_favicon(host: &str) -> (Option<Vec<u8>>, bool) {
    let url = format!(
        "https://www.google.com/s2/favicons?sz=64&domain={}",
        urlencoding_host(host)
    );
    let response = match crate::api::client::get()
        .get(&url)
        .timeout(std::time::Duration::from_secs(8))
        .send()
        .await
    {
        Ok(response) => response,
        Err(_) => return (None, false),
    };
    if !response.status().is_success() {
        // Resolver-level trouble; a genuinely unknown domain still gets a 200
        // with the fallback glyph, so treat this as transient.
        return (None, false);
    }
    let bytes = match response.bytes().await {
        Ok(bytes) => bytes,
        Err(_) => return (None, false),
    };
    if bytes.is_empty() || bytes.len() > FAVICON_MAX_BYTES {
        return (None, true);
    }
    (Some(bytes.to_vec()), true)
}

/// Hostnames are already restricted to URL-safe characters by
/// `normalize_favicon_host`; anything else is dropped rather than escaped.
fn urlencoding_host(host: &str) -> String {
    host.chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-'))
        .collect()
}

fn favicon_cache_path(app: &tauri::AppHandle, host: &str) -> Option<PathBuf> {
    use tauri::Manager;
    let dir = app.path().app_data_dir().ok()?.join("favicon-cache");
    let mut hash: u64 = 0xcbf29ce484222325;
    for b in host.as_bytes() {
        hash ^= *b as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    Some(dir.join(format!("{hash:016x}.img")))
}

#[cfg(test)]
mod tests {
    use super::normalize_favicon_host;

    #[test]
    fn normalizes_urls_and_hosts_to_one_identity() {
        for input in [
            "mail.google.com",
            "MAIL.GOOGLE.COM",
            "https://mail.google.com/mail/u/0#inbox",
            "  http://user@mail.google.com:8080/x?y=1 ",
            "mail.google.com.",
        ] {
            assert_eq!(
                normalize_favicon_host(input).as_deref(),
                Some("mail.google.com"),
                "input: {input}"
            );
        }
        for input in ["", "   ", "localhost", "://"] {
            assert_eq!(normalize_favicon_host(input), None, "input: {input}");
        }
    }
}
