//! Update check + in-app install, with pre-update DB backup.

use super::*;

// ---------- updates ----------

#[tauri::command]
pub async fn check_for_update() -> Result<Option<serde_json::Value>, String> {
    match crate::api::updater::check().await {
        Ok(Some(info)) => Ok(Some(serde_json::json!({
            "version": info.version,
            "downloadUrl": info.download_url,
        }))),
        Ok(None) => Ok(None),
        Err(e) => {
            log::warn!("Update check failed: {e}");
            Ok(None)
        }
    }
}

#[tauri::command]
pub async fn install_update(app: AppHandle, download_url: String) -> Result<(), String> {
    #[cfg(not(windows))]
    {
        let _ = (&app, &download_url);
        Err(
            "In-app update is only available on Windows. Download the latest release from GitHub."
                .into(),
        )
    }

    #[cfg(windows)]
    {
        use std::io::Write;
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;

        let _ = &app; // AppHandle kept in the signature for cross-platform parity

        // Back up the database before touching anything. Derive the path from the
        // canonical app-data helper (NOT app.path().app_data_dir(), which resolves
        // against the bundle identifier and may point at a different file).
        let db_path = crate::app_db_path();
        if db_path.exists() {
            // Hold the shared connection lock for the copy so a concurrent
            // write or WAL checkpoint can't leave the .db and -wal backups
            // mismatched - every other DB access in this app goes through
            // the same lock, so this fully serializes against them.
            let db = app.state::<DbHandle>().inner().clone();
            let guard = db.lock();
            if guard.is_ok() {
                let _ = backup_sqlite_database(&db_path);
            }
        }

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

        let installer = std::env::temp_dir().join("verenu-update.exe");
        let mut f = std::fs::File::create(&installer).map_err(|e| e.to_string())?;
        f.write_all(&bytes).map_err(|e| e.to_string())?;
        drop(f);

        // Batch launcher: waits for this process to exit, runs the installer silently,
        // then relaunches the app. cmd.exe avoids PowerShell execution-policy issues;
        // CREATE_NO_WINDOW suppresses any console flash.
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

        // Exit immediately so the binary is free before the installer starts.
        std::process::exit(0)
    }
}

#[cfg_attr(not(windows), allow(dead_code))]
pub fn backup_sqlite_database(db_path: &std::path::Path) -> std::io::Result<()> {
    std::fs::copy(db_path, db_path.with_extension("db.bak"))?;

    let wal_path = path_with_suffix(db_path, "-wal");
    if wal_path.exists() {
        std::fs::copy(&wal_path, path_with_suffix(db_path, "-wal.bak"))?;
    }

    Ok(())
}

#[cfg_attr(not(windows), allow(dead_code))]
pub fn path_with_suffix(path: &std::path::Path, suffix: &str) -> std::path::PathBuf {
    let mut path_with_suffix = path.to_path_buf().into_os_string();
    path_with_suffix.push(suffix);
    std::path::PathBuf::from(path_with_suffix)
}

