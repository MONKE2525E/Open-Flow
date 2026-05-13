use anyhow::Result;
use rusqlite::{Connection, params};
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
  wrong            TEXT    NOT NULL UNIQUE,
  correct          TEXT    NOT NULL,
  auto_learned     INTEGER NOT NULL DEFAULT 0,
  correction_count INTEGER NOT NULL DEFAULT 0,
  created_at       DATETIME NOT NULL DEFAULT (datetime('now'))
);
CREATE TABLE IF NOT EXISTS snippets (
  id         INTEGER PRIMARY KEY AUTOINCREMENT,
  trigger    TEXT    NOT NULL UNIQUE,
  expansion  TEXT    NOT NULL,
  use_count  INTEGER NOT NULL DEFAULT 0,
  created_at DATETIME NOT NULL DEFAULT (datetime('now'))
);
";

pub fn open(path: &str) -> Result<Db> {
    let conn = Connection::open(path)?;
    conn.execute_batch(SCHEMA)?;
    conn.execute_batch("PRAGMA journal_mode=WAL;")?;
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
    pub use_count: i64,
    pub created_at: String,
}

// ---------- queries ----------

pub fn insert_transcription(
    db: &Db,
    raw: &str, clean: &str,
    words: i64, duration_ms: i64, api_used: &str,
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
    let rows = stmt.query_map([], |r| {
        Ok(RecentEntry {
            id:         r.get(0)?,
            clean_text: r.get(1)?,
            words:      r.get(2)?,
            created_at: r.get(3)?,
        })
    })?
    .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

pub fn query_stats(db: &Db) -> Result<Stats> {
    let conn = db.lock().unwrap();

    let total_words: i64 = conn.query_row(
        "SELECT COALESCE(SUM(words),0) FROM transcriptions", [],
        |r| r.get(0),
    )?;

    let avg_wpm: f64 = conn.query_row(
        "SELECT COALESCE(AVG(CAST(words AS REAL)*60000.0/duration_ms),0.0) \
         FROM transcriptions WHERE duration_ms > 0", [],
        |r| r.get(0),
    )?;

    let mut stmt = conn.prepare(
        "SELECT DISTINCT date(created_at) FROM transcriptions ORDER BY 1 DESC"
    )?;
    let dates: Vec<String> = stmt.query_map([], |r| r.get(0))?
        .filter_map(|r| r.ok())
        .collect();

    let today = chrono::Utc::now().date_naive();
    let day_streak = compute_streak(&dates, today);

    Ok(Stats { total_words, avg_wpm, day_streak })
}

/// Pure function: given a descending list of ISO date strings and a reference
/// date, returns the consecutive-day streak ending on or before `today`.
pub fn compute_streak(dates: &[String], today: chrono::NaiveDate) -> i64 {
    let mut streak = 0i64;
    let mut expected = today;
    for d in dates {
        if let Ok(parsed) = chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d") {
            if parsed != expected { break; }
            streak += 1;
            let Some(prev) = expected.pred_opt() else { break };
            expected = prev;
        }
    }
    streak
}

// ---------- snippets ----------

pub fn insert_snippet(db: &Db, trigger: &str, expansion: &str) -> Result<()> {
    let conn = db.lock().unwrap();
    conn.execute(
        "INSERT INTO snippets (trigger, expansion) VALUES (?1, ?2)",
        params![trigger, expansion],
    )?;
    Ok(())
}

pub fn update_snippet(db: &Db, id: i64, trigger: &str, expansion: &str) -> Result<()> {
    let conn = db.lock().unwrap();
    conn.execute(
        "UPDATE snippets SET trigger=?2, expansion=?3 WHERE id=?1",
        params![id, trigger, expansion],
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
        "SELECT id, trigger, expansion, use_count, created_at \
         FROM snippets ORDER BY created_at DESC",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(Snippet {
            id:        r.get(0)?,
            trigger:   r.get(1)?,
            expansion: r.get(2)?,
            use_count: r.get(3)?,
            created_at: r.get(4)?,
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
