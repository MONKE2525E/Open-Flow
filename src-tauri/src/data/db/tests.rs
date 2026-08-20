use super::*;

fn test_db() -> Db {
    open(":memory:").expect("test db")
}

fn temp_db_path(name: &str) -> std::path::PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    std::env::temp_dir().join(format!("verenu_{name}_{}_{}.db", std::process::id(), nanos))
}

#[test]
fn context_stats_count_only_that_contexts_dictations() {
    let db = test_db();
    let everywhere = everywhere_context_id(&db).expect("everywhere");
    let work = insert_context_returning(&db, "Work", None, None, None, None, false).expect("context");

    insert_transcription_returning(&db, "a b c", "a b c", 3, 1000, "test", None, Some(work.id))
        .expect("insert");
    insert_transcription_returning(&db, "d e", "d e", 2, 1000, "test", None, Some(work.id))
        .expect("insert");
    insert_transcription_returning(&db, "z", "z", 9, 1000, "test", None, Some(everywhere))
        .expect("insert");
    // Pre-v18 history carries no context and must not be attributed anywhere.
    insert_transcription_returning(&db, "orphan", "orphan", 7, 1000, "test", None, None)
        .expect("insert");

    let stats = query_context_stats(&db, work.id).expect("stats");
    assert_eq!(stats.dictations, 2);
    assert_eq!(stats.words, 5);
    assert!(stats.last_used_at.is_some());

    let everywhere_stats = query_context_stats(&db, everywhere).expect("stats");
    assert_eq!(everywhere_stats.dictations, 1);
    assert_eq!(everywhere_stats.words, 9);

    // A context that has never been used reports zeroes, not an error.
    let unused = insert_context_returning(&db, "Unused", None, None, None, None, false).expect("ctx");
    let unused_stats = query_context_stats(&db, unused.id).expect("stats");
    assert_eq!(unused_stats.dictations, 0);
    assert_eq!(unused_stats.words, 0);
    assert!(unused_stats.last_used_at.is_none());
}

