//! App mappings, snippets, and dictionary (the user content library).

use super::*;

// ---------- app mappings ----------

#[tauri::command]
pub async fn get_installed_apps() -> Vec<InstalledApp> {
    match tokio::task::spawn_blocking(crate::system::apps::list_installed_apps).await {
        Ok(apps) => apps,
        Err(e) => {
            log::error!("Task to get installed apps panicked: {e}");
            Vec::new()
        }
    }
}

#[tauri::command]
pub async fn get_app_mappings(app: AppHandle) -> Result<Vec<AppMapping>, String> {
    let store = app.store("settings.json").map_err(|e| e.to_string())?;
    let mappings = store
        .get(store::APP_MAPPINGS)
        .and_then(|v| serde_json::from_value::<Vec<AppMapping>>(v).ok())
        .unwrap_or_default();
    Ok(mappings)
}

#[tauri::command]
pub async fn save_app_mappings(app: AppHandle, mappings: Vec<AppMapping>) -> Result<(), String> {
    let store = app.store("settings.json").map_err(|e| e.to_string())?;
    let value = serde_json::to_value(mappings).map_err(|e| e.to_string())?;
    store.set(store::APP_MAPPINGS, value);
    store.save().map_err(|e| e.to_string())
}

// ---------- snippets ----------

#[tauri::command]
pub async fn get_snippets(app: AppHandle) -> Result<Vec<db::Snippet>, String> {
    let db = app.state::<DbHandle>().inner().clone();
    tokio::task::spawn_blocking(move || {
        let rows = db::query_snippets(&db).map_err(|e| e.to_string())?;
        if crate::system::logger::is_verbose() {
            log::info!("snippets:get count={}", rows.len());
        }
        Ok(rows)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn create_snippet(
    app: AppHandle,
    trigger: String,
    expansion: String,
    instructions: String,
) -> Result<db::CreatedRecordMeta, String> {
    let db = app.state::<DbHandle>().inner().clone();
    tokio::task::spawn_blocking(move || {
        log::info!(
            "snippets:create trigger_chars={} expansion_chars={} instructions_chars={}",
            trigger.chars().count(),
            expansion.chars().count(),
            instructions.chars().count()
        );
        let created = db::insert_snippet_returning(&db, &trigger, &expansion, &instructions)
            .map_err(|e| {
                log::warn!("snippets:create failed: {e}");
                e.to_string()
            })?;
        log::info!("snippets:create ok id={}", created.id);
        Ok(created)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn edit_snippet(
    app: AppHandle,
    id: i64,
    trigger: String,
    expansion: String,
    instructions: String,
) -> Result<(), String> {
    let db = app.state::<DbHandle>().inner().clone();
    tokio::task::spawn_blocking(move || {
        db::update_snippet(&db, id, &trigger, &expansion, &instructions).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn remove_snippet(app: AppHandle, id: i64) -> Result<(), String> {
    let db = app.state::<DbHandle>().inner().clone();
    tokio::task::spawn_blocking(move || db::delete_snippet(&db, id).map_err(|e| e.to_string()))
        .await
        .map_err(|e| e.to_string())?
}

// ---------- dictionary ----------

#[tauri::command]
pub async fn get_dictionary(app: AppHandle) -> Result<Vec<db::DictionaryEntry>, String> {
    let db = app.state::<DbHandle>().inner().clone();
    tokio::task::spawn_blocking(move || db::query_dictionary(&db).map_err(|e| e.to_string()))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn create_dictionary_entry(
    app: AppHandle,
    term: String,
    mistake: Option<String>,
) -> Result<db::CreatedRecordMeta, String> {
    let db = app.state::<DbHandle>().inner().clone();
    tokio::task::spawn_blocking(move || {
        log::info!(
            "dictionary:create term_chars={} mistake_chars={}",
            term.chars().count(),
            mistake.as_deref().map_or(0, |m| m.chars().count())
        );
        db::insert_dictionary_entry_returning(&db, &term, mistake.as_deref()).map_err(|e| {
            log::warn!("dictionary:create failed: {e}");
            e.to_string()
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn edit_dictionary_entry(
    app: AppHandle,
    id: i64,
    term: String,
    mistake: Option<String>,
) -> Result<(), String> {
    let db = app.state::<DbHandle>().inner().clone();
    tokio::task::spawn_blocking(move || {
        db::update_dictionary_entry(&db, id, &term, mistake.as_deref()).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn remove_dictionary_entry(app: AppHandle, id: i64) -> Result<(), String> {
    let db = app.state::<DbHandle>().inner().clone();
    tokio::task::spawn_blocking(move || {
        db::delete_dictionary_entry(&db, id).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn get_auto_learn_status_summary(
    app: AppHandle,
) -> Result<db::AutoLearnStatusSummary, String> {
    let db = app.state::<DbHandle>().inner().clone();
    tokio::task::spawn_blocking(move || {
        db::get_auto_learn_status_summary(&db).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn get_recent_auto_learn_activity(
    app: AppHandle,
    limit: Option<i64>,
) -> Result<Vec<db::AutoLearnEvent>, String> {
    let db = app.state::<DbHandle>().inner().clone();
    tokio::task::spawn_blocking(move || {
        db::get_recent_auto_learn_activity(&db, limit.unwrap_or(20)).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

