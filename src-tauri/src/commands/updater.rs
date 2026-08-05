//! Update check + in-app install, with pre-update DB backup.

use super::*;
#[cfg(any(target_os = "macos", windows))]
use tauri_plugin_shell::ShellExt;

// ---------- updates ----------

#[tauri::command]
pub async fn check_for_update(
    app: AppHandle,
) -> Result<Option<crate::api::updater::UpdateInfo>, String> {
    let channel = selected_update_channel(&app)?;

    match crate::api::updater::check(channel).await {
        Ok(update) => Ok(update),
        Err(e) => {
            log::warn!("Update check failed: {e}");
            Ok(None)
        }
    }
}

/// Developer-only reinstall path. It deliberately uses the same selected
/// stable/beta channel as normal update checks but does not require the chosen
/// release to be newer than the currently installed version.
#[tauri::command]
pub async fn reinstall_latest_update(app: AppHandle) -> Result<String, String> {
    let channel = selected_update_channel(&app)?;
    let update = crate::api::updater::latest_installable(channel)
        .await
        .map_err(|e| format!("Could not look up the latest release: {e}"))?
        .ok_or_else(|| "No compatible release asset is available for this device.".to_string())?;
    let version = update.version.clone();
    install_update(app, update.download_url).await?;
    Ok(version)
}

fn selected_update_channel(app: &AppHandle) -> Result<crate::api::updater::UpdateChannel, String> {
    let handle = store::settings_handle(app)?;
    let beta_enabled = handle
        .get(store::BETA_UPDATES_ENABLED)
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    Ok(if beta_enabled {
        crate::api::updater::UpdateChannel::Beta
    } else {
        crate::api::updater::UpdateChannel::Stable
    })
}