#[test]
fn set_context_pinned_stamps_and_clears_pinned_at() {
    let db = test_db();
    let work = insert_context_returning(&db, "Work", None, None, None, None, false).expect("context");
    assert!(work.pinned_at.is_none());

    set_context_pinned(&db, work.id, true).expect("pin");
    assert!(query_context(&db, work.id).expect("row").pinned_at.is_some());

    set_context_pinned(&db, work.id, false).expect("unpin");
    assert!(query_context(&db, work.id).expect("row").pinned_at.is_none());

    // Everywhere is an ordinary row for pinning — only deletion is barred.
    let everywhere = everywhere_context_id(&db).expect("everywhere");
    set_context_pinned(&db, everywhere, true).expect("pin everywhere");
    assert!(query_context(&db, everywhere)
        .expect("row")
        .pinned_at
        .is_some());
    set_context_pinned(&db, everywhere, false).expect("unpin everywhere");
    assert!(query_context(&db, everywhere)
        .expect("row")
        .pinned_at
        .is_none());
    assert!(delete_context(&db, everywhere).is_err());

    // Renaming and restyling it are allowed; the pipeline resolves the
    // is_everywhere flag, not the name.
    update_context(&db, everywhere, "Fallback").expect("rename everywhere");
    update_context_color(&db, everywhere, Some("var(--accent)")).expect("recolor everywhere");
    let renamed = query_context(&db, everywhere).expect("row");
    assert_eq!(renamed.name, "Fallback");
    assert!(renamed.is_everywhere);
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
    insert_transcription_returning(&db, "second clip", "second clip", 2, 1000, "test", None, None)
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
fn open_alone_does_not_seed_the_dictionary() {
    // seed_default_dictionary_entries is deliberately NOT part of the
    // generic migration chain (see its doc comment) — open() alone must
    // leave test/fixture databases pristine.
    let db = test_db();
    let entries = query_dictionary(&db).expect("query dictionary");
    assert!(entries.is_empty());
}

#[test]
fn open_self_heals_database_stuck_at_v2_with_legacy_dictionary() {
    // Databases stranded at user_version = 2 by the legacy
    // non-transactional v2 migration keep the old `wrong`/`correct`
    // dictionary shape (and may also lack `snippets.instructions` and
    // `pending_corrections`). Without the self-heal, every dictionary
    // query fails with `no such column: term` — and v7's spoken-words
    // backfill would fail on the missing `instructions` column.
    let path = temp_db_path("v2_legacy_dictionary");
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
             CREATE TABLE dictionary (
               id INTEGER PRIMARY KEY AUTOINCREMENT,
               wrong TEXT NOT NULL,
               correct TEXT NOT NULL,
               auto_learned INTEGER NOT NULL DEFAULT 0,
               correction_count INTEGER NOT NULL DEFAULT 0,
               created_at DATETIME NOT NULL DEFAULT (datetime('now'))
             );
             CREATE TABLE snippets (
               id INTEGER PRIMARY KEY AUTOINCREMENT,
               trigger TEXT NOT NULL UNIQUE,
               expansion TEXT NOT NULL,
               use_count INTEGER NOT NULL DEFAULT 0,
               created_at DATETIME NOT NULL DEFAULT (datetime('now'))
             );
             INSERT INTO dictionary (wrong, correct, auto_learned)
               VALUES ('Varinu', 'Verenu', 1);
             INSERT INTO transcriptions (raw_text, clean_text, words, duration_ms, api_used)
               VALUES ('hello sig world', 'clean', 3, 1000, 'test');
             INSERT INTO snippets (trigger, expansion) VALUES ('sig', 'signature');
             PRAGMA user_version = 2;",
        )
        .expect("seed stuck db");
    }

    let db = open(path.to_str().expect("path string")).expect("open self-heals stuck db");

    // The dictionary must be queryable through the modern `term` column,
    // with the legacy pair migrated into `term`/`mistake`.
    let entries = query_dictionary(&db).expect("query dictionary");
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].term, "Verenu");
    assert_eq!(entries[0].mistake.as_deref(), Some("Varinu"));
    // New entries must insert against the modern UNIQUE(term) constraint.
    insert_dictionary_entry(&db, "Tauri", Some("Tari")).expect("insert modern entry");
    // The v2 siblings must exist too.
    assert!(
        table_has_column(&lock_conn(&db).expect("lock"), "snippets", "instructions")
            .expect("instructions column"),
        "snippets.instructions must be self-healed"
    );
    assert!(
        table_exists(&lock_conn(&db).expect("lock"), "pending_corrections").expect("table"),
        "pending_corrections must be self-healed"
    );
    // Transcriptions with snippet triggers still backfill correctly.
    let conn = lock_conn(&db).expect("lock");
    let spoken_words: i64 = conn
        .query_row(
            "SELECT spoken_words FROM transcriptions WHERE raw_text = 'hello sig world'",
            [],
            |r| r.get(0),
        )
        .expect("spoken words backfilled");
    assert_eq!(spoken_words, 2);
    // And the whole chain must have reached the current version.
    let version: i64 = conn
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .expect("version");
    assert_eq!(version, 19);
    drop(conn);
    drop(db);
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(path.with_extension("db-wal"));
    let _ = std::fs::remove_file(path.with_extension("db-shm"));
}

#[test]
fn open_with_recovery_does_not_quarantine_non_corruption_errors() {
    // A database path that cannot be opened (a directory is not a
    // database) is NOT corruption: it must surface as an error without
    // moving anything aside, so a transiently-locked healthy database can
    // never be silently replaced with an empty fresh one.
    let parent = std::env::temp_dir().join(format!(
        "verenu_noncorrupt_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time")
            .as_nanos()
    ));
    std::fs::create_dir_all(&parent).expect("create parent");
    let path = parent.join("verenu.db");
    std::fs::create_dir(&path).expect("create dir-as-db");

    let err = open_with_recovery(&path).expect_err("a directory is not a database");
    assert!(
        err.to_string().to_lowercase().contains("unable to open"),
        "unexpected error: {err}"
    );

    let quarantined = std::fs::read_dir(&parent)
        .expect("list dir")
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.contains(".corrupt-"))
        .collect::<Vec<_>>();
    assert!(
        quarantined.is_empty(),
        "no quarantine files may be created for a non-corruption error: {quarantined:?}"
    );

    let _ = std::fs::remove_dir(&path);
    let _ = std::fs::remove_dir(&parent);
}

