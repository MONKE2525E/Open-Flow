use super::snippets;
use anyhow::Result;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex, MutexGuard};

pub type Db = Arc<Mutex<Connection>>;
pub const SNIPPET_TRIGGER_CHAR_LIMIT: usize = 300;
pub const DICTIONARY_ENTRY_CHAR_LIMIT: usize = 120;

fn lock_conn(db: &Db) -> Result<MutexGuard<'_, Connection>> {
    db.lock()
        .map_err(|_| anyhow::anyhow!("Database lock was poisoned"))
}

fn require_nonempty_trimmed(field: &str, value: &str) -> Result<String> {
    let normalized = value.trim();
    if normalized.is_empty() {
        return Err(anyhow::anyhow!("{field} cannot be empty"));
    }
    Ok(normalized.to_string())
}

fn normalize_optional_trimmed(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn normalize_multiline(value: &str) -> String {
    value
        .replace("\r\n", "\n")
        .replace('\r', "\n")
        .trim()
        .to_string()
}

fn validate_char_limit(field: &str, value: &str, limit: usize) -> Result<()> {
    if value.chars().count() > limit {
        return Err(anyhow::anyhow!(
            "{field} must be {limit} characters or fewer"
        ));
    }
    Ok(())
}

fn require_row_changed(changed: usize, item: &str, id: i64) -> Result<()> {
    if changed == 0 {
        return Err(anyhow::anyhow!("{item} {id} was not found"));
    }
    Ok(())
}

fn table_has_column(conn: &Connection, table: &str, column: &str) -> Result<bool> {
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

fn compute_spoken_words(conn: &Connection, raw_text: &str) -> Result<i64> {
    let snippets = load_snippet_rows(conn)?;
    Ok(snippets::count_words_without_snippet_triggers(
        raw_text,
        &snippets,
    ))
}

fn backfill_spoken_words(conn: &Connection) -> Result<()> {
    let snippets = load_snippet_rows(conn)?;
    let mut select = conn.prepare("SELECT id, raw_text FROM transcriptions WHERE spoken_words IS NULL")?;
    let rows = select
        .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut update = conn.prepare("UPDATE transcriptions SET spoken_words = ?2 WHERE id = ?1")?;

    for (id, raw_text) in rows {
        let spoken_words = snippets::count_words_without_snippet_triggers(&raw_text, &snippets);
        update.execute(params![id, spoken_words])?;
    }

    Ok(())
}

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
";

pub fn open(path: &str) -> Result<Db> {
    let mut conn = Connection::open(path)?;
    conn.execute_batch(SCHEMA)?;
    conn.execute_batch("PRAGMA journal_mode=WAL;")?;

    let user_version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;

    if user_version < 2 {
        let db_path = std::path::Path::new(path);
        if db_path.exists() {
            let _ = std::fs::copy(db_path, db_path.with_extension("db.bak"));
        }

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
        conn.execute_batch("BEGIN;")?;
        if let Err(err) = (|| -> Result<()> {
            ensure_table_column(
                &conn,
                "dictionary",
                "confidence_tier",
                "ALTER TABLE dictionary ADD COLUMN confidence_tier TEXT NOT NULL DEFAULT 'low';",
            )?;
            ensure_table_column(
                &conn,
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
        })() {
            let _ = conn.execute_batch("ROLLBACK;");
            return Err(err);
        }
        conn.execute_batch("COMMIT;")?;
    }
    if user_version < 5 {
        conn.execute_batch("BEGIN;")?;
        if let Err(err) = (|| -> Result<()> {
            ensure_cleanup_cache_schema(&conn)?;
            conn.execute_batch("PRAGMA user_version = 5;")?;
            Ok(())
        })() {
            let _ = conn.execute_batch("ROLLBACK;");
            return Err(err);
        }
        conn.execute_batch("COMMIT;")?;
    }
    if user_version < 6 {
        conn.execute_batch("BEGIN;")?;
        if let Err(err) = (|| -> Result<()> {
            ensure_cleanup_cache_schema(&conn)?;
            conn.execute_batch("PRAGMA user_version = 6;")?;
            Ok(())
        })() {
            let _ = conn.execute_batch("ROLLBACK;");
            return Err(err);
        }
        conn.execute_batch("COMMIT;")?;
    }
    if user_version < 7 {
        conn.execute_batch("BEGIN;")?;
        if let Err(err) = (|| -> Result<()> {
            ensure_table_column(
                &conn,
                "transcriptions",
                "spoken_words",
                "ALTER TABLE transcriptions ADD COLUMN spoken_words INTEGER;",
            )?;
            backfill_spoken_words(&conn)?;
            conn.execute_batch("PRAGMA user_version = 7;")?;
            Ok(())
        })() {
            let _ = conn.execute_batch("ROLLBACK;");
            return Err(err);
        }
        conn.execute_batch("COMMIT;")?;
    }
    ensure_cleanup_cache_schema(&conn)?;

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

// ---------- structs ----------

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RecentEntry {
    pub id: i64,
    pub clean_text: String,
    pub words: i64,
    pub created_at: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Stats {
    pub total_words: i64,
    pub avg_wpm: f64,
    pub day_streak: i64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Snippet {
    pub id: i64,
    pub trigger: String,
    pub expansion: String,
    pub instructions: String,
    pub use_count: i64,
    pub created_at: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CreatedRecordMeta {
    pub id: i64,
    pub created_at: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CleanupCacheEntry {
    pub key: String,
    pub clean_text: String,
    pub hit_count: i64,
    pub created_at: String,
    pub last_hit_at: String,
    pub expires_at: String,
    pub is_snippet: bool,
}

// ---------- queries ----------

pub fn insert_transcription_returning(
    db: &Db,
    raw: &str,
    clean: &str,
    words: i64,
    duration_ms: i64,
    api_used: &str,
) -> Result<RecentEntry> {
    let conn = lock_conn(db)?;
    let spoken_words = compute_spoken_words(&conn, raw)?;
    Ok(conn.query_row(
        "INSERT INTO transcriptions (raw_text, clean_text, words, spoken_words, duration_ms, api_used) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6) \
         RETURNING id, clean_text, words, created_at",
        params![raw, clean, words, spoken_words, duration_ms, api_used],
        |r| {
            Ok(RecentEntry {
                id: r.get(0)?,
                clean_text: r.get(1)?,
                words: r.get(2)?,
                created_at: r.get(3)?,
            })
        },
    )?)
}

pub fn query_recent(db: &Db) -> Result<Vec<RecentEntry>> {
    let conn = lock_conn(db)?;
    let mut stmt = conn.prepare(
        "SELECT id, clean_text, words, created_at \
         FROM transcriptions ORDER BY created_at DESC",
    )?;
    let rows = stmt
        .query_map([], |r| {
            Ok(RecentEntry {
                id: r.get(0)?,
                clean_text: r.get(1)?,
                words: r.get(2)?,
                created_at: r.get(3)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

pub fn query_stats(db: &Db) -> Result<Stats> {
    let conn = lock_conn(db)?;

    let total_words: i64 = conn.query_row(
        "SELECT COALESCE(SUM(words),0) FROM transcriptions",
        [],
        |r| r.get(0),
    )?;
    let avg_wpm: f64 = conn.query_row(
        "SELECT COALESCE(AVG(CAST(spoken_words AS REAL) * 60000.0 / duration_ms), 0.0)
         FROM transcriptions
         WHERE duration_ms > 0 AND spoken_words > 0",
        [],
        |r| r.get(0),
    )?;

    let day_streak: i64 = conn.query_row(
        "WITH consecutive AS (
           SELECT DISTINCT date(created_at, 'localtime') AS d
           FROM transcriptions
           ORDER BY d DESC
         )
         SELECT COUNT(*) FROM (
           SELECT d,
                  ROW_NUMBER() OVER (ORDER BY d DESC) AS rn,
                  julianday(date('now','localtime')) - julianday(d) AS gap
           FROM consecutive
         )
         WHERE gap = rn - 1",
        [],
        |r| r.get(0),
    )?;

    Ok(Stats {
        total_words,
        avg_wpm,
        day_streak,
    })
}

// ---------- dictionary ----------

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

#[allow(dead_code)]
pub fn insert_dictionary_entry(db: &Db, term: &str, mistake: Option<&str>) -> Result<()> {
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
    conn.execute(
        "INSERT INTO dictionary (term, mistake, confidence_tier, last_seen_at) VALUES (?1, ?2, 'manual', datetime('now'))",
        params![normalized_term, normalized_mistake],
    )?;
    Ok(())
}

pub fn insert_dictionary_entry_returning(
    db: &Db,
    term: &str,
    mistake: Option<&str>,
) -> Result<CreatedRecordMeta> {
    let normalized_term = require_nonempty_trimmed("Term", term)?;
    let normalized_mistake = normalize_optional_trimmed(mistake);
    validate_char_limit("Term", &normalized_term, DICTIONARY_ENTRY_CHAR_LIMIT)?;
    if let Some(m) = normalized_mistake.as_deref() {
        validate_char_limit("Often mistranscribed as", m, DICTIONARY_ENTRY_CHAR_LIMIT)?;
    }

    // Insert and read last_insert_rowid under a single lock to prevent another
    // thread's insert racing between the two acquisitions and returning the wrong id.
    let conn = lock_conn(db)?;
    conn.execute(
        "INSERT INTO dictionary (term, mistake, confidence_tier, last_seen_at) VALUES (?1, ?2, 'manual', datetime('now'))",
        params![normalized_term, normalized_mistake],
    )?;
    let id = conn.last_insert_rowid();
    let created_at = conn.query_row(
        "SELECT created_at FROM dictionary WHERE id=?1",
        params![id],
        |r| r.get(0),
    )?;
    Ok(CreatedRecordMeta { id, created_at })
}

pub fn insert_dictionary_entry_from_backup(
    db: &Db,
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
    let conn = lock_conn(db)?;
    conn.execute(
        "INSERT INTO dictionary (term, mistake, auto_learned, correction_count, confidence_tier, last_seen_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, datetime('now'))",
        params![normalized_term, normalized_mistake, auto_learned as i64, correction_count, confidence_tier],
    )?;
    Ok(())
}

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

pub fn mark_auto_learn_candidate_promoted(db: &Db, wrong: &str, correct: &str) -> Result<()> {
    let conn = lock_conn(db)?;
    conn.execute(
        "UPDATE auto_learn_candidates
         SET promoted_at = datetime('now')
         WHERE wrong_word = ?1 AND correct_word = ?2",
        params![wrong, correct],
    )?;
    Ok(())
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

pub fn insert_pending_correction(db: &Db, wrong: &str, correct: &str) -> Result<()> {
    let conn = lock_conn(db)?;
    conn.execute(
        "INSERT INTO pending_corrections (wrong_word, correct_word) VALUES (?1, ?2)",
        params![wrong, correct],
    )?;
    Ok(())
}

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

// ---------- snippets ----------

#[allow(dead_code)]
pub fn insert_snippet(db: &Db, trigger: &str, expansion: &str, instructions: &str) -> Result<()> {
    let normalized_trigger = require_nonempty_trimmed("Trigger", trigger)?;
    validate_char_limit("Trigger", &normalized_trigger, SNIPPET_TRIGGER_CHAR_LIMIT)?;
    let normalized_expansion = normalize_multiline(expansion);
    if normalized_expansion.is_empty() {
        return Err(anyhow::anyhow!("Expansion cannot be empty"));
    }
    let normalized_instructions = normalize_multiline(instructions);

    let conn = lock_conn(db)?;
    conn.execute(
        "INSERT INTO snippets (trigger, expansion, instructions) VALUES (?1, ?2, ?3)",
        params![
            normalized_trigger,
            normalized_expansion,
            normalized_instructions
        ],
    )?;
    Ok(())
}

pub fn insert_snippet_returning(
    db: &Db,
    trigger: &str,
    expansion: &str,
    instructions: &str,
) -> Result<CreatedRecordMeta> {
    let normalized_trigger = require_nonempty_trimmed("Trigger", trigger)?;
    validate_char_limit("Trigger", &normalized_trigger, SNIPPET_TRIGGER_CHAR_LIMIT)?;
    let normalized_expansion = normalize_multiline(expansion);
    if normalized_expansion.is_empty() {
        return Err(anyhow::anyhow!("Expansion cannot be empty"));
    }
    let normalized_instructions = normalize_multiline(instructions);

    // Insert and read last_insert_rowid under a single lock to prevent another
    // thread's insert racing between the two acquisitions and returning the wrong id.
    let conn = lock_conn(db)?;
    conn.execute(
        "INSERT INTO snippets (trigger, expansion, instructions) VALUES (?1, ?2, ?3)",
        params![
            normalized_trigger,
            normalized_expansion,
            normalized_instructions
        ],
    )?;
    let id = conn.last_insert_rowid();
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
    let normalized_instructions = normalize_multiline(instructions);

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
    let conn = lock_conn(db)?;
    let changed = conn.execute("DELETE FROM snippets WHERE id=?1", params![id])?;
    require_row_changed(changed, "Snippet", id)?;
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

// ---------- cleanup cache ----------

pub fn cleanup_cache_get_active(db: &Db, key: &str) -> Result<Option<CleanupCacheEntry>> {
    let conn = lock_conn(db)?;
    let now_epoch = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);

    let mut epoch_stmt = conn.prepare(
        "SELECT key,
                clean_text,
                hit_count,
                COALESCE(datetime(created_at_epoch, 'unixepoch'), created_at),
                COALESCE(datetime(last_hit_at_epoch, 'unixepoch'), last_hit_at),
                COALESCE(datetime(expires_at_epoch, 'unixepoch'), expires_at),
                is_snippet
         FROM cleanup_cache
         WHERE key = ?1
           AND expires_at_epoch > ?2
         LIMIT 1",
    )?;
    let mut epoch_rows = epoch_stmt.query(params![key, now_epoch])?;
    if let Some(row) = epoch_rows.next()? {
        return Ok(Some(CleanupCacheEntry {
            key: row.get(0)?,
            clean_text: row.get(1)?,
            hit_count: row.get(2)?,
            created_at: row.get(3)?,
            last_hit_at: row.get(4)?,
            expires_at: row.get(5)?,
            is_snippet: row.get::<_, i64>(6)? != 0,
        }));
    }

    let mut fallback_stmt = conn.prepare(
        "SELECT key,
                clean_text,
                hit_count,
                created_at,
                last_hit_at,
                expires_at,
                is_snippet
         FROM cleanup_cache
         WHERE key = ?1
           AND expires_at_epoch IS NULL
           AND expires_at > datetime('now')
         LIMIT 1",
    )?;
    let mut fallback_rows = fallback_stmt.query(params![key])?;
    let Some(row) = fallback_rows.next()? else {
        return Ok(None);
    };
    Ok(Some(CleanupCacheEntry {
        key: row.get(0)?,
        clean_text: row.get(1)?,
        hit_count: row.get(2)?,
        created_at: row.get(3)?,
        last_hit_at: row.get(4)?,
        expires_at: row.get(5)?,
        is_snippet: row.get::<_, i64>(6)? != 0,
    }))
}

pub fn cleanup_cache_insert_new(
    db: &Db,
    key: &str,
    clean_text: &str,
    expires_at: &str,
    is_snippet: bool,
) -> Result<()> {
    let conn = lock_conn(db)?;
    conn.execute(
        "INSERT OR REPLACE INTO cleanup_cache
         (key, clean_text, hit_count, created_at, last_hit_at, expires_at,
          created_at_epoch, last_hit_at_epoch, expires_at_epoch, is_snippet)
         VALUES (?1, ?2, 1, datetime('now'), datetime('now'), ?3,
                 CAST(strftime('%s', 'now') AS INTEGER),
                 CAST(strftime('%s', 'now') AS INTEGER),
                 CAST(strftime('%s', ?3 || 'Z') AS INTEGER),
                 ?4)",
        params![key, clean_text, expires_at, is_snippet as i64],
    )?;
    Ok(())
}

pub fn cleanup_cache_touch_hit(
    db: &Db,
    key: &str,
    new_hit_count: i64,
    last_hit_at: &str,
    expires_at: &str,
) -> Result<()> {
    let conn = lock_conn(db)?;
    conn.execute(
        "UPDATE cleanup_cache
         SET hit_count = ?2,
             last_hit_at = ?3,
             expires_at = ?4,
             last_hit_at_epoch = CAST(strftime('%s', ?3 || 'Z') AS INTEGER),
             expires_at_epoch = CAST(strftime('%s', ?4 || 'Z') AS INTEGER)
         WHERE key = ?1",
        params![key, new_hit_count, last_hit_at, expires_at],
    )?;
    Ok(())
}

pub fn cleanup_cache_prune_expired(db: &Db) -> Result<usize> {
    let conn = lock_conn(db)?;
    let changed_epoch = conn.execute(
        "DELETE FROM cleanup_cache
         WHERE (expires_at_epoch IS NOT NULL
                AND expires_at_epoch <= CAST(strftime('%s', 'now') AS INTEGER))
            OR (last_hit_at_epoch IS NOT NULL
                AND last_hit_at_epoch <= CAST(strftime('%s', 'now', '-2 days') AS INTEGER)
                AND is_snippet = 0)",
        [],
    )?;
    let changed_fallback = conn.execute(
        "DELETE FROM cleanup_cache
         WHERE (expires_at_epoch IS NULL
                AND expires_at <= datetime('now'))
            OR (last_hit_at_epoch IS NULL
                AND last_hit_at <= datetime('now', '-2 days')
                AND is_snippet = 0)",
        [],
    )?;
    Ok(changed_epoch + changed_fallback)
}

pub fn cleanup_cache_clear_all(db: &Db) -> Result<usize> {
    let conn = lock_conn(db)?;
    let changed = conn.execute("DELETE FROM cleanup_cache", [])?;
    Ok(changed)
}

pub fn cleanup_cache_count(db: &Db) -> Result<i64> {
    let conn = lock_conn(db)?;
    conn.query_row("SELECT COUNT(*) FROM cleanup_cache", [], |r| r.get(0))
        .map_err(Into::into)
}

pub fn cleanup_cache_delete_by_key(db: &Db, key: &str) -> Result<()> {
    let conn = lock_conn(db)?;
    conn.execute("DELETE FROM cleanup_cache WHERE key = ?1", params![key])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_db() -> Db {
        open(":memory:").expect("test db")
    }

    fn temp_db_path(name: &str) -> std::path::PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "open_flow_{name}_{}_{}.db",
            std::process::id(),
            nanos
        ))
    }

    #[test]
    fn open_repairs_legacy_cleanup_cache_missing_epoch_columns() {
        let path = temp_db_path("legacy_cleanup_cache");
        {
            let conn = Connection::open(&path).expect("create legacy db");
            conn.execute_batch(
                "CREATE TABLE cleanup_cache (
                   key         TEXT PRIMARY KEY,
                   clean_text  TEXT NOT NULL,
                   hit_count   INTEGER NOT NULL DEFAULT 0,
                   created_at  DATETIME NOT NULL DEFAULT (datetime('now')),
                   last_hit_at DATETIME NOT NULL DEFAULT (datetime('now')),
                   expires_at  DATETIME NOT NULL,
                   is_snippet  INTEGER NOT NULL DEFAULT 0
                 );
                 INSERT INTO cleanup_cache
                   (key, clean_text, hit_count, created_at, last_hit_at, expires_at, is_snippet)
                 VALUES
                   ('legacy', 'hello', 1, '2026-01-01 00:00:00', '2026-01-01 00:00:00', '2999-01-01 00:00:00', 0);
                 PRAGMA user_version = 6;",
            )
            .expect("seed legacy db");
        }

        let db = open(path.to_str().expect("path string")).expect("open repairs legacy db");
        assert!(cleanup_cache_get_active(&db, "legacy")
            .expect("query repaired row")
            .is_some());

        let conn = lock_conn(&db).expect("lock");
        assert!(table_has_column(&conn, "cleanup_cache", "expires_at_epoch").expect("column"));
        assert!(table_has_column(&conn, "cleanup_cache", "last_hit_at_epoch").expect("column"));
        drop(conn);
        drop(db);
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(path.with_extension("db-wal"));
        let _ = std::fs::remove_file(path.with_extension("db-shm"));
    }

    #[test]
    fn open_backfills_legacy_spoken_words_column() {
        let path = temp_db_path("legacy_spoken_words");
        {
            let conn = Connection::open(&path).expect("create legacy db");
            conn.execute_batch(
                "CREATE TABLE transcriptions (
                   id INTEGER PRIMARY KEY AUTOINCREMENT,
                   raw_text TEXT NOT NULL,
                   clean_text TEXT NOT NULL,
                   words INTEGER NOT NULL DEFAULT 0,
                   duration_ms INTEGER NOT NULL DEFAULT 0,
                   api_used TEXT NOT NULL DEFAULT '',
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
                 INSERT INTO snippets (trigger, expansion) VALUES ('sig', 'signature');
                 INSERT INTO transcriptions (raw_text, clean_text, words, duration_ms, api_used)
                   VALUES ('hello sig world', 'clean', 3, 1000, 'test');
                 PRAGMA user_version = 6;",
            )
            .expect("seed legacy db");
        }

        let db = open(path.to_str().expect("path string")).expect("open repairs legacy db");
        let conn = lock_conn(&db).expect("lock");
        let spoken_words: i64 = conn
            .query_row(
                "SELECT spoken_words FROM transcriptions LIMIT 1",
                [],
                |r| r.get(0),
            )
            .expect("spoken words");
        assert_eq!(spoken_words, 2);
        drop(conn);
        drop(db);
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(path.with_extension("db-wal"));
        let _ = std::fs::remove_file(path.with_extension("db-shm"));
    }

    #[test]
    fn open_repairs_db_stuck_at_v7_without_spoken_words_column() {
        // Simulates a database left at user_version = 7 by an interrupted
        // migration (e.g. during the Verenu rename/update) where the
        // ALTER TABLE for spoken_words never actually landed. The
        // `if user_version < 7` migration block would never run again for
        // such a database, so it must be self-healed unconditionally.
        let path = temp_db_path("v7_missing_spoken_words");
        {
            let conn = Connection::open(&path).expect("create stuck db");
            conn.execute_batch(
                "CREATE TABLE transcriptions (
                   id INTEGER PRIMARY KEY AUTOINCREMENT,
                   raw_text TEXT NOT NULL,
                   clean_text TEXT NOT NULL,
                   words INTEGER NOT NULL DEFAULT 0,
                   duration_ms INTEGER NOT NULL DEFAULT 0,
                   api_used TEXT NOT NULL DEFAULT '',
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
                 INSERT INTO snippets (trigger, expansion) VALUES ('sig', 'signature');
                 INSERT INTO transcriptions (raw_text, clean_text, words, duration_ms, api_used)
                   VALUES ('hello sig world', 'clean', 3, 1000, 'test');
                 PRAGMA user_version = 7;",
            )
            .expect("seed stuck db");
        }

        let db = open(path.to_str().expect("path string")).expect("open repairs stuck db");

        // Inserting a new transcription must succeed now that spoken_words exists.
        insert_transcription_returning(&db, "second clip", "second clip", 2, 1000, "test")
            .expect("insert after repair");

        let conn = lock_conn(&db).expect("lock");
        let spoken_words: i64 = conn
            .query_row(
                "SELECT spoken_words FROM transcriptions WHERE raw_text = 'hello sig world'",
                [],
                |r| r.get(0),
            )
            .expect("spoken words backfilled");
        assert_eq!(spoken_words, 2);
        drop(conn);
        drop(db);
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(path.with_extension("db-wal"));
        let _ = std::fs::remove_file(path.with_extension("db-shm"));
    }

    #[test]
    fn auto_learn_does_not_overwrite_manual_dictionary_entry() {
        let db = test_db();

        insert_dictionary_entry(&db, "Kubernetes", Some("manual typo")).expect("manual insert");
        let promoted =
            insert_dictionary_entry_auto_learned(&db, "Kubernetes", Some("Koobernetes"), "high")
                .expect("auto insert");

        assert!(!promoted);
        let entries = query_dictionary(&db).expect("dictionary");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].term, "Kubernetes");
        assert_eq!(entries[0].mistake.as_deref(), Some("manual typo"));
        assert!(!entries[0].auto_learned);
        assert_eq!(entries[0].correction_count, 0);
    }

    #[test]
    fn auto_learn_updates_only_exact_existing_pair() {
        let db = test_db();

        assert!(insert_dictionary_entry_auto_learned(
            &db,
            "Kubernetes",
            Some("Koobernetes"),
            "high"
        )
        .expect("first insert"));
        assert!(insert_dictionary_entry_auto_learned(
            &db,
            "Kubernetes",
            Some("Koobernetes"),
            "high"
        )
        .expect("same pair"));
        assert!(!insert_dictionary_entry_auto_learned(
            &db,
            "Kubernetes",
            Some("Kubernetties"),
            "low",
        )
        .expect("different pair"));

        let entries = query_dictionary(&db).expect("dictionary");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].mistake.as_deref(), Some("Koobernetes"));
        assert_eq!(entries[0].correction_count, 2);
    }

    #[test]
    fn cleanup_cache_insert_get_and_clear() {
        let db = test_db();
        cleanup_cache_insert_new(&db, "abc", "hello", "2999-01-01 00:00:00", false)
            .expect("insert");
        let hit = cleanup_cache_get_active(&db, "abc")
            .expect("query")
            .expect("exists");
        assert_eq!(hit.clean_text, "hello");
        assert_eq!(hit.hit_count, 1);

        assert_eq!(cleanup_cache_count(&db).expect("count"), 1);
        assert_eq!(cleanup_cache_clear_all(&db).expect("clear"), 1);
        assert_eq!(cleanup_cache_count(&db).expect("count"), 0);
    }

    #[test]
    fn cleanup_cache_prunes_expired_only() {
        let db = test_db();
        cleanup_cache_insert_new(&db, "old", "x", "2000-01-01 00:00:00", false)
            .expect("insert old");
        cleanup_cache_insert_new(&db, "live", "y", "2999-01-01 00:00:00", false)
            .expect("insert live");

        assert_eq!(cleanup_cache_prune_expired(&db).expect("prune"), 1);
        assert!(cleanup_cache_get_active(&db, "old")
            .expect("query old")
            .is_none());
        assert!(cleanup_cache_get_active(&db, "live")
            .expect("query live")
            .is_some());
    }

    #[test]
    fn cleanup_cache_get_active_supports_null_epoch_fallback() {
        let db = test_db();
        cleanup_cache_insert_new(&db, "legacy", "hello", "2999-01-01 00:00:00", false)
            .expect("insert");

        let conn = lock_conn(&db).expect("lock");
        conn.execute(
            "UPDATE cleanup_cache
             SET expires_at_epoch = NULL
             WHERE key = 'legacy'",
            [],
        )
        .expect("null out epoch");
        drop(conn);

        assert!(cleanup_cache_get_active(&db, "legacy")
            .expect("query")
            .is_some());
    }

    #[test]
    fn cleanup_cache_get_active_handles_null_created_and_last_hit_epochs() {
        let db = test_db();
        cleanup_cache_insert_new(&db, "partial", "hello", "2999-01-01 00:00:00", false)
            .expect("insert");

        let conn = lock_conn(&db).expect("lock");
        conn.execute(
            "UPDATE cleanup_cache
             SET created_at_epoch = NULL,
                 last_hit_at_epoch = NULL
             WHERE key = 'partial'",
            [],
        )
        .expect("null out partial epochs");
        drop(conn);

        let row = cleanup_cache_get_active(&db, "partial")
            .expect("query")
            .expect("row exists");
        assert_eq!(row.key, "partial");
        assert_eq!(row.clean_text, "hello");
    }

    #[test]
    fn cleanup_cache_epoch_columns_treat_utc_text_as_utc() {
        let db = test_db();
        cleanup_cache_insert_new(&db, "utc", "value", "2026-01-01 00:00:00", false)
            .expect("insert");

        let conn = lock_conn(&db).expect("lock");
        let inserted_expiry_epoch: i64 = conn
            .query_row(
                "SELECT expires_at_epoch FROM cleanup_cache WHERE key = 'utc'",
                [],
                |r| r.get(0),
            )
            .expect("select insert epoch");
        drop(conn);
        assert_eq!(inserted_expiry_epoch, 1_767_225_600);

        cleanup_cache_touch_hit(&db, "utc", 2, "2026-01-02 03:04:05", "2026-02-03 04:05:06")
            .expect("touch");
        let conn = lock_conn(&db).expect("lock");
        let (last_hit_epoch, expires_epoch): (i64, i64) = conn
            .query_row(
                "SELECT last_hit_at_epoch, expires_at_epoch FROM cleanup_cache WHERE key = 'utc'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .expect("select touched epochs");

        assert_eq!(last_hit_epoch, 1_767_323_045);
        assert_eq!(expires_epoch, 1_770_091_506);
    }

    #[test]
    fn cache_rejection_delete_removes_entry() {
        let db = test_db();
        cleanup_cache_insert_new(&db, "key1", "bad answer", "2999-01-01 00:00:00", false)
            .expect("insert");

        // Verify it's cached.
        assert!(cleanup_cache_get_active(&db, "key1")
            .expect("get")
            .is_some());

        // Simulate rejection monitor firing.
        cleanup_cache_delete_by_key(&db, "key1").expect("delete");

        // Entry must be gone â€” next dictation will hit the LLM.
        assert!(cleanup_cache_get_active(&db, "key1")
            .expect("get after")
            .is_none());
        assert_eq!(cleanup_cache_count(&db).expect("count"), 0);
    }

    #[test]
    fn cache_rejection_leaves_other_keys_intact() {
        let db = test_db();
        cleanup_cache_insert_new(&db, "target", "bad", "2999-01-01 00:00:00", false)
            .expect("target");
        cleanup_cache_insert_new(&db, "bystander", "good", "2999-01-01 00:00:00", false)
            .expect("bystander");

        cleanup_cache_delete_by_key(&db, "target").expect("delete");

        assert!(cleanup_cache_get_active(&db, "target")
            .expect("target")
            .is_none());
        assert!(
            cleanup_cache_get_active(&db, "bystander")
                .expect("bystander")
                .is_some(),
            "unrelated entry must survive"
        );
    }

    #[test]
    fn cache_rejection_after_hit_removes_entry() {
        let db = test_db();
        cleanup_cache_insert_new(&db, "k", "stale text", "2999-01-01 00:00:00", false)
            .expect("insert");

        // Simulate a cache hit (the phrase was served from cache once).
        cleanup_cache_touch_hit(&db, "k", 2, "2026-01-01 00:00:00", "2999-01-01 00:00:00")
            .expect("touch");

        let hit = cleanup_cache_get_active(&db, "k")
            .expect("get")
            .expect("exists");
        assert_eq!(hit.hit_count, 2);

        // User deletes output â†’ rejection monitor fires.
        cleanup_cache_delete_by_key(&db, "k").expect("delete");

        assert!(cleanup_cache_get_active(&db, "k")
            .expect("get after")
            .is_none());
    }

    #[test]
    fn stats_avg_wpm_ignores_snippet_triggers_even_when_stored_words_are_inflated() {
        let db = test_db();
        insert_snippet(
            &db,
            "sig",
            "A long email signature with a bunch of words",
            "",
        )
        .expect("snippet");
        insert_transcription_returning(&db, "sig.", "A long email signature", 9, 2000, "test")
            .expect("transcription");

        let stats = query_stats(&db).expect("stats");

        assert_eq!(stats.total_words, 9);
        assert_eq!(stats.avg_wpm, 0.0);
    }

    #[test]
    fn stats_avg_wpm_counts_only_non_snippet_spoken_words() {
        let db = test_db();
        insert_snippet(&db, "sig", "A long email signature", "").expect("snippet");
        insert_transcription_returning(
            &db,
            "please add sig thanks",
            "Please add signature",
            4,
            2000,
            "test",
        )
        .expect("transcription");

        let stats = query_stats(&db).expect("stats");

        assert_eq!(stats.avg_wpm, 90.0);
    }

    #[test]
    fn stats_avg_wpm_excludes_pure_snippet_rows_from_average() {
        let db = test_db();
        insert_snippet(&db, "sig", "A long email signature", "").expect("snippet");
        insert_transcription_returning(&db, "hello world", "hello world", 2, 1000, "test")
            .expect("normal transcription");
        insert_transcription_returning(&db, "sig.", "A long email signature", 4, 1000, "test")
            .expect("snippet transcription");

        let stats = query_stats(&db).expect("stats");

        assert_eq!(stats.avg_wpm, 120.0);
    }

    #[test]
    fn stats_avg_wpm_streams_large_transcription_sets() {
        let db = test_db();
        insert_snippet(&db, "sig", "signature block", "").expect("snippet");

        for idx in 0..500 {
            insert_transcription_returning(
                &db,
                if idx % 2 == 0 {
                    "hello world"
                } else {
                    "hello sig world"
                },
                "clean",
                3,
                1000,
                "test",
            )
            .expect("transcription");
        }

        let stats = query_stats(&db).expect("stats");

        assert_eq!(stats.total_words, 1500);
        assert!(stats.avg_wpm > 0.0);
    }

    #[test]
    fn dict_rejection_only_removes_auto_learned_entries() {
        let db = test_db();

        // Manual entry â€” must survive rejection.
        insert_dictionary_entry(&db, "groq", Some("grog")).expect("manual");
        let manual_id = query_dictionary(&db)
            .expect("query")
            .into_iter()
            .find(|e| e.term == "groq")
            .expect("find")
            .id;

        // Auto-learned entry â€” must be removed.
        insert_dictionary_entry_auto_learned(&db, "Tauri", Some("Tari"), "high").expect("auto");
        let auto_id = query_dictionary(&db)
            .expect("query")
            .into_iter()
            .find(|e| e.term == "Tauri")
            .expect("find")
            .id;

        delete_auto_learned_entries_by_ids(&db, &[manual_id, auto_id]).expect("reject");

        let remaining: Vec<_> = query_dictionary(&db).expect("query after");
        assert_eq!(remaining.len(), 1, "only manual entry survives");
        assert_eq!(remaining[0].term, "groq");
    }

    #[test]
    fn dict_rejection_cleans_up_pending_corrections() {
        let db = test_db();

        insert_dictionary_entry_auto_learned(&db, "Tauri", Some("Tari"), "low").expect("insert");
        let id = query_dictionary(&db)
            .expect("query")
            .into_iter()
            .next()
            .expect("entry")
            .id;

        // Simulate pending correction records that led to the promotion.
        insert_pending_correction(&db, "Tari", "Tauri").expect("pending 1");
        insert_pending_correction(&db, "Tari", "Tauri").expect("pending 2");
        assert_eq!(
            count_pending_corrections_recent(&db, "Tari", "Tauri", 7).expect("count"),
            2
        );

        // Rejection monitor fires.
        delete_auto_learned_entries_by_ids(&db, &[id]).expect("reject");

        // Dictionary entry gone.
        assert_eq!(query_dictionary(&db).expect("query after").len(), 0);
        // Pending corrections also purged â€” prevents immediate re-promotion.
        assert_eq!(
            count_pending_corrections_recent(&db, "Tari", "Tauri", 7).expect("count after"),
            0
        );
    }

    #[test]
    fn cache_rejection_full_lifecycle() {
        // End-to-end: insert â†’ hit (cache serves stale) â†’ reject â†’ miss (LLM runs again).
        let db = test_db();
        let key = "chromium-is-a-web-browser-base";
        let bad_answer = "bad cached answer";

        // First dictation: LLM runs, result cached.
        cleanup_cache_insert_new(&db, key, bad_answer, "2999-01-01 00:00:00", false)
            .expect("insert");
        assert_eq!(cleanup_cache_count(&db).expect("count"), 1);

        // Second dictation: cache hit, stale answer served.
        let entry = cleanup_cache_get_active(&db, key)
            .expect("get")
            .expect("hit");
        assert_eq!(entry.clean_text, bad_answer);
        cleanup_cache_touch_hit(&db, key, 2, "2026-01-01 00:00:00", "2999-01-01 00:00:00")
            .expect("touch");

        // User deletes output within 10s â†’ monitor fires.
        cleanup_cache_delete_by_key(&db, key).expect("delete");

        // Third dictation: cache miss, LLM runs again with fresh context.
        assert!(
            cleanup_cache_get_active(&db, key)
                .expect("get after")
                .is_none(),
            "cache must be empty after rejection so next dictation hits the LLM"
        );
    }

    #[test]
    fn dict_rejection_bulk_removes_multiple_entries() {
        let db = test_db();
        insert_dictionary_entry_auto_learned(&db, "groq", Some("grog"), "high").expect("1");
        insert_dictionary_entry_auto_learned(&db, "Tauri", Some("Tari"), "high").expect("2");
        let ids: Vec<i64> = query_dictionary(&db)
            .expect("query")
            .iter()
            .map(|e| e.id)
            .collect();
        assert_eq!(ids.len(), 2);
        delete_auto_learned_entries_by_ids(&db, &ids).expect("bulk delete");
        assert_eq!(query_dictionary(&db).expect("after").len(), 0);
    }

    #[test]
    fn manual_dictionary_entries_trim_and_keep_longer_phrases() {
        let db = test_db();
        let long_term =
            "A longer dictionary phrase that still fits inside the supported limit exactly fine";
        let long_mistake = "A slightly mangled version of that longer phrase for recognition";

        insert_dictionary_entry(
            &db,
            &format!("  {long_term}  "),
            Some(&format!("  {long_mistake}  ")),
        )
        .expect("insert trimmed long entry");

        let entry = query_dictionary(&db)
            .expect("query")
            .into_iter()
            .next()
            .expect("entry");
        assert_eq!(entry.term, long_term);
        assert_eq!(entry.mistake.as_deref(), Some(long_mistake));
    }

    #[test]
    fn dictionary_entry_rejects_values_beyond_limit() {
        let db = test_db();
        let too_long = "x".repeat(DICTIONARY_ENTRY_CHAR_LIMIT + 1);
        let err = insert_dictionary_entry(&db, &too_long, None).expect_err("reject long term");
        assert!(
            err.to_string().contains("120 characters or fewer"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn snippet_update_normalizes_expansion_whitespace() {
        let db = test_db();
        insert_snippet(&db, "sig", "Hi", "").expect("insert");
        let snippet = query_snippets(&db)
            .expect("query")
            .into_iter()
            .next()
            .expect("snippet");
        // Pasted text often arrives with CRLF line endings and trailing whitespace/newlines.
        // The backend normalizes these so paste behaves like typing.
        update_snippet(&db, snippet.id, "sig", "Line one\r\nLine two  \n", "").expect("update");
        let updated = query_snippets(&db)
            .expect("query after")
            .into_iter()
            .next()
            .expect("updated");
        assert_eq!(updated.expansion, "Line one\nLine two");
    }

    #[test]
    fn deleting_missing_entries_returns_an_error() {
        let db = test_db();
        let dict_err = delete_dictionary_entry(&db, 999).expect_err("missing dictionary entry");
        assert!(
            dict_err
                .to_string()
                .contains("Dictionary entry 999 was not found"),
            "unexpected dictionary error: {dict_err}"
        );

        let snippet_err = delete_snippet(&db, 999).expect_err("missing snippet");
        assert!(
            snippet_err
                .to_string()
                .contains("Snippet 999 was not found"),
            "unexpected snippet error: {snippet_err}"
        );
    }

    #[test]
    fn manual_delete_of_auto_learned_entry_purges_pending_corrections() {
        let db = test_db();

        insert_dictionary_entry_auto_learned(&db, "Tauri", Some("Tari"), "low").expect("insert");
        let id = query_dictionary(&db)
            .expect("query")
            .into_iter()
            .next()
            .expect("entry")
            .id;

        insert_pending_correction(&db, "Tari", "Tauri").expect("pending");
        assert_eq!(
            count_pending_corrections_recent(&db, "Tari", "Tauri", 7).expect("count before"),
            1
        );

        delete_dictionary_entry(&db, id).expect("delete");

        assert_eq!(query_dictionary(&db).expect("query after").len(), 0);
        assert_eq!(
            count_pending_corrections_recent(&db, "Tari", "Tauri", 7).expect("count after"),
            0,
            "pending corrections must be purged when the auto-learned entry is manually deleted"
        );
    }
}
