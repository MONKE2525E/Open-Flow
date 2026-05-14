use anyhow::Result;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};

pub type Db = Arc<Mutex<Connection>>;

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS transcriptions (
  id          INTEGER PRIMARY KEY AUTOINCREMENT,
  raw_text    TEXT    NOT NULL,
  clean_text  TEXT    NOT NULL,
  words       INTEGER NOT NULL DEFAULT 0,
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
";

pub fn open(path: &str) -> Result<Db> {
    let conn = Connection::open(path)?;
    conn.execute_batch(SCHEMA)?;
    conn.execute_batch("PRAGMA journal_mode=WAL;")?;
    // Migrate existing tables — ignore errors if already applied.
    // IMPORTANT: each migration uses BEGIN/COMMIT for atomicity, followed by an
    // explicit ROLLBACK. If the migration fails mid-way (e.g. referencing a column
    // that no longer exists after a prior migration), sqlite3_exec aborts but leaves
    // the BEGIN open. Without the ROLLBACK cleanup, every subsequent INSERT the app
    // makes would execute inside that ghost transaction and be silently discarded on
    // connection close — causing all user data to vanish on restart.
    let _ = conn
        .execute_batch("ALTER TABLE snippets ADD COLUMN instructions TEXT NOT NULL DEFAULT '';");
    // Migrate dictionary to final schema: term (required) + mistake (optional).
    // Handles all prior states (original wrong/correct columns, or already migrated).
    let _ = conn.execute_batch("
        BEGIN;
        CREATE TABLE IF NOT EXISTS dictionary_v3 (
            id               INTEGER PRIMARY KEY AUTOINCREMENT,
            term             TEXT    NOT NULL UNIQUE,
            mistake          TEXT,
            auto_learned     INTEGER NOT NULL DEFAULT 0,
            correction_count INTEGER NOT NULL DEFAULT 0,
            created_at       DATETIME NOT NULL DEFAULT (datetime('now'))
        );
        INSERT OR IGNORE INTO dictionary_v3 (id, term, mistake, auto_learned, correction_count, created_at)
            SELECT
                id,
                COALESCE(correct, wrong),
                CASE WHEN correct IS NOT NULL THEN wrong ELSE NULL END,
                auto_learned, correction_count, created_at
            FROM dictionary;
        DROP TABLE dictionary;
        ALTER TABLE dictionary_v3 RENAME TO dictionary;
        COMMIT;
    ");
    // Clean up any dangling transaction left by a failed migration above.
    // If the migration succeeded, ROLLBACK errors (no active tx) — let _ = ignores it.
    // If it failed, ROLLBACK undoes the partial BEGIN so the connection is clean.
    let _ = conn.execute_batch("ROLLBACK;");
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

// ---------- queries ----------

pub fn insert_transcription(
    db: &Db,
    raw: &str,
    clean: &str,
    words: i64,
    duration_ms: i64,
    api_used: &str,
) -> Result<()> {
    let conn = db.lock().unwrap();
    conn.execute(
        "INSERT INTO transcriptions (raw_text, clean_text, words, duration_ms, api_used) \
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![raw, clean, words, duration_ms, api_used],
    )?;
    Ok(())
}

pub fn query_recent(db: &Db) -> Result<Vec<RecentEntry>> {
    let conn = db.lock().unwrap();
    let mut stmt = conn.prepare(
        "SELECT id, clean_text, words, created_at \
         FROM transcriptions ORDER BY created_at DESC LIMIT 30",
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
    let conn = db.lock().unwrap();

    let total_words: i64 = conn.query_row(
        "SELECT COALESCE(SUM(words),0) FROM transcriptions",
        [],
        |r| r.get(0),
    )?;

    let avg_wpm: f64 = conn.query_row(
        "SELECT COALESCE(AVG(CAST(words AS REAL)*60000.0/duration_ms),0.0) \
         FROM transcriptions WHERE duration_ms > 0",
        [],
        |r| r.get(0),
    )?;

    let mut stmt =
        conn.prepare("SELECT DISTINCT date(created_at) FROM transcriptions ORDER BY 1 DESC")?;
    let dates: Vec<String> = stmt
        .query_map([], |r| r.get(0))?
        .filter_map(|r| r.ok())
        .collect();

    let today = chrono::Utc::now().date_naive();
    let day_streak = compute_streak(&dates, today);

    Ok(Stats {
        total_words,
        avg_wpm,
        day_streak,
    })
}

/// Pure function: given a descending list of ISO date strings and a reference
/// date, returns the consecutive-day streak ending on or before `today`.
pub fn compute_streak(dates: &[String], today: chrono::NaiveDate) -> i64 {
    let mut streak = 0i64;
    let mut expected = today;
    for d in dates {
        if let Ok(parsed) = chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d") {
            if parsed != expected {
                break;
            }
            streak += 1;
            let Some(prev) = expected.pred_opt() else {
                break;
            };
            expected = prev;
        }
    }
    streak
}

// ---------- dictionary ----------

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DictionaryEntry {
    pub id: i64,
    pub term: String,
    pub mistake: Option<String>,
    pub auto_learned: bool,
    pub correction_count: i64,
    pub created_at: String,
}

pub fn query_dictionary(db: &Db) -> Result<Vec<DictionaryEntry>> {
    let conn = db.lock().unwrap();
    let mut stmt = conn.prepare(
        "SELECT id, term, mistake, auto_learned, correction_count, created_at \
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
                created_at: r.get(5)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

pub fn insert_dictionary_entry(db: &Db, term: &str, mistake: Option<&str>) -> Result<()> {
    let conn = db.lock().unwrap();
    conn.execute(
        "INSERT INTO dictionary (term, mistake) VALUES (?1, ?2)",
        params![term, mistake],
    )?;
    Ok(())
}

pub fn insert_dictionary_entry_auto_learned(db: &Db, term: &str, mistake: Option<&str>) -> Result<()> {
    let conn = db.lock().unwrap();
    conn.execute(
        "INSERT OR IGNORE INTO dictionary (term, mistake, auto_learned) VALUES (?1, ?2, 1)",
        params![term, mistake],
    )?;
    Ok(())
}

pub fn update_dictionary_entry(db: &Db, id: i64, term: &str, mistake: Option<&str>) -> Result<()> {
    let conn = db.lock().unwrap();
    conn.execute(
        "UPDATE dictionary SET term=?2, mistake=?3 WHERE id=?1",
        params![id, term, mistake],
    )?;
    Ok(())
}

pub fn delete_dictionary_entry(db: &Db, id: i64) -> Result<()> {
    let conn = db.lock().unwrap();
    conn.execute("DELETE FROM dictionary WHERE id=?1", params![id])?;
    Ok(())
}

// ---------- snippets ----------

pub fn insert_snippet(db: &Db, trigger: &str, expansion: &str, instructions: &str) -> Result<()> {
    let conn = db.lock().unwrap();
    conn.execute(
        "INSERT INTO snippets (trigger, expansion, instructions) VALUES (?1, ?2, ?3)",
        params![trigger, expansion, instructions],
    )?;
    Ok(())
}

pub fn update_snippet(
    db: &Db,
    id: i64,
    trigger: &str,
    expansion: &str,
    instructions: &str,
) -> Result<()> {
    let conn = db.lock().unwrap();
    conn.execute(
        "UPDATE snippets SET trigger=?2, expansion=?3, instructions=?4 WHERE id=?1",
        params![id, trigger, expansion, instructions],
    )?;
    Ok(())
}

pub fn delete_snippet(db: &Db, id: i64) -> Result<()> {
    let conn = db.lock().unwrap();
    conn.execute("DELETE FROM snippets WHERE id=?1", params![id])?;
    Ok(())
}

pub fn query_snippets(db: &Db) -> Result<Vec<Snippet>> {
    let conn = db.lock().unwrap();
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
    let conn = db.lock().unwrap();
    conn.execute(
        "UPDATE snippets SET use_count = use_count + 1 WHERE id=?1",
        params![id],
    )?;
    Ok(())
}
