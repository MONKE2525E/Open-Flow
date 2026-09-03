//! Thin wrapper over tauri-plugin-notification for app-level system
//! notifications. Keeping these calls in Rust gives Windows notifications
//! Verenu's native app identity instead of the WebView host process.

use tauri::AppHandle;
#[cfg(windows)]
use tauri::Emitter;
use tauri_plugin_notification::NotificationExt;

#[cfg(windows)]
use std::sync::atomic::{AtomicBool, Ordering};

#[cfg(windows)]
const WINDOWS_DEV_APP_ID: &str = "com.verenu.app.dev";

#[cfg(windows)]
static WINDOWS_DEV_IDENTITY_READY: AtomicBool = AtomicBool::new(false);

#[cfg(all(windows, debug_assertions))]
static WINDOWS_DEV_SHORTCUT_ICON: std::sync::Mutex<Option<std::path::PathBuf>> =
    std::sync::Mutex::new(None);

#[derive(Clone, Copy)]
enum NotificationDestination {
    Home,
    Models,
}

impl NotificationDestination {
    #[cfg(windows)]
    const fn event_payload(self) -> &'static str {
        match self {
            Self::Home => "home",
            Self::Models => "models",
        }
    }
}

fn show(
    app: &AppHandle,
    title: &str,
    body: impl Into<String>,
    destination: NotificationDestination,
) -> Result<(), String> {
    let body = body.into();

    #[cfg(not(windows))]
    let _ = destination;

    #[cfg(windows)]
    {
        if let Err(err) = show_windows(app, title, &body, destination) {
            log::debug!("notify: {title} native notification failed: {err}");
        } else {
            return Ok(());
        }
    }

    let result = app.notification().builder().title(title).body(body).show();
    if let Err(err) = result {
        log::debug!("notify: {title} notification failed: {err}");
        return Err(err.to_string());
    }
    Ok(())
}

/// Raises a download-complete system notification from the backend so it can
/// arrive while the main window is hidden.
pub fn notify_model_download_complete(app: &AppHandle, model_name: &str) {
    let _ = show(
        app,
        "Model ready",
        format!("{model_name} finished downloading and is ready to use."),
        NotificationDestination::Models,
    );
}

pub fn notify_update_available(app: &AppHandle, version: &str) -> Result<(), String> {
    show(
        app,
        "Verenu update available",
        format!("Version v{version} is ready. Open Verenu to update."),
        NotificationDestination::Home,
    )
}

pub fn notify_provider_and_global_message(
    app: &AppHandle,
    provider_summary: &str,
    global_message: &str,
) -> Result<(), String> {
    let body = match (provider_summary.is_empty(), global_message.is_empty()) {
        (false, false) => format!("{provider_summary}\n\n{global_message}"),
        (false, true) => provider_summary.to_owned(),
        (true, false) => global_message.to_owned(),
        (true, true) => return Err("Notification message is empty.".to_owned()),
    };
    show(
        app,
        "Verenu service notice",
        body,
        NotificationDestination::Home,
    )
}

/// Sends a selected notification example through the same native path used by
/// background notifications.
pub fn notify_test_notification(app: &AppHandle, notification_type: &str) -> Result<(), String> {
    match notification_type {
        "update" => notify_update_available(app, "0.16.1"),
        "model" => show(
            app,
            "Model ready",
            "Parakeet finished downloading and is ready to use.",
            NotificationDestination::Models,
        ),
        "service" => notify_provider_and_global_message(
            app,
            "Groq: Some requests may be delayed or unavailable.",
            "Verenu has an important service update.",
        ),
        other => Err(format!("Unknown notification test type: {other}")),
    }
}

#[cfg(windows)]
fn show_windows(
    app: &AppHandle,
    title: &str,
    body: &str,
    destination: NotificationDestination,
) -> Result<(), String> {
    let app_id = if WINDOWS_DEV_IDENTITY_READY.load(Ordering::Acquire) {
        WINDOWS_DEV_APP_ID
    } else {
        "com.verenu.app"
    };
    let mut notification = notify_rust::Notification::new();
    notification.summary(title).body(body).app_id(app_id);

    let handle = notification.show().map_err(|err| err.to_string())?;
    let app = app.clone();
    let destination = destination.event_payload();
    std::thread::spawn(move || {
        if let Err(err) =
            handle.wait_for_response(|response: &notify_rust::NotificationResponse| {
                if response.is_default_action() {
                    crate::show_main_window(&app);
                    if let Err(err) = app.emit("verenu:notification-clicked", destination) {
                        log::debug!("notify: could not emit notification click event: {err}");
                    }
                }
            })
        {
            log::debug!("notify: notification click listener failed: {err}");
        }
    });
    Ok(())
}