#[test]
fn open_with_recovery_quarantines_a_corrupt_database_and_opens_fresh() {
    let path = temp_db_path("corrupt_db_recovery");
    std::fs::write(&path, b"this is not a sqlite database at all").expect("write garbage");

    // Plain open must fail (the corrupt file is a real error, not something
    // to silently paper over)…
    assert!(open(&path).is_err());

    // …but the startup path must recover: quarantine + fresh database.
    let db = open_with_recovery(&path).expect("recovery opens a fresh database");
    insert_transcription_returning(&db, "fresh", "fresh", 1, 1000, "test", None, None)
        .expect("fresh db is writable");

    // The corrupt file must be preserved for diagnosis, not deleted.
    let quarantine = std::fs::read_dir(path.parent().expect("parent"))
        .expect("list dir")
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| {
            let stem = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default();
            name.starts_with(stem) && name.contains(".db.corrupt-")
        })
        .collect::<Vec<_>>();
    assert_eq!(
        quarantine.len(),
        1,
        "exactly one quarantine file for this database: {quarantine:?}"
    );

    drop(db);
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(path.with_extension("db-wal"));
    let _ = std::fs::remove_file(path.with_extension("db-shm"));
    if let Some(parent) = path.parent() {
        let _ = std::fs::remove_file(parent.join(&quarantine[0]));
    }
}

#[test]
fn open_with_recovery_leaves_a_healthy_database_untouched() {
    let path = temp_db_path("healthy_db_recovery");
    {
        let db = open(&path).expect("healthy open");
        insert_transcription_returning(&db, "keep me", "keep me", 2, 1000, "test", None, None)
            .expect("insert");
    }
    let db = open_with_recovery(&path).expect("recovery keeps healthy db");
    let entries = query_recent_page(&db, 10, 0, None, None).expect("query");
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].clean_text, "keep me");
    drop(db);
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(path.with_extension("db-wal"));
    let _ = std::fs::remove_file(path.with_extension("db-shm"));
}

#[test]
fn open_with_recovery_quarantines_db_and_sidecars_together() {
    // Real-world corruption is a bad main file while the -wal/-shm
    // sidecars from a healthy session are still present (torn disk
    // write, failed update, partial restore). SQLite deletes stale
    // sidecars itself during a failed open, so the quarantine helper's
    // "move the whole set" contract is tested directly.
    let path = temp_db_path("sidecar_recovery");
    {
        let db = open(&path).expect("create db");
        insert_transcription_returning(&db, "first", "first", 1, 1000, "test", None, None)
            .expect("insert");
        drop(db);
    }
    let wal_path = path.with_extension("db-wal");
    let shm_path = path.with_extension("db-shm");
    std::fs::write(&wal_path, b"stale wal").expect("create stale wal sidecar");
    std::fs::write(&shm_path, b"stale shm").expect("create stale shm sidecar");

    assert!(
        super::quarantine_corrupt_db_files(&path),
        "all three files must be moved aside"
    );

    assert!(!path.exists(), "main file moved aside");
    assert!(!wal_path.exists(), "wal sidecar moved aside");
    assert!(!shm_path.exists(), "shm sidecar moved aside");

    let stem = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default();
    let quarantined: Vec<_> = std::fs::read_dir(path.parent().expect("parent"))
        .expect("dir")
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.starts_with(stem) && name.contains(".corrupt-"))
        .collect();
    assert_eq!(
        quarantined.len(),
        3,
        "main + wal + shm all quarantined: {quarantined:?}"
    );

    for name in quarantined {
        let _ = std::fs::remove_file(path.parent().expect("parent").join(name));
    }
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(wal_path);
    let _ = std::fs::remove_file(shm_path);
}

