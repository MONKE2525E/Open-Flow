//! Transcription history queries and lifetime/derived stats.

use anyhow::Result;
use rusqlite::params;
use serde::{Deserialize, Serialize};

use super::*;

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

pub fn insert_transcription_returning(
    db: &Db,
    raw: &str,
    clean: &str,
    words: i64,
    duration_ms: i64,
    api_used: &str,
) -> Result<RecentEntry> {
    let mut conn = lock_conn(db)?;
    let spoken_words = compute_spoken_words(&conn, raw)?;
    let tx = conn.transaction()?;
    let entry = tx.query_row(
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
    )?;
    // Lifetime counter is intentionally separate from the transcriptions
    // table so history retention pruning never shrinks it. Committed in the
    // same transaction as the insert so a crash between the two can't leave
    // total_words permanently undercounted.
    tx.execute(
        "UPDATE lifetime_stats SET total_words = total_words + ?1 WHERE id = 1",
        params![words],
    )?;
    tx.commit()?;
    Ok(entry)
}

/// Lifetime counter for dictionary substitutions actually applied to
/// dictations. Like `total_words`, it is only ever incremented — never
/// recomputed from history — so retention pruning can't shrink it. `count`
/// is the number of dictionary substitution events from one dictation.
pub fn increment_lifetime_dictionary_fixes(db: &Db, count: i64) -> Result<()> {
    if count <= 0 {
        return Ok(());
    }
    let conn = lock_conn(db)?;
    conn.execute(
        "UPDATE lifetime_stats SET dictionary_fixes = dictionary_fixes + ?1 WHERE id = 1",
        params![count],
    )?;
    Ok(())
}

pub fn query_recent(db: &Db) -> Result<Vec<RecentEntry>> {
    let conn = lock_conn(db)?;
    // We order by id DESC instead of created_at DESC because id is the autoincrementing
    // primary key. Since IDs are monotonically increasing, this retrieves items in the
    // same chronological order but leverages the primary key index directly, avoiding
    // full table scans and manual sorting overhead in SQLite.
    let mut stmt = conn.prepare(
        "SELECT id, clean_text, words, created_at \
         FROM transcriptions ORDER BY id DESC",
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

pub fn query_recent_page(db: &Db, limit: usize, offset: usize) -> Result<Vec<RecentEntry>> {
    let conn = lock_conn(db)?;
    let limit = limit.clamp(1, 500) as i64;
    let offset = offset.min(i64::MAX as usize) as i64;
    // We order by id DESC instead of created_at DESC because id is the autoincrementing
    // primary key. Since IDs are monotonically increasing, this retrieves items in the
    // same chronological order but leverages the primary key index directly, avoiding
    // full table scans and manual sorting overhead in SQLite.
    let mut stmt = conn.prepare(
        "SELECT id, clean_text, words, created_at \
         FROM transcriptions ORDER BY id DESC LIMIT ?1 OFFSET ?2",
    )?;
    let rows = stmt
        .query_map(params![limit, offset], |r| {
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
        "SELECT total_words FROM lifetime_stats WHERE id = 1",
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

pub fn count_transcriptions_older_than(db: &Db, max_age_days: i64) -> Result<i64> {
    let conn = lock_conn(db)?;
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM transcriptions WHERE created_at < datetime('now', ?1)",
        params![format!("-{} days", max_age_days.max(1))],
        |r| r.get(0),
    )?;
    Ok(count)
}

pub fn prune_transcriptions_older_than(db: &Db, max_age_days: i64) -> Result<usize> {
    let conn = lock_conn(db)?;
    let changed = conn.execute(
        "DELETE FROM transcriptions WHERE created_at < datetime('now', ?1)",
        params![format!("-{} days", max_age_days.max(1))],
    )?;
    Ok(changed)
}

#[cfg(test)]
mod tests {
    use super::{insert_transcription_returning, query_recent_page};

    #[test]
    fn query_recent_page_applies_limit_and_offset() {
        let db = crate::data::db::open(":memory:").expect("db");
        for i in 0..5 {
            insert_transcription_returning(
                &db,
                &format!("raw {i}"),
                &format!("clean {i}"),
                i + 1,
                1000,
                "groq/whisper-large-v3-turbo",
            )
            .expect("insert transcription");
        }

        let first_page = query_recent_page(&db, 2, 0).expect("first page");
        assert_eq!(first_page.len(), 2);
        assert_eq!(first_page[0].clean_text, "clean 4");
        assert_eq!(first_page[1].clean_text, "clean 3");

        let second_page = query_recent_page(&db, 2, 2).expect("second page");
        assert_eq!(second_page.len(), 2);
        assert_eq!(second_page[0].clean_text, "clean 2");
        assert_eq!(second_page[1].clean_text, "clean 1");
    }
}
