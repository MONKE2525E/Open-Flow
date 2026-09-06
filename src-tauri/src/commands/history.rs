//! Transcription history, stats, and cleanup-cache status.

use super::*;

const SPACE_CONSTRAINED_THRESHOLD_BYTES: u64 = 1_073_741_824;
// ---------- history / stats ----------

#[tauri::command]
pub async fn get_recent(
    app: AppHandle,
    limit: Option<usize>,
    offset: Option<usize>,
    search: Option<String>,
    app_name: Option<String>,
) -> Result<Vec<db::RecentEntry>, String> {
    let db = db_state(&app);
    let limit = limit.unwrap_or(100);
    let offset = offset.unwrap_or(0);
    let search = search.filter(|s| !s.trim().is_empty());
    let app_name = app_name.filter(|s| !s.trim().is_empty());
    run_blocking("get_recent", move || {
        db::query_recent_page(&db, limit, offset, search.as_deref(), app_name.as_deref())
            .map_err(|e| e.to_string())
    })
    .await
}

/// Distinct apps present in transcription history, for the History app filter.
#[tauri::command]
pub async fn get_history_apps(app: AppHandle) -> Result<Vec<String>, String> {
    let db = db_state(&app);
    run_blocking("get_history_apps", move || {
        db::query_distinct_apps(&db).map_err(|e| e.to_string())
    })
    .await
}

#[tauri::command]
pub async fn get_stats(app: AppHandle) -> Result<db::Stats, String> {
    let db = db_state(&app);
    run_blocking("get_stats", move || {
        db::query_stats(&db).map_err(|e| e.to_string())
    })
    .await
}

/// Aggregated insights for the Insights page. `days` is 7 | 30 | 90 | 0,
/// where 0 means all time. `context_id` narrows every per-dictation figure to
/// one context group; `None` covers all of them.
#[tauri::command]
pub async fn get_insights(
    app: AppHandle,
    days: i64,
    context_id: Option<i64>,
) -> Result<db::Insights, String> {
    let db = db_state(&app);
    run_blocking("get_insights", move || {
        db::query_insights(&db, days, context_id).map_err(|e| e.to_string())
    })
    .await
}

/// Returns the locally cached OpenRouter rate table, refreshing it at most
/// once every 48 hours. A failed refresh falls back to the last good snapshot
/// so a pricing outage never blocks the Insights page.
#[tauri::command]
pub async fn get_insights_pricing(app: AppHandle) -> Result<db::PricingSnapshot, String> {
    let db = db_state(&app);
    let fresh_db = db.clone();
    let fresh = run_blocking("get_insights_pricing_freshness", move || {
        db::pricing_cache_is_fresh(&fresh_db).map_err(|e| e.to_string())
    })
    .await?;

    if !fresh {
        match crate::api::openrouter::fetch_model_pricing().await {
            Ok(rates) => {
                let fetched_at = chrono::Utc::now().timestamp();
                let write_db = db.clone();
                run_blocking("save_insights_pricing", move || {
                    db::replace_pricing_cache(&write_db, fetched_at, &rates)
                        .map_err(|e| e.to_string())
                })
                .await?;
            }
            Err(error) => {
                log::warn!("insights: OpenRouter pricing refresh failed: {error}");
            }
        }
    }

    run_blocking("get_insights_pricing", move || {
        db::query_pricing_snapshot(&db).map_err(|e| e.to_string())
    })
    .await
}

#[tauri::command]
pub async fn count_old_transcriptions(app: AppHandle, retention: String) -> Result<i64, String> {
    let Some(days) = store::history_retention_days(&retention) else {
        return Ok(0);
    };
    let db = db_state(&app);
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
        // The pill shows its own sanitized message; the command return must
        // not leak the raw provider context (AUTH_401 wire format, bodies).
        .map_err(|e| crate::api::user_facing_error(&e))
}
use crate::system::memory::free_bytes_for_path;

#[tauri::command]
pub async fn clear_cleanup_cache(app: AppHandle) -> Result<usize, String> {
    let db = db_state(&app);
    run_blocking("clear_cleanup_cache", move || {
        db::cleanup_cache_clear_all(&db).map_err(|e| e.to_string())
    })
    .await
}

#[tauri::command]
pub async fn get_cleanup_cache_status(app: AppHandle) -> Result<CleanupCacheStatus, String> {
    let db = db_state(&app);
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
