//! Context and context-scoped library commands.

use super::*;

#[tauri::command]
pub async fn get_contexts(app: AppHandle) -> Result<Vec<db::Context>, String> {
    let db = db_state(&app);
    run_blocking("get_contexts", move || {
        db::query_contexts(&db).map_err(|e| e.to_string())
    })
    .await
}

#[tauri::command]
pub async fn create_context(
    app: AppHandle,
    name: String,
    icon: Option<String>,
    tone: Option<String>,
    cleanup_intensity: Option<String>,
    custom_instructions: Option<String>,
    contextual_formatting_disabled: bool,
) -> Result<db::Context, String> {
    let db = db_state(&app);
    run_blocking("create_context", move || {
        db::insert_context_returning(
            &db,
            &name,
            icon.as_deref(),
            tone.as_deref(),
            cleanup_intensity.as_deref(),
            custom_instructions.as_deref(),
            contextual_formatting_disabled,
        )
        .map_err(|e| e.to_string())
    })
    .await
}

#[tauri::command]
pub async fn update_context(app: AppHandle, context_id: i64, name: String) -> Result<(), String> {
    let db = db_state(&app);
    run_blocking("update_context", move || {
        db::update_context(&db, context_id, &name).map_err(|e| e.to_string())
    })
    .await
}

#[tauri::command]
pub async fn update_context_settings(
    app: AppHandle,
    context_id: i64,
    icon: Option<String>,
    tone: Option<String>,
    cleanup_intensity: Option<String>,
    custom_instructions: Option<String>,
    contextual_formatting_disabled: bool,
) -> Result<(), String> {
    let db = db_state(&app);
    run_blocking("update_context_settings", move || {
        db::update_context_settings(
            &db,
            context_id,
            icon.as_deref(),
            tone.as_deref(),
            cleanup_intensity.as_deref(),
            custom_instructions.as_deref(),
            contextual_formatting_disabled,
        )
        .map_err(|e| e.to_string())
    })
    .await
}

#[tauri::command]
pub async fn update_context_color(
    app: AppHandle,
    context_id: i64,
    color: Option<String>,
) -> Result<(), String> {
    let db = db_state(&app);
    run_blocking("update_context_color", move || {
        db::update_context_color(&db, context_id, color.as_deref()).map_err(|e| e.to_string())
    })
    .await
}

#[tauri::command]
pub async fn get_context_stats(
    app: AppHandle,
    context_id: i64,
) -> Result<db::ContextStats, String> {
    let db = db_state(&app);
    run_blocking("get_context_stats", move || {
        db::query_context_stats(&db, context_id).map_err(|e| e.to_string())
    })
    .await
}

#[tauri::command]
pub async fn set_context_pinned(
    app: AppHandle,
    context_id: i64,
    pinned: bool,
) -> Result<(), String> {
    let db = db_state(&app);
    run_blocking("set_context_pinned", move || {
        db::set_context_pinned(&db, context_id, pinned).map_err(|e| e.to_string())
    })
    .await
}

#[tauri::command]
pub async fn delete_context(app: AppHandle, context_id: i64) -> Result<(), String> {
    let db = db_state(&app);
    run_blocking("delete_context", move || {
        db::delete_context(&db, context_id).map_err(|e| e.to_string())
    })
    .await
}

#[tauri::command]
pub async fn get_context_targets(
    app: AppHandle,
    context_id: Option<i64>,
) -> Result<Vec<db::ContextTarget>, String> {
    let db = db_state(&app);
    run_blocking("get_context_targets", move || {
        let installed_apps = crate::system::apps::list_installed_apps();
        db::reconcile_context_targets(&db, &installed_apps).map_err(|e| e.to_string())?;
        db::query_context_targets(&db, context_id).map_err(|e| e.to_string())
    })
    .await
}

#[tauri::command]
pub async fn assign_context_target(
    app: AppHandle,
    context_id: i64,
    executable: String,
    app_name: Option<String>,
    developer: Option<String>,
) -> Result<db::ContextTarget, String> {
    let db = db_state(&app);
    run_blocking("assign_context_target", move || {
        db::assign_context_target_with_metadata(
            &db,
            context_id,
            &executable,
            app_name.as_deref(),
            developer.as_deref(),
        )
        .map_err(|e| e.to_string())
    })
    .await
}

