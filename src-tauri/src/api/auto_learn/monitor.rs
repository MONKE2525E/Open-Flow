use super::*;

static ACTIVE_MONITORS: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();

pub(super) fn active_monitors() -> &'static Mutex<HashSet<String>> {
    ACTIVE_MONITORS.get_or_init(|| Mutex::new(HashSet::new()))
}

pub(super) struct MonitorKeyGuard {
    key: String,
}

impl MonitorKeyGuard {
    pub(super) fn new(key: String) -> Self {
        Self { key }
    }
}

impl Drop for MonitorKeyGuard {
    fn drop(&mut self) {
        if let Ok(mut active) = active_monitors().lock() {
            active.remove(&self.key);
        }
    }
}

#[cfg(test)]
pub(super) fn diff_words(original: &str, current: &str) -> Vec<(String, String)> {
    detect_span_corrections(original, current)
        .into_iter()
        .map(|c| (c.mistake, c.correction))
        .collect()
}

pub(super) fn record_candidate(
    db: &DbHandle,
    recorded_this_session: &mut HashSet<(String, String)>,
    app_context: &str,
    mistake: String,
    correction: String,
    confidence: f64,
) -> bool {
    let key = (mistake.clone(), correction.clone());
    if recorded_this_session.contains(&key) {
        let _ = db::log_auto_learn_event(
            db,
            "candidate",
            "duplicate_in_session",
            app_context,
            "",
            "",
            confidence,
        );
        return false;
    }
    recorded_this_session.insert(key);
    let (mistake_hash, correction_hash) = pair_hash(&mistake, &correction);

    if confidence < MIN_CANDIDATE_CONFIDENCE {
        let _ = db::log_auto_learn_event(
            db,
            "candidate",
            "low_confidence",
            app_context,
            &mistake_hash,
            &correction_hash,
            confidence,
        );
        return false;
    }

    let confidence_avg =
        match db::upsert_auto_learn_candidate(db, &mistake, &correction, confidence) {
            Ok(confidence_avg) => confidence_avg,
            Err(e) => {
                log::warn!("auto-learn candidate upsert failed: {e}");
                let _ = db::log_auto_learn_event(
                    db,
                    "candidate",
                    "candidate_upsert_failed",
                    app_context,
                    &mistake_hash,
                    &correction_hash,
                    confidence,
                );
                return false;
            }
        };

    let tier = if confidence_avg >= HIGH_CONFIDENCE_TIER {
        "high"
    } else if confidence_avg >= MEDIUM_CONFIDENCE_TIER {
        "medium"
    } else {
        "low"
    };
    let threshold = if confidence_avg >= FAST_PROMOTION_CONFIDENCE {
        PROMOTION_THRESHOLD_FAST
    } else {
        PROMOTION_THRESHOLD_DEFAULT
    };

    // The pending insert, threshold count, `promoted_at` claim, and dictionary
    // upsert happen in ONE transaction inside the DB layer. Concurrent monitors
    // observing the same pair can no longer both pass the threshold and both
    // "promote" it (double events / inflated correction_count), and a rejection
    // that purges the candidate mid-flight can no longer be undone by an
    // in-flight promotion recreating the rejected row.
    match db::auto_learn_promote(
        db,
        &mistake,
        &correction,
        tier,
        PENDING_RETENTION_DAYS,
        threshold,
    ) {
        Ok(db::AutoLearnPromoteResult::Promoted) => {
            let _ = db::log_auto_learn_event(
                db,
                "promotion",
                "promoted",
                app_context,
                &mistake_hash,
                &correction_hash,
                confidence,
            );
            true
        }
        Ok(db::AutoLearnPromoteResult::BelowThreshold { .. }) => {
            let _ = db::log_auto_learn_event(
                db,
                "candidate",
                "below_threshold",
                app_context,
                &mistake_hash,
                &correction_hash,
                confidence,
            );
            false
        }
        Ok(db::AutoLearnPromoteResult::Blocked) => {
            log::debug!("auto-learn: promotion skipped because a manual dictionary entry exists");
            let _ = db::log_auto_learn_event(
                db,
                "promotion",
                "promotion_skipped",
                app_context,
                &mistake_hash,
                &correction_hash,
                confidence,
            );
            false
        }
        Ok(db::AutoLearnPromoteResult::AlreadyPromoted) => {
            log::debug!(
                "auto-learn: promotion skipped — a concurrent monitor or rejection already claimed this pair"
            );
            let _ = db::log_auto_learn_event(
                db,
                "promotion",
                "promotion_skipped",
                app_context,
                &mistake_hash,
                &correction_hash,
                confidence,
            );
            false
        }
        Err(e) => {
            log::warn!("auto-learn dictionary promotion failed: {e}");
            let _ = db::log_auto_learn_event(
                db,
                "promotion",
                "promotion_failed",
                app_context,
                &mistake_hash,
                &correction_hash,
                confidence,
            );
            false
        }
    }
}
pub(super) fn auto_learn_event_mode_enabled(app: &AppHandle) -> bool {
    store::settings_handle(app)
        .ok()
        .and_then(|settings| settings.get(store::AUTO_LEARN_EVENT_MODE))
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}

