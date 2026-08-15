#![allow(dead_code)]

//! Database schema definition, connection `open`, and versioned migrations.

use anyhow::Result;
use rusqlite::{params, Connection};
use std::sync::{Arc, Mutex};

use super::*;

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS transcriptions (
  id          INTEGER PRIMARY KEY AUTOINCREMENT,
  raw_text    TEXT    NOT NULL,
  clean_text  TEXT    NOT NULL,
  words       INTEGER NOT NULL DEFAULT 0,
  spoken_words INTEGER,
  duration_ms INTEGER NOT NULL DEFAULT 0,
  api_used    TEXT    NOT NULL DEFAULT '',
  created_at  DATETIME NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX IF NOT EXISTS idx_transcriptions_created_at
  ON transcriptions(created_at);
CREATE TABLE IF NOT EXISTS lifetime_stats (
  id               INTEGER PRIMARY KEY CHECK (id = 1),
  total_words      INTEGER NOT NULL DEFAULT 0,
  dictionary_fixes INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE IF NOT EXISTS dictionary (
  id               INTEGER PRIMARY KEY AUTOINCREMENT,
  term             TEXT    NOT NULL UNIQUE,
  mistake          TEXT,
  auto_learned     INTEGER NOT NULL DEFAULT 0,
  correction_count INTEGER NOT NULL DEFAULT 0,
  confidence_tier  TEXT    NOT NULL DEFAULT 'low',
  last_seen_at     DATETIME,
  created_at       DATETIME NOT NULL DEFAULT (datetime('now'))
);
CREATE TABLE IF NOT EXISTS snippets (
  id           INTEGER PRIMARY KEY AUTOINCREMENT,
  trigger      TEXT    NOT NULL UNIQUE,
  expansion    TEXT    NOT NULL,
  instructions TEXT    NOT NULL DEFAULT '',
  use_count    INTEGER NOT NULL DEFAULT 0,
  created_at   DATETIME NOT NULL DEFAULT (datetime('now'))
);
CREATE TABLE IF NOT EXISTS contexts (
  id                INTEGER PRIMARY KEY AUTOINCREMENT,
  name              TEXT NOT NULL COLLATE NOCASE UNIQUE,
  is_everywhere     INTEGER NOT NULL DEFAULT 0 CHECK (is_everywhere IN (0, 1)),
  icon              TEXT,
  tone              TEXT,
  cleanup_intensity TEXT,
  color             TEXT,
  created_at        DATETIME NOT NULL DEFAULT (datetime('now')),
  updated_at        DATETIME NOT NULL DEFAULT (datetime('now'))
);
CREATE UNIQUE INDEX IF NOT EXISTS idx_contexts_everywhere
  ON contexts(is_everywhere) WHERE is_everywhere = 1;
CREATE TABLE IF NOT EXISTS context_targets (
  id           INTEGER PRIMARY KEY AUTOINCREMENT,
  context_id   INTEGER NOT NULL REFERENCES contexts(id) ON DELETE CASCADE,
  executable   TEXT NOT NULL COLLATE NOCASE UNIQUE,
  created_at   DATETIME NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX IF NOT EXISTS idx_context_targets_context_id
  ON context_targets(context_id);
CREATE TABLE IF NOT EXISTS context_website_targets (
  id           INTEGER PRIMARY KEY AUTOINCREMENT,
  context_id   INTEGER NOT NULL REFERENCES contexts(id) ON DELETE CASCADE,
  domain       TEXT NOT NULL COLLATE NOCASE UNIQUE,
  created_at   DATETIME NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX IF NOT EXISTS idx_context_website_targets_context_id
  ON context_website_targets(context_id);
CREATE TABLE IF NOT EXISTS dictionary_contexts (
  context_id    INTEGER NOT NULL REFERENCES contexts(id) ON DELETE CASCADE,
  dictionary_id INTEGER NOT NULL REFERENCES dictionary(id) ON DELETE CASCADE,
  PRIMARY KEY (context_id, dictionary_id)
);
CREATE INDEX IF NOT EXISTS idx_dictionary_contexts_dictionary_id
  ON dictionary_contexts(dictionary_id);
CREATE TABLE IF NOT EXISTS snippet_contexts (
  context_id INTEGER NOT NULL REFERENCES contexts(id) ON DELETE CASCADE,
  snippet_id INTEGER NOT NULL REFERENCES snippets(id) ON DELETE CASCADE,
  PRIMARY KEY (context_id, snippet_id)
);
CREATE INDEX IF NOT EXISTS idx_snippet_contexts_snippet_id
  ON snippet_contexts(snippet_id);
CREATE TABLE IF NOT EXISTS pending_corrections (
  id           INTEGER PRIMARY KEY AUTOINCREMENT,
  wrong_word   TEXT    NOT NULL,
  correct_word TEXT    NOT NULL,
  created_at   DATETIME NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX IF NOT EXISTS idx_pending_words
  ON pending_corrections(wrong_word, correct_word);
CREATE TABLE IF NOT EXISTS auto_learn_events (
  id             INTEGER PRIMARY KEY AUTOINCREMENT,
  event_type     TEXT    NOT NULL,
  reason_code    TEXT    NOT NULL DEFAULT '',
  app_context    TEXT    NOT NULL DEFAULT '',
  mistake_hash   TEXT    NOT NULL DEFAULT '',
  correction_hash TEXT   NOT NULL DEFAULT '',
  confidence     REAL    NOT NULL DEFAULT 0.0,
  created_at     DATETIME NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX IF NOT EXISTS idx_auto_learn_events_event_type
  ON auto_learn_events(event_type, created_at);
CREATE TABLE IF NOT EXISTS auto_learn_candidates (
  id                 INTEGER PRIMARY KEY AUTOINCREMENT,
  wrong_word         TEXT    NOT NULL,
  correct_word       TEXT    NOT NULL,
  confidence_sum     REAL    NOT NULL DEFAULT 0.0,
  confidence_avg     REAL    NOT NULL DEFAULT 0.0,
  seen_count         INTEGER NOT NULL DEFAULT 0,
  last_seen_at       DATETIME NOT NULL DEFAULT (datetime('now')),
  cooldown_until     DATETIME,
  promoted_at        DATETIME,
  UNIQUE(wrong_word, correct_word)
);
CREATE INDEX IF NOT EXISTS idx_auto_learn_candidates_seen
  ON auto_learn_candidates(last_seen_at);
CREATE TABLE IF NOT EXISTS cleanup_cache (
  key         TEXT PRIMARY KEY,
  clean_text  TEXT NOT NULL,
  hit_count   INTEGER NOT NULL DEFAULT 0,
  created_at  DATETIME NOT NULL DEFAULT (datetime('now')),
  last_hit_at DATETIME NOT NULL DEFAULT (datetime('now')),
  expires_at  DATETIME NOT NULL,
  created_at_epoch  INTEGER,
  last_hit_at_epoch INTEGER,
  expires_at_epoch  INTEGER,
  is_snippet  INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX IF NOT EXISTS idx_cleanup_cache_expires_at
  ON cleanup_cache(expires_at);
CREATE INDEX IF NOT EXISTS idx_cleanup_cache_last_hit_at
  ON cleanup_cache(last_hit_at);
CREATE TABLE IF NOT EXISTS seeded_defaults (
  key TEXT PRIMARY KEY
);
CREATE TABLE IF NOT EXISTS api_calls (
  id                INTEGER PRIMARY KEY AUTOINCREMENT,
  transcription_id  INTEGER NOT NULL,
  model             TEXT    NOT NULL,
  provider          TEXT    NOT NULL,
  task              TEXT    NOT NULL,
  audio_ms          INTEGER NOT NULL DEFAULT 0,
  input_chars       INTEGER NOT NULL DEFAULT 0,
  output_chars      INTEGER NOT NULL DEFAULT 0,
  created_at        DATETIME NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX IF NOT EXISTS idx_api_calls_created_at
  ON api_calls(created_at);
CREATE INDEX IF NOT EXISTS idx_api_calls_transcription_id
  ON api_calls(transcription_id);
";

/// Opens the database, and if it (or its WAL sidecar) is corrupt, quarantines
/// the corrupt files and retries with a fresh database instead of failing the
/// whole app. Used at startup: a `db::open` panic there would otherwise put
/// Verenu in a crash loop (the startup-recovery relaunch re-panics on the
/// same corrupt file), with no way for the user to recover besides manually
/// deleting their database. Only a truly unwritable directory (fresh open also
/// failing) is allowed to bubble up as a hard startup error.
pub fn open_with_recovery(path: impl AsRef<std::path::Path>) -> Result<Db> {
    match open(path.as_ref()) {
        Ok(db) => Ok(db),
        Err(first_err) => {
            // Quarantine only genuine corruption. A transient failure (file
            // locked or busy, an unwritable directory, an I/O error) is not
            // corruption: moving a healthy database aside would silently
            // discard the user's history in favor of a fresh empty file.
            if !open_error_is_corruption(&first_err) || !quarantine_corrupt_db_files(path.as_ref())
            {
                return Err(first_err);
            }
            log::error!(
                "database failed to open and was moved aside for diagnosis; starting with a fresh database: {first_err}"
            );
            open(path.as_ref()).map_err(|second_err| {
                anyhow::anyhow!(
                    "database failed to open ({first_err}); quarantined and fresh reopen also failed: {second_err}"
                )
            })
        }
    }
}

/// Whether a failed [`open`] is caused by an actually corrupt database rather
/// than a transient or environmental error. `SQLITE_NOTADB` covers a main file
/// that is not a SQLite database and a wedged WAL; `SQLITE_CORRUPT` covers a
/// malformed database image or schema. Everything else (busy/locked, cannot
/// open, I/O) is left untouched so the user's data is never moved aside for a
/// problem that is not corruption.
fn open_error_is_corruption(err: &anyhow::Error) -> bool {
    matches!(
        err.downcast_ref::<rusqlite::Error>(),
        Some(rusqlite::Error::SqliteFailure(code, _))
            if code.code == rusqlite::ErrorCode::NotADatabase
                || code.code == rusqlite::ErrorCode::DatabaseCorrupt
    )
}

/// Renames the database file plus its WAL/SHM sidecars to
/// `.corrupt-<nanos>` names so they are preserved for diagnosis but can never
/// block a fresh open. Returns `false` (and leaves the files in place) if any
/// rename fails, so the caller keeps the original error instead of masking it
/// with a worse one. A database whose WAL file is wedged is just as
/// un-openable as a bad main file, so all three are quarantined together.
pub(crate) fn quarantine_corrupt_db_files(db_path: &std::path::Path) -> bool {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);

    let mut all_moved = true;
    for extension in ["db", "db-wal", "db-shm"] {
        let mut candidate = db_path.to_path_buf();
        candidate.set_extension(extension);
        if !candidate.exists() {
            continue;
        }
        let mut quarantined = candidate.clone();
        quarantined.set_extension(format!("{extension}.corrupt-{nanos}"));
        if let Err(err) = std::fs::rename(&candidate, &quarantined) {
            // File name only — the full path is a user-local file location
            // that must not appear in logs.
            log::warn!(
                "failed to quarantine corrupt database file {:?}: {err}",
                candidate.file_name()
            );
            all_moved = false;
        }
    }
    all_moved
}

pub fn open(path: impl AsRef<std::path::Path>) -> Result<Db> {
    let db_path = path.as_ref();
    // Connection::open creates the file if it doesn't exist, so this check
    // must run before open() - otherwise it's always true (even on a brand
    // new install) and a pointless db.bak gets created on first launch.
    let db_existed_before_open = db_path.exists();
    let mut conn = Connection::open(db_path)?;
    // user_version lives in the file header, so it's readable before SCHEMA
    // is applied. Read it here (and take the pre-migration backup) before any
    // statement touches the file, so db.bak is a true snapshot of what the
    // user had - not a copy already overwritten by SCHEMA's CREATE TABLE IF
    // NOT EXISTS statements.
    let user_version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if user_version < 2 && db_existed_before_open {
        let _ = std::fs::copy(db_path, db_path.with_extension("db.bak"));
    }

    conn.execute_batch(SCHEMA)?;
    conn.execute_batch("PRAGMA journal_mode=WAL;")?;

    if user_version < 2 {
        // IMPORTANT: each migration uses BEGIN/COMMIT for atomicity, followed by an
        // explicit ROLLBACK. If the migration fails mid-way, sqlite3_exec aborts but
        // leaves the BEGIN open. Without the ROLLBACK cleanup, every subsequent INSERT
        // would execute inside that ghost transaction and be silently discarded on
        // connection close — causing all user data to vanish on restart.
        let _ = conn.execute_batch(
            "ALTER TABLE snippets ADD COLUMN instructions TEXT NOT NULL DEFAULT '';",
        );
        let _ = conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS pending_corrections (
               id           INTEGER PRIMARY KEY AUTOINCREMENT,
               wrong_word   TEXT    NOT NULL,
               correct_word TEXT    NOT NULL,
               created_at   DATETIME NOT NULL DEFAULT (datetime('now'))
             );
             CREATE INDEX IF NOT EXISTS idx_pending_words
               ON pending_corrections(wrong_word, correct_word);",
        );
        // Migrate dictionary to final schema: term (required) + mistake (optional).
        // Handles all prior states (original wrong/correct columns, or already migrated).
        let _ = conn.execute_batch(
            "BEGIN;
             CREATE TABLE IF NOT EXISTS dictionary_v3 (
                 id               INTEGER PRIMARY KEY AUTOINCREMENT,
                 term             TEXT    NOT NULL UNIQUE,
                 mistake          TEXT,
                 auto_learned     INTEGER NOT NULL DEFAULT 0,
                 correction_count INTEGER NOT NULL DEFAULT 0,
                 created_at       DATETIME NOT NULL DEFAULT (datetime('now'))
             );
             INSERT OR IGNORE INTO dictionary_v3
                 (id, term, mistake, auto_learned, correction_count, created_at)
                 SELECT id,
                        COALESCE(correct, wrong),
                        CASE WHEN correct IS NOT NULL THEN wrong ELSE NULL END,
                        auto_learned, correction_count, created_at
                 FROM dictionary;
             DROP TABLE dictionary;
             ALTER TABLE dictionary_v3 RENAME TO dictionary;
             COMMIT;",
        );
        // Clean up any dangling transaction left by a failed migration above.
        let _ = conn.execute_batch("ROLLBACK;");
        conn.execute_batch("PRAGMA user_version = 2;")?;
    }
    if user_version == 2 {
        // Older v2 databases can have the legacy dictionary shape and a
        // snippets table that predates the instructions column. Re-run the
        // idempotent v2 repair before advancing through later migrations.
        run_migration(&mut conn, apply_v2_migration)?;
    }
    if user_version < 3 {
        let res = conn.execute_batch(
            "BEGIN;
             CREATE TABLE IF NOT EXISTS cleanup_cache (
               key         TEXT PRIMARY KEY,
               clean_text  TEXT NOT NULL,
               hit_count   INTEGER NOT NULL DEFAULT 0,
               created_at  DATETIME NOT NULL DEFAULT (datetime('now')),
               last_hit_at DATETIME NOT NULL DEFAULT (datetime('now')),
               expires_at  DATETIME NOT NULL,
               created_at_epoch  INTEGER,
               last_hit_at_epoch INTEGER,
               expires_at_epoch  INTEGER
              );
             CREATE INDEX IF NOT EXISTS idx_cleanup_cache_expires_at
               ON cleanup_cache(expires_at);
             CREATE INDEX IF NOT EXISTS idx_cleanup_cache_last_hit_at
               ON cleanup_cache(last_hit_at);
             PRAGMA user_version = 3;
             COMMIT;",
        );
        if let Err(err) = res {
            let _ = conn.execute_batch("ROLLBACK;");
            return Err(err.into());
        }
    }
    if user_version < 4 {
        run_migration(&mut conn, |conn| {
            ensure_table_column(
                conn,
                "dictionary",
                "confidence_tier",
                "ALTER TABLE dictionary ADD COLUMN confidence_tier TEXT NOT NULL DEFAULT 'low';",
            )?;
            ensure_table_column(
                conn,
                "dictionary",
                "last_seen_at",
                "ALTER TABLE dictionary ADD COLUMN last_seen_at DATETIME;",
            )?;
            conn.execute_batch(
                "CREATE TABLE IF NOT EXISTS auto_learn_events (
                   id             INTEGER PRIMARY KEY AUTOINCREMENT,
                   event_type     TEXT    NOT NULL,
                   reason_code    TEXT    NOT NULL DEFAULT '',
                   app_context    TEXT    NOT NULL DEFAULT '',
                   mistake_hash   TEXT    NOT NULL DEFAULT '',
                   correction_hash TEXT   NOT NULL DEFAULT '',
                   confidence     REAL    NOT NULL DEFAULT 0.0,
                   created_at     DATETIME NOT NULL DEFAULT (datetime('now'))
                 );
                 CREATE INDEX IF NOT EXISTS idx_auto_learn_events_event_type
                   ON auto_learn_events(event_type, created_at);
                 CREATE TABLE IF NOT EXISTS auto_learn_candidates (
                   id                 INTEGER PRIMARY KEY AUTOINCREMENT,
                   wrong_word         TEXT    NOT NULL,
                   correct_word       TEXT    NOT NULL,
                   confidence_sum     REAL    NOT NULL DEFAULT 0.0,
                   confidence_avg     REAL    NOT NULL DEFAULT 0.0,
                   seen_count         INTEGER NOT NULL DEFAULT 0,
                   last_seen_at       DATETIME NOT NULL DEFAULT (datetime('now')),
                   cooldown_until     DATETIME,
                   promoted_at        DATETIME,
                   UNIQUE(wrong_word, correct_word)
                 );
                 CREATE INDEX IF NOT EXISTS idx_auto_learn_candidates_seen
                   ON auto_learn_candidates(last_seen_at);
                 PRAGMA user_version = 4;",
            )?;
            Ok(())
        })?;
    }
    if user_version < 5 {
        run_migration(&mut conn, |conn| {
            ensure_cleanup_cache_schema(conn)?;
            conn.execute_batch("PRAGMA user_version = 5;")?;
            Ok(())
        })?;
    }
    if user_version < 6 {
        run_migration(&mut conn, |conn| {
            ensure_cleanup_cache_schema(conn)?;
            conn.execute_batch("PRAGMA user_version = 6;")?;
            Ok(())
        })?;
    }
    if user_version < 7 {
        run_migration(&mut conn, |conn| {
            ensure_table_column(
                conn,
                "transcriptions",
                "spoken_words",
                "ALTER TABLE transcriptions ADD COLUMN spoken_words INTEGER;",
            )?;
            backfill_spoken_words(conn)?;
            conn.execute_batch("PRAGMA user_version = 7;")?;
            Ok(())
        })?;
    }
    if user_version < 8 {
        let res = conn.execute_batch(
            "BEGIN;
             CREATE INDEX IF NOT EXISTS idx_transcriptions_created_at
               ON transcriptions(created_at);
             PRAGMA user_version = 8;
             COMMIT;",
        );
        if let Err(err) = res {
            let _ = conn.execute_batch("ROLLBACK;");
            return Err(err.into());
        }
    }
    if user_version < 9 {
        // Seed the lifetime word counter from existing history so upgrading
        // users don't see it reset to zero. From here on it's only ever
        // incremented on insert, never recomputed from transcriptions, so
        // history retention pruning can't shrink it.
        let res = conn.execute_batch(
            "BEGIN;
             INSERT OR IGNORE INTO lifetime_stats (id, total_words)
               SELECT 1, COALESCE(SUM(words), 0) FROM transcriptions;
             PRAGMA user_version = 9;
             COMMIT;",
        );
        if let Err(err) = res {
            let _ = conn.execute_batch("ROLLBACK;");
            return Err(err.into());
        }
    }
    if user_version < 10 {
        // Per-call API usage records for the Insights cost card. Written
        // from the pipeline at finalize time; historical transcriptions
        // predating this table simply have no cost data. Also declared in
        // SCHEMA above so an interrupted migration can't leave a database
        // without the table (the CREATE TABLE IF NOT EXISTS self-heals).
        let res = conn.execute_batch(
            "BEGIN;
             CREATE TABLE IF NOT EXISTS api_calls (
               id                INTEGER PRIMARY KEY AUTOINCREMENT,
               transcription_id  INTEGER NOT NULL,
               model             TEXT    NOT NULL,
               provider          TEXT    NOT NULL,
               task              TEXT    NOT NULL,
               audio_ms          INTEGER NOT NULL DEFAULT 0,
               input_chars       INTEGER NOT NULL DEFAULT 0,
               output_chars      INTEGER NOT NULL DEFAULT 0,
               created_at        DATETIME NOT NULL DEFAULT (datetime('now'))
             );
             CREATE INDEX IF NOT EXISTS idx_api_calls_created_at
               ON api_calls(created_at);
             PRAGMA user_version = 10;
             COMMIT;",
        );
        if let Err(err) = res {
            let _ = conn.execute_batch("ROLLBACK;");
            return Err(err.into());
        }
    }
    if user_version < 11 {
        // Real lifetime counter for dictionary substitutions actually
        // applied to dictations (incremented from the pipeline with the
        // `applied_dict_ids` count). Mirrors `total_words`: never recomputed
        // from history, so retention pruning can't shrink it. The column is
        // declared in SCHEMA for fresh databases; ensure_table_column is
        // idempotent for databases that already have it.
        run_migration(&mut conn, |conn| {
            ensure_table_column(
                conn,
                "lifetime_stats",
                "dictionary_fixes",
                "ALTER TABLE lifetime_stats ADD COLUMN dictionary_fixes INTEGER NOT NULL DEFAULT 0;",
            )?;
            conn.execute_batch("PRAGMA user_version = 11;")?;
            Ok(())
        })?;
    }
    if user_version < 12 {
        log::info!("db: migrating schema {user_version} -> 12");
        run_migration(&mut conn, |conn| {
            apply_v12_context_migration(conn)?;
            conn.execute_batch("PRAGMA user_version = 12;")?;
            Ok(())
        })?;
    }
    if user_version < 13 {
        log::info!("db: migrating schema {user_version} -> 13");
        // Per-dictation foreground app (the lowercase executable name, e.g.
        // "outlook.exe") so History can filter/annotate by app. The value was
        // never persisted before v13, so past rows keep NULL and simply have
        // no app metadata. Declared in SCHEMA for fresh databases;
        // ensure_table_column is idempotent for databases that already have it.
        run_migration(&mut conn, |conn| {
            ensure_table_column(
                conn,
                "transcriptions",
                "app_name",
                "ALTER TABLE transcriptions ADD COLUMN app_name TEXT;",
            )?;
            conn.execute_batch(
                "CREATE INDEX IF NOT EXISTS idx_transcriptions_app_name
                   ON transcriptions(app_name);
                 PRAGMA user_version = 13;",
            )?;
            Ok(())
        })?;
    }
    if user_version < 14 {
        log::info!("db: migrating schema {user_version} -> 14");
        // Per-context icon/tone/cleanup override and website-domain targets,
        // so a context can be activated by browser domain (not just exe) and
        // can override the global tone/cleanup intensity while active.
        // Columns declared in SCHEMA for fresh databases; ensure_table_column
        // is idempotent for databases that already have them.
        run_migration(&mut conn, |conn| {
            ensure_table_column(conn, "contexts", "icon", "ALTER TABLE contexts ADD COLUMN icon TEXT;")?;
            ensure_table_column(conn, "contexts", "tone", "ALTER TABLE contexts ADD COLUMN tone TEXT;")?;
            ensure_table_column(
                conn,
                "contexts",
                "cleanup_intensity",
                "ALTER TABLE contexts ADD COLUMN cleanup_intensity TEXT;",
            )?;
            conn.execute_batch(
                "CREATE TABLE IF NOT EXISTS context_website_targets (
                   id           INTEGER PRIMARY KEY AUTOINCREMENT,
                   context_id   INTEGER NOT NULL REFERENCES contexts(id) ON DELETE CASCADE,
                   domain       TEXT NOT NULL COLLATE NOCASE UNIQUE,
                   created_at   DATETIME NOT NULL DEFAULT (datetime('now'))
                 );
                 CREATE INDEX IF NOT EXISTS idx_context_website_targets_context_id
                   ON context_website_targets(context_id);
                 PRAGMA user_version = 14;",
            )?;
            Ok(())
        })?;
    }
    if user_version < 15 {
        log::info!("db: migrating schema {user_version} -> 15");
        // Per-context accent color (a small curated swatch, picked via
        // right-click on the context's sidebar icon) so contexts can be
        // told apart at a glance in the rail. Column declared in SCHEMA for
        // fresh databases; ensure_table_column is idempotent for databases
        // that already have it.
        run_migration(&mut conn, |conn| {
            ensure_table_column(conn, "contexts", "color", "ALTER TABLE contexts ADD COLUMN color TEXT;")?;
            conn.execute_batch("PRAGMA user_version = 15;")?;
            Ok(())
        })?;
    }
    ensure_cleanup_cache_schema(&conn)?;
    // Index only needed by existing databases: the SCHEMA above declares it
    // for fresh installs, and the v10 migration block adds it for databases
    // created between v10 and v11. This line heals any database that predates
    // the column's index.
    conn.execute_batch(
        "CREATE INDEX IF NOT EXISTS idx_api_calls_transcription_id
           ON api_calls(transcription_id);",
    )?;

    // Self-heal: some databases ended up with user_version >= 7 without the
    // spoken_words column from that migration actually landing (an
    // interrupted migration during the Verenu rename/update). The column
    // addition and backfill run in one transaction so a failed backfill
    // rolls back the column too, letting this retry on the next launch.
    // Errors are propagated: without this column, every transcription
    // insert fails, so running in this state is worse than failing to open.
    if !table_has_column(&conn, "transcriptions", "spoken_words")? {
        let tx = conn.transaction()?;
        let res = (|| -> Result<()> {
            tx.execute_batch("ALTER TABLE transcriptions ADD COLUMN spoken_words INTEGER;")?;
            backfill_spoken_words(&tx)?;
            Ok(())
        })();
        match res {
            Ok(()) => {
                tx.commit()?;
            }
            Err(err) => {
                log::error!("Failed to self-heal spoken_words column: {err}");
                return Err(err);
            }
        }
    }

    Ok(Arc::new(Mutex::new(conn)))
}

fn run_migration(conn: &mut Connection, f: impl FnOnce(&Connection) -> Result<()>) -> Result<()> {
    let tx = conn.transaction()?;
    f(&tx)?;
    tx.commit()?;
    Ok(())
}

/// The schema work of the v2 migration, shared between the `user_version < 2`
/// path (which bumps the version marker afterwards) and the self-heal path for
/// databases stranded by the legacy non-transactional v2 migration. Must run
/// inside a transaction; every statement is idempotent.
fn apply_v2_migration(conn: &Connection) -> Result<()> {
    ensure_table_column(
        conn,
        "snippets",
        "instructions",
        "ALTER TABLE snippets ADD COLUMN instructions TEXT NOT NULL DEFAULT '';",
    )?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS pending_corrections (
           id           INTEGER PRIMARY KEY AUTOINCREMENT,
           wrong_word   TEXT    NOT NULL,
           correct_word TEXT    NOT NULL,
           created_at   DATETIME NOT NULL DEFAULT (datetime('now'))
         );
         CREATE INDEX IF NOT EXISTS idx_pending_words
           ON pending_corrections(wrong_word, correct_word);",
    )?;
    // Migrate dictionary to final schema: term (required) + mistake (optional).
    // No-op when the modern shape is already present.
    rebuild_legacy_dictionary(conn)?;
    Ok(())
}

/// Adds context scoping without changing the existing dictionary/snippet rows.
/// The one-time reassignment is deliberately part of the same transaction as
/// the schema work so an interrupted upgrade cannot strand content without an
/// active context.
fn apply_v12_context_migration(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS contexts (
           id            INTEGER PRIMARY KEY AUTOINCREMENT,
           name          TEXT NOT NULL COLLATE NOCASE UNIQUE,
           is_everywhere INTEGER NOT NULL DEFAULT 0 CHECK (is_everywhere IN (0, 1)),
           created_at    DATETIME NOT NULL DEFAULT (datetime('now')),
           updated_at    DATETIME NOT NULL DEFAULT (datetime('now'))
         );
         CREATE UNIQUE INDEX IF NOT EXISTS idx_contexts_everywhere
           ON contexts(is_everywhere) WHERE is_everywhere = 1;
         CREATE TABLE IF NOT EXISTS context_targets (
           id           INTEGER PRIMARY KEY AUTOINCREMENT,
           context_id   INTEGER NOT NULL REFERENCES contexts(id) ON DELETE CASCADE,
           executable   TEXT NOT NULL COLLATE NOCASE UNIQUE,
           created_at   DATETIME NOT NULL DEFAULT (datetime('now'))
         );
         CREATE INDEX IF NOT EXISTS idx_context_targets_context_id
           ON context_targets(context_id);
         CREATE TABLE IF NOT EXISTS dictionary_contexts (
           context_id    INTEGER NOT NULL REFERENCES contexts(id) ON DELETE CASCADE,
           dictionary_id INTEGER NOT NULL REFERENCES dictionary(id) ON DELETE CASCADE,
           PRIMARY KEY (context_id, dictionary_id)
         );
         CREATE INDEX IF NOT EXISTS idx_dictionary_contexts_dictionary_id
           ON dictionary_contexts(dictionary_id);
         CREATE TABLE IF NOT EXISTS snippet_contexts (
           context_id INTEGER NOT NULL REFERENCES contexts(id) ON DELETE CASCADE,
           snippet_id INTEGER NOT NULL REFERENCES snippets(id) ON DELETE CASCADE,
           PRIMARY KEY (context_id, snippet_id)
         );
         CREATE INDEX IF NOT EXISTS idx_snippet_contexts_snippet_id
           ON snippet_contexts(snippet_id);
         INSERT OR IGNORE INTO contexts (id, name, is_everywhere)
           VALUES (1, 'Everywhere', 1);
         INSERT OR IGNORE INTO dictionary_contexts (context_id, dictionary_id)
           SELECT 1, id FROM dictionary;
         INSERT OR IGNORE INTO snippet_contexts (context_id, snippet_id)
           SELECT 1, id FROM snippets;",
    )?;
    Ok(())
}

/// Rebuilds `dictionary` into the modern `term`/`mistake` shape when the table
/// still has the pre-v3 `wrong`/`correct` columns. No-op when `term` already
/// exists; errors when the shape is undetectable so the caller can surface it
/// instead of silently corrupting data.
fn rebuild_legacy_dictionary(conn: &Connection) -> Result<()> {
    if table_has_column(conn, "dictionary", "term")? {
        return Ok(());
    }
    if !table_has_column(conn, "dictionary", "correct")? {
        anyhow::bail!(
            "dictionary table has neither the modern `term` column nor the legacy `wrong`/`correct` columns"
        );
    }
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS dictionary_v3 (
            id               INTEGER PRIMARY KEY AUTOINCREMENT,
            term             TEXT    NOT NULL UNIQUE,
            mistake          TEXT,
            auto_learned     INTEGER NOT NULL DEFAULT 0,
            correction_count INTEGER NOT NULL DEFAULT 0,
            created_at       DATETIME NOT NULL DEFAULT (datetime('now'))
        );
        INSERT OR IGNORE INTO dictionary_v3
            (id, term, mistake, auto_learned, correction_count, created_at)
            SELECT id,
                   COALESCE(correct, wrong),
                   CASE WHEN correct IS NOT NULL THEN wrong ELSE NULL END,
                   auto_learned, correction_count, created_at
            FROM dictionary;
        DROP TABLE dictionary;
        ALTER TABLE dictionary_v3 RENAME TO dictionary;",
    )?;
    // Complete the modern shape: the v3-era rebuild DDL predates the v4
    // columns, and a legacy table may or may not have gained them before the
    // rebuild ran. Both ensures are idempotent no-ops when already present.
    ensure_table_column(
        conn,
        "dictionary",
        "confidence_tier",
        "ALTER TABLE dictionary ADD COLUMN confidence_tier TEXT NOT NULL DEFAULT 'low';",
    )?;
    ensure_table_column(
        conn,
        "dictionary",
        "last_seen_at",
        "ALTER TABLE dictionary ADD COLUMN last_seen_at DATETIME;",
    )?;
    Ok(())
}

pub fn table_exists(conn: &Connection, name: &str) -> Result<bool> {
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
        [name],
        |r| r.get(0),
    )?;
    Ok(count > 0)
}