/// Prepare a real Windows app identity for development notifications.
///
/// Windows toast notifications from unpackaged development executables need a
/// Start Menu shortcut with an AppUserModelId. Without that shortcut, the
/// Windows notification stack labels the toast as Windows PowerShell.
#[cfg(target_os = "windows")]
pub fn prepare_windows_notification_identity(themed_icon: &std::path::Path) {
    #[cfg(all(windows, debug_assertions))]
    match sync_windows_dev_shortcut_icon(themed_icon) {
        Ok(()) => WINDOWS_DEV_IDENTITY_READY.store(true, Ordering::Release),
        Err(err) => log::warn!("Could not register the Windows notification identity: {err}"),
    }

    #[cfg(not(all(windows, debug_assertions)))]
    let _ = themed_icon;
}

#[cfg(target_os = "windows")]
pub fn refresh_windows_notification_identity(themed_icon: &std::path::Path) {
    #[cfg(debug_assertions)]
    if let Err(err) = sync_windows_dev_shortcut_icon(themed_icon) {
        log::warn!("Could not refresh the Windows development shortcut icon: {err}");
    }

    #[cfg(not(debug_assertions))]
    let _ = themed_icon;
}

#[cfg(all(windows, debug_assertions))]
fn sync_windows_dev_shortcut_icon(themed_icon: &std::path::Path) -> Result<(), String> {
    let mut current = WINDOWS_DEV_SHORTCUT_ICON
        .lock()
        .map_err(|_| "development shortcut icon lock was poisoned".to_owned())?;
    if current.as_deref() == Some(themed_icon) {
        return Ok(());
    }
    install_windows_dev_shortcut(themed_icon)?;
    *current = Some(themed_icon.to_path_buf());
    Ok(())
}