pub(super) fn event_mode_poll_sleep_duration(hook_ready: bool) -> std::time::Duration {
    if hook_ready {
        std::time::Duration::from_millis(EVENT_MONITOR_POLL_MS)
    } else {
        std::time::Duration::from_secs(POLL_INTERVAL_SECS)
    }
}

pub fn start_monitor(injected_text: String, app_context: String, db: DbHandle, app: AppHandle) {
    if injected_text.split_whitespace().count() < 2 {
        let _ = db::log_auto_learn_event(&db, "monitor", "too_short", &app_context, "", "", 0.0);
        return;
    }
    let key = monitor_key(&injected_text, &app_context);
    let inserted = match active_monitors().lock() {
        Ok(mut active) => active.insert(key.clone()),
        Err(_) => false,
    };
    if !inserted {
        let _ =
            db::log_auto_learn_event(&db, "monitor", "duplicate_skip", &app_context, "", "", 0.0);
        return;
    }
    let _ = db::log_auto_learn_event(&db, "monitor", "started", &app_context, "", "", 0.0);

    let event_mode = auto_learn_event_mode_enabled(&app);

    std::thread::spawn(move || {
        let _monitor_guard = MonitorKeyGuard::new(key);
        #[cfg(windows)]
        let _event_mode_hook_guard = if event_mode {
            Some(EventModeHookGuard::new())
        } else {
            None
        };

        if let Err(e) = db::prune_pending_corrections(&db, PENDING_RETENTION_DAYS) {
            log::warn!("auto-learn prune failed: {e}");
        }
        let _ = db::log_auto_learn_event(
            &db,
            "monitor",
            if event_mode {
                "event_mode"
            } else {
                "poll_mode"
            },
            &app_context,
            "",
            "",
            0.0,
        );

        std::thread::sleep(std::time::Duration::from_millis(BASELINE_CAPTURE_DELAY_MS));
        let mut baseline_text = capture_baseline_text(&injected_text);

        if baseline_text.is_none() {
            std::thread::sleep(std::time::Duration::from_millis(BASELINE_RETRY_DELAY_MS));
            baseline_text = capture_baseline_text(&injected_text);
        }

        let Some(baseline_text) = baseline_text else {
            log::debug!("auto-learn: could not anchor injected text in focused control");
            let _ =
                db::log_auto_learn_event(&db, "anchor", "anchor_miss", &app_context, "", "", 0.0);
            return;
        };
        let _ = db::log_auto_learn_event(&db, "anchor", "anchor_ok", &app_context, "", "", 0.0);

        let mut stable_text_gate = StableTextGate::default();
        let deadline =
            std::time::Instant::now() + std::time::Duration::from_secs(MONITOR_WINDOW_SECS);
        let mut recorded_this_session: HashSet<(String, String)> = HashSet::new();
        #[cfg(windows)]
        let mut last_event_seq = VALUE_CHANGE_SEQ.load(Ordering::Relaxed);

        loop {
            if std::time::Instant::now() >= deadline {
                break;
            }

            if event_mode {
                #[cfg(windows)]
                {
                    if ensure_value_change_hook() {
                        let timeout_at = std::time::Instant::now()
                            + std::time::Duration::from_secs(POLL_INTERVAL_SECS);
                        let mut saw_event = false;
                        while std::time::Instant::now() < timeout_at {
                            let seq = VALUE_CHANGE_SEQ.load(Ordering::Relaxed);
                            if seq != last_event_seq {
                                last_event_seq = seq;
                                saw_event = true;
                                break;
                            }
                            std::thread::sleep(event_mode_poll_sleep_duration(true));
                        }
                        if !saw_event {
                            continue;
                        }
                    } else {
                        std::thread::sleep(event_mode_poll_sleep_duration(false));
                    }
                }
                #[cfg(not(windows))]
                std::thread::sleep(event_mode_poll_sleep_duration(false));
            } else {
                std::thread::sleep(std::time::Duration::from_secs(POLL_INTERVAL_SECS));
            }

            let Some(current_text) = read_focused_text() else {
                continue;
            };

            let Some(stable_text) = stable_text_gate.observe(current_text) else {
                continue;
            };
            let _ = db::log_auto_learn_event(
                &db,
                "stable_text",
                "stable_pass",
                &app_context,
                "",
                "",
                0.0,
            );

            let diffs =
                detect_corrections_from_anchored_text(&injected_text, &baseline_text, stable_text);

            for candidate in diffs {
                if record_candidate(
                    &db,
                    &mut recorded_this_session,
                    &app_context,
                    candidate.mistake,
                    candidate.correction,
                    candidate.confidence,
                ) {
                    log::info!("auto-learn: promoted candidate pair");
                    app.emit("verenu:dictionary-updated", ()).ok();
                }
            }
        }
        let _ = db::log_auto_learn_event(&db, "monitor", "timeout", &app_context, "", "", 0.0);
    });
}