pub fn table_has_column(conn: &Connection, table: &str, column: &str) -> Result<bool> {
    let pragma = format!("PRAGMA table_info({table})");
    let mut stmt = conn.prepare(&pragma)?;
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        let name: String = row.get(1)?;
        if name == column {
            return Ok(true);
        }
    }
    Ok(false)
}

fn ensure_table_column(
    conn: &Connection,
    table: &str,
    column: &str,
    def_sql: &str,
) -> Result<bool> {
    if table_has_column(conn, table, column)? {
        return Ok(false);
    }
    conn.execute_batch(def_sql)?;
    Ok(true)
}

fn ensure_cleanup_cache_schema(conn: &Connection) -> Result<()> {
    let mut repaired = false;
    repaired |= ensure_table_column(
        conn,
        "cleanup_cache",
        "created_at_epoch",
        "ALTER TABLE cleanup_cache ADD COLUMN created_at_epoch INTEGER;",
    )?;
    repaired |= ensure_table_column(
        conn,
        "cleanup_cache",
        "last_hit_at_epoch",
        "ALTER TABLE cleanup_cache ADD COLUMN last_hit_at_epoch INTEGER;",
    )?;
    repaired |= ensure_table_column(
        conn,
        "cleanup_cache",
        "expires_at_epoch",
        "ALTER TABLE cleanup_cache ADD COLUMN expires_at_epoch INTEGER;",
    )?;
    repaired |= ensure_table_column(
        conn,
        "cleanup_cache",
        "is_snippet",
        "ALTER TABLE cleanup_cache ADD COLUMN is_snippet INTEGER NOT NULL DEFAULT 0;",
    )?;
    if repaired {
        conn.execute_batch(
            "UPDATE cleanup_cache
             SET created_at_epoch = COALESCE(created_at_epoch, CAST(strftime('%s', created_at || 'Z') AS INTEGER)),
                 last_hit_at_epoch = COALESCE(last_hit_at_epoch, CAST(strftime('%s', last_hit_at || 'Z') AS INTEGER)),
                 expires_at_epoch = COALESCE(expires_at_epoch, CAST(strftime('%s', expires_at || 'Z') AS INTEGER))
             WHERE created_at_epoch IS NULL
                OR last_hit_at_epoch IS NULL
                OR expires_at_epoch IS NULL;",
        )?;
    }
    conn.execute_batch(
        "CREATE INDEX IF NOT EXISTS idx_cleanup_cache_expires_at_epoch
           ON cleanup_cache(expires_at_epoch);
         CREATE INDEX IF NOT EXISTS idx_cleanup_cache_last_hit_at_epoch
           ON cleanup_cache(last_hit_at_epoch);",
    )?;
    Ok(())
}

fn load_snippet_rows(conn: &Connection) -> Result<Vec<Snippet>> {
    let mut snippet_stmt = conn.prepare(
        "SELECT id, trigger, expansion, instructions, use_count, created_at \
         FROM snippets",
    )?;
    let rows = snippet_stmt
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

pub fn compute_spoken_words(conn: &Connection, raw_text: &str) -> Result<i64> {
    let snippets = load_snippet_rows(conn)?;
    Ok(crate::data::snippets::count_words_without_snippet_triggers(
        raw_text, &snippets,
    ))
}

fn backfill_spoken_words(conn: &Connection) -> Result<()> {
    let snippets = load_snippet_rows(conn)?;
    let mut select =
        conn.prepare("SELECT id, raw_text FROM transcriptions WHERE spoken_words IS NULL")?;
    let rows = select
        .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut update = conn.prepare("UPDATE transcriptions SET spoken_words = ?2 WHERE id = ?1")?;

    for (id, raw_text) in rows {
        let spoken_words =
            crate::data::snippets::count_words_without_snippet_triggers(&raw_text, &snippets);
        update.execute(params![id, spoken_words])?;
    }

    Ok(())
}
