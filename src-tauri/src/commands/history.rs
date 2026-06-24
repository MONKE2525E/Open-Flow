//! Transcription history, stats, and cleanup-cache status.

use super::*;

const SPACE_CONSTRAINED_THRESHOLD_BYTES: u64 = 1_073_741_824;
// ---------- history / stats ----------

#[tauri::command]
pub async fn get_recent(
    app: AppHandle,
    limit: Option<usize>,
    offset: Option<usize>,
) -> Result<Vec<db::RecentEntry>, String> {
    let db = app.state::<DbHandle>().inner().clone();
    let limit = limit.unwrap_or(100);
    let offset = offset.unwrap_or(0);
    run_blocking("get_recent", move || {
        db::query_recent_page(&db, limit, offset).map_err(|e| e.to_string())
    })
    .await
}

#[tauri::command]
pub async fn get_stats(app: AppHandle) -> Result<db::Stats, String> {
    let db = app.state::<DbHandle>().inner().clone();
    run_blocking("get_stats", move || {
        db::query_stats(&db).map_err(|e| e.to_string())
    })
    .await
}

#[tauri::command]
pub async fn count_old_transcriptions(app: AppHandle, retention: String) -> Result<i64, String> {
    let Some(days) = store::history_retention_days(&retention) else {
        return Ok(0);
    };
    let db = app.state::<DbHandle>().inner().clone();
    run_blocking("count_old_transcriptions", move || {
        db::count_transcriptions_older_than(&db, days).map_err(|e| e.to_string())
    })
    .await
}

#[tauri::command]
pub async fn retry_transcription(
    app: AppHandle,
    state: tauri::State<'_, SharedState>,
) -> Result<db::RecentEntry, String> {
    pipeline::retry_transcription_impl(&app, &state)
        .await
        .map_err(|e| e.to_string())
}

#[cfg(target_os = "windows")]
fn free_bytes_for_path(path: &std::path::Path) -> Result<u64, String> {
    use std::os::windows::ffi::OsStrExt;
    use windows::core::PCWSTR;
    use windows::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;

    let mut free_bytes_available: u64 = 0;
    let mut total_bytes: u64 = 0;
    let mut total_free_bytes: u64 = 0;
    let wide: Vec<u16> = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    let result = unsafe {
        GetDiskFreeSpaceExW(
            PCWSTR(wide.as_ptr()),
            Some(&mut free_bytes_available),
            Some(&mut total_bytes),
            Some(&mut total_free_bytes),
        )
    };
    result
        .map(|_| free_bytes_available)
        .map_err(|_| "Failed to read free disk space".to_string())
}

#[cfg(target_os = "macos")]
fn free_bytes_for_path(path: &std::path::Path) -> Result<u64, String> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;

    let c_path =
        CString::new(path.as_os_str().as_bytes()).map_err(|_| "Invalid path".to_string())?;
    let mut stat: libc::statvfs = unsafe { std::mem::zeroed() };
    let rc = unsafe { libc::statvfs(c_path.as_ptr(), &mut stat) };
    if rc != 0 {
        return Err("Failed to read free disk space".to_string());
    }
    Ok(stat.f_bavail as u64 * stat.f_frsize as u64)
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
fn free_bytes_for_path(_path: &std::path::Path) -> Result<u64, String> {
    Ok(u64::MAX)
}

#[tauri::command]
pub async fn clear_cleanup_cache(app: AppHandle) -> Result<usize, String> {
    let db = app.state::<DbHandle>().inner().clone();
    run_blocking("clear_cleanup_cache", move || {
        db::cleanup_cache_clear_all(&db).map_err(|e| e.to_string())
    })
    .await
}

#[tauri::command]
pub async fn get_cleanup_cache_status(app: AppHandle) -> Result<CleanupCacheStatus, String> {
    let db = app.state::<DbHandle>().inner().clone();
    let app_data = crate::app_data_dir();
    let (free_bytes, entry_count) = run_blocking("get_cleanup_cache_status", move || {
        let free = free_bytes_for_path(&app_data)
            .map_err(|e| format!("Failed to read free disk space: {e}"))?;
        let count = db::cleanup_cache_count(&db)
            .map_err(|e| format!("Failed to count cleanup cache entries: {e}"))?;
        Ok::<_, String>((free, count))
    })
    .await?;
    Ok(CleanupCacheStatus {
        entry_count,
        is_space_constrained: free_bytes < SPACE_CONSTRAINED_THRESHOLD_BYTES,
        free_bytes,
    })
}