#[cfg(all(windows, debug_assertions))]
fn install_windows_dev_shortcut(themed_icon: &std::path::Path) -> Result<(), String> {
    use std::mem::ManuallyDrop;
    use std::path::PathBuf;
    use windows::core::{Interface, GUID, PCWSTR, PWSTR};
    use windows::Win32::Foundation::RPC_E_CHANGED_MODE;
    use windows::Win32::Storage::EnhancedStorage::PKEY_AppUserModel_ID;
    use windows::Win32::Storage::FileSystem::WIN32_FIND_DATAW;
    use windows::Win32::System::Com::StructuredStorage::{
        PropVariantClear, PROPVARIANT, PROPVARIANT_0, PROPVARIANT_0_0, PROPVARIANT_0_0_0,
    };
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoTaskMemAlloc, CoUninitialize, IPersistFile,
        CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, STGM_READ,
    };
    use windows::Win32::System::Variant::VT_LPWSTR;
    use windows::Win32::UI::Shell::PropertiesSystem::IPropertyStore;
    use windows::Win32::UI::Shell::{IShellLinkW, SHChangeNotify, SHCNE_UPDATEITEM, SHCNF_PATHW};

    const CLSID_SHELL_LINK: GUID = GUID::from_u128(0x00021401_0000_0000_c000_000000000046);

    fn wide(value: impl AsRef<std::ffi::OsStr>) -> Vec<u16> {
        use std::os::windows::ffi::OsStrExt;
        value
            .as_ref()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect()
    }

    fn shortcut_path() -> Result<PathBuf, String> {
        let app_data =
            std::env::var_os("APPDATA").ok_or_else(|| "APPDATA is not available".to_owned())?;
        Ok(PathBuf::from(app_data)
            .join("Microsoft")
            .join("Windows")
            .join("Start Menu")
            .join("Programs")
            .join("Verenu Development.lnk"))
    }

    fn app_id_propvariant() -> Result<PROPVARIANT, String> {
        let value: Vec<u16> = WINDOWS_DEV_APP_ID
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        let bytes = value.len() * std::mem::size_of::<u16>();
        let ptr = unsafe { CoTaskMemAlloc(bytes) as *mut u16 };
        if ptr.is_null() {
            return Err("CoTaskMemAlloc failed".to_owned());
        }
        unsafe { std::ptr::copy_nonoverlapping(value.as_ptr(), ptr, value.len()) };

        Ok(PROPVARIANT {
            Anonymous: PROPVARIANT_0 {
                Anonymous: ManuallyDrop::new(PROPVARIANT_0_0 {
                    vt: VT_LPWSTR,
                    wReserved1: 0,
                    wReserved2: 0,
                    wReserved3: 0,
                    Anonymous: PROPVARIANT_0_0_0 {
                        pwszVal: PWSTR(ptr),
                    },
                }),
            },
        })
    }

    let shortcut = shortcut_path()?;
    let exe = std::env::current_exe().map_err(|err| err.to_string())?;
    if let Some(parent) = shortcut.parent() {
        std::fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }

    let initialized = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
    let should_uninitialize = if initialized.is_ok() {
        true
    } else if initialized == RPC_E_CHANGED_MODE {
        // COM was already initialized on this thread with MTA. COM calls are
        // still valid, but this call did not add a reference to release.
        false
    } else {
        return Err(format!("CoInitializeEx failed: {initialized:?}"));
    };

    let result = (|| {
        let shell_link: IShellLinkW = unsafe {
            CoCreateInstance(&CLSID_SHELL_LINK, None, CLSCTX_INPROC_SERVER)
                .map_err(|err| err.to_string())?
        };
        let exe_wide = wide(&exe);
        let icon_wide = wide(themed_icon);
        let description = wide("Verenu development notifications");
        unsafe {
            shell_link
                .SetPath(PCWSTR(exe_wide.as_ptr()))
                .map_err(|err| err.to_string())?;
            shell_link
                .SetDescription(PCWSTR(description.as_ptr()))
                .map_err(|err| err.to_string())?;
            shell_link
                .SetIconLocation(PCWSTR(icon_wide.as_ptr()), 0)
                .map_err(|err| err.to_string())?;
        }

        let property_store: IPropertyStore = shell_link.cast().map_err(|err| err.to_string())?;
        let mut app_id = app_id_propvariant()?;
        let result = unsafe {
            property_store
                .SetValue(&PKEY_AppUserModel_ID, &app_id)
                .and_then(|_| property_store.Commit())
                .map_err(|err| err.to_string())
        };
        unsafe {
            let _ = PropVariantClear(&mut app_id);
        }
        result?;

        let shortcut_wide = wide(&shortcut);
        let persist_file: IPersistFile = shell_link.cast().map_err(|err| err.to_string())?;
        unsafe {
            persist_file
                .Save(PCWSTR(shortcut_wide.as_ptr()), true)
                .map_err(|err| err.to_string())?;
        }

        let verify_link: IShellLinkW = unsafe {
            CoCreateInstance(&CLSID_SHELL_LINK, None, CLSCTX_INPROC_SERVER)
                .map_err(|err| err.to_string())?
        };
        let verify_persist: IPersistFile = verify_link.cast().map_err(|err| err.to_string())?;
        unsafe {
            verify_persist
                .Load(PCWSTR(shortcut_wide.as_ptr()), STGM_READ)
                .map_err(|err| err.to_string())?;
        }
        // Heap-allocated: two 32 KiB stack arrays would eat 128 KiB of stack
        // in one frame for a check that runs once per icon change.
        let mut target_buffer = vec![0_u16; 32768];
        let mut icon_buffer = vec![0_u16; 32768];
        let mut find_data = WIN32_FIND_DATAW::default();
        let mut icon_index = 0;
        unsafe {
            verify_link
                .GetPath(&mut target_buffer, &mut find_data, 0)
                .map_err(|err| err.to_string())?;
            verify_link
                .GetIconLocation(&mut icon_buffer, &mut icon_index)
                .map_err(|err| err.to_string())?;
        }
        let from_wide = |buffer: &[u16]| {
            String::from_utf16_lossy(
                &buffer[..buffer.iter().position(|c| *c == 0).unwrap_or(buffer.len())],
            )
        };
        let saved_target = from_wide(&target_buffer);
        let saved_icon = from_wide(&icon_buffer);
        let verify_store: IPropertyStore = verify_link.cast().map_err(|err| err.to_string())?;
        let app_id = unsafe {
            let mut value = verify_store
                .GetValue(&PKEY_AppUserModel_ID)
                .map_err(|err| err.to_string())?;
            let result = if value.Anonymous.Anonymous.vt == VT_LPWSTR {
                value
                    .Anonymous
                    .Anonymous
                    .Anonymous
                    .pwszVal
                    .to_string()
                    .unwrap_or_default()
            } else {
                String::new()
            };
            let _ = PropVariantClear(&mut value);
            result
        };
        log::info!(
            "Windows development shortcut saved: target={saved_target}, icon={saved_icon},{icon_index}, aumid={app_id}"
        );
        // The shell may echo the icon path back with different casing or
        // separators than the path we set, so compare a normalized form
        // (backslash separators, case-insensitive) rather than exact bytes.
        let normalize_icon_path = |path: &std::path::Path| {
            path.as_os_str()
                .to_string_lossy()
                .replace('/', "\\")
                .to_lowercase()
        };
        if normalize_icon_path(std::path::Path::new(&saved_icon))
            != normalize_icon_path(themed_icon)
            || app_id != WINDOWS_DEV_APP_ID
        {
            return Err(format!(
                "development shortcut verification failed: expected icon={} and aumid={WINDOWS_DEV_APP_ID}, got icon={saved_icon} and aumid={app_id}",
                themed_icon.display()
            ));
        }
        unsafe {
            SHChangeNotify(
                SHCNE_UPDATEITEM,
                SHCNF_PATHW,
                Some(shortcut_wide.as_ptr().cast()),
                None,
            );
        }
        Ok(())
    })();

    if should_uninitialize {
        unsafe { CoUninitialize() };
    }
    result
}