#[test]
fn seed_default_dictionary_entries_adds_a_verenu_entry_with_every_known_variant() {
    let db = test_db();
    seed_default_dictionary_entries(&db).expect("seed");
    let entries = query_dictionary(&db).expect("query dictionary");
    let verenu = entries
        .iter()
        .find(|e| e.term == "Verenu")
        .expect("default Verenu entry seeded");
    assert!(
        !verenu.auto_learned,
        "seeded entry must be manual, not gated by distinctiveness"
    );
    let mistake = verenu.mistake.as_deref().unwrap_or_default();
    for variant in [
        "Varinu", "Verena", "Virinu", "Varino", "Varinew", "Varina", "Verminu", "Varinian",
        "Marino", "Zarinu", "Berenu", "Ferenu", "Werenu", "Verinu", "Varineu",
    ] {
        assert!(mistake.contains(variant), "missing variant: {variant}");
    }
}

#[test]
fn seed_default_dictionary_entries_does_not_resurrect_a_deleted_entry() {
    let path = temp_db_path("verenu_seed_deletion");
    {
        let db = open(path.to_str().expect("path string")).expect("first open");
        seed_default_dictionary_entries(&db).expect("first seed");
        let entries = query_dictionary(&db).expect("query dictionary");
        let verenu_id = entries
            .iter()
            .find(|e| e.term == "Verenu")
            .expect("seeded on first call")
            .id;
        delete_dictionary_entry(&db, verenu_id).expect("user deletes the default entry");
    }
    // Reopening and re-seeding must not resurrect it — the marker table
    // makes this a no-op after the first successful seed, regardless of
    // whether the user has since deleted the row.
    let db = open(path.to_str().expect("path string")).expect("second open");
    seed_default_dictionary_entries(&db).expect("second seed is a no-op");
    let entries = query_dictionary(&db).expect("query dictionary after reopen");
    assert!(!entries.iter().any(|e| e.term == "Verenu"));
    drop(db);

    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(path.with_extension("db-wal"));
    let _ = std::fs::remove_file(path.with_extension("db-shm"));
}

#[test]
fn seed_default_dictionary_entries_merges_into_a_preexisting_manual_verenu_entry() {
    // The actual observed bug: a user who'd already hand-added a
    // "Verenu" -> "Vernu" correction weeks before this feature existed
    // hit the dictionary's UNIQUE(term) constraint, silently dropping
    // every known variant from the original INSERT OR IGNORE — leaving
    // only "Vernu", which doesn't match "Varino"/"Varinu" and so never
    // fired. Merging into the existing row instead must add every known
    // variant while preserving the user's own "Vernu".
    let db = test_db();
    insert_dictionary_entry(&db, "Verenu", Some("Vernu")).expect("user's own manual entry");

    seed_default_dictionary_entries(&db).expect("seed merges into existing entry");

    let entries = query_dictionary(&db).expect("query dictionary");
    assert_eq!(
        entries.iter().filter(|e| e.term == "Verenu").count(),
        1,
        "must not create a duplicate row"
    );
    let verenu = entries.iter().find(|e| e.term == "Verenu").expect("entry");
    let mistake = verenu.mistake.as_deref().unwrap_or_default();
    assert!(mistake.contains("Vernu"), "user's own variant must survive");
    for variant in [
        "Varinu", "Verena", "Virinu", "Varino", "Varinew", "Varina", "Verminu", "Varinian",
        "Marino", "Zarinu", "Berenu", "Ferenu", "Werenu", "Verinu", "Varineu",
    ] {
        assert!(mistake.contains(variant), "missing variant: {variant}");
    }
}

