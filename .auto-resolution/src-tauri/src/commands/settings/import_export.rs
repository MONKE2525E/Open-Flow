use super::*;

// Stats are included in the backup for informational reference only; they derive
// from transcription history which is not backed up and cannot be restored.
#[derive(serde::Serialize, serde::Deserialize, Default)]
pub struct ExportStats {
    pub total_words: i64,
    pub avg_wpm: f64,
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct ExportDictionaryEntry {
    pub term: String,
    pub mistake: Option<String>,
    pub auto_learned: bool,
    pub confidence_tier: String,
    pub correction_count: i64,
    pub created_at: String,
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct ExportSnippet {
    pub trigger: String,
    pub expansion: String,
    pub instructions: String,
    pub created_at: String,
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct ExportPayload {
    pub version: String,
    pub app_version: String,
    pub exported_at: String,
    #[serde(default, skip_deserializing)]
    pub stats: ExportStats,
    pub settings: serde_json::Value,
    #[serde(default)]
    pub dictionary: Vec<ExportDictionaryEntry>,
    #[serde(default)]
    pub snippets: Vec<ExportSnippet>,
}

#[derive(serde::Serialize)]
pub struct ImportSummary {
    pub settings_applied: usize,
    pub settings_skipped: usize,
    pub dictionary_inserted: usize,
    pub dictionary_skipped: usize,
    pub dictionary_already_existed: usize,
    pub snippets_inserted: usize,
    pub snippets_skipped: usize,
    pub snippets_already_existed: usize,
}

#[tauri::command]
pub async fn export_data(
    app: AppHandle,
    db: tauri::State<'_, crate::DbHandle>,
) -> Result<String, String> {
    let db = db.inner().clone();
    run_blocking("export_data", move || {
        let settings = store::settings_snapshot(&app)?;
        let mut settings_map = serde_json::Map::new();
        for key in exportable_setting_keys() {
            if let Some(value) = settings.get_cloned(key) {
                settings_map.insert(key.to_string(), value);
            }
        }

        let stats = db::query_stats(&db).map_err(|e| e.to_string())?;
        let dictionary = db::query_dictionary(&db).map_err(|e| e.to_string())?;
        let snippets = db::query_snippets(&db).map_err(|e| e.to_string())?;

        let now = chrono::Local::now();
        let payload = ExportPayload {
            version: "1".to_string(),
            app_version: env!("CARGO_PKG_VERSION").to_string(),
            exported_at: now.to_rfc3339(),
            stats: ExportStats {
                total_words: stats.total_words,
                avg_wpm: stats.avg_wpm,
            },
            settings: serde_json::Value::Object(settings_map),
            dictionary: dictionary
                .into_iter()
                .map(|e| ExportDictionaryEntry {
                    term: e.term,
                    mistake: e.mistake,
                    auto_learned: e.auto_learned,
                    confidence_tier: e.confidence_tier,
                    correction_count: e.correction_count,
                    created_at: e.created_at,
                })
                .collect(),
            snippets: snippets
                .into_iter()
                .map(|s| ExportSnippet {
                    trigger: s.trigger,
                    expansion: s.expansion,
                    instructions: s.instructions,
                    created_at: s.created_at,
                })
                .collect(),
        };

        let json = serde_json::to_string_pretty(&payload)
            .map_err(|e| format!("Serialization failed: {e}"))?;

        let downloads = app
            .path()
            .download_dir()
            .map_err(|e| format!("Failed to resolve Downloads directory: {e}"))?;
        std::fs::create_dir_all(&downloads)
            .map_err(|e| format!("Failed to create Downloads path: {e}"))?;
        let path = downloads.join(format!(
            "verenu-backup-{}.json",
            now.format("%Y%m%d-%H%M%S")
        ));
        std::fs::write(&path, json).map_err(|e| format!("Failed to write backup file: {e}"))?;

        let path_label = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("verenu-backup.json");
        log::info!("export_data: wrote backup_file={path_label}");
        Ok(path.display().to_string())
    })
    .await
}

#[tauri::command]
pub async fn import_data(
    app: AppHandle,
    db: tauri::State<'_, crate::DbHandle>,
    json: String,
) -> Result<ImportSummary, String> {
    let db = db.inner().clone();
    run_blocking("import_data", move || {
        let payload: ExportPayload = serde_json::from_str(&json)
            .map_err(|e| format!("Invalid backup file: {e}"))?;

        if payload.version != "1" {
            return Err(format!(
                "Unsupported backup version '{}'. Only version '1' is supported.",
                payload.version
            ));
        }

        let settings = store::settings_handle(&app)?;
        let mut settings_applied = 0usize;
        let mut settings_skipped = 0usize;
        let mut appearance_mode_applied = false;
        let mut history_prune_days: Option<i64> = None;

        if !payload.settings.is_object() {
            log::warn!("import_data: 'settings' field is not a JSON object — skipping settings restore");
        }
        if let Some(obj) = payload.settings.as_object() {
            for (key, value) in obj {
                if !is_exportable_setting_key(key) {
                    settings_skipped += 1;
                    continue;
                }
                match validate_setting(key, value) {
                    Ok(()) => {
                        settings.set(key.clone(), value.clone())?;
                        if key == store::APPEARANCE_MODE {
                            appearance_mode_applied = true;
                        }
                        // Mirror save_setting's side effect: a backup that
                        // tightens history retention must prune immediately,
                        // not silently wait for the next app restart.
                        if key == store::HISTORY_RETENTION {
                            history_prune_days =
                                value.as_str().and_then(store::history_retention_days);
                        }
                        settings_applied += 1;
                    }
                    Err(e) => {
                        log::warn!("import_data: skipping invalid setting '{key}': {e}");
                        settings_skipped += 1;
                    }
                }
            }
            settings.save()?;
        }

        if appearance_mode_applied {
            crate::apply_runtime_icons(&app, None);
        }

        if let Some(days) = history_prune_days {
            match db::prune_transcriptions_older_than(&db, days) {
                Ok(deleted) if deleted > 0 => {
                    log::info!("import_data: pruned {deleted} transcriptions older than {days} days");
                    let _ = app.emit("verenu:history-pruned", ());
                }
                Ok(_) => {}
                Err(e) => {
                    log::warn!("import_data: history prune after import failed: {e}");
                }
            }
        }

        // The frontend keeps several settings mirrored in shared state
        // (appearance, cleanup toggle, beta updates, setup flag, retention
        // dropdown). It re-reads them on this event so a fresh import isn't
        // visually undone by stale in-memory values until the next restart.
        let _ = app.emit("verenu:settings-imported", ());

        let mut dictionary_inserted = 0usize;
        let mut dictionary_skipped = 0usize;
        let mut dictionary_already_existed = 0usize;
        let mut snippets_inserted = 0usize;
        let mut snippets_skipped = 0usize;
        let mut snippets_already_existed = 0usize;

        // Bulk-import dictionary entries and snippets inside a single
        // transaction (and a single lock acquisition) instead of one
        // implicit transaction per row - hundreds of individually committed
        // inserts each force a disk sync, which is slow, and leaves a
        // partially-imported database if the process dies mid-import.
        {
            let mut conn = db
                .lock()
                .map_err(|_| "Database lock was poisoned".to_string())?;
            let tx = conn.transaction().map_err(|e| e.to_string())?;

            for (index, entry) in payload.dictionary.iter().enumerate() {
                if entry.term.trim().is_empty() {
                    dictionary_skipped += 1;
                    continue;
                }
                match db::insert_dictionary_entry_from_backup_conn(
                    &tx,
                    &entry.term,
                    entry.mistake.as_deref(),
                    entry.auto_learned,
                    &entry.confidence_tier,
                    entry.correction_count,
                ) {
                    Ok(()) => dictionary_inserted += 1,
                    Err(e) => {
                        let msg = e.to_string();
                        if msg.contains("UNIQUE constraint failed") {
                            dictionary_already_existed += 1;
                        } else {
                            log::warn!(
                                "import_data: dictionary insert error row={} chars={} error={msg}",
                                index,
                                entry.term.chars().count()
                            );
                            dictionary_skipped += 1;
                        }
                    }
                }
            }

            for (index, snippet) in payload.snippets.iter().enumerate() {
                if snippet.trigger.trim().is_empty() || snippet.expansion.trim().is_empty() {
                    snippets_skipped += 1;
                    continue;
                }
                match db::insert_snippet_returning_conn(
                    &tx,
                    &snippet.trigger,
                    &snippet.expansion,
                    &snippet.instructions,
                    None,
                ) {
                    Ok(_) => snippets_inserted += 1,
                    Err(e) => {
                        let msg = e.to_string();
                        if msg.contains("UNIQUE constraint failed") {
                            snippets_already_existed += 1;
                        } else {
                            log::warn!(
                                "import_data: snippet insert error row={} trigger_chars={} expansion_chars={} error={msg}",
                                index,
                                snippet.trigger.chars().count(),
                                snippet.expansion.chars().count()
                            );
                            snippets_skipped += 1;
                        }
                    }
                }
            }

            tx.commit().map_err(|e| e.to_string())?;
        }

        log::info!(
            "import_data: settings={}/skip={} dict={}/skip={}/existed={} snip={}/skip={}/existed={}",
            settings_applied, settings_skipped,
            dictionary_inserted, dictionary_skipped, dictionary_already_existed,
            snippets_inserted, snippets_skipped, snippets_already_existed,
        );

        Ok(ImportSummary {
            settings_applied,
            settings_skipped,
            dictionary_inserted,
            dictionary_skipped,
            dictionary_already_existed,
            snippets_inserted,
            snippets_skipped,
            snippets_already_existed,
        })
    })
    .await
}
