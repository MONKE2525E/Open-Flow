//! Update check + in-app install, with pre-update DB backup.

use super::*;
#[cfg(target_os = "macos")]
use tauri_plugin_shell::ShellExt;

// ---------- updates ----------

#[tauri::command]
pub async fn check_for_update() -> Result<Option<crate::api::updater::UpdateInfo>, String> {
    match crate::api::updater::check().await {
        Ok(update) => Ok(update),
        Err(e) => {
            log::warn!("Update check failed: {e}");
            Ok(None)
        }
    }
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
        const CREATE_NO_WINDOW: u32 = 0x08000000;

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

            let installer = std::env::temp_dir().join("verenu-update.exe");
            let mut f = std::fs::File::create(&installer).map_err(|e| e.to_string())?;
            f.write_all(&bytes).map_err(|e| e.to_string())?;
            drop(f);

            // Batch launcher: waits for this process to exit, runs the installer
            // silently, then relaunches the app. cmd.exe avoids PowerShell
            // execution-policy issues; CREATE_NO_WINDOW suppresses any console flash.
            let current_exe = std::env::current_exe().map_err(|e| e.to_string())?;
            let script = format!(
                "@echo off\r\ntimeout /t 2 /nobreak >nul\r\n\"{}\" /S\r\nstart \"\" \"{}\"\r\n",
                installer.display(),
                current_exe.display()
            );
            let script_path = std::env::temp_dir().join("verenu-updater.cmd");
            std::fs::write(&script_path, &script).map_err(|e| e.to_string())?;

            std::process::Command::new("cmd")
                .arg("/c")
                .arg(&script_path)
                .creation_flags(CREATE_NO_WINDOW)
                .spawn()
                .map_err(|e| e.to_string())?;

            Ok(())
        })
        .await
        .map_err(|e| e.to_string())??;

        // Exit immediately so the binary is free before the installer starts.
        std::process::exit(0)
    }
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
        other => Err(format!("Backup did not complete in a single step: {other:?}")),
    }
}