#[tauri::command]
pub async fn remove_context_target(
    app: AppHandle,
    context_id: i64,
    executable: String,
) -> Result<(), String> {
    let db = db_state(&app);
    run_blocking("remove_context_target", move || {
        db::remove_context_target(&db, context_id, &executable).map_err(|e| e.to_string())
    })
    .await
}

#[tauri::command]
pub async fn get_context_websites(
    app: AppHandle,
    context_id: Option<i64>,
) -> Result<Vec<db::ContextWebsiteTarget>, String> {
    let db = db_state(&app);
    run_blocking("get_context_websites", move || {
        db::query_context_website_targets(&db, context_id).map_err(|e| e.to_string())
    })
    .await
}

/// DNS-only existence check — resolving the hostname is enough to confirm the
/// domain is real without the cost/fragility of an actual HTTP request (which
/// can fail for reasons unrelated to the domain existing, like no HTTPS
/// server or a firewall). Never errors: a lookup failure just means "no".
#[tauri::command]
pub async fn check_domain_exists(domain: String) -> Result<bool, String> {
    let host = domain.trim().to_string();
    if host.is_empty() {
        return Ok(false);
    }
    let lookup = tokio::task::spawn_blocking(move || {
        use std::net::ToSocketAddrs;
        (host.as_str(), 443u16)
            .to_socket_addrs()
            .map(|mut addrs| addrs.next().is_some())
            .unwrap_or(false)
    });
    match tokio::time::timeout(std::time::Duration::from_secs(4), lookup).await {
        Ok(Ok(exists)) => Ok(exists),
        _ => Ok(false),
    }
}

#[tauri::command]
pub async fn assign_context_website(
    app: AppHandle,
    context_id: i64,
    domain: String,
) -> Result<db::ContextWebsiteTarget, String> {
    let db = db_state(&app);
    run_blocking("assign_context_website", move || {
        db::assign_context_website(&db, context_id, &domain).map_err(|e| e.to_string())
    })
    .await
}

#[tauri::command]
pub async fn remove_context_website(
    app: AppHandle,
    context_id: i64,
    domain: String,
) -> Result<(), String> {
    let db = db_state(&app);
    run_blocking("remove_context_website", move || {
        db::remove_context_website(&db, context_id, &domain).map_err(|e| e.to_string())
    })
    .await
}

#[tauri::command]
pub async fn get_context_dictionary(
    app: AppHandle,
    context_id: i64,
) -> Result<Vec<db::DictionaryEntry>, String> {
    let db = db_state(&app);
    run_blocking("get_context_dictionary", move || {
        db::query_dictionary_for_context(&db, context_id).map_err(|e| e.to_string())
    })
    .await
}

#[tauri::command]
pub async fn get_context_snippets(
    app: AppHandle,
    context_id: i64,
) -> Result<Vec<db::Snippet>, String> {
    let db = db_state(&app);
    run_blocking("get_context_snippets", move || {
        db::query_snippets_for_context(&db, context_id).map_err(|e| e.to_string())
    })
    .await
}

#[tauri::command]
pub async fn set_dictionary_context_assignment(
    app: AppHandle,
    context_id: i64,
    dictionary_id: i64,
    assigned: bool,
) -> Result<(), String> {
    let db = db_state(&app);
    run_blocking("set_dictionary_context_assignment", move || {
        db::set_dictionary_context_assignment(&db, context_id, dictionary_id, assigned)
            .map_err(|e| e.to_string())
    })
    .await
}

#[tauri::command]
pub async fn set_snippet_context_assignment(
    app: AppHandle,
    context_id: i64,
    snippet_id: i64,
    assigned: bool,
) -> Result<(), String> {
    let db = db_state(&app);
    run_blocking("set_snippet_context_assignment", move || {
        db::set_snippet_context_assignment(&db, context_id, snippet_id, assigned)
            .map_err(|e| e.to_string())
    })
    .await
}
