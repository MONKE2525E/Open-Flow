//! Dictionary entries, auto-learn events/candidates, and pending corrections.

use anyhow::Result;
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

use super::*;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DictionaryEntry {
    pub id: i64,
    pub term: String,
    pub mistake: Option<String>,
    pub auto_learned: bool,
    pub correction_count: i64,
    pub confidence_tier: String,
    pub last_seen_at: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AutoLearnEvent {
    pub id: i64,
    pub event_type: String,
    pub reason_code: String,
    pub app_context: String,
    pub mistake_hash: String,
    pub correction_hash: String,
    pub confidence: f64,
    pub created_at: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AutoLearnStatusSummary {
    pub monitors_started: i64,
    pub anchor_misses: i64,
    pub low_confidence_rejections: i64,
    pub promotions: i64,
    pub duplicate_monitor_skips: i64,
    pub timeout_finishes: i64,
}

pub fn query_dictionary(db: &Db) -> Result<Vec<DictionaryEntry>> {
    let conn = lock_conn(db)?;
    let mut stmt = conn.prepare(
        "SELECT id, term, mistake, auto_learned, correction_count, confidence_tier, last_seen_at, created_at \
         FROM dictionary ORDER BY created_at DESC",
    )?;
    let rows = stmt
        .query_map([], |r| {
            Ok(DictionaryEntry {
                id: r.get(0)?,
                term: r.get(1)?,
                mistake: r.get(2)?,
                auto_learned: r.get::<_, i64>(3)? != 0,
                correction_count: r.get(4)?,
                confidence_tier: r.get(5)?,
                last_seen_at: r.get(6)?,
                created_at: r.get(7)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

pub fn query_dictionary_for_context(db: &Db, context_id: i64) -> Result<Vec<DictionaryEntry>> {
    let conn = lock_conn(db)?;
    let mut stmt = conn.prepare(
        "SELECT d.id, d.term, d.mistake, d.auto_learned, d.correction_count,
                d.confidence_tier, d.last_seen_at, d.created_at
         FROM dictionary d
         INNER JOIN dictionary_contexts dc ON dc.dictionary_id = d.id
         WHERE dc.context_id = ?1
         ORDER BY d.created_at DESC",
    )?;
    let rows = stmt
        .query_map(params![context_id], |r| {
            Ok(DictionaryEntry {
                id: r.get(0)?,
                term: r.get(1)?,
                mistake: r.get(2)?,
                auto_learned: r.get::<_, i64>(3)? != 0,
                correction_count: r.get(4)?,
                confidence_tier: r.get(5)?,
                last_seen_at: r.get(6)?,
                created_at: r.get(7)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

#[cfg(test)]
pub fn insert_dictionary_entry(db: &Db, term: &str, mistake: Option<&str>) -> Result<()> {
    insert_dictionary_entry_returning(db, term, mistake, None)?;
    Ok(())
}

/// Creates a dictionary entry, or — when `context_id` names a specific
/// (non-Everywhere) context and the term already exists elsewhere — links the
/// existing entry into that context instead of failing on the term's UNIQUE
/// constraint. Only errors on a duplicate when the entry is already assigned
/// to that same context; a term already scoped to a *different* context is
/// fair game to also assign here, since a term can belong to more than one
/// context at once (see `dictionary_contexts`).
///
/// `context_id: None` (used by the legacy standalone Dictionary page and bulk
/// import) keeps the original strict behavior: duplicate terms always fail,
/// and new entries land in Everywhere.
pub fn insert_dictionary_entry_returning(
    db: &Db,
    term: &str,
    mistake: Option<&str>,
    context_id: Option<i64>,
) -> Result<CreatedRecordMeta> {
    let normalized_term = require_nonempty_trimmed("Term", term)?;
    let normalized_mistake = normalize_optional_trimmed(mistake);
    validate_char_limit("Term", &normalized_term, DICTIONARY_ENTRY_CHAR_LIMIT)?;
    if let Some(m) = normalized_mistake.as_deref() {
        validate_char_limit("Often mistranscribed as", m, DICTIONARY_ENTRY_CHAR_LIMIT)?;
    }

    // Insert, assign, and read last_insert_rowid under a single lock to prevent
    // another thread's insert racing between the two acquisitions.
    let mut conn = lock_conn(db)?;
    let tx = conn.transaction()?;
    let everywhere_id = ensure_everywhere_context_conn(&tx)?;
    let target_context = context_id.filter(|id| *id != everywhere_id);

    if let Some(target_context) = target_context {
        let existing: Option<i64> = tx
            .query_row(
                "SELECT id FROM dictionary WHERE term = ?1",
                params![normalized_term],
                |row| row.get(0),
            )
            .optional()?;
        if let Some(id) = existing {
            let already_in_context: bool = tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM dictionary_contexts WHERE context_id = ?1 AND dictionary_id = ?2)",
                params![target_context, id],
                |row| row.get(0),
            )?;
            if already_in_context {
                anyhow::bail!("\"{normalized_term}\" is already in this context");
            }
            let existing_mistake: Option<String> = tx.query_row(
                "SELECT mistake FROM dictionary WHERE id = ?1",
                params![id],
                |row| row.get(0),
            )?;
            if existing_mistake != normalized_mistake {
                anyhow::bail!("\"{normalized_term}\" already exists with a different correction");
            }
            tx.execute(
                "INSERT OR IGNORE INTO dictionary_contexts (context_id, dictionary_id) VALUES (?1, ?2)",
                params![target_context, id],
            )?;
            let created_at = tx.query_row(
                "SELECT created_at FROM dictionary WHERE id=?1",
                params![id],
                |r| r.get(0),
            )?;
            tx.commit()?;
            return Ok(CreatedRecordMeta { id, created_at });
        }
    }

    tx.execute(
        "INSERT INTO dictionary (term, mistake, confidence_tier, last_seen_at) VALUES (?1, ?2, 'manual', datetime('now'))",
        params![normalized_term, normalized_mistake],
    )?;
    let id = tx.last_insert_rowid();
    tx.execute(
        "INSERT OR IGNORE INTO dictionary_contexts (context_id, dictionary_id) VALUES (?1, ?2)",
        params![target_context.unwrap_or(everywhere_id), id],
    )?;
    let created_at = tx.query_row(
        "SELECT created_at FROM dictionary WHERE id=?1",
        params![id],
        |r| r.get(0),
    )?;
    tx.commit()?;
    Ok(CreatedRecordMeta { id, created_at })
}

/// Applies a user-approved repair as one database transaction. The expected
/// values are checked while the same connection is locked, so a concurrent
/// auto-learn or settings UI update cannot be silently overwritten.
#[allow(clippy::too_many_arguments)]
pub fn apply_dictionary_repair(
    db: &Db,
    operation: &str,
    dictionary_id: Option<i64>,
    term: Option<&str>,
    mistake: Option<&str>,
    context_id: i64,
    expected_term: Option<&str>,
    expected_mistake: Option<&str>,
) -> Result<i64> {
    let mut conn = lock_conn(db)?;
    let tx = conn.transaction()?;
    let context_exists: i64 = tx.query_row(
        "SELECT COUNT(*) FROM contexts WHERE id = ?1",
        params![context_id],
        |row| row.get(0),
    )?;
    if context_exists == 0 {
        anyhow::bail!("Repair scope does not exist")
    }

    let id = match operation {
        "add" => {
            let term = require_nonempty_trimmed("Term", term.unwrap_or_default())?;
            let mistake =
                require_nonempty_trimmed("Often mistranscribed as", mistake.unwrap_or_default())?;
            validate_char_limit("Term", &term, DICTIONARY_ENTRY_CHAR_LIMIT)?;
            validate_char_limit(
                "Often mistranscribed as",
                &mistake,
                DICTIONARY_ENTRY_CHAR_LIMIT,
            )?;
            tx.execute(
                "INSERT INTO dictionary (term, mistake, confidence_tier, last_seen_at) VALUES (?1, ?2, 'manual', datetime('now'))",
                params![term, mistake],
            )?;
            tx.last_insert_rowid()
        }
        "update" => {
            let id = dictionary_id.ok_or_else(|| anyhow::anyhow!("Missing dictionary target"))?;
            let current: (String, Option<String>) = tx.query_row(
                "SELECT term, mistake FROM dictionary WHERE id = ?1",
                params![id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )?;
            if Some(current.0.as_str()) != expected_term || current.1.as_deref() != expected_mistake
            {
                anyhow::bail!("Dictionary entry changed while you were reviewing it")
            }
            let term = require_nonempty_trimmed("Term", term.unwrap_or_default())?;
            validate_char_limit("Term", &term, DICTIONARY_ENTRY_CHAR_LIMIT)?;
            let normalized_mistake = normalize_optional_trimmed(mistake);
            if let Some(value) = normalized_mistake.as_deref() {
                validate_char_limit(
                    "Often mistranscribed as",
                    value,
                    DICTIONARY_ENTRY_CHAR_LIMIT,
                )?;
            }
            let changed = tx.execute(
                "UPDATE dictionary SET term = ?1, mistake = ?2 WHERE id = ?3",
                params![term, normalized_mistake, id],
            )?;
            if changed != 1 {
                anyhow::bail!("Dictionary entry no longer exists")
            }
            id
        }
        "remove" => {
            let id = dictionary_id.ok_or_else(|| anyhow::anyhow!("Missing dictionary target"))?;
            let current: (String, Option<String>) = tx.query_row(
                "SELECT term, mistake FROM dictionary WHERE id = ?1",
                params![id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )?;
            if Some(current.0.as_str()) != expected_term || current.1.as_deref() != expected_mistake
            {
                anyhow::bail!("Dictionary entry changed while you were reviewing it")
            }
            tx.execute(
                "DELETE FROM dictionary_contexts WHERE dictionary_id = ?1",
                params![id],
            )?;
            if tx.execute("DELETE FROM dictionary WHERE id = ?1", params![id])? != 1 {
                anyhow::bail!("Dictionary entry no longer exists")
            }
            id
        }
        _ => anyhow::bail!("Unsupported dictionary repair operation"),
    };

    if operation != "remove" {
        tx.execute(
            "DELETE FROM dictionary_contexts WHERE dictionary_id = ?1",
            params![id],
        )?;
        tx.execute(
            "INSERT INTO dictionary_contexts (context_id, dictionary_id) VALUES (?1, ?2)",
            params![context_id, id],
        )?;
    }
    tx.commit()?;
    Ok(id)
}

/// Ensures the dictionary's "Verenu" entry (if any) lists every known
/// mistranscription observed in practice from local speech-to-text models —
/// small local STT models get their own product name wrong almost every
/// time, unlike cloud Whisper models, which get a spelling hint baked into
/// their transcription prompt (see `TRANSCRIPTION_GLOSSARY`) that local
/// engines have no equivalent for (`transcribe-rs`'s `TranscribeOptions` has
/// no prompt/vocabulary field at all). `mistake` is comma-separated (see
/// `parse_dictionary_mistakes` in `data::dictionary`), same as a snippet's
/// multi-trigger field.
///
/// Two cases, handled differently:
/// - **An entry for "Verenu" already exists** (a prior run of this
///   function, or — the real bug this fixes — the user's own manual entry
///   predating this feature entirely, e.g. a hand-added `Verenu -> Vernu`).
///   Any of the known variants missing from its `mistake` list are merged
///   in; anything already there (including variants this function doesn't
///   know about) is left untouched. This branch is safe and idempotent to
///   run on every launch, unconditionally — it only ever adds, so it also
///   self-heals a database like this one where `INSERT OR IGNORE` had
///   silently lost every known variant to a `UNIQUE` conflict against a
///   pre-existing row, without clobbering what the user already had.
/// - **No entry exists.** Create one with the full known list — but only
///   the first time ever, gated on the `seeded_defaults` marker table, so a
///   user who deletes the entry entirely doesn't see it recreated on the
///   next launch. (Not `PRAGMA user_version`: that's schema.rs's own
///   migration counter, and claiming a version number here for a one-off
///   data seed would collide with any future structural migration needing
///   the same number.)
///
/// Deliberately NOT part of the generic migration chain in `schema::open` —
/// that function is called directly by `open(":memory:")` all over the test
/// suite (fixtures, unit tests), and seeding real product data into every
/// ephemeral test database would silently change dictionary counts/contents
/// those tests don't expect. This is called once per launch, explicitly,
/// only where the real user database opens (`main.rs`).
pub fn seed_default_dictionary_entries(db: &Db) -> Result<()> {
    const MARKER: &str = "verenu_dictionary_v1";
    // Zarinu is evidenced live: Cohere (local STT) consistently rendered
    // "Verenu" with a leading Z across multiple dictations in the same
    // session (confirmed by the speaker directly: "Cohere is trying to say
    // it with a Z sometimes"). Berenu/Ferenu/Werenu/Verinu extend the same
    // voiced/voiceless and vowel-substitution confusions the existing list
    // already covers (B/V, F/V, W/V, e/i) — invented non-words, so they
    // carry no real-word collision risk the way a plausible English word
    // would. Varineu is also evidenced live: Cohere transcribed "named
    // Verenu" as "named Varineu" verbatim in both raw and cleaned text.
    const KNOWN_VARIANTS: [&str; 15] = [
        "Varinu", "Verena", "Virinu", "Varino", "Varinew", "Varina", "Verminu", "Varinian",
        "Marino", "Zarinu", "Berenu", "Ferenu", "Werenu", "Verinu", "Varineu",
    ];

    let conn = lock_conn(db)?;
    let existing: Option<(i64, Option<String>)> = {
        let mut stmt =
            conn.prepare("SELECT id, mistake FROM dictionary WHERE term = 'Verenu' LIMIT 1")?;
        let mut rows = stmt.query([])?;
        match rows.next()? {
            Some(row) => Some((row.get(0)?, row.get(1)?)),
            None => None,
        }
    };

    match existing {
        Some((id, mistake)) => {
            let mut variants: Vec<String> = mistake
                .as_deref()
                .unwrap_or_default()
                .split(',')
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty())
                .collect();
            let mut changed = false;
            for known in KNOWN_VARIANTS {
                if !variants.iter().any(|v| v.eq_ignore_ascii_case(known)) {
                    variants.push(known.to_string());
                    changed = true;
                }
            }
            if changed {
                conn.execute(
                    "UPDATE dictionary SET mistake = ?1 WHERE id = ?2",
                    params![variants.join(", "), id],
                )?;
            }
            conn.execute(
                "INSERT OR IGNORE INTO seeded_defaults (key) VALUES (?1)",
                params![MARKER],
            )?;
        }
        None => {
            let already_seeded: i64 = conn.query_row(
                "SELECT COUNT(*) FROM seeded_defaults WHERE key = ?1",
                params![MARKER],
                |r| r.get(0),
            )?;
            if already_seeded > 0 {
                return Ok(());
            }
            let mut conn = conn;
            let tx = conn.transaction()?;
            tx.execute(
                "INSERT INTO dictionary (term, mistake, confidence_tier) VALUES ('Verenu', ?1, 'manual')",
                params![KNOWN_VARIANTS.join(", ")],
            )?;
            let id = tx.last_insert_rowid();
            let everywhere_id = ensure_everywhere_context_conn(&tx)?;
            tx.execute(
                "INSERT INTO dictionary_contexts (context_id, dictionary_id) VALUES (?1, ?2)",
                params![everywhere_id, id],
            )?;
            tx.execute(
                "INSERT INTO seeded_defaults (key) VALUES (?1)",
                params![MARKER],
            )?;
            tx.commit()?;
        }
    }
    Ok(())
}

/// Inserts a dictionary entry restored from a backup file. Takes an
/// already-locked connection so a caller doing many inserts (bulk import)
/// can wrap them all in one transaction instead of locking per row.
pub fn insert_dictionary_entry_from_backup_conn(
    conn: &rusqlite::Connection,
    term: &str,
    mistake: Option<&str>,
    auto_learned: bool,
    confidence_tier: &str,
    correction_count: i64,
) -> Result<()> {
    let normalized_term = require_nonempty_trimmed("Term", term)?;
    let normalized_mistake = normalize_optional_trimmed(mistake);
    validate_char_limit("Term", &normalized_term, DICTIONARY_ENTRY_CHAR_LIMIT)?;
    if let Some(m) = normalized_mistake.as_deref() {
        validate_char_limit("Often mistranscribed as", m, DICTIONARY_ENTRY_CHAR_LIMIT)?;
    }
    conn.execute(
        "INSERT INTO dictionary (term, mistake, auto_learned, correction_count, confidence_tier, last_seen_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, datetime('now'))",
        params![normalized_term, normalized_mistake, auto_learned as i64, correction_count, confidence_tier],
    )?;
    let id = conn.last_insert_rowid();
    let everywhere_id = ensure_everywhere_context_conn(conn)?;
    conn.execute(
        "INSERT OR IGNORE INTO dictionary_contexts (context_id, dictionary_id) VALUES (?1, ?2)",
        params![everywhere_id, id],
    )?;
    Ok(())
}

#[cfg(test)]
pub fn insert_dictionary_entry_auto_learned(
    db: &Db,
    term: &str,
    mistake: Option<&str>,
    confidence_tier: &str,
) -> Result<bool> {
    let normalized_term = require_nonempty_trimmed("Term", term)?;
    let normalized_mistake = normalize_optional_trimmed(mistake);
    validate_char_limit("Term", &normalized_term, DICTIONARY_ENTRY_CHAR_LIMIT)?;
    if let Some(mistake) = normalized_mistake.as_deref() {
        validate_char_limit(
            "Often mistranscribed as",
            mistake,
            DICTIONARY_ENTRY_CHAR_LIMIT,
        )?;
    }

    let conn = lock_conn(db)?;
    let changed = conn.execute(
        "INSERT INTO dictionary (term, mistake, auto_learned, correction_count, confidence_tier) \
         VALUES (?1, ?2, 1, 1, ?3) \
         ON CONFLICT(term) DO UPDATE SET \
           correction_count = correction_count + 1, \
           confidence_tier = ?3, \
           last_seen_at = datetime('now') \
         WHERE dictionary.auto_learned = 1 \
           AND COALESCE(dictionary.mistake, '') = COALESCE(excluded.mistake, '')",
        params![normalized_term, normalized_mistake, confidence_tier],
    )?;
    let id: i64 = conn.query_row(
        "SELECT id FROM dictionary WHERE term = ?1",
        params![normalized_term],
        |r| r.get(0),
    )?;
    let everywhere_id = ensure_everywhere_context_conn(&conn)?;
    conn.execute(
        "INSERT OR IGNORE INTO dictionary_contexts (context_id, dictionary_id) VALUES (?1, ?2)",
        params![everywhere_id, id],
    )?;
    Ok(changed > 0)
}

pub fn log_auto_learn_event(
    db: &Db,
    event_type: &str,
    reason_code: &str,
    app_context: &str,
    mistake_hash: &str,
    correction_hash: &str,
    confidence: f64,
) -> Result<()> {
    let conn = lock_conn(db)?;
    conn.execute(
        "INSERT INTO auto_learn_events
         (event_type, reason_code, app_context, mistake_hash, correction_hash, confidence)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            event_type,
            reason_code,
            app_context,
            mistake_hash,
            correction_hash,
            confidence
        ],
    )?;
    Ok(())
}

/// Records a confidence observation for a (wrong, correct) pair and returns
/// the running average confidence across all observations of that pair.
pub fn upsert_auto_learn_candidate(
    db: &Db,
    wrong: &str,
    correct: &str,
    confidence: f64,
) -> Result<f64> {
    let conn = lock_conn(db)?;
    conn.query_row(
        "INSERT INTO auto_learn_candidates
         (wrong_word, correct_word, confidence_sum, confidence_avg, seen_count, last_seen_at)
         VALUES (?1, ?2, ?3, ?3, 1, datetime('now'))
         ON CONFLICT(wrong_word, correct_word) DO UPDATE SET
           confidence_sum = auto_learn_candidates.confidence_sum + excluded.confidence_sum,
           seen_count = auto_learn_candidates.seen_count + 1,
           confidence_avg = (auto_learn_candidates.confidence_sum + excluded.confidence_sum) / (auto_learn_candidates.seen_count + 1),
           last_seen_at = datetime('now')
         RETURNING confidence_avg",
        params![wrong, correct, confidence],
        |r| r.get(0),
    )
    .map_err(Into::into)
}

/// Outcome of an [`auto_learn_promote`] attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutoLearnPromoteResult {
    /// The pair was promoted to the dictionary (or its existing auto-learned
    /// entry was reinforced) and the candidate's `promoted_at` gate claimed.
    Promoted,
    /// Pending-correction count hasn't reached the threshold yet. The pending
    /// row was recorded so the next session counts toward the threshold.
    BelowThreshold { pending_count: i64 },
    /// A manual dictionary entry for the same term blocks auto-learn. The
    /// candidate gate was NOT claimed, so removing the manual entry later lets
    /// the pair be learned anew.
    Blocked,
    /// A concurrent monitor (or a rejection that already purged the candidate)
    /// won the promotion claim first. The pair must not promote again.
    AlreadyPromoted,
}

/// Atomically records a pending correction, checks the promotion threshold,
/// claims the candidate's `promoted_at` gate, and promotes the pair into the
/// dictionary — all in one transaction under the single DB lock.
///
/// Auto-learn monitors are intentionally concurrent (one per dictation), and
/// two monitors can observe the same `(wrong, correct)` pair within the same
/// 2-day window. Previously the pending-count read and the dictionary upsert
/// were separate lock acquisitions, so two monitors could BOTH pass the
/// threshold and BOTH "promote" the same pair — double `promoted` events and
/// an inflated `correction_count`. A rejection monitor could also delete the
/// entry/candidate in the gap, and the in-flight promotion would re-create the
/// rejected entry. `promoted_at` is the single promotion gate: it is claimed
/// here (atomically, `IS NULL` guard) and only cleared by the rejection /
/// manual-delete paths, which purge the candidate row entirely.
pub fn auto_learn_promote(
    db: &Db,
    wrong: &str,
    correct: &str,
    confidence_tier: &str,
    pending_retention_days: i64,
    threshold: i64,
) -> Result<AutoLearnPromoteResult> {
    let mut conn = lock_conn(db)?;
    let tx = conn.transaction()?;
    let everywhere_id = ensure_everywhere_context_conn(&tx)?;

    tx.execute(
        "INSERT INTO pending_corrections (wrong_word, correct_word) VALUES (?1, ?2)",
        params![wrong, correct],
    )?;

    let pending_count: i64 = tx.query_row(
        "SELECT COUNT(*) FROM pending_corrections
         WHERE wrong_word = ?1 AND correct_word = ?2
           AND created_at >= datetime('now', ?3)",
        params![
            wrong,
            correct,
            format!("-{} days", pending_retention_days.max(1))
        ],
        |r| r.get(0),
    )?;

    if pending_count < threshold.max(1) {
        tx.commit()?;
        return Ok(AutoLearnPromoteResult::BelowThreshold { pending_count });
    }

    // A manual entry must block auto-learn WITHOUT claiming the candidate, so
    // that deleting the manual entry later allows the pair to be learned anew.
    let manual_exists: i64 = tx.query_row(
        "SELECT COUNT(*) FROM dictionary WHERE term = ?1 AND auto_learned = 0",
        params![correct],
        |r| r.get(0),
    )?;
    if manual_exists > 0 {
        tx.commit()?;
        return Ok(AutoLearnPromoteResult::Blocked);
    }

    // Atomic claim on the candidate. 0 rows means the candidate is already
    // promoted (a concurrent monitor won the race) or was purged by a
    // rejection / manual delete — either way this pair must not promote again.
    let claimed = tx.execute(
        "UPDATE auto_learn_candidates
         SET promoted_at = datetime('now')
         WHERE wrong_word = ?1 AND correct_word = ?2
           AND promoted_at IS NULL",
        params![wrong, correct],
    )?;
    if claimed == 0 {
        tx.commit()?;
        return Ok(AutoLearnPromoteResult::AlreadyPromoted);
    }

    // Promote into the dictionary. An existing auto-learned entry for the same
    // pair (a later learning window) increments its lifetime correction count;
    // a manual entry was already excluded above, and an auto-learned entry for
    // a DIFFERENT mistake can't be matched (RETURNING yields no row).
    let promoted: Option<i64> = tx
        .query_row(
            "INSERT INTO dictionary (term, mistake, auto_learned, correction_count, confidence_tier)
             VALUES (?1, ?2, 1, 1, ?3)
             ON CONFLICT(term) DO UPDATE SET
               correction_count = correction_count + 1,
               confidence_tier = ?3,
               last_seen_at = datetime('now')
             WHERE dictionary.auto_learned = 1
               AND COALESCE(dictionary.mistake, '') = COALESCE(?2, '')
             RETURNING id",
            params![correct, wrong, confidence_tier],
            |r| r.get(0),
        )
        .optional()?;

    if promoted.is_none() {
        // An auto-learned entry for a DIFFERENT mistake won the upsert's
        // conflict clause. Release the claimed gate so this pair can still be
        // learned once that entry is removed — otherwise the claim would burn
        // the pair's last chance permanently while reporting Blocked.
        tx.execute(
            "UPDATE auto_learn_candidates SET promoted_at = NULL
             WHERE wrong_word = ?1 AND correct_word = ?2",
            params![wrong, correct],
        )?;
        tx.commit()?;
        return Ok(AutoLearnPromoteResult::Blocked);
    }

    let dictionary_id: i64 = tx.query_row(
        "SELECT id FROM dictionary WHERE term = ?1",
        params![correct],
        |r| r.get(0),
    )?;
    tx.execute(
        "INSERT OR IGNORE INTO dictionary_contexts (context_id, dictionary_id) VALUES (?1, ?2)",
        params![everywhere_id, dictionary_id],
    )?;

    tx.commit()?;
    Ok(AutoLearnPromoteResult::Promoted)
}

pub fn get_auto_learn_status_summary(db: &Db) -> Result<AutoLearnStatusSummary> {
    let conn = lock_conn(db)?;
    let count_by = |event_type: &str, reason_code: &str| -> Result<i64> {
        conn.query_row(
            "SELECT COUNT(*) FROM auto_learn_events WHERE event_type = ?1 AND reason_code = ?2",
            params![event_type, reason_code],
            |r| r.get(0),
        )
        .map_err(Into::into)
    };
    let monitors_started: i64 = conn.query_row(
        "SELECT COUNT(*) FROM auto_learn_events WHERE event_type = 'monitor'",
        [],
        |r| r.get(0),
    )?;
    let promotions: i64 = conn.query_row(
        "SELECT COUNT(*) FROM auto_learn_events WHERE event_type = 'promotion' AND reason_code = 'promoted'",
        [],
        |r| r.get(0),
    )?;
    Ok(AutoLearnStatusSummary {
        monitors_started,
        anchor_misses: count_by("anchor", "anchor_miss")?,
        low_confidence_rejections: count_by("candidate", "low_confidence")?,
        promotions,
        duplicate_monitor_skips: count_by("monitor", "duplicate_skip")?,
        timeout_finishes: count_by("monitor", "timeout")?,
    })
}

pub fn get_recent_auto_learn_activity(db: &Db, limit: i64) -> Result<Vec<AutoLearnEvent>> {
    let conn = lock_conn(db)?;
    let mut stmt = conn.prepare(
        "SELECT id, event_type, reason_code, app_context, mistake_hash, correction_hash, confidence, created_at
         FROM auto_learn_events
         ORDER BY created_at DESC
         LIMIT ?1",
    )?;
    let rows = stmt
        .query_map(params![limit.max(1)], |r| {
            Ok(AutoLearnEvent {
                id: r.get(0)?,
                event_type: r.get(1)?,
                reason_code: r.get(2)?,
                app_context: r.get(3)?,
                mistake_hash: r.get(4)?,
                correction_hash: r.get(5)?,
                confidence: r.get(6)?,
                created_at: r.get(7)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

#[cfg(test)]
pub fn insert_pending_correction(db: &Db, wrong: &str, correct: &str) -> Result<()> {
    let conn = lock_conn(db)?;
    conn.execute(
        "INSERT INTO pending_corrections (wrong_word, correct_word) VALUES (?1, ?2)",
        params![wrong, correct],
    )?;
    Ok(())
}

#[cfg(test)]
pub fn count_pending_corrections_recent(
    db: &Db,
    wrong: &str,
    correct: &str,
    max_age_days: i64,
) -> Result<i64> {
    let conn = lock_conn(db)?;
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM pending_corrections \
         WHERE wrong_word=?1 AND correct_word=?2 \
         AND created_at >= datetime('now', ?3)",
        params![wrong, correct, format!("-{} days", max_age_days.max(1))],
        |r| r.get(0),
    )?;
    Ok(count)
}

pub fn prune_pending_corrections(db: &Db, max_age_days: i64) -> Result<usize> {
    let conn = lock_conn(db)?;
    let changed = conn.execute(
        "DELETE FROM pending_corrections \
         WHERE created_at < datetime('now', ?1)",
        params![format!("-{} days", max_age_days.max(1))],
    )?;
    Ok(changed)
}

/// Bounded retention for the auto-learn bookkeeping tables, so they cannot
/// grow without limit on long-lived installs: monitor/promotion audit events
/// are pruned after 30 days, and candidate rows that have not been seen in 90
/// days (promoted or not) are dropped — a candidate that is still alive after
/// 90 days of inactivity has outlived its learning window. Rejection and
/// manual-delete already purge their own rows; this catches everything else.
/// Called at startup alongside the cleanup-cache prune.
pub fn prune_auto_learn_retention(db: &Db) -> Result<usize> {
    let conn = lock_conn(db)?;
    let events = conn.execute(
        "DELETE FROM auto_learn_events \
         WHERE created_at < datetime('now', '-30 days')",
        [],
    )?;
    let candidates = conn.execute(
        "DELETE FROM auto_learn_candidates \
         WHERE last_seen_at < datetime('now', '-90 days')",
        [],
    )?;
    Ok(events + candidates)
}

pub fn update_dictionary_entry(db: &Db, id: i64, term: &str, mistake: Option<&str>) -> Result<()> {
    let normalized_term = require_nonempty_trimmed("Term", term)?;
    let normalized_mistake = normalize_optional_trimmed(mistake);
    validate_char_limit("Term", &normalized_term, DICTIONARY_ENTRY_CHAR_LIMIT)?;
    if let Some(mistake) = normalized_mistake.as_deref() {
        validate_char_limit(
            "Often mistranscribed as",
            mistake,
            DICTIONARY_ENTRY_CHAR_LIMIT,
        )?;
    }

    let conn = lock_conn(db)?;
    let changed = conn.execute(
        "UPDATE dictionary SET term=?2, mistake=?3 WHERE id=?1",
        params![id, normalized_term, normalized_mistake],
    )?;
    require_row_changed(changed, "Dictionary entry", id)?;
    Ok(())
}

pub fn delete_dictionary_entry(db: &Db, id: i64) -> Result<()> {
    let mut conn = lock_conn(db)?;
    let tx = conn.transaction()?;

    // Capture metadata before deleting so we can clean up auto-learn history.
    let info: Option<(String, Option<String>, i64)> = tx
        .query_row(
            "SELECT term, mistake, auto_learned FROM dictionary WHERE id=?1",
            params![id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .ok();

    tx.execute(
        "DELETE FROM dictionary_contexts WHERE dictionary_id = ?1",
        params![id],
    )?;
    let changed = tx.execute("DELETE FROM dictionary WHERE id=?1", params![id])?;
    require_row_changed(changed, "Dictionary entry", id)?;

    // If this was an auto-learned entry, remove its pending corrections and
    // candidates so the auto-learn system cannot immediately re-promote it.
    if let Some((term, Some(mistake), 1)) = info {
        tx.execute(
            "DELETE FROM pending_corrections WHERE wrong_word = ?1 AND correct_word = ?2",
            params![mistake, term],
        )?;
        tx.execute(
            "DELETE FROM auto_learn_candidates WHERE wrong_word = ?1 AND correct_word = ?2",
            params![mistake, term],
        )?;
    }

    tx.commit()?;
    Ok(())
}

// Conservative batch size well below SQLite's SQLITE_MAX_VARIABLE_NUMBER (default 999).
const SQL_BATCH_SIZE: usize = 500;

pub fn delete_auto_learned_entries_by_ids(db: &Db, ids: &[i64]) -> Result<()> {
    if ids.is_empty() {
        return Ok(());
    }
    let mut conn = lock_conn(db)?;
    let tx = conn.transaction()?;
    {
        let mut del_pending_stmt = tx.prepare(
            "DELETE FROM pending_corrections WHERE wrong_word = ?1 AND correct_word = ?2",
        )?;
        let mut del_candidate_stmt = tx.prepare(
            "DELETE FROM auto_learn_candidates WHERE wrong_word = ?1 AND correct_word = ?2",
        )?;

        for chunk in ids.chunks(SQL_BATCH_SIZE) {
            let placeholders = vec!["?"; chunk.len()].join(", ");

            let select_sql = format!(
                "SELECT term, mistake FROM dictionary \
                 WHERE id IN ({placeholders}) AND auto_learned = 1 AND mistake IS NOT NULL"
            );
            let pairs: Vec<(String, String)> = tx
                .prepare(&select_sql)?
                .query_map(rusqlite::params_from_iter(chunk.iter()), |r| {
                    Ok((r.get(0)?, r.get(1)?))
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;

            let delete_dict_sql =
                format!("DELETE FROM dictionary WHERE id IN ({placeholders}) AND auto_learned = 1");
            tx.execute(&delete_dict_sql, rusqlite::params_from_iter(chunk.iter()))?;

            for (term, mistake) in pairs {
                del_pending_stmt.execute(params![mistake, term])?;
                del_candidate_stmt.execute(params![mistake, term])?;
            }
        }
    }
    tx.commit()?;
    Ok(())
}