#[test]
fn seed_default_dictionary_entries_does_not_duplicate_variants_already_present() {
    let db = test_db();
    insert_dictionary_entry(&db, "Verenu", Some("Varinu, Verena")).expect("partial entry");

    seed_default_dictionary_entries(&db).expect("seed");
    seed_default_dictionary_entries(&db).expect("seed again is a no-op");

    let entries = query_dictionary(&db).expect("query dictionary");
    let mistake = entries
        .iter()
        .find(|e| e.term == "Verenu")
        .and_then(|e| e.mistake.clone())
        .unwrap_or_default();
    assert_eq!(
        mistake.matches("Varinu").count(),
        1,
        "an already-present variant must not be duplicated"
    );
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
fn auto_learn_promote_promotes_when_pending_reaches_threshold() {
    let db = test_db();

    // Session 1: pending count reaches 1, below the default threshold of 2.
    upsert_auto_learn_candidate(&db, "Koobernetes", "Kubernetes", 0.6).expect("candidate");
    let first = auto_learn_promote(&db, "Koobernetes", "Kubernetes", "medium", 2, 2)
        .expect("first promote call");
    assert_eq!(
        first,
        AutoLearnPromoteResult::BelowThreshold { pending_count: 1 }
    );
    assert!(query_dictionary(&db).expect("dictionary").is_empty());

    // Session 2: pending count reaches 2 — the pair promotes.
    let second = auto_learn_promote(&db, "Koobernetes", "Kubernetes", "medium", 2, 2)
        .expect("second promote call");
    assert_eq!(second, AutoLearnPromoteResult::Promoted);

    let entries = query_dictionary(&db).expect("dictionary");
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].term, "Kubernetes");
    assert_eq!(entries[0].mistake.as_deref(), Some("Koobernetes"));
    assert!(entries[0].auto_learned);
}

#[test]
fn auto_learn_promote_is_atomic_against_double_promotion() {
    // Two concurrent monitors both observe the pair and both believe the
    // pending count has crossed the threshold (two prior sessions already
    // recorded pending rows). Only ONE may actually promote — the second
    // caller's atomic `promoted_at` claim must be refused.
    let db = test_db();
    upsert_auto_learn_candidate(&db, "Koobernetes", "Kubernetes", 0.6).expect("candidate");
    insert_pending_correction(&db, "Koobernetes", "Kubernetes").expect("prior pending 1");
    insert_pending_correction(&db, "Koobernetes", "Kubernetes").expect("prior pending 2");

    let first =
        auto_learn_promote(&db, "Koobernetes", "Kubernetes", "medium", 2, 2).expect("first");
    assert_eq!(first, AutoLearnPromoteResult::Promoted);

    let second =
        auto_learn_promote(&db, "Koobernetes", "Kubernetes", "medium", 2, 2).expect("second");
    assert_eq!(
        second,
        AutoLearnPromoteResult::AlreadyPromoted,
        "second concurrent promotion must be refused"
    );

    let entries = query_dictionary(&db).expect("dictionary");
    assert_eq!(entries.len(), 1);
    assert_eq!(
        entries[0].correction_count, 1,
        "correction_count must not be inflated by the concurrent duplicate"
    );
}

#[test]
fn auto_learn_promote_does_not_resurrect_a_rejected_candidate() {
    // A rejection deletes the auto-learned entry AND purges its candidate
    // rows. An in-flight promotion for that pair must not re-create it: the
    // `promoted_at` claim no-ops against a purged candidate.
    let db = test_db();
    upsert_auto_learn_candidate(&db, "Koobernetes", "Kubernetes", 0.6).expect("candidate");
    insert_pending_correction(&db, "Koobernetes", "Kubernetes").expect("prior pending");
    assert_eq!(
        auto_learn_promote(&db, "Koobernetes", "Kubernetes", "medium", 2, 2).expect("promote"),
        AutoLearnPromoteResult::Promoted
    );
    let id = query_dictionary(&db)
        .expect("dictionary")
        .into_iter()
        .next()
        .expect("entry")
        .id;

    // Rejection fires: entry + candidate + pending rows purged together.
    delete_auto_learned_entries_by_ids(&db, &[id]).expect("reject");
    assert!(query_dictionary(&db).expect("dictionary after").is_empty());

    // A stale monitor that already passed its count read tries to promote.
    // Its count is re-evaluated inside the SAME transaction as the claim
    // and dict insert, against post-rejection state: the pending rows are
    // gone, so it lands BelowThreshold and must NOT re-create the entry.
    let stale =
        auto_learn_promote(&db, "Koobernetes", "Kubernetes", "medium", 2, 2).expect("stale");
    assert_ne!(
        stale,
        AutoLearnPromoteResult::Promoted,
        "rejected pair must not be resurrected by an in-flight monitor"
    );
    assert!(
        query_dictionary(&db)
            .expect("dictionary after stale")
            .is_empty(),
        "rejected pair must not be resurrected"
    );
}

