//! SQLite access layer for Verenu.
//!
//! Split by domain into submodules; every public item is re-exported here so the
//! external surface (`crate::data::db::*`) is unchanged. Add new queries to the
//! submodule that matches their table. Shared low-level pieces (`Db`, `lock_conn`,
//! `CreatedRecordMeta`) live in this file; reusable validation/normalization
//! helpers live in `validation`. API keys are never stored here — see
//! `data/credentials.rs`.

use anyhow::Result;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex, MutexGuard};

mod cleanup_cache;
mod dictionary;
mod schema;
mod snippets;
mod transcriptions;
mod validation;

pub use cleanup_cache::*;
pub use dictionary::*;
pub use schema::*;
pub use snippets::*;
pub use transcriptions::*;
pub use validation::*;

pub type Db = Arc<Mutex<Connection>>;

fn lock_conn(db: &Db) -> Result<MutexGuard<'_, Connection>> {
    db.lock()
        .map_err(|_| anyhow::anyhow!("Database lock was poisoned"))
}

/// Metadata returned after inserting a row: the new id and its `created_at`.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CreatedRecordMeta {
    pub id: i64,
    pub created_at: String,
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
            "verenu_{name}_{}_{}.db",
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
            .query_row("SELECT spoken_words FROM transcriptions LIMIT 1", [], |r| {
                r.get(0)
            })
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
    fn prune_transcriptions_older_than_deletes_only_old_rows() {
        let db = test_db();
        insert_transcription_returning(&db, "old one", "old one", 2, 1000, "test")
            .expect("old transcription");
        insert_transcription_returning(&db, "recent one", "recent one", 2, 1000, "test")
            .expect("recent transcription");
        {
            let conn = lock_conn(&db).expect("lock");
            conn.execute(
                "UPDATE transcriptions SET created_at = datetime('now', '-30 days') WHERE clean_text = 'old one'",
                [],
            )
            .expect("backdate old row");
        }

        assert_eq!(count_transcriptions_older_than(&db, 7).expect("count"), 1);
        assert_eq!(prune_transcriptions_older_than(&db, 7).expect("prune"), 1);

        let conn = lock_conn(&db).expect("lock");
        let remaining: i64 = conn
            .query_row("SELECT COUNT(*) FROM transcriptions", [], |r| r.get(0))
            .expect("remaining count");
        assert_eq!(remaining, 1);
        let remaining_text: String = conn
            .query_row("SELECT clean_text FROM transcriptions", [], |r| r.get(0))
            .expect("remaining text");
        assert_eq!(remaining_text, "recent one");
    }

    #[test]
    fn pruning_old_transcriptions_does_not_reduce_lifetime_word_total() {
        let db = test_db();
        insert_transcription_returning(&db, "old one", "old one", 5, 1000, "test")
            .expect("old transcription");
        insert_transcription_returning(&db, "recent one", "recent one", 3, 1000, "test")
            .expect("recent transcription");
        {
            let conn = lock_conn(&db).expect("lock");
            conn.execute(
                "UPDATE transcriptions SET created_at = datetime('now', '-30 days') WHERE clean_text = 'old one'",
                [],
            )
            .expect("backdate old row");
        }

        let before = query_stats(&db).expect("stats before prune").total_words;
        assert_eq!(before, 8);

        let deleted = prune_transcriptions_older_than(&db, 7).expect("prune");
        assert_eq!(deleted, 1);

        let after = query_stats(&db).expect("stats after prune").total_words;
        assert_eq!(
            after, 8,
            "lifetime word counter must not shrink when old history is pruned"
        );
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
