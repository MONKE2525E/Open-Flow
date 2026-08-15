//! Contexts, foreground executable targets, and content assignments.

use anyhow::Result;
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

use super::*;

pub const EVERYWHERE_CONTEXT_ID: i64 = 1;

/// Excludes the built-in Everywhere context — a user can create up to this
/// many context groups of their own.
pub const MAX_USER_CONTEXTS: i64 = 200;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Context {
    pub id: i64,
    pub name: String,
    pub is_everywhere: bool,
    pub icon: Option<String>,
    pub tone: Option<String>,
    pub cleanup_intensity: Option<String>,
    pub color: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ContextTarget {
    pub id: i64,
    pub context_id: i64,
    pub executable: String,
    pub created_at: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ContextWebsiteTarget {
    pub id: i64,
    pub context_id: i64,
    pub domain: String,
    pub created_at: String,
}

fn context_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Context> {
    Ok(Context {
        id: row.get(0)?,
        name: row.get(1)?,
        is_everywhere: row.get::<_, i64>(2)? != 0,
        icon: row.get(3)?,
        tone: row.get(4)?,
        cleanup_intensity: row.get(5)?,
        color: row.get(6)?,
        created_at: row.get(7)?,
        updated_at: row.get(8)?,
    })
}

fn normalize_context_name(name: &str) -> Result<String> {
    let normalized = require_nonempty_trimmed("Context name", name)?;
    validate_char_limit("Context name", &normalized, CONTEXT_NAME_CHAR_LIMIT)?;
    Ok(normalized)
}

fn normalize_executable(executable: &str) -> Result<String> {
    let normalized = require_nonempty_trimmed("Executable", executable)?.to_lowercase();
    validate_char_limit("Executable", &normalized, CONTEXT_EXECUTABLE_CHAR_LIMIT)?;
    Ok(normalized)
}

/// Strips a scheme/path/port from a pasted URL down to a bare domain, so
/// users can paste `https://mail.google.com/mail/u/0` or type `google.com`
/// interchangeably.
fn normalize_domain(domain: &str) -> Result<String> {
    let trimmed = require_nonempty_trimmed("Website", domain)?.to_lowercase();
    let without_scheme = trimmed.split("://").last().unwrap_or(&trimmed);
    let host = without_scheme
        .split(['/', '?', '#'])
        .next()
        .unwrap_or(without_scheme);
    let host = host.split('@').next_back().unwrap_or(host); // strip user@ prefix
    let host = host.split(':').next().unwrap_or(host); // strip :port
    let normalized = require_nonempty_trimmed("Website", host)?;
    validate_char_limit("Website", &normalized, CONTEXT_DOMAIN_CHAR_LIMIT)?;
    Ok(normalized)
}

pub(crate) fn ensure_everywhere_context_conn(conn: &rusqlite::Connection) -> Result<i64> {
    conn.execute(
        "INSERT OR IGNORE INTO contexts (id, name, is_everywhere) VALUES (?1, 'Everywhere', 1)",
        params![EVERYWHERE_CONTEXT_ID],
    )?;
    conn.query_row(
        "SELECT id FROM contexts WHERE is_everywhere = 1",
        [],
        |row| row.get(0),
    )
    .map_err(Into::into)
}

pub fn everywhere_context_id(db: &Db) -> Result<i64> {
    let conn = lock_conn(db)?;
    ensure_everywhere_context_conn(&conn)
}

pub fn query_contexts(db: &Db) -> Result<Vec<Context>> {
    let conn = lock_conn(db)?;
    let mut stmt = conn.prepare(
        "SELECT id, name, is_everywhere, icon, tone, cleanup_intensity, color, created_at, updated_at
         FROM contexts
         ORDER BY is_everywhere DESC, name COLLATE NOCASE ASC",
    )?;
    let rows = stmt
        .query_map([], context_from_row)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

#[cfg_attr(not(test), allow(dead_code))]
pub fn query_context(db: &Db, context_id: i64) -> Result<Context> {
    let conn = lock_conn(db)?;
    query_context_conn(&conn, context_id)
}

fn query_context_conn(conn: &rusqlite::Connection, context_id: i64) -> Result<Context> {
    conn.query_row(
        "SELECT id, name, is_everywhere, icon, tone, cleanup_intensity, color, created_at, updated_at
         FROM contexts WHERE id = ?1",
        params![context_id],
        context_from_row,
    )
    .map_err(Into::into)
}

pub fn insert_context_returning(
    db: &Db,
    name: &str,
    icon: Option<&str>,
    tone: Option<&str>,
    cleanup_intensity: Option<&str>,
) -> Result<Context> {
    let normalized_name = normalize_context_name(name)?;
    if normalized_name.eq_ignore_ascii_case("Everywhere") {
        anyhow::bail!("The Everywhere context already exists");
    }

    let conn = lock_conn(db)?;
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM contexts WHERE is_everywhere = 0",
        [],
        |row| row.get(0),
    )?;
    if count >= MAX_USER_CONTEXTS {
        anyhow::bail!("You've reached the limit of {MAX_USER_CONTEXTS} context groups");
    }
    conn.execute(
        "INSERT INTO contexts (name, is_everywhere, icon, tone, cleanup_intensity) VALUES (?1, 0, ?2, ?3, ?4)",
        params![
            normalized_name,
            normalize_optional_trimmed(icon),
            normalize_optional_trimmed(tone),
            normalize_optional_trimmed(cleanup_intensity)
        ],
    )?;
    let id = conn.last_insert_rowid();
    query_context_conn(&conn, id)
}

pub fn update_context(db: &Db, context_id: i64, name: &str) -> Result<()> {
    let normalized_name = normalize_context_name(name)?;
    let conn = lock_conn(db)?;
    let context = query_context_conn(&conn, context_id)?;
    if context.is_everywhere {
        anyhow::bail!("The Everywhere context cannot be renamed");
    }
    let changed = conn.execute(
        "UPDATE contexts SET name = ?2, updated_at = datetime('now') WHERE id = ?1",
        params![context_id, normalized_name],
    )?;
    require_row_changed(changed, "Context", context_id)
}

/// Sets the context's icon/tone/cleanup override in one shot (always
/// overwrites all three — `None` clears a field back to "use default").
/// Kept separate from `update_context` so the plain rename flow is untouched.
pub fn update_context_settings(
    db: &Db,
    context_id: i64,
    icon: Option<&str>,
    tone: Option<&str>,
    cleanup_intensity: Option<&str>,
) -> Result<()> {
    let conn = lock_conn(db)?;
    let context = query_context_conn(&conn, context_id)?;
    if context.is_everywhere {
        anyhow::bail!("The Everywhere context cannot have a tone override");
    }
    let changed = conn.execute(
        "UPDATE contexts SET icon = ?2, tone = ?3, cleanup_intensity = ?4, updated_at = datetime('now') WHERE id = ?1",
        params![
            context_id,
            normalize_optional_trimmed(icon),
            normalize_optional_trimmed(tone),
            normalize_optional_trimmed(cleanup_intensity)
        ],
    )?;
    require_row_changed(changed, "Context", context_id)
}

/// Sets (or clears, with `None`) the context's accent color independently of
/// `update_context_settings` — the right-click color picker shouldn't need
/// to know or resend the context's current icon/tone/cleanup override just
/// to change this one field.
pub fn update_context_color(db: &Db, context_id: i64, color: Option<&str>) -> Result<()> {
    let conn = lock_conn(db)?;
    let context = query_context_conn(&conn, context_id)?;
    if context.is_everywhere {
        anyhow::bail!("The Everywhere context cannot have a color override");
    }
    let changed = conn.execute(
        "UPDATE contexts SET color = ?2, updated_at = datetime('now') WHERE id = ?1",
        params![context_id, normalize_optional_trimmed(color)],
    )?;
    require_row_changed(changed, "Context", context_id)
}

pub fn delete_context(db: &Db, context_id: i64) -> Result<()> {
    let mut conn = lock_conn(db)?;
    let context = query_context_conn(&conn, context_id)?;
    if context.is_everywhere {
        anyhow::bail!("The Everywhere context cannot be deleted");
    }

    let tx = conn.transaction()?;
    let everywhere_id = ensure_everywhere_context_conn(&tx)?;
    tx.execute(
        "INSERT OR IGNORE INTO dictionary_contexts (context_id, dictionary_id)
         SELECT ?1, dictionary_id FROM dictionary_contexts WHERE context_id = ?2",
        params![everywhere_id, context_id],
    )?;
    tx.execute(
        "INSERT OR IGNORE INTO snippet_contexts (context_id, snippet_id)
         SELECT ?1, snippet_id FROM snippet_contexts WHERE context_id = ?2",
        params![everywhere_id, context_id],
    )?;
    tx.execute(
        "DELETE FROM context_targets WHERE context_id = ?1",
        params![context_id],
    )?;
    tx.execute(
        "DELETE FROM context_website_targets WHERE context_id = ?1",
        params![context_id],
    )?;
    tx.execute(
        "DELETE FROM dictionary_contexts WHERE context_id = ?1",
        params![context_id],
    )?;
    tx.execute(
        "DELETE FROM snippet_contexts WHERE context_id = ?1",
        params![context_id],
    )?;
    let changed = tx.execute("DELETE FROM contexts WHERE id = ?1", params![context_id])?;
    require_row_changed(changed, "Context", context_id)?;
    tx.commit()?;
    Ok(())
}

pub fn query_context_targets(db: &Db, context_id: Option<i64>) -> Result<Vec<ContextTarget>> {
    let conn = lock_conn(db)?;
    let mut stmt = conn.prepare(
        "SELECT id, context_id, executable, created_at
         FROM context_targets
         WHERE (?1 IS NULL OR context_id = ?1)
         ORDER BY executable COLLATE NOCASE ASC",
    )?;
    let rows = stmt
        .query_map(params![context_id], |row| {
            Ok(ContextTarget {
                id: row.get(0)?,
                context_id: row.get(1)?,
                executable: row.get(2)?,
                created_at: row.get(3)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

pub fn assign_context_target(db: &Db, context_id: i64, executable: &str) -> Result<ContextTarget> {
    let normalized_executable = normalize_executable(executable)?;
    let mut conn = lock_conn(db)?;
    let context = query_context_conn(&conn, context_id)?;
    if context.is_everywhere {
        anyhow::bail!("The Everywhere context cannot have executable targets");
    }

    let tx = conn.transaction()?;
    tx.execute(
        "INSERT INTO context_targets (context_id, executable) VALUES (?1, ?2)
         ON CONFLICT(executable) DO UPDATE SET context_id = excluded.context_id",
        params![context_id, normalized_executable],
    )?;
    let target = tx.query_row(
        "SELECT id, context_id, executable, created_at
         FROM context_targets WHERE executable = ?1",
        params![normalized_executable],
        |row| {
            Ok(ContextTarget {
                id: row.get(0)?,
                context_id: row.get(1)?,
                executable: row.get(2)?,
                created_at: row.get(3)?,
            })
        },
    )?;
    tx.commit()?;
    Ok(target)
}

pub fn remove_context_target(db: &Db, context_id: i64, executable: &str) -> Result<()> {
    let normalized_executable = normalize_executable(executable)?;
    let conn = lock_conn(db)?;
    let changed = conn.execute(
        "DELETE FROM context_targets WHERE context_id = ?1 AND executable = ?2",
        params![context_id, normalized_executable],
    )?;
    require_row_changed(changed, "Context target", context_id)
}

pub fn query_context_website_targets(
    db: &Db,
    context_id: Option<i64>,
) -> Result<Vec<ContextWebsiteTarget>> {
    let conn = lock_conn(db)?;
    let mut stmt = conn.prepare(
        "SELECT id, context_id, domain, created_at
         FROM context_website_targets
         WHERE (?1 IS NULL OR context_id = ?1)
         ORDER BY domain COLLATE NOCASE ASC",
    )?;
    let rows = stmt
        .query_map(params![context_id], |row| {
            Ok(ContextWebsiteTarget {
                id: row.get(0)?,
                context_id: row.get(1)?,
                domain: row.get(2)?,
                created_at: row.get(3)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

pub fn assign_context_website(
    db: &Db,
    context_id: i64,
    domain: &str,
) -> Result<ContextWebsiteTarget> {
    let normalized_domain = normalize_domain(domain)?;
    let mut conn = lock_conn(db)?;
    let context = query_context_conn(&conn, context_id)?;
    if context.is_everywhere {
        anyhow::bail!("The Everywhere context cannot have website targets");
    }

    let tx = conn.transaction()?;
    tx.execute(
        "INSERT INTO context_website_targets (context_id, domain) VALUES (?1, ?2)
         ON CONFLICT(domain) DO UPDATE SET context_id = excluded.context_id",
        params![context_id, normalized_domain],
    )?;
    let target = tx.query_row(
        "SELECT id, context_id, domain, created_at
         FROM context_website_targets WHERE domain = ?1",
        params![normalized_domain],
        |row| {
            Ok(ContextWebsiteTarget {
                id: row.get(0)?,
                context_id: row.get(1)?,
                domain: row.get(2)?,
                created_at: row.get(3)?,
            })
        },
    )?;
    tx.commit()?;
    Ok(target)
}

pub fn remove_context_website(db: &Db, context_id: i64, domain: &str) -> Result<()> {
    let normalized_domain = normalize_domain(domain)?;
    let conn = lock_conn(db)?;
    let changed = conn.execute(
        "DELETE FROM context_website_targets WHERE context_id = ?1 AND domain = ?2",
        params![context_id, normalized_domain],
    )?;
    require_row_changed(changed, "Context website", context_id)
}

/// Resolve exactly one context for a foreground executable, optionally
/// refined by the active browser tab's domain. A target never inherits
/// multiple contexts; domain match takes priority over the exe match (it's
/// the more specific signal), and an unmatched/empty executable resolves to
/// the stable Everywhere context.
pub fn resolve_context_for_target(
    db: &Db,
    executable: &str,
    domain: Option<&str>,
) -> Result<Context> {
    let conn = lock_conn(db)?;
    let everywhere_id = ensure_everywhere_context_conn(&conn)?;
    let normalized_executable = executable.trim().to_lowercase();
    let normalized_domain = domain.map(str::trim).filter(|d| !d.is_empty()).map(str::to_lowercase);

    if let Some(domain) = &normalized_domain {
        let context_id: Option<i64> = conn
            .query_row(
                "SELECT context_id FROM context_website_targets WHERE domain = ?1",
                params![domain],
                |row| row.get(0),
            )
            .optional()?;
        if let Some(context_id) = context_id {
            return query_context_conn(&conn, context_id);
        }
    }

    if normalized_executable.is_empty() {
        return query_context_conn(&conn, everywhere_id);
    }

    let context_id: Option<i64> = conn
        .query_row(
            "SELECT context_id FROM context_targets WHERE executable = ?1",
            params![normalized_executable],
            |row| row.get(0),
        )
        .optional()?;
    query_context_conn(&conn, context_id.unwrap_or(everywhere_id))
}

pub fn set_dictionary_context_assignment(
    db: &Db,
    context_id: i64,
    dictionary_id: i64,
    assigned: bool,
) -> Result<()> {
    let conn = lock_conn(db)?;
    query_context_conn(&conn, context_id)?;
    let exists: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM dictionary WHERE id = ?1)",
        params![dictionary_id],
        |row| row.get(0),
    )?;
    if !exists {
        anyhow::bail!("Dictionary entry {dictionary_id} was not found");
    }
    if assigned {
        conn.execute(
            "INSERT OR IGNORE INTO dictionary_contexts (context_id, dictionary_id)
             VALUES (?1, ?2)",
            params![context_id, dictionary_id],
        )?;
    } else {
        conn.execute(
            "DELETE FROM dictionary_contexts WHERE context_id = ?1 AND dictionary_id = ?2",
            params![context_id, dictionary_id],
        )?;
    }
    Ok(())
}

pub fn set_snippet_context_assignment(
    db: &Db,
    context_id: i64,
    snippet_id: i64,
    assigned: bool,
) -> Result<()> {
    let conn = lock_conn(db)?;
    query_context_conn(&conn, context_id)?;
    let exists: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM snippets WHERE id = ?1)",
        params![snippet_id],
        |row| row.get(0),
    )?;
    if !exists {
        anyhow::bail!("Snippet {snippet_id} was not found");
    }
    if assigned {
        conn.execute(
            "INSERT OR IGNORE INTO snippet_contexts (context_id, snippet_id)
             VALUES (?1, ?2)",
            params![context_id, snippet_id],
        )?;
    } else {
        conn.execute(
            "DELETE FROM snippet_contexts WHERE context_id = ?1 AND snippet_id = ?2",
            params![context_id, snippet_id],
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migration_assigns_existing_content_to_everywhere() {
        let db = open(":memory:").expect("db");
        insert_dictionary_entry(&db, "Verenu", Some("Varinu")).expect("dictionary");
        insert_snippet(&db, "sig", "signature", "").expect("snippet");

        let everywhere = query_context(&db, EVERYWHERE_CONTEXT_ID).expect("Everywhere");
        assert!(everywhere.is_everywhere);
        assert_eq!(
            query_dictionary_for_context(&db, everywhere.id)
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            query_snippets_for_context(&db, everywhere.id)
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn version_11_content_is_assigned_during_context_migration() {
        let path = std::env::temp_dir().join(format!(
            "verenu_context_migration_{}_{}.db",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        {
            let conn = rusqlite::Connection::open(&path).expect("legacy db");
            conn.execute_batch(
                "CREATE TABLE dictionary (
                   id INTEGER PRIMARY KEY AUTOINCREMENT,
                   term TEXT NOT NULL UNIQUE,
                   mistake TEXT,
                   auto_learned INTEGER NOT NULL DEFAULT 0,
                   correction_count INTEGER NOT NULL DEFAULT 0,
                   confidence_tier TEXT NOT NULL DEFAULT 'low',
                   last_seen_at DATETIME,
                   created_at DATETIME NOT NULL DEFAULT (datetime('now'))
                 );
                 CREATE TABLE snippets (
                   id INTEGER PRIMARY KEY AUTOINCREMENT,
                   trigger TEXT NOT NULL UNIQUE,
                   expansion TEXT NOT NULL,
                   instructions TEXT NOT NULL DEFAULT '',
                   use_count INTEGER NOT NULL DEFAULT 0,
                   created_at DATETIME NOT NULL DEFAULT (datetime('now'))
                 );
                 INSERT INTO dictionary (term, mistake) VALUES ('Verenu', 'Varinu');
                 INSERT INTO snippets (trigger, expansion) VALUES ('sig', 'signature');
                 PRAGMA user_version = 11;",
            )
            .expect("seed legacy content");
        }

        let db = open(&path).expect("migrate legacy db");
        assert_eq!(
            query_dictionary_for_context(&db, EVERYWHERE_CONTEXT_ID)
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            query_snippets_for_context(&db, EVERYWHERE_CONTEXT_ID)
                .unwrap()
                .len(),
            1
        );
        drop(db);
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(path.with_extension("db-wal"));
        let _ = std::fs::remove_file(path.with_extension("db-shm"));
    }

    #[test]
    fn content_assignment_is_scoped_and_reversible() {
        let db = open(":memory:").expect("db");
        let context = insert_context_returning(&db, "Writing", None, None, None).expect("context");
        insert_dictionary_entry(&db, "Verenu", Some("Varinu")).expect("dictionary");
        insert_snippet(&db, "sig", "signature", "").expect("snippet");
        let dictionary_id = query_dictionary(&db).unwrap()[0].id;
        let snippet_id = query_snippets(&db).unwrap()[0].id;

        assert!(query_dictionary_for_context(&db, context.id)
            .unwrap()
            .is_empty());
        assert!(query_snippets_for_context(&db, context.id)
            .unwrap()
            .is_empty());

        set_dictionary_context_assignment(&db, context.id, dictionary_id, true).unwrap();
        set_snippet_context_assignment(&db, context.id, snippet_id, true).unwrap();
        assert_eq!(
            query_dictionary_for_context(&db, context.id).unwrap().len(),
            1
        );
        assert_eq!(
            query_snippets_for_context(&db, context.id).unwrap().len(),
            1
        );

        set_dictionary_context_assignment(&db, context.id, dictionary_id, false).unwrap();
        set_snippet_context_assignment(&db, context.id, snippet_id, false).unwrap();
        assert!(query_dictionary_for_context(&db, context.id)
            .unwrap()
            .is_empty());
        assert!(query_snippets_for_context(&db, context.id)
            .unwrap()
            .is_empty());
        assert_eq!(
            query_dictionary_for_context(&db, EVERYWHERE_CONTEXT_ID)
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            query_snippets_for_context(&db, EVERYWHERE_CONTEXT_ID)
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn target_resolution_returns_one_context_and_falls_back_to_everywhere() {
        let db = open(":memory:").expect("db");
        let context = insert_context_returning(&db, "Writing", None, None, None).expect("context");
        assign_context_target(&db, context.id, "Code.EXE").expect("target");

        assert_eq!(
            resolve_context_for_target(&db, "code.exe", None)
                .expect("resolve")
                .id,
            context.id
        );
        assert_eq!(
            resolve_context_for_target(&db, "unknown.exe", None)
                .expect("fallback")
                .id,
            EVERYWHERE_CONTEXT_ID
        );
    }

    #[test]
    fn website_domain_match_takes_priority_over_executable_match() {
        let db = open(":memory:").expect("db");
        let exe_context = insert_context_returning(&db, "Browsing", None, None, None).expect("exe context");
        let site_context = insert_context_returning(&db, "Work Email", None, None, None).expect("site context");
        assign_context_target(&db, exe_context.id, "chrome.exe").expect("exe target");
        assign_context_website(&db, site_context.id, "mail.google.com").expect("website target");

        assert_eq!(
            resolve_context_for_target(&db, "chrome.exe", Some("mail.google.com"))
                .expect("resolve")
                .id,
            site_context.id
        );
        assert_eq!(
            resolve_context_for_target(&db, "chrome.exe", Some("docs.google.com"))
                .expect("fallback to exe")
                .id,
            exe_context.id
        );
        assert_eq!(
            resolve_context_for_target(&db, "chrome.exe", None)
                .expect("no domain")
                .id,
            exe_context.id
        );
    }

    #[test]
    fn website_domain_normalizes_pasted_urls() {
        let db = open(":memory:").expect("db");
        let context = insert_context_returning(&db, "Work Email", None, None, None).expect("context");
        let target = assign_context_website(&db, context.id, "https://Mail.Google.com/mail/u/0?tab=rm")
            .expect("assign");
        assert_eq!(target.domain, "mail.google.com");
    }

    #[test]
    fn assigning_a_target_replaces_its_previous_context() {
        let db = open(":memory:").expect("db");
        let first = insert_context_returning(&db, "First", None, None, None).expect("first");
        let second = insert_context_returning(&db, "Second", None, None, None).expect("second");
        assign_context_target(&db, first.id, "editor.exe").expect("first target");
        assign_context_target(&db, second.id, "EDITOR.EXE").expect("replacement target");

        let targets = query_context_targets(&db, None).expect("targets");
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].context_id, second.id);
    }

    #[test]
    fn deleting_a_context_returns_items_to_everywhere() {
        let db = open(":memory:").expect("db");
        let context = insert_context_returning(&db, "Temporary", None, None, None).expect("context");
        insert_dictionary_entry(&db, "Tauri", Some("Tari")).expect("dictionary");
        insert_snippet(&db, "sig", "signature", "").expect("snippet");
        let dictionary_id = query_dictionary(&db).unwrap()[0].id;
        let snippet_id = query_snippets(&db).unwrap()[0].id;

        set_dictionary_context_assignment(&db, context.id, dictionary_id, true).unwrap();
        set_snippet_context_assignment(&db, context.id, snippet_id, true).unwrap();
        assign_context_website(&db, context.id, "mail.google.com").unwrap();
        set_dictionary_context_assignment(&db, EVERYWHERE_CONTEXT_ID, dictionary_id, false)
            .unwrap();
        set_snippet_context_assignment(&db, EVERYWHERE_CONTEXT_ID, snippet_id, false).unwrap();

        delete_context(&db, context.id).expect("delete context");

        assert!(query_context_website_targets(&db, None)
            .unwrap()
            .is_empty());

        assert_eq!(
            query_dictionary_for_context(&db, EVERYWHERE_CONTEXT_ID)
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            query_snippets_for_context(&db, EVERYWHERE_CONTEXT_ID)
                .unwrap()
                .len(),
            1
        );
    }
}
