//! Snippet CRUD and use-count tracking.

use anyhow::Result;
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

use super::*;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Snippet {
    pub id: i64,
    pub trigger: String,
    pub expansion: String,
    pub instructions: String,
    pub use_count: i64,
    pub created_at: String,
}

#[cfg(test)]
pub fn insert_snippet(db: &Db, trigger: &str, expansion: &str, instructions: &str) -> Result<()> {
    insert_snippet_returning(db, trigger, expansion, instructions, None)?;
    Ok(())
}

pub fn insert_snippet_returning(
    db: &Db,
    trigger: &str,
    expansion: &str,
    instructions: &str,
    context_id: Option<i64>,
) -> Result<CreatedRecordMeta> {
    // Insert and read last_insert_rowid under a single lock to prevent another
    // thread's insert racing between the two acquisitions and returning the wrong id.
    let conn = lock_conn(db)?;
    insert_snippet_returning_conn(&conn, trigger, expansion, instructions, context_id)
}

/// Same as `insert_snippet_returning` but takes an already-locked connection,
/// so a caller doing many inserts (e.g. bulk import) can wrap them all in one
/// transaction instead of locking per row.
///
/// `context_id: None` (bulk import, legacy standalone Snippets page) keeps
/// the original strict behavior: a duplicate trigger always fails, and new
/// snippets land in Everywhere. When `context_id` names a specific context
/// and the trigger already exists elsewhere, the existing snippet is linked
/// into that context instead of failing — see the matching dictionary
/// version of this logic for the reasoning.
pub fn insert_snippet_returning_conn(
    conn: &rusqlite::Connection,
    trigger: &str,
    expansion: &str,
    instructions: &str,
    context_id: Option<i64>,
) -> Result<CreatedRecordMeta> {
    let normalized_trigger = require_nonempty_trimmed("Trigger", trigger)?;
    validate_char_limit("Trigger", &normalized_trigger, SNIPPET_TRIGGER_CHAR_LIMIT)?;
    let normalized_expansion = normalize_multiline(expansion);
    if normalized_expansion.is_empty() {
        return Err(anyhow::anyhow!("Expansion cannot be empty"));
    }
    validate_char_limit(
        "Expansion",
        &normalized_expansion,
        SNIPPET_EXPANSION_CHAR_LIMIT,
    )?;
    let normalized_instructions = normalize_multiline(instructions);
    validate_char_limit(
        "Cleanup instructions",
        &normalized_instructions,
        SNIPPET_INSTRUCTIONS_CHAR_LIMIT,
    )?;

    let everywhere_id = ensure_everywhere_context_conn(conn)?;
    let target_context = context_id.filter(|id| *id != everywhere_id);

    if let Some(target_context) = target_context {
        let existing: Option<i64> = conn
            .query_row(
                "SELECT id FROM snippets WHERE trigger = ?1",
                params![normalized_trigger],
                |row| row.get(0),
            )
            .optional()?;
        if let Some(id) = existing {
            let already_in_context: bool = conn.query_row(
                "SELECT EXISTS(SELECT 1 FROM snippet_contexts WHERE context_id = ?1 AND snippet_id = ?2)",
                params![target_context, id],
                |row| row.get(0),
            )?;
            if already_in_context {
                anyhow::bail!("\"{normalized_trigger}\" is already in this context");
            }
            let existing_payload: (String, String) = conn.query_row(
                "SELECT expansion, instructions FROM snippets WHERE id = ?1",
                params![id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )?;
            if existing_payload != (normalized_expansion.clone(), normalized_instructions.clone()) {
                anyhow::bail!(
                    "\"{normalized_trigger}\" already exists with different content"
                );
            }
            conn.execute(
                "INSERT OR IGNORE INTO snippet_contexts (context_id, snippet_id) VALUES (?1, ?2)",
                params![target_context, id],
            )?;
            let created_at = conn.query_row(
                "SELECT created_at FROM snippets WHERE id=?1",
                params![id],
                |r| r.get(0),
            )?;
            return Ok(CreatedRecordMeta { id, created_at });
        }
    }

    conn.execute(
        "INSERT INTO snippets (trigger, expansion, instructions) VALUES (?1, ?2, ?3)",
        params![
            normalized_trigger,
            normalized_expansion,
            normalized_instructions
        ],
    )?;
    let id = conn.last_insert_rowid();
    conn.execute(
        "INSERT OR IGNORE INTO snippet_contexts (context_id, snippet_id) VALUES (?1, ?2)",
        params![target_context.unwrap_or(everywhere_id), id],
    )?;
    let created_at = conn.query_row(
        "SELECT created_at FROM snippets WHERE id=?1",
        params![id],
        |r| r.get(0),
    )?;
    Ok(CreatedRecordMeta { id, created_at })
}

pub fn update_snippet(
    db: &Db,
    id: i64,
    trigger: &str,
    expansion: &str,
    instructions: &str,
) -> Result<()> {
    let normalized_trigger = require_nonempty_trimmed("Trigger", trigger)?;
    validate_char_limit("Trigger", &normalized_trigger, SNIPPET_TRIGGER_CHAR_LIMIT)?;
    let normalized_expansion = normalize_multiline(expansion);
    if normalized_expansion.is_empty() {
        return Err(anyhow::anyhow!("Expansion cannot be empty"));
    }
    validate_char_limit(
        "Expansion",
        &normalized_expansion,
        SNIPPET_EXPANSION_CHAR_LIMIT,
    )?;
    let normalized_instructions = normalize_multiline(instructions);
    validate_char_limit(
        "Cleanup instructions",
        &normalized_instructions,
        SNIPPET_INSTRUCTIONS_CHAR_LIMIT,
    )?;

    let conn = lock_conn(db)?;
    let changed = conn.execute(
        "UPDATE snippets SET trigger=?2, expansion=?3, instructions=?4 WHERE id=?1",
        params![
            id,
            normalized_trigger,
            normalized_expansion,
            normalized_instructions
        ],
    )?;
    require_row_changed(changed, "Snippet", id)?;
    Ok(())
}

pub fn delete_snippet(db: &Db, id: i64) -> Result<()> {
    let mut conn = lock_conn(db)?;
    let tx = conn.transaction()?;
    let changed = tx.execute("DELETE FROM snippets WHERE id=?1", params![id])?;
    require_row_changed(changed, "Snippet", id)?;
    tx.execute(
        "DELETE FROM snippet_contexts WHERE snippet_id = ?1",
        params![id],
    )?;
    tx.commit()?;
    Ok(())
}

pub fn query_snippets(db: &Db) -> Result<Vec<Snippet>> {
    let conn = lock_conn(db)?;
    let mut stmt = conn.prepare(
        "SELECT id, trigger, expansion, instructions, use_count, created_at \
         FROM snippets ORDER BY created_at DESC",
    )?;
    let rows = stmt
        .query_map([], |r| {
            Ok(Snippet {
                id: r.get(0)?,
                trigger: r.get(1)?,
                expansion: r.get(2)?,
                instructions: r.get(3)?,
                use_count: r.get(4)?,
                created_at: r.get(5)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

pub fn query_snippets_for_context(db: &Db, context_id: i64) -> Result<Vec<Snippet>> {
    let conn = lock_conn(db)?;
    let mut stmt = conn.prepare(
        "SELECT s.id, s.trigger, s.expansion, s.instructions, s.use_count, s.created_at
         FROM snippets s
         INNER JOIN snippet_contexts sc ON sc.snippet_id = s.id
         WHERE sc.context_id = ?1
         ORDER BY s.created_at DESC",
    )?;
    let rows = stmt
        .query_map(params![context_id], |r| {
            Ok(Snippet {
                id: r.get(0)?,
                trigger: r.get(1)?,
                expansion: r.get(2)?,
                instructions: r.get(3)?,
                use_count: r.get(4)?,
                created_at: r.get(5)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

pub fn increment_snippet_use(db: &Db, id: i64) -> Result<()> {
    let conn = lock_conn(db)?;
    conn.execute(
        "UPDATE snippets SET use_count = use_count + 1 WHERE id=?1",
        params![id],
    )?;
    Ok(())
}

pub fn increment_snippet_use_counts(db: &Db, counts: &[(i64, i64)]) -> Result<()> {
    if counts.is_empty() {
        return Ok(());
    }
    let mut conn = lock_conn(db)?;
    let tx = conn.transaction()?;
    {
        let mut stmt = tx.prepare("UPDATE snippets SET use_count = use_count + ?2 WHERE id=?1")?;
        for (id, count) in counts.iter().copied() {
            if count <= 0 {
                continue;
            }
            stmt.execute(params![id, count])?;
        }
    }
    tx.commit()?;
    Ok(())
}