#[test]
fn auto_learn_promote_can_relearn_after_rejection() {
    // After a rejection fully purges the candidate, a genuinely new learning
    // window (a fresh candidate row with promoted_at IS NULL) can promote.
    let db = test_db();
    upsert_auto_learn_candidate(&db, "Koobernetes", "Kubernetes", 0.6).expect("candidate");
    insert_pending_correction(&db, "Koobernetes", "Kubernetes").expect("prior pending");
    assert_eq!(
        auto_learn_promote(&db, "Koobernetes", "Kubernetes", "medium", 2, 2).expect("promote"),
        AutoLearnPromoteResult::Promoted
    );
    let id = query_dictionary(&db)
        .expect("dictionary")
        .into_iter()
        .next()
        .expect("entry")
        .id;
    delete_auto_learned_entries_by_ids(&db, &[id]).expect("reject");

    // New learning episode: fresh candidate (promoted_at NULL), two sessions.
    upsert_auto_learn_candidate(&db, "Koobernetes", "Kubernetes", 0.6)
        .expect("candidate again");
    assert_eq!(
        auto_learn_promote(&db, "Koobernetes", "Kubernetes", "medium", 2, 2).expect("s1"),
        AutoLearnPromoteResult::BelowThreshold { pending_count: 1 }
    );
    assert_eq!(
        auto_learn_promote(&db, "Koobernetes", "Kubernetes", "medium", 2, 2).expect("s2"),
        AutoLearnPromoteResult::Promoted
    );
    let entries = query_dictionary(&db).expect("dictionary");
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].correction_count, 1);
}

#[test]
fn auto_learn_promote_manual_entry_blocks_without_claiming() {
    let db = test_db();
    insert_dictionary_entry(&db, "Kubernetes", Some("user typo")).expect("manual");
    upsert_auto_learn_candidate(&db, "Koobernetes", "Kubernetes", 0.6).expect("candidate");
    insert_pending_correction(&db, "Koobernetes", "Kubernetes").expect("pending 1");
    insert_pending_correction(&db, "Koobernetes", "Kubernetes").expect("pending 2");

    // Threshold is satisfied, but the manual entry blocks promotion on
    // every attempt (and records the pending rows for later sessions).
    for _ in 0..2 {
        assert_eq!(
            auto_learn_promote(&db, "Koobernetes", "Kubernetes", "medium", 2, 2)
                .expect("promote"),
            AutoLearnPromoteResult::Blocked
        );
    }
    let entries = query_dictionary(&db).expect("dictionary");
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].term, "Kubernetes");
    assert_eq!(entries[0].mistake.as_deref(), Some("user typo"));
    assert!(!entries[0].auto_learned);

    // The candidate was NOT claimed, so removing the manual entry later
    // lets the pair be learned anew.
    let conn = lock_conn(&db).expect("lock");
    let promoted_at: Option<String> = conn
        .query_row(
            "SELECT promoted_at FROM auto_learn_candidates WHERE wrong_word='Koobernetes'",
            [],
            |r| r.get(0),
        )
        .expect("promoted_at");
    assert!(
        promoted_at.is_none(),
        "gate must not be claimed for a manual block"
    );
    drop(conn);
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
    let (inserted_expiry_epoch, created_at): (i64, String) = conn
        .query_row(
            "SELECT expires_at_epoch, created_at FROM cleanup_cache WHERE key = 'utc'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .expect("select insert epoch");
    drop(conn);
    assert_eq!(inserted_expiry_epoch, 1_767_225_600);

    cleanup_cache_touch_hit(
        &db,
        "utc",
        &created_at,
        1,
        2,
        "2026-01-02 03:04:05",
        "2026-02-03 04:05:06",
    )
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

    // Entry must be gone — next dictation will hit the LLM.
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
    let created_at = cleanup_cache_get_active(&db, "k")
        .expect("get")
        .expect("exists")
        .created_at;
    cleanup_cache_touch_hit(
        &db,
        "k",
        &created_at,
        1,
        2,
        "2026-01-01 00:00:00",
        "2999-01-01 00:00:00",
    )
    .expect("touch");

    let hit = cleanup_cache_get_active(&db, "k")
        .expect("get")
        .expect("exists");
    assert_eq!(hit.hit_count, 2);

    // User deletes output → rejection monitor fires.
    cleanup_cache_delete_by_key(&db, "k").expect("delete");

    assert!(cleanup_cache_get_active(&db, "k")
        .expect("get after")
        .is_none());
}

