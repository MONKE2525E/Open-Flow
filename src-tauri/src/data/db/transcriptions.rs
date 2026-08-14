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
    pub duration_ms: i64,
    pub app_name: Option<String>,
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
    app_name: Option<&str>,
) -> Result<RecentEntry> {
    let mut conn = lock_conn(db)?;
    let spoken_words = compute_spoken_words(&conn, raw)?;
    let tx = conn.transaction()?;
    let entry = tx.query_row(
        "INSERT INTO transcriptions (raw_text, clean_text, words, spoken_words, duration_ms, api_used, app_name) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7) \
         RETURNING id, clean_text, words, duration_ms, app_name, created_at",
        params![raw, clean, words, spoken_words, duration_ms, api_used, app_name],
        |r| {
            Ok(RecentEntry {
                id: r.get(0)?,
                clean_text: r.get(1)?,
                words: r.get(2)?,
                duration_ms: r.get(3)?,
                app_name: r.get(4)?,
                created_at: r.get(5)?,
            })
        },
    )?;
    // Lifetime counter is intentionally separate from the transcriptions
    // table so history retention pruning never shrinks it. Committed in the
    // same transaction as the insert so a crash between the two can't leave
    // total_words permanently undercounted. Upsert because a fresh database
    // (no transcriptions at migration time) may have no id=1 row yet.
    tx.execute(
        "INSERT INTO lifetime_stats (id, total_words) VALUES (1, ?1)
         ON CONFLICT(id) DO UPDATE SET total_words = total_words + ?1",
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
        "INSERT INTO lifetime_stats (id, dictionary_fixes) VALUES (1, ?1)
         ON CONFLICT(id) DO UPDATE SET dictionary_fixes = dictionary_fixes + ?1",
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
        "SELECT id, clean_text, words, duration_ms, app_name, created_at \
         FROM transcriptions ORDER BY id DESC",
    )?;
    let rows = stmt
        .query_map([], |r| {
            Ok(RecentEntry {
                id: r.get(0)?,
                clean_text: r.get(1)?,
                words: r.get(2)?,
                duration_ms: r.get(3)?,
                app_name: r.get(4)?,
                created_at: r.get(5)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

/// Escapes `%`, `_`, and `\` so a user's search text is treated literally inside
/// a `LIKE ... ESCAPE '\'` pattern instead of acting as wildcards.
fn escape_like(s: &str) -> String {
    s.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_")
}

/// Recent transcription history, newest-first. `search` (when present) matches
/// case-insensitively against the cleaned text, the raw transcription, AND the
/// app name — each whitespace-separated term must match at least one of those
/// fields (multi-term AND), so typing an app name like "chrome" works straight
/// in the search box; `app_name` (when present) narrows to a single app.
/// Search/filtering lives in SQLite so pagination stays intact and the whole
/// table never reaches the frontend — a `LIKE` scan over short dictation text
/// is cheap even for large histories, and the `idx_transcriptions_app_name`
/// index narrows app-filtered queries before the text scan runs.
pub fn query_recent_page(
    db: &Db,
    limit: usize,
    offset: usize,
    search: Option<&str>,
    app_name: Option<&str>,
) -> Result<Vec<RecentEntry>> {
    let conn = lock_conn(db)?;
    let limit = limit.clamp(1, 500) as i64;
    let offset = offset.min(i64::MAX as usize) as i64;
    let search = search.map(str::trim).filter(|s| !s.is_empty());
    let app_name = app_name.map(str::trim).filter(|s| !s.is_empty());
    let terms: Vec<String> = search
        .map(|s| s.split_whitespace().map(escape_like).collect())
        .unwrap_or_default();

    // We order by id DESC instead of created_at DESC because id is the
    // autoincrementing primary key. Since IDs are monotonically increasing,
    // this retrieves items in the same chronological order but leverages the
    // primary key index directly, avoiding full table scans and manual sorting
    // overhead in SQLite.
    let mut sql = String::from(
        "SELECT id, clean_text, words, duration_ms, app_name, created_at \
         FROM transcriptions WHERE (? IS NULL OR app_name = ?)",
    );
    let app_value = app_name
        .map(|s| rusqlite::types::Value::from(s.to_string()))
        .unwrap_or(rusqlite::types::Value::Null);
    let mut values: Vec<rusqlite::types::Value> = vec![app_value.clone(), app_value];
    for term in &terms {
        sql.push_str(
            " AND (lower(clean_text) LIKE '%' || lower(?) || '%' ESCAPE '\\' \
             OR lower(raw_text) LIKE '%' || lower(?) || '%' ESCAPE '\\' \
             OR lower(app_name) LIKE '%' || lower(?) || '%' ESCAPE '\\')",
        );
        for _ in 0..3 {
            values.push(rusqlite::types::Value::from(term.clone()));
        }
    }
    sql.push_str(" ORDER BY id DESC LIMIT ? OFFSET ?");
    values.push(rusqlite::types::Value::from(limit));
    values.push(rusqlite::types::Value::from(offset));

    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt
        .query_map(rusqlite::params_from_iter(values.iter()), |r| {
            Ok(RecentEntry {
                id: r.get(0)?,
                clean_text: r.get(1)?,
                words: r.get(2)?,
                duration_ms: r.get(3)?,
                app_name: r.get(4)?,
                created_at: r.get(5)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

/// Distinct apps that have dictation history, for the History app filter. Only
/// non-empty names are returned; pre-v13 rows have no app and simply appear
/// under the unfiltered view.
pub fn query_distinct_apps(db: &Db) -> Result<Vec<String>> {
    let conn = lock_conn(db)?;
    let mut stmt = conn.prepare(
        "SELECT DISTINCT app_name FROM transcriptions \
         WHERE app_name IS NOT NULL AND app_name != '' ORDER BY app_name",
    )?;
    let rows = stmt
        .query_map([], |r| r.get::<_, String>(0))?
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
    let mut conn = lock_conn(db)?;
    let tx = conn.transaction()?;
    let changed = tx.execute(
        "DELETE FROM transcriptions WHERE created_at < datetime('now', ?1)",
        params![format!("-{} days", max_age_days.max(1))],
    )?;
    if changed > 0 {
        // History pruning must not orphan the per-call cost rows used by
        // Insights: rows for deleted transcriptions are unrepresentable in
        // the UI and would otherwise accumulate forever.
        tx.execute(
            "DELETE FROM api_calls WHERE transcription_id NOT IN (SELECT id FROM transcriptions)",
            [],
        )?;
    }
    tx.commit()?;
    Ok(changed)
}

#[cfg(test)]
mod tests {
    use super::{insert_transcription_returning, query_distinct_apps, query_recent_page};

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
                None,
            )
            .expect("insert transcription");
        }

        let first_page = query_recent_page(&db, 2, 0, None, None).expect("first page");
        assert_eq!(first_page.len(), 2);
        assert_eq!(first_page[0].clean_text, "clean 4");
        assert_eq!(first_page[1].clean_text, "clean 3");

        let second_page = query_recent_page(&db, 2, 2, None, None).expect("second page");
        assert_eq!(second_page.len(), 2);
        assert_eq!(second_page[0].clean_text, "clean 2");
        assert_eq!(second_page[1].clean_text, "clean 1");
    }

    #[test]
    fn query_recent_page_filters_by_search_case_insensitive_and_partial() {
        let db = crate::data::db::open(":memory:").expect("db");
        insert_transcription_returning(&db, "raw apple pie", "Clean Apple Pie", 3, 1000, "t", None)
            .expect("insert apple");
        insert_transcription_returning(&db, "raw banana", "Clean Banana Split", 2, 1000, "t", None)
            .expect("insert banana");
        insert_transcription_returning(&db, "raw raisin", "Clean Raisin Bread", 2, 1000, "t", None)
            .expect("insert raisin");

        // Partial + case-insensitive on clean_text.
        let hits = query_recent_page(&db, 50, 0, Some("apple"), None).expect("search apple");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].clean_text, "Clean Apple Pie");

        // Lowercase query matches uppercase stored text.
        let hits = query_recent_page(&db, 50, 0, Some("banana split"), None).expect("search banana");
        assert_eq!(hits.len(), 1);

        // Case-insensitive on raw_text too.
        let hits = query_recent_page(&db, 50, 0, Some("RAW RAISIN"), None).expect("search raw");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].clean_text, "Clean Raisin Bread");

        // No match.
        let hits = query_recent_page(&db, 50, 0, Some("kiwi"), None).expect("search kiwi");
        assert!(hits.is_empty());

        // Missing search returns everything.
        let hits = query_recent_page(&db, 50, 0, None, None).expect("all");
        assert_eq!(hits.len(), 3);
    }

    #[test]
    fn query_recent_page_filters_by_app_and_combines_with_search() {
        let db = crate::data::db::open(":memory:").expect("db");
        insert_transcription_returning(&db, "raw a", "Clean A", 1, 1000, "t", Some("outlook.exe"))
            .expect("insert outlook a");
        insert_transcription_returning(&db, "raw b", "Clean B", 1, 1000, "t", Some("outlook.exe"))
            .expect("insert outlook b");
        insert_transcription_returning(&db, "raw c", "Clean C", 1, 1000, "t", Some("code.exe"))
            .expect("insert code c");

        let outlook = query_recent_page(&db, 50, 0, None, Some("outlook.exe")).expect("outlook");
        assert_eq!(outlook.len(), 2);
        assert!(outlook.iter().all(|e| e.app_name.as_deref() == Some("outlook.exe")));

        // App + search combine: only Outlook rows matching the search text.
        let combined =
            query_recent_page(&db, 50, 0, Some("clean b"), Some("outlook.exe")).expect("combined");
        assert_eq!(combined.len(), 1);
        assert_eq!(combined[0].clean_text, "Clean B");

        // Unknown app matches nothing.
        let none = query_recent_page(&db, 50, 0, None, Some("slack.exe")).expect("slack");
        assert!(none.is_empty());

        // Entries round-trip their app name + duration.
        assert_eq!(outlook[0].duration_ms, 1000);
    }

    #[test]
    fn query_recent_page_treats_like_wildcards_in_search_literally() {
        let db = crate::data::db::open(":memory:").expect("db");
        insert_transcription_returning(&db, "raw 100%", "Clean 100% Sure", 3, 1000, "t", None)
            .expect("insert percent");

        // A literal "%" must match its own character, not act as a wildcard.
        let hits = query_recent_page(&db, 50, 0, Some("100%"), None).expect("literal percent");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].clean_text, "Clean 100% Sure");

        let hits = query_recent_page(&db, 50, 0, Some("100"), None).expect("numeric");
        assert_eq!(hits.len(), 1);
    }

    #[test]
    fn query_recent_page_matches_app_name_from_search_box() {
        let db = crate::data::db::open(":memory:").expect("db");
        insert_transcription_returning(&db, "raw a", "Clean A", 1, 1000, "t", Some("chrome.exe"))
            .expect("insert chrome");
        insert_transcription_returning(&db, "raw b", "Clean B", 1, 1000, "t", Some("outlook.exe"))
            .expect("insert outlook");

        // Typing an app name finds that app's dictations without the dropdown.
        let hits = query_recent_page(&db, 50, 0, Some("chrome"), None).expect("search chrome");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].clean_text, "Clean A");

        // Case-insensitive partial matches the .exe too.
        let hits = query_recent_page(&db, 50, 0, Some("LOOK"), None).expect("search look");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].clean_text, "Clean B");
    }

    #[test]
    fn query_recent_page_multi_term_search_requires_all_terms() {
        let db = crate::data::db::open(":memory:").expect("db");
        insert_transcription_returning(
            &db,
            "raw apple pie",
            "Send the quarterly report",
            4,
            1000,
            "t",
            Some("outlook.exe"),
        )
        .expect("insert quarterly");
        insert_transcription_returning(
            &db,
            "raw banana",
            "Send a follow-up to Dan",
            5,
            1000,
            "t",
            None,
        )
        .expect("insert follow-up");
        insert_transcription_returning(
            &db,
            "raw raisin",
            "Refactor the report module",
            4,
            1000,
            "t",
            Some("code.exe"),
        )
        .expect("insert report");

        // Both terms must match (AND) across the searched fields.
        let hits = query_recent_page(&db, 50, 0, Some("send report"), None).expect("two terms");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].clean_text, "Send the quarterly report");

        // Term order within the query doesn't matter.
        let hits = query_recent_page(&db, 50, 0, Some("REPORT send"), None).expect("reversed");
        assert_eq!(hits.len(), 1);

        // One term in text + one in the app name: both must hold.
        let hits = query_recent_page(&db, 50, 0, Some("report code"), None).expect("text+app");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].clean_text, "Refactor the report module");

        // A term matching nothing kills the whole query.
        let hits = query_recent_page(&db, 50, 0, Some("send zzz"), None).expect("no match");
        assert!(hits.is_empty());

        // Extra whitespace is ignored.
        let hits = query_recent_page(&db, 50, 0, Some("  send   report "), None).expect("padded");
        assert_eq!(hits.len(), 1);
    }

    #[test]
    fn query_distinct_apps_returns_non_empty_unique_names() {
        let db = crate::data::db::open(":memory:").expect("db");
        insert_transcription_returning(&db, "raw a", "Clean A", 1, 1000, "t", Some("outlook.exe"))
            .expect("insert outlook");
        insert_transcription_returning(&db, "raw b", "Clean B", 1, 1000, "t", Some("outlook.exe"))
            .expect("insert outlook again");
        insert_transcription_returning(&db, "raw c", "Clean C", 1, 1000, "t", Some("code.exe"))
            .expect("insert code");
        insert_transcription_returning(&db, "raw d", "Clean D", 1, 1000, "t", None)
            .expect("insert no app");

        let apps = query_distinct_apps(&db).expect("distinct apps");
        assert_eq!(apps, vec!["code.exe".to_string(), "outlook.exe".to_string()]);
    }
}
