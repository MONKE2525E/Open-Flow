use super::*;

#[derive(Debug)]
enum RejectionTarget {
    DictEntries { ids: Vec<i64> },
    CacheKey { key: String },
}

impl RejectionTarget {
    fn monitor_key_prefix(&self) -> &'static str {
        match self {
            RejectionTarget::DictEntries { .. } => "rejection",
            RejectionTarget::CacheKey { .. } => "cache_rejection",
        }
    }

    fn window_secs(&self) -> u64 {
        match self {
            RejectionTarget::DictEntries { .. } => REJECTION_WINDOW_SECS,
            RejectionTarget::CacheKey { .. } => CACHE_REJECTION_WINDOW_SECS,
        }
    }
}

fn apply_rejection(target: &RejectionTarget, db: &DbHandle, app: &AppHandle, prefix: &str) {
    match target {
        RejectionTarget::DictEntries { ids } => {
            if let Err(e) = db::delete_auto_learned_entries_by_ids(db, ids) {
                log::warn!("{prefix}: delete failed: {e}");
            } else {
                app.emit("verenu:dictionary-entry-rejected", ids.len()).ok();
            }
        }
        RejectionTarget::CacheKey { key } => {
            if let Err(e) = db::cleanup_cache_delete_by_key(db, key) {
                log::warn!("{prefix}: delete failed: {e}");
            } else {
                app.emit("verenu:cleanup-cache-invalidated", ()).ok();
            }
        }
    }
}

#[cfg(windows)]
pub(super) fn is_target_window_focused(target_hwnd: usize) -> bool {
    use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;
    unsafe { GetForegroundWindow().0 as usize == target_hwnd }
}

#[cfg(not(windows))]
pub(super) fn is_target_window_focused(_target_hwnd: usize) -> bool {
    true
}

fn run_rejection_monitor(
    injected_text: String,
    target: RejectionTarget,
    target_hwnd: usize,
    db: DbHandle,
    app: AppHandle,
) {
    let prefix = target.monitor_key_prefix();
    let (key_hash, _) = pair_hash(&injected_text, prefix);
    let key = format!("{prefix}:{key_hash}");
    let inserted = match active_monitors().lock() {
        Ok(mut active) => active.insert(key.clone()),
        Err(_) => false,
    };
    if !inserted {
        return;
    }

    tauri::async_runtime::spawn(async move {
        let _guard = MonitorKeyGuard::new(key);
        let prefix = target.monitor_key_prefix();

        tokio::time::sleep(std::time::Duration::from_millis(BASELINE_CAPTURE_DELAY_MS)).await;
        let mut baseline = tokio::task::spawn_blocking({
            let text = injected_text.clone();
            move || capture_baseline_text_any(&text)
        })
        .await
        .ok()
        .flatten();

        if baseline.is_none() {
            tokio::time::sleep(std::time::Duration::from_millis(BASELINE_RETRY_DELAY_MS)).await;
            baseline = tokio::task::spawn_blocking({
                let text = injected_text.clone();
                move || capture_baseline_text_any(&text)
            })
            .await
            .ok()
            .flatten();
        }

        let Some(baseline_text) = baseline else {
            // Text not found at capture time — either UIAutomation is unavailable,
            // or the user deleted the output before the 250ms baseline window.
            // Only fire if the original window is still focused; a window switch
            // in that 750ms window would cause a false positive otherwise.
            let should_fire = tokio::task::spawn_blocking(move || {
                read_focused_text().is_some() && is_target_window_focused(target_hwnd)
            })
            .await
            .unwrap_or(false);
            if should_fire {
                log::info!("{prefix}: text absent at baseline, firing rejection");
                apply_rejection(&target, &db, &app, prefix);
            } else {
                log::debug!("{prefix}: anchor miss or window switched, skipping");
            }
            return;
        };
        let Some(anchor) = find_last_anchor(&baseline_text, &injected_text) else {
            log::debug!("{prefix}: anchor not found");
            return;
        };

        let rejection_threshold = injected_text.chars().count() / 10;
        let baseline_char_count = baseline_text.chars().count();
        let deadline =
            tokio::time::Instant::now() + std::time::Duration::from_secs(target.window_secs());

        loop {
            if tokio::time::Instant::now() >= deadline {
                break;
            }

            tokio::time::sleep(std::time::Duration::from_millis(REJECTION_POLL_MS)).await;

            let current = match tokio::task::spawn_blocking(read_focused_text).await {
                Ok(Some(t)) => t,
                _ => continue,
            };

            let rejected = match current_anchored_span(&baseline_text, &current, anchor) {
                Some(span) => span.chars().count() <= rejection_threshold,
                // Anchor tracking lost (edit too complex for prefix/suffix heuristic).
                // Reject if the injected text is completely absent AND the document
                // shrank — confirming deletion rather than a stale baseline.
                None => {
                    !current.contains(injected_text.as_str())
                        && current.chars().count() < baseline_char_count
                }
            };

            if rejected {
                // Guard against false positives from window switches: only fire
                // if the original injection window is still in the foreground.
                let still_focused =
                    tokio::task::spawn_blocking(move || is_target_window_focused(target_hwnd))
                        .await
                        .unwrap_or(false);
                if still_focused {
                    log::info!("{prefix}: deletion detected, firing rejection");
                    apply_rejection(&target, &db, &app, prefix);
                    return;
                }
                log::debug!("{prefix}: rejection signal but window switched, ignoring");
            }
        }
        log::debug!("{prefix}: window expired, no rejection detected");
    });
}

pub fn start_rejection_monitor(
    injected_text: String,
    applied_entry_ids: Vec<i64>,
    target_hwnd: usize,
    db: DbHandle,
    app: AppHandle,
) {
    if applied_entry_ids.is_empty() {
        return;
    }
    run_rejection_monitor(
        injected_text,
        RejectionTarget::DictEntries {
            ids: applied_entry_ids,
        },
        target_hwnd,
        db,
        app,
    );
}

pub fn start_cache_rejection_monitor(
    injected_text: String,
    cache_key: String,
    target_hwnd: usize,
    db: DbHandle,
    app: AppHandle,
) {
    run_rejection_monitor(
        injected_text,
        RejectionTarget::CacheKey { key: cache_key },
        target_hwnd,
        db,
        app,
    );
}