#[test]
fn pruning_history_removes_orphaned_api_cost_rows() {
    let db = test_db();
    let old = insert_transcription_returning(&db, "old", "old", 2, 1000, "test", None, None)
        .expect("old transcription");
    let recent = insert_transcription_returning(&db, "recent", "recent", 3, 1000, "test", None, None)
        .expect("recent transcription");
    let call = |transcription_id: i64| ApiCall {
        transcription_id,
        model: "test-model".into(),
        provider: "groq".into(),
        task: "transcribe".into(),
        audio_ms: 100,
        input_chars: 1,
        output_chars: 2,
        created_at: "2026-01-01 00:00:00".into(),
    };
    insert_api_calls(&db, &[call(old.id), call(recent.id)]).expect("insert api calls");
    {
        let conn = lock_conn(&db).expect("lock");
        conn.execute(
            "UPDATE transcriptions SET created_at = datetime('now', '-30 days') WHERE clean_text = 'old'",
            [],
        )
        .expect("backdate old row");
    }

    assert_eq!(prune_transcriptions_older_than(&db, 7).expect("prune"), 1);

    let conn = lock_conn(&db).expect("lock");
    let remaining_calls: i64 = conn
        .query_row("SELECT COUNT(*) FROM api_calls", [], |r| r.get(0))
        .expect("remaining calls");
    assert_eq!(
        remaining_calls, 1,
        "cost rows for pruned transcriptions must be removed"
    );
    let orphan: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM api_calls WHERE transcription_id = ?1",
            [old.id],
            |r| r.get(0),
        )
        .expect("orphan count");
    assert_eq!(orphan, 0, "no cost rows may outlive their transcription");
    drop(conn);
}

#[test]
fn auto_learn_retention_prunes_only_stale_rows() {
    let db = test_db();
    upsert_auto_learn_candidate(&db, "Koobernetes", "Kubernetes", 0.6).expect("candidate");
    upsert_auto_learn_candidate(&db, "Tari", "Tauri", 0.6).expect("candidate");
    log_auto_learn_event(&db, "monitor", "started", "", "", "", 0.0).expect("event");
    {
        let conn = lock_conn(&db).expect("lock");
        conn.execute(
            "UPDATE auto_learn_events SET created_at = datetime('now', '-40 days')",
            [],
        )
        .expect("backdate events");
        conn.execute(
            "UPDATE auto_learn_candidates
             SET last_seen_at = datetime('now', '-100 days')
             WHERE wrong_word = 'Koobernetes'",
            [],
        )
        .expect("backdate stale candidate");
    }

    let pruned = prune_auto_learn_retention(&db).expect("prune");
    assert_eq!(pruned, 2, "one stale event + one stale candidate");

    let conn = lock_conn(&db).expect("lock");
    let events: i64 = conn
        .query_row("SELECT COUNT(*) FROM auto_learn_events", [], |r| r.get(0))
        .expect("events");
    assert_eq!(events, 0, "all events were stale");
    let mut stmt = conn
        .prepare("SELECT wrong_word FROM auto_learn_candidates")
        .expect("prepare");
    let candidates: Vec<String> = stmt
        .query_map([], |r| r.get::<_, String>(0))
        .expect("candidates")
        .collect::<rusqlite::Result<Vec<String>>>()
        .expect("collect");
    drop(stmt);
    assert_eq!(
        candidates,
        vec!["Tari".to_string()],
        "fresh candidate survives"
    );
    drop(conn);
}