#[tauri::command]
pub async fn install_update(app: AppHandle, download_url: String) -> Result<(), String> {
    // Defense-in-depth: `download_url` ultimately originates from a GitHub
    // release asset (`browser_download_url`), but this command accepts it
    // straight from the frontend and either opens it (macOS) or downloads and
    // executes it (Windows). Refuse anything that isn't an official release
    // asset URL so a compromised/spoofed frontend can't turn this into an
    // arbitrary download-and-execute primitive.
    if !crate::api::updater::is_authorized_release_asset_url(&download_url) {
        return Err("Refusing to install update from an unauthorized URL.".into());
    }

    #[cfg(target_os = "macos")]
    {
        // `Shell::open` is soft-deprecated in favour of tauri-plugin-opener,
        // but the shell plugin is already the only one wired up here and
        // opening the verified release URL in the user's browser to start the
        // DMG download is exactly what we want. Allow the deprecation rather
        // than pulling in (and configuring capabilities for) another plugin.
        #[allow(deprecated)]
        app.shell()
            .open(download_url, None)
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    #[cfg(not(any(windows, target_os = "macos")))]
    {
        let _ = (&app, &download_url);
        Err("Updates are only supported on Windows and macOS.".into())
    }

    #[cfg(windows)]
    {
        use std::io::Write;
        use std::os::windows::process::CommandExt;

        // MSI packages are valid release assets but do not support NSIS's
        // `/S` switch. Let Windows download/open them normally instead of
        // pretending they can use the in-app silent installer.
        if !is_silent_nsis_setup_url(&download_url) {
            #[allow(deprecated)]
            app.shell()
                .open(download_url, None)
                .map_err(|e| e.to_string())?;
            return Ok(());
        }

        let db = app.state::<DbHandle>().inner().clone();

        let bytes = crate::api::client::get()
            .get(&download_url)
            .header("User-Agent", "verenu")
            .send()
            .await
            .and_then(|r| r.error_for_status())
            .map_err(|e| e.to_string())?
            .bytes()
            .await
            .map_err(|e| e.to_string())?;

        // Everything from here on is blocking file/registry/process I/O -
        // run it off the async executor so it can't stall other Tokio tasks
        // (audio capture, hotkey handling, etc.) while the installer stages.
        tokio::task::spawn_blocking(move || -> Result<(), String> {
            // Back up the database before touching anything. Derive the path from
            // the canonical app-data helper (NOT app.path().app_data_dir(), which
            // resolves against the bundle identifier and may point at a different
            // file).
            let db_path = crate::app_db_path();
            if db_path.exists() {
                let lock_result = db.lock();
                if let Ok(conn) = lock_result {
                    let backup_path = db_path.with_extension("db.bak");
                    let _ = backup_sqlite_database(&conn, &backup_path);
                }
            }

            let installer = unique_update_installer_path()?;
            let mut f = match std::fs::File::create(&installer) {
                Ok(file) => file,
                Err(error) => return Err(error.to_string()),
            };
            if let Err(error) = f.write_all(&bytes) {
                let _ = std::fs::remove_file(&installer);
                return Err(error.to_string());
            }
            drop(f);

            // Start a second copy of Verenu in a private helper mode. Unlike the
            // old `.cmd` launcher, this never invokes cmd.exe or PowerShell, and
            // it waits for both this process and the NSIS installer before it
            // relaunches the installed binary. The old handoff only slept for
            // two seconds, so it could reopen Verenu while files were still
            // being replaced.
            let current_exe = match std::env::current_exe() {
                Ok(exe) => exe,
                Err(error) => {
                    let _ = std::fs::remove_file(&installer);
                    return Err(error.to_string());
                }
            };
            // The helper cannot run from the installed binary itself: Windows
            // keeps the helper's image locked, so NSIS would wait forever while
            // trying to replace Verenu.exe. Run a temporary copy instead and
            // pass the real executable path back for the final relaunch.
            let helper_exe = match unique_update_helper_path() {
                Ok(path) => path,
                Err(error) => {
                    let _ = std::fs::remove_file(&installer);
                    return Err(error);
                }
            };
            if let Err(error) = std::fs::copy(&current_exe, &helper_exe) {
                let _ = std::fs::remove_file(&installer);
                let _ = std::fs::remove_file(&helper_exe);
                return Err(error.to_string());
            }
            let parent_pid = std::process::id().to_string();
            if let Err(err) = std::process::Command::new(&helper_exe)
                .arg("--apply-update")
                .arg("--update-installer")
                .arg(&installer)
                .arg("--update-parent-pid")
                .arg(parent_pid)
                .arg("--update-target")
                .arg(&current_exe)
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .creation_flags(CREATE_NO_WINDOW | DETACHED_PROCESS)
                .spawn()
            {
                let _ = std::fs::remove_file(&installer);
                let _ = std::fs::remove_file(&helper_exe);
                return Err(err.to_string());
            }

            Ok(())
        })
        .await
        .map_err(|e| e.to_string())??;

        // Exit immediately so the binary is free before the installer starts.
        std::process::exit(0)
    }
}

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;
#[cfg(windows)]
const DETACHED_PROCESS: u32 = 0x0000_0008;

/// Run before Tauri is initialized when this executable was spawned as the
/// private update helper. Returns `true` only when the helper arguments were
/// complete and the caller should exit instead of launching the application.
#[cfg(windows)]
pub(crate) fn run_update_helper_if_requested() -> bool {
    let Some(args) = parse_update_helper_args(std::env::args_os().skip(1)) else {
        return false;
    };
    let helper_exe = std::env::current_exe().ok();

    if let Err(err) = apply_downloaded_update(&args, helper_exe.as_deref()) {
        early_update_helper_warn(&err);
        // A failed install should still leave Verenu usable when the original
        // process has already exited. Do not launch a second copy while the
        // parent is still alive after a wait timeout.
        if !process_is_running(args.parent_pid) {
            let _ = relaunch_installed_app(&args.target, helper_exe.as_deref());
        }
    }
    true
}

#[cfg(windows)]
#[derive(Debug, PartialEq, Eq)]
struct UpdateHelperArgs {
    installer: std::path::PathBuf,
    parent_pid: u32,
    target: std::path::PathBuf,
}

#[cfg(windows)]
fn parse_update_helper_args<I>(args: I) -> Option<UpdateHelperArgs>
where
    I: IntoIterator<Item = std::ffi::OsString>,
{
    let mut args = args.into_iter();
    if args.next()?.to_string_lossy() != "--apply-update" {
        return None;
    }

    let mut installer = None;
    let mut parent_pid = None;
    let mut target = None;
    while let Some(arg) = args.next() {
        match arg.to_string_lossy().as_ref() {
            "--update-installer" => installer = args.next().map(std::path::PathBuf::from),
            "--update-parent-pid" => {
                parent_pid = args
                    .next()
                    .and_then(|value| value.to_string_lossy().parse::<u32>().ok())
            }
            "--update-target" => target = args.next().map(std::path::PathBuf::from),
            _ => return None,
        }
    }

    Some(UpdateHelperArgs {
        installer: installer?,
        parent_pid: parent_pid?,
        target: target?,
    })
}

#[cfg(windows)]
fn apply_downloaded_update(
    args: &UpdateHelperArgs,
    helper_exe: Option<&std::path::Path>,
) -> Result<(), String> {
    use std::os::windows::process::CommandExt;

    if !args.installer.is_file() {
        return Err("Downloaded update installer is missing.".into());
    }

    if let Err(error) = wait_for_process_exit(args.parent_pid) {
        let _ = std::fs::remove_file(&args.installer);
        return Err(error);
    }

    let status = std::process::Command::new(&args.installer)
        .arg("/S")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .creation_flags(CREATE_NO_WINDOW)
        .status();
    let _ = std::fs::remove_file(&args.installer);
    let status = status.map_err(|e| format!("Could not start the downloaded update: {e}"))?;
    if !status.success() {
        return Err(format!("Update installer exited with status {status}."));
    }

    relaunch_installed_app(&args.target, helper_exe)
}

#[cfg(windows)]
fn wait_for_process_exit(pid: u32) -> Result<(), String> {
    use windows::Win32::Foundation::{CloseHandle, ERROR_INVALID_PARAMETER, WAIT_OBJECT_0};
    use windows::Win32::System::Threading::{
        OpenProcess, WaitForSingleObject, PROCESS_SYNCHRONIZE,
    };

    unsafe {
        let handle = match OpenProcess(PROCESS_SYNCHRONIZE, false, pid) {
            Ok(handle) => handle,
            Err(err)
                if err.code() == windows::core::HRESULT::from_win32(ERROR_INVALID_PARAMETER.0) =>
            {
                return Ok(());
            }
            Err(err) => return Err(format!("Could not wait for Verenu to close: {err}")),
        };
        let result = WaitForSingleObject(handle, 15_000);
        let _ = CloseHandle(handle);
        if result != WAIT_OBJECT_0 {
            return Err("Verenu did not finish closing before the update timed out.".into());
        }
    }
    Ok(())
}

#[cfg(windows)]
fn process_is_running(pid: u32) -> bool {
    use windows::Win32::Foundation::{CloseHandle, ERROR_INVALID_PARAMETER, WAIT_TIMEOUT};
    use windows::Win32::System::Threading::{OpenProcess, WaitForSingleObject, PROCESS_SYNCHRONIZE};

    unsafe {
        match OpenProcess(PROCESS_SYNCHRONIZE, false, pid) {
            Ok(handle) => {
                let wait_result = WaitForSingleObject(handle, 0);
                let _ = CloseHandle(handle);
                wait_result == WAIT_TIMEOUT
            }
            Err(error)
                if error.code() == windows::core::HRESULT::from_win32(ERROR_INVALID_PARAMETER.0) =>
            {
                false
            }
            Err(_) => true,
        }
    }
}

#[cfg(windows)]
fn relaunch_installed_app(
    target: &std::path::Path,
    helper_exe: Option<&std::path::Path>,
) -> Result<(), String> {
    use std::os::windows::process::CommandExt;

    let mut command = std::process::Command::new(target);
    if let Some(helper_exe) = helper_exe {
        command.arg(format!(
            "--cleanup-update-helper={}",
            helper_exe.to_string_lossy()
        ));
    }
    command
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .creation_flags(CREATE_NO_WINDOW | DETACHED_PROCESS)
        .spawn()
        .map_err(|e| format!("Could not relaunch Verenu after the update: {e}"))?;
    Ok(())
}

#[cfg(windows)]
fn unique_update_installer_path() -> Result<std::path::PathBuf, String> {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos();
    Ok(std::env::temp_dir().join(format!("verenu-update-{}-{nonce}.exe", std::process::id())))
}

#[cfg(windows)]
fn unique_update_helper_path() -> Result<std::path::PathBuf, String> {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos();
    Ok(std::env::temp_dir().join(format!(
        "verenu-update-helper-{}-{nonce}.exe",
        std::process::id()
    )))
}

#[cfg(windows)]
fn early_update_helper_warn(message: &str) {
    use windows::core::PCWSTR;
    use windows::Win32::System::Diagnostics::Debug::OutputDebugStringW;

    let mut wide: Vec<u16> = format!("Verenu update helper: {message}")
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    unsafe { OutputDebugStringW(PCWSTR(wide.as_mut_ptr())) };
}

#[cfg(any(test, windows))]
fn is_silent_nsis_setup_url(url: &str) -> bool {
    reqwest::Url::parse(url)
        .ok()
        .is_some_and(|parsed| parsed.path().to_ascii_lowercase().ends_with("-setup.exe"))
}

// Uses SQLite's Online Backup API rather than copying the .db/-wal files
// directly: a raw file copy of a live WAL-mode database isn't atomic, so a
// concurrent write or checkpoint between copying the two files could leave
// them mismatched. The Backup API produces one consistent, complete .db
// file regardless of what else the connection is doing.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn backup_sqlite_database(
    conn: &rusqlite::Connection,
    backup_path: &std::path::Path,
) -> Result<(), String> {
    let mut backup_conn = rusqlite::Connection::open(backup_path).map_err(|e| e.to_string())?;
    let backup =
        rusqlite::backup::Backup::new(conn, &mut backup_conn).map_err(|e| e.to_string())?;
    // A negative page count backs up everything in one step instead of the
    // paced small-batch-with-delay loop run_to_completion uses for live
    // databases - holding the source lock for the duration is fine here
    // since the caller already holds the app's only connection and is about
    // to exit right after this call, so there's nothing else to avoid blocking.
    match backup.step(-1).map_err(|e| e.to_string())? {
        rusqlite::backup::StepResult::Done => Ok(()),
        other => Err(format!(
            "Backup did not complete in a single step: {other:?}"
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::is_silent_nsis_setup_url;

    #[test]
    fn only_nsis_setup_urls_use_the_silent_installer_handoff() {
        assert!(is_silent_nsis_setup_url(
            "https://github.com/MONKE2525E/Verenu/releases/download/v0.16.0/Verenu_0.16.0_x64-setup.exe"
        ));
        assert!(!is_silent_nsis_setup_url(
            "https://github.com/MONKE2525E/Verenu/releases/download/v0.16.0/Verenu_0.16.0_x64_en-US.msi"
        ));
        assert!(!is_silent_nsis_setup_url(
            "https://github.com/MONKE2525E/Verenu/releases/download/v0.16.0/Verenu-portable.exe"
        ));
    }

    #[cfg(windows)]
    #[test]
    fn update_helper_requires_complete_and_exact_arguments() {
        use super::parse_update_helper_args;

        let valid = parse_update_helper_args([
            "--apply-update".into(),
            "--update-installer".into(),
            "C:\\Temp\\verenu-update.exe".into(),
            "--update-parent-pid".into(),
            "123".into(),
            "--update-target".into(),
            "C:\\Program Files\\Verenu\\Verenu.exe".into(),
        ])
        .expect("valid helper args");
        assert_eq!(valid.parent_pid, 123);
        assert_eq!(
            valid.installer,
            std::path::PathBuf::from("C:\\Temp\\verenu-update.exe")
        );
        assert_eq!(
            valid.target,
            std::path::PathBuf::from("C:\\Program Files\\Verenu\\Verenu.exe")
        );

        assert!(parse_update_helper_args(["--apply-update".into()]).is_none());
        assert!(parse_update_helper_args([
            "--apply-update".into(),
            "--update-installer".into(),
            "C:\\Temp\\verenu-update.exe".into(),
            "--unexpected".into(),
        ])
        .is_none());
    }
}