#[test]
fn auto_learn_promote_different_mistake_releases_the_gate() {
    let db = test_db();

    // Promote a first pair for the term.
    upsert_auto_learn_candidate(&db, "Koobernetes", "Kubernetes", 0.6).expect("candidate");
    insert_pending_correction(&db, "Koobernetes", "Kubernetes").expect("pending");
    insert_pending_correction(&db, "Koobernetes", "Kubernetes").expect("pending");
    assert_eq!(
        auto_learn_promote(&db, "Koobernetes", "Kubernetes", "medium", 2, 2).expect("promote"),
        AutoLearnPromoteResult::Promoted
    );

    // A second pair for the same term with a different mistake is blocked
    // by the existing auto-learned entry.
    upsert_auto_learn_candidate(&db, "Kubernetz", "Kubernetes", 0.6).expect("candidate");
    insert_pending_correction(&db, "Kubernetz", "Kubernetes").expect("pending");
    insert_pending_correction(&db, "Kubernetz", "Kubernetes").expect("pending");
    assert_eq!(
        auto_learn_promote(&db, "Kubernetz", "Kubernetes", "medium", 2, 2).expect("blocked"),
        AutoLearnPromoteResult::Blocked
    );

    // …but the gate must not be burned: the candidate is still claimable.
    let conn = lock_conn(&db).expect("lock");
    let promoted_at: Option<String> = conn
        .query_row(
            "SELECT promoted_at FROM auto_learn_candidates WHERE wrong_word = 'Kubernetz'",
            [],
            |r| r.get(0),
        )
        .expect("gate");
    assert!(
        promoted_at.is_none(),
        "different-mistake block must release the promoted_at claim"
    );
    drop(conn);

    // Removing the conflicting entry lets the pair be learned after all.
    let entries = query_dictionary(&db).expect("dictionary");
    let id = entries
        .iter()
        .find(|e| e.term == "Kubernetes")
        .expect("entry")
        .id;
    delete_dictionary_entry(&db, id).expect("delete conflicting entry");

    assert_eq!(
        auto_learn_promote(&db, "Kubernetz", "Kubernetes", "medium", 2, 2).expect("relearn"),
        AutoLearnPromoteResult::Promoted
    );
    let entries = query_dictionary(&db).expect("dictionary");
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].term, "Kubernetes");
    assert_eq!(entries[0].mistake.as_deref(), Some("Kubernetz"));
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
    insert_transcription_returning(
        &db,
        "sig.",
        "A long email signature",
        9,
        2000,
        "test",
        None,
    None,
    )
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
        None,
    None,
    )
    .expect("transcription");

    let stats = query_stats(&db).expect("stats");

    assert_eq!(stats.avg_wpm, 90.0);
}

#[test]
fn stats_avg_wpm_excludes_pure_snippet_rows_from_average() {
    let db = test_db();
    insert_snippet(&db, "sig", "A long email signature", "").expect("snippet");
    insert_transcription_returning(&db, "hello world", "hello world", 2, 1000, "test", None, None)
        .expect("normal transcription");
    insert_transcription_returning(
        &db,
        "sig.",
        "A long email signature",
        4,
        1000,
        "test",
        None,
    None,
    )
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
            None,
        None,
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
    insert_transcription_returning(&db, "old one", "old one", 2, 1000, "test", None, None)
        .expect("old transcription");
    insert_transcription_returning(&db, "recent one", "recent one", 2, 1000, "test", None, None)
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
    insert_transcription_returning(&db, "old one", "old one", 5, 1000, "test", None, None)
        .expect("old transcription");
    insert_transcription_returning(&db, "recent one", "recent one", 3, 1000, "test", None, None)
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

    // Manual entry — must survive rejection.
    insert_dictionary_entry(&db, "groq", Some("grog")).expect("manual");
    let manual_id = query_dictionary(&db)
        .expect("query")
        .into_iter()
        .find(|e| e.term == "groq")
        .expect("find")
        .id;

    // Auto-learned entry — must be removed.
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
    // Pending corrections also purged — prevents immediate re-promotion.
    assert_eq!(
        count_pending_corrections_recent(&db, "Tari", "Tauri", 7).expect("count after"),
        0
    );
}

#[test]
fn cache_rejection_full_lifecycle() {
    // End-to-end: insert → hit (cache serves stale) → reject → miss (LLM runs again).
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
    cleanup_cache_touch_hit(
        &db,
        key,
        &entry.created_at,
        1,
        2,
        "2026-01-01 00:00:00",
        "2999-01-01 00:00:00",
    )
    .expect("touch");

    // User deletes output within 10s → monitor fires.
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
