//! Tests for the sync engine: change capture, LWW merging, deletes,
//! natural-key conflicts, snapshots, counter/settings merge, and a full
//! protocol session over an in-memory duplex stream.

use anyhow::Result;
use serde_json::json;

use super::engine::{self, SyncHost, SYNCABLE_SETTINGS};
use super::protocol::Message;
use super::store as sync_store;
use crate::data::db;
use crate::DbHandle;

// ---- helpers ----

fn test_db(device_uuid: &str) -> DbHandle {
    let conn = db::open(":memory:").expect("test db");
    {
        let guard = conn.lock().expect("lock");
        sync_store::ensure_self_identity(&guard, device_uuid, "Test Device").expect("identity");
    }
    conn
}

fn uuid(prefix: &str) -> String {
    format!(
        "{prefix}-{}",
        crate::sync::identity::fingerprint_of(prefix.as_bytes())
            .get(..8)
            .expect("len")
            .to_string()
    )
}

fn row_uuid(conn: &rusqlite::Connection, table: &str, id: i64) -> String {
    conn.query_row(
        &format!("SELECT uuid FROM {table} WHERE id = ?1"),
        rusqlite::params![id],
        |r| r.get(0),
    )
    .expect("row uuid")
}

fn count(conn: &rusqlite::Connection, sql: &str) -> i64 {
    conn.query_row(sql, [], |r| r.get(0)).expect("count")
}

/// Exchanges the full collapsed change sets between two databases until both
/// converge (two rounds is always enough for these tests).
fn exchange(a: &DbHandle, b: &DbHandle) {
    for _ in 0..2 {
        for (from, to) in [(a, b), (b, a)] {
            let ops = {
                let conn = from.lock().expect("lock");
                let mut progress = engine::SnapshotProgress::default();
                let (ops, _cursor, _done) =
                    engine::collect_ops(&conn, 0, false, 10_000, &mut progress).expect("collect");
                ops
            };
            let summary = {
                let conn = to.lock().expect("lock");
                engine::apply_ops(&conn, &ops).expect("apply")
            };
            let _ = summary;
        }
    }
}

struct TestHost {
    uuid: String,
    settings: std::sync::Mutex<std::collections::HashMap<String, serde_json::Value>>,
    stamps: std::sync::Mutex<std::collections::HashMap<String, (i64, String)>>,
}

impl TestHost {
    fn new(uuid: &str) -> Self {
        Self {
            uuid: uuid.to_string(),
            settings: Default::default(),
            stamps: Default::default(),
        }
    }
}

impl SyncHost for TestHost {
    fn device_uuid(&self) -> String {
        self.uuid.clone()
    }

    fn device_name(&self) -> String {
        "Test".to_string()
    }

    fn app_version(&self) -> String {
        "test".to_string()
    }

    fn settings_payload(&self) -> Result<Vec<super::protocol::SettingRecord>> {
        let settings = self.settings.lock().expect("settings");
        let stamps = self.stamps.lock().expect("stamps");
        Ok(settings
            .iter()
            .map(|(key, value)| super::protocol::SettingRecord {
                key: key.clone(),
                value: value.clone(),
                ts_ms: stamps.get(key).map(|s| s.0).unwrap_or(0),
                origin: stamps.get(key).map(|s| s.1.clone()).unwrap_or_default(),
            })
            .collect())
    }

    fn apply_remote_setting(&self, key: &str, value: &serde_json::Value) -> Result<(), String> {
        self.settings
            .lock()
            .expect("settings")
            .insert(key.to_string(), value.clone());
        Ok(())
    }
}

fn peer_of(device_uuid: &str) -> sync_store::SyncPeer {
    sync_store::SyncPeer {
        device_uuid: device_uuid.to_string(),
        name: "Peer".to_string(),
        cert_fp: String::new(),
        added_at: String::new(),
        last_sync_at: None,
        send_cursor: 0,
        needs_snapshot: true,
        last_error: None,
    }
}

// ---- change capture ----

#[test]
fn triggers_capture_content_changes() {
    let a_uuid = uuid("aaaa");
    let db = test_db(&a_uuid);
    let dict =
        db::insert_dictionary_entry_returning(&db, "Groq", Some("Grock"), None).expect("insert");
    let snippet =
        db::insert_snippet_returning(&db, "addr", "123 Main St", "", None).expect("snippet");
    let context =
        db::insert_context_returning(&db, "Work", None, None, None, None, false).expect("context");
    db::assign_context_target(&db, context.id, "code.exe").expect("target");

    let conn = db.lock().expect("lock");
    let log_count = count(&conn, "SELECT COUNT(*) FROM sync_log");
    assert!(log_count >= 4, "expected >= 4 log entries, got {log_count}");
    let dict_uuid = row_uuid(&conn, "dictionary", dict.id);
    let stamped = sync_store::latest_op_stamp(&conn, "dictionary", &dict_uuid)
        .expect("stamp")
        .expect("stamp exists");
    assert_eq!(stamped.1, a_uuid, "origin should be this device");
    let snippet_uuid = row_uuid(&conn, "snippets", snippet.id);
    assert!(
        uuid::Uuid::parse_str(&dict_uuid).is_ok(),
        "trigger-generated dictionary UUID should be canonical"
    );
    assert!(
        uuid::Uuid::parse_str(&snippet_uuid).is_ok(),
        "trigger-generated snippet UUID should be canonical"
    );
    assert!(
        sync_store::latest_op_stamp(&conn, "snippets", &snippet_uuid)
            .expect("stamp")
            .is_some(),
        "snippet change captured"
    );
}

#[test]
fn engine_applied_ops_are_not_recaptured() {
    let db = test_db(&uuid("bbbb"));
    let op = test_dictionary_op("term-a", 100);
    {
        let conn = db.lock().expect("lock");
        engine::apply_ops(&conn, &[op.clone()]).expect("apply");
    }
    let conn = db.lock().expect("lock");
    // Exactly one log entry: the manually logged remote op (with its original
    // stamp), not a re-captured local echo.
    assert_eq!(count(&conn, "SELECT COUNT(*) FROM sync_log"), 1);
    let stamp = sync_store::latest_op_stamp(&conn, "dictionary", &op.row_uuid)
        .expect("stamp")
        .expect("stamp");
    assert_eq!(stamp.0, 100, "remote stamp must be preserved");
}

fn test_dictionary_op(term: &str, ts_ms: i64) -> super::protocol::SyncOp {
    super::protocol::SyncOp {
        table: "dictionary".to_string(),
        row_uuid: uuid(term),
        op: "upsert".to_string(),
        ts_ms,
        origin: uuid("origin"),
        origin_seq: ts_ms,
        payload: Some(json!({
            "term": term,
            "mistake": null,
            "auto_learned": false,
            "correction_count": 0,
            "confidence_tier": "manual",
            "last_seen_at": null,
            "created_at": "2026-01-01 00:00:00",
        })),
    }
}

// ---- bidirectional merge ----

#[test]
fn content_syncs_both_directions_without_duplicates() {
    let a = test_db(&uuid("aaaa"));
    let b = test_db(&uuid("bbbb"));

    let entry_a =
        db::insert_dictionary_entry_returning(&a, "Groq", Some("Grock"), None).expect("a entry");
    let _snippet_b =
        db::insert_snippet_returning(&b, "addr", "123 Main St", "", None).expect("b snippet");
    let ctx_a =
        db::insert_context_returning(&a, "Work", None, None, None, None, false).expect("ctx");
    db::assign_context_target(&a, ctx_a.id, "code.exe").expect("target");
    // Scope A's dictionary entry to A's context.
    db::set_dictionary_context_assignment(&a, ctx_a.id, entry_a.id, true).expect("assign");

    exchange(&a, &b);

    let conn_b = b.lock().expect("lock");
    assert_eq!(count(&conn_b, "SELECT COUNT(*) FROM dictionary"), 1);
    assert_eq!(count(&conn_b, "SELECT COUNT(*) FROM snippets"), 1);
    assert_eq!(
        count(
            &conn_b,
            "SELECT COUNT(*) FROM contexts WHERE is_everywhere = 0"
        ),
        1
    );
    assert_eq!(count(&conn_b, "SELECT COUNT(*) FROM context_targets"), 1);
    assert_eq!(
        count(&conn_b, "SELECT COUNT(*) FROM dictionary_contexts"),
        1
    );
    drop(conn_b);

    // And back: B's snippet reaches A.
    let conn_a = a.lock().expect("lock");
    assert_eq!(count(&conn_a, "SELECT COUNT(*) FROM snippets"), 1);
    let snippet_on_a: String = conn_a
        .query_row("SELECT expansion FROM snippets", [], |r| r.get(0))
        .expect("snippet");
    assert_eq!(snippet_on_a, "123 Main St");
    drop(conn_a);

    // Idempotence: exchanging again must not create duplicates.
    exchange(&a, &b);
    let conn_b = b.lock().expect("lock");
    assert_eq!(count(&conn_b, "SELECT COUNT(*) FROM dictionary"), 1);
    assert_eq!(count(&conn_b, "SELECT COUNT(*) FROM snippets"), 1);
}

#[test]
fn edits_and_deletes_propagate() {
    let a = test_db(&uuid("aaaa"));
    let b = test_db(&uuid("bbbb"));

    // A creates an entry; it reaches B.
    let entry =
        db::insert_dictionary_entry_returning(&a, "Groq", Some("Grock"), None).expect("entry");
    exchange(&a, &b);
    {
        let conn = b.lock().expect("lock");
        assert_eq!(count(&conn, "SELECT COUNT(*) FROM dictionary"), 1);
    }

    // B edits it offline; the edit reaches A.
    let local_id = {
        let conn = b.lock().expect("lock");
        conn.query_row("SELECT id FROM dictionary", [], |r| r.get(0))
            .expect("id")
    };
    db::update_dictionary_entry(&b, local_id, "Groq", Some("Groqck")).expect("edit");
    exchange(&a, &b);
    {
        let conn = a.lock().expect("lock");
        let mistake: String = conn
            .query_row(
                "SELECT mistake FROM dictionary WHERE id = ?1",
                rusqlite::params![entry.id],
                |r| r.get(0),
            )
            .expect("mistake");
        assert_eq!(mistake, "Groqck", "B's edit reached A");
    }

    // B deletes it; the delete reaches A and nothing resurrects.
    db::delete_dictionary_entry(&b, local_id).expect("delete on b");
    exchange(&a, &b);
    let conn_a = a.lock().expect("lock");
    assert_eq!(
        count(&conn_a, "SELECT COUNT(*) FROM dictionary"),
        0,
        "delete propagated"
    );
    drop(conn_a);
    let conn_b = b.lock().expect("lock");
    assert_eq!(count(&conn_b, "SELECT COUNT(*) FROM dictionary"), 0);
}

#[test]
fn delete_loses_to_a_newer_edit() {
    let db = test_db(&uuid("aaaa"));
    let upsert = test_dictionary_op("term-x", 5_000);
    {
        let conn = db.lock().expect("lock");
        engine::apply_ops(&conn, &[upsert.clone()]).expect("apply");
        let delete = super::protocol::SyncOp {
            table: "dictionary".to_string(),
            row_uuid: upsert.row_uuid.clone(),
            op: "delete".to_string(),
            ts_ms: 1_000, // older than the upsert
            origin: uuid("origin"),
            origin_seq: 1_000,
            payload: None,
        };
        let summary = engine::apply_ops(&conn, &[delete]).expect("apply delete");
        assert_eq!(
            summary.skipped, 1,
            "older delete must lose to the newer edit"
        );
    }
    let conn = db.lock().expect("lock");
    assert_eq!(count(&conn, "SELECT COUNT(*) FROM dictionary"), 1);
}

#[test]
fn natural_key_conflict_converges_to_one_row() {
    let a = test_db(&uuid("aaaa"));
    let b = test_db(&uuid("bbbb"));

    // Both devices independently create the same dictionary term.
    db::insert_dictionary_entry_returning(&a, "Verenu", None, None).expect("a row");
    db::insert_dictionary_entry_returning(&b, "Verenu", None, None).expect("b row");
    exchange(&a, &b);

    for (name, db_handle) in [("a", &a), ("b", &b)] {
        let conn = db_handle.lock().expect("lock");
        let n = count(&conn, "SELECT COUNT(*) FROM dictionary");
        assert_eq!(n, 1, "device {name} should have exactly one Verenu entry");
        let uuids: Vec<String> = {
            let mut stmt = conn.prepare("SELECT uuid FROM dictionary").expect("q");
            let rows = stmt
                .query_map([], |r| r.get(0))
                .expect("map")
                .collect::<rusqlite::Result<Vec<_>>>()
                .expect("rows");
            rows
        };
        drop(conn);
        // Same surviving uuid on both devices.
        let other = if name == "a" { &b } else { &a };
        let conn_other = other.lock().expect("lock");
        let other_uuids: Vec<String> = {
            let mut stmt = conn_other
                .prepare("SELECT uuid FROM dictionary")
                .expect("q");
            stmt.query_map([], |r| r.get(0))
                .expect("map")
                .collect::<rusqlite::Result<Vec<_>>>()
                .expect("rows")
        };
        assert_eq!(uuids, other_uuids, "same row must win on both devices");
    }
}

#[test]
fn older_op_never_overwrites_newer() {
    let db = test_db(&uuid("aaaa"));
    let newer = test_dictionary_op("term-new", 5_000);
    {
        let conn = db.lock().expect("lock");
        engine::apply_ops(&conn, &[newer.clone()]).expect("apply newer");
        // The same row re-delivered with an older stamp must be skipped.
        let mut stale = newer.clone();
        stale.ts_ms = 999;
        stale.origin_seq = 999;
        let summary = engine::apply_ops(&conn, &[stale]).expect("apply stale");
        assert_eq!(summary.skipped, 1, "stale op should be skipped");
        assert_eq!(summary.applied, 0);
    }
    let conn = db.lock().expect("lock");
    let term: String = conn
        .query_row("SELECT term FROM dictionary", [], |r| r.get(0))
        .expect("term");
    assert_eq!(term, "term-new");
}

// ---- full session over an in-memory stream ----

#[tokio::test]
async fn session_snapshot_seeds_a_new_device() {
    let a = test_db(&uuid("aaaa"));
    let b = test_db(&uuid("bbbb"));

    for i in 0..5 {
        db::insert_dictionary_entry_returning(&a, &format!("term{i}"), None, None).expect("entry");
    }
    db::insert_snippet_returning(&a, "sig", "Best regards", "", None).expect("snippet");
    let ctx =
        db::insert_context_returning(&a, "Meetings", None, None, None, None, false).expect("ctx");
    db::assign_context_website(&a, ctx.id, "meet.example.com").expect("site");

    let host_a = TestHost::new(&uuid("aaaa"));
    let host_b = TestHost::new(&uuid("bbbb"));
    let (mut side_a, mut side_b) = tokio::io::duplex(1024 * 1024);

    pair_test_dbs(&a, &b, &host_a.uuid, &host_b.uuid);
    let peer_a = peer_of(&host_b.uuid);
    let peer_b = peer_of(&host_a.uuid);
    let (session_a, session_b) = tokio::join!(
        engine::run_session(&a, &host_a, &mut side_a, true, &peer_a),
        engine::run_session(&b, &host_b, &mut side_b, false, &peer_b),
    );
    session_a.expect("session a");
    session_b.expect("session b");

    let conn_b = b.lock().expect("lock");
    assert_eq!(count(&conn_b, "SELECT COUNT(*) FROM dictionary"), 5);
    assert_eq!(count(&conn_b, "SELECT COUNT(*) FROM snippets"), 1);
    assert_eq!(
        count(&conn_b, "SELECT COUNT(*) FROM context_website_targets"),
        1
    );
    // B recorded the snapshot position, and A no longer owes B a snapshot.
    let recv_cursor = sync_store::peer_recv_cursor(&conn_b, &host_a.uuid).expect("pos");
    assert!(recv_cursor > 0, "B should have recorded A's log position");
    drop(conn_b);
    let conn_a = a.lock().expect("lock");
    let (_, needs_snapshot) = sync_store::peer_send_position(&conn_a, &host_b.uuid).expect("pos");
    assert!(!needs_snapshot, "A should have cleared the snapshot flag");
}

#[test]
fn stale_snapshot_does_not_overwrite_newer_local_row() {
    let a = test_db(&uuid("aaaa"));
    let b = test_db(&uuid("bbbb"));
    let old = test_dictionary_op("snapshot-old", 1_000);
    let mut newer = old.clone();
    newer.ts_ms = 5_000;
    newer.origin_seq = 5_000;
    newer.payload = Some(json!({
        "term": "local-newer",
        "mistake": null,
        "auto_learned": false,
        "correction_count": 0,
        "confidence_tier": "manual",
        "last_seen_at": null,
        "created_at": "2026-01-01 00:00:00",
    }));

    {
        let conn = a.lock().expect("lock");
        engine::apply_ops(&conn, &[old]).expect("old row");
    }
    {
        let conn = b.lock().expect("lock");
        engine::apply_ops(&conn, &[newer]).expect("newer row");
    }

    let snapshot = {
        let conn = a.lock().expect("lock");
        let mut progress = engine::SnapshotProgress::default();
        let mut snapshot = Vec::new();
        loop {
            let (ops, _cursor, done) =
                engine::collect_ops(&conn, 0, true, 1, &mut progress).expect("collect snapshot");
            snapshot.extend(ops);
            if done {
                break;
            }
        }
        snapshot
    };
    {
        let conn = b.lock().expect("lock");
        engine::apply_ops(&conn, &snapshot).expect("apply snapshot");
        let term: String = conn
            .query_row("SELECT term FROM dictionary", [], |r| r.get(0))
            .expect("term");
        assert_eq!(term, "local-newer");
    }
}

#[tokio::test]
async fn session_exchanges_changes_incrementally() {
    let a = test_db(&uuid("aaaa"));
    let b = test_db(&uuid("bbbb"));
    let host_a = TestHost::new(&uuid("aaaa"));
    let host_b = TestHost::new(&uuid("bbbb"));

    // First session: seed.
    db::insert_dictionary_entry_returning(&a, "one", None, None).expect("entry");
    run_two_sessions(&a, &b, &host_a, &host_b).await;
    let conn_b = b.lock().expect("lock");
    assert_eq!(count(&conn_b, "SELECT COUNT(*) FROM dictionary"), 1);
    drop(conn_b);

    // Offline edits on both sides, then reconnect: both changes must land.
    db::insert_dictionary_entry_returning(&a, "from-a", None, None).expect("a entry");
    db::insert_snippet_returning(&b, "from-b", "B", "", None).expect("b snippet");
    run_two_sessions(&a, &b, &host_a, &host_b).await;

    let conn_a = a.lock().expect("lock");
    assert_eq!(count(&conn_a, "SELECT COUNT(*) FROM dictionary"), 2);
    assert_eq!(count(&conn_a, "SELECT COUNT(*) FROM snippets"), 1);
    drop(conn_a);
    let conn_b = b.lock().expect("lock");
    assert_eq!(count(&conn_b, "SELECT COUNT(*) FROM dictionary"), 2);
    assert_eq!(count(&conn_b, "SELECT COUNT(*) FROM snippets"), 1);
}

async fn run_two_sessions(a: &DbHandle, b: &DbHandle, host_a: &TestHost, host_b: &TestHost) {
    pair_test_dbs(a, b, &host_a.uuid, &host_b.uuid);
    let (mut side_a, mut side_b) = tokio::io::duplex(1024 * 1024);
    let peer_a = peer_of(&host_b.uuid);
    let peer_b = peer_of(&host_a.uuid);
    let (r1, r2) = tokio::join!(
        engine::run_session(a, host_a, &mut side_a, true, &peer_a),
        engine::run_session(b, host_b, &mut side_b, false, &peer_b),
    );
    r1.expect("session a");
    r2.expect("session b");
}

// ---- counters + settings ----

#[tokio::test]
async fn lifetime_counters_merge_without_double_counting() {
    let a = test_db(&uuid("aaaa"));
    let b = test_db(&uuid("bbbb"));
    let host_a = TestHost::new(&uuid("aaaa"));
    let host_b = TestHost::new(&uuid("bbbb"));

    // Each device dictated: A 100 words, B 40 words.
    db::insert_transcription_returning(
        &a,
        "one two three",
        "one two three",
        100,
        6_000,
        "",
        None,
        None,
    )
    .expect("transcribe a");
    db::insert_transcription_returning(&b, "hello world", "hello world", 40, 4_000, "", None, None)
        .expect("transcribe b");

    run_two_sessions(&a, &b, &host_a, &host_b).await;

    for (name, handle) in [("a", &a), ("b", &b)] {
        let conn = handle.lock().expect("lock");
        let (words, _fixes) = sync_store::effective_lifetime_totals(&conn).expect("totals");
        assert_eq!(words, 140, "device {name} merged word count");
        assert_eq!(count(&conn, "SELECT COUNT(*) FROM transcriptions"), 2);
        // History rows arrived with their original timestamps and text.
        let raw: String = conn
            .query_row(
                "SELECT raw_text FROM transcriptions WHERE words = 100",
                [],
                |r| r.get(0),
            )
            .expect("row");
        assert_eq!(raw, "one two three");
    }
}

#[tokio::test]
async fn settings_lww_applies_newer_remote_value() {
    let a = test_db(&uuid("aaaa"));
    let b = test_db(&uuid("bbbb"));
    let host_a = TestHost::new(&uuid("aaaa"));
    let host_b = TestHost::new(&uuid("bbbb"));

    // Realistic wall-clock stamps so the seeding baseline (now_ms) compares
    // the way production stamps do.
    let base = sync_store::now_ms();
    host_a
        .settings
        .lock()
        .expect("s")
        .insert("default_tone".to_string(), json!("formal"));
    host_a.stamps.lock().expect("s").insert(
        "default_tone".to_string(),
        (base + 5_000, host_a.uuid.clone()),
    );

    run_two_sessions(&a, &b, &host_a, &host_b).await;
    let applied = host_b
        .settings
        .lock()
        .expect("s")
        .get("default_tone")
        .cloned();
    assert_eq!(applied, Some(json!("formal")), "newer remote setting wins");

    // B's older local change (stamped before A's) must not overwrite it.
    host_b
        .settings
        .lock()
        .expect("s")
        .insert("default_tone".to_string(), json!("casual"));
    host_b.stamps.lock().expect("s").insert(
        "default_tone".to_string(),
        (base + 1_000, host_b.uuid.clone()),
    );
    // Production stamps the DB when a setting changes locally (save_setting
    // hook); mirror that here so B's LWW stamp matches its value.
    {
        let conn = b.lock().expect("lock");
        sync_store::set_setting_stamp(&conn, "default_tone", base + 1_000, &host_b.uuid)
            .expect("stamp");
    }
    run_two_sessions(&a, &b, &host_a, &host_b).await;
    let applied = host_b
        .settings
        .lock()
        .expect("s")
        .get("default_tone")
        .cloned();
    assert_eq!(applied, Some(json!("formal")), "older local value loses");
}

#[test]
fn syncable_settings_exclude_device_local_keys() {
    // Device-specific and secret keys must never appear in the allowlist.
    for key in [
        crate::data::store::KEY_GROQ,
        crate::data::store::KEY_OPENAI,
        crate::data::store::KEY_GOOGLE,
        crate::data::store::KEY_ASSEMBLYAI,
        crate::data::store::MICROPHONE_DEVICE,
        crate::data::store::HOTKEY,
        crate::data::store::REPAIR_HOTKEY,
        crate::data::store::AUTOSTART_ENABLED,
        crate::data::store::SETUP_COMPLETE,
        crate::data::store::FORCE_SETUP_ON_LAUNCH,
    ] {
        assert!(
            !SYNCABLE_SETTINGS.contains(&key),
            "{key} must stay device-local"
        );
    }
}

// ---- protocol framing ----

#[tokio::test]
async fn messages_roundtrip_through_the_framer() {
    use super::protocol::{read_message, send_message};
    let (mut client, mut server) = tokio::io::duplex(64 * 1024);
    let hello = Message::Hello(super::protocol::Hello {
        device_uuid: "device-1".to_string(),
        device_name: "Device One".to_string(),
        protocol: super::protocol::PROTOCOL_VERSION,
        app_version: "0.0.0".to_string(),
    });
    let (send, read) = tokio::join!(send_message(&mut client, &hello), async {
        let message: Message = read_message(&mut server).await.expect("read");
        message
    });
    send.expect("send");
    match read {
        Message::Hello(h) => {
            assert_eq!(h.device_uuid, "device-1");
            assert_eq!(h.device_name, "Device One");
        }
        other => panic!("wrong message: {other:?}"),
    }
}

// ---- pairing handshake ----

#[tokio::test]
async fn pairing_succeeds_with_matching_code_and_fails_with_wrong_code() {
    use super::pairing::{self, IdentityExchange};
    use super::protocol::{read_message, send_message};

    let identity_a = IdentityExchange {
        device_uuid: uuid("aaaa"),
        device_name: "A".to_string(),
        cert_der: vec![1, 2, 3],
    };
    let identity_b = IdentityExchange {
        device_uuid: uuid("bbbb"),
        device_name: "B".to_string(),
        cert_der: vec![4, 5, 6],
    };
    let identity_b_for_second = identity_b.clone();
    let code = pairing::generate_pairing_code();
    assert_eq!(code.len(), 6);

    let (mut a, mut b) = tokio::io::duplex(64 * 1024);
    let (state_a, msg_a) = pairing::initiator_start(&code);
    let code_b = code.clone();
    let expected_uuid = identity_b.device_uuid.clone();
    let expected_uuid_for_initiator = expected_uuid.clone();
    let id_a = identity_a.clone();
    let (initiator, responder) = tokio::join!(
        async move {
            send_message(
                &mut a,
                &Message::PairRequest {
                    device_uuid: id_a.device_uuid.clone(),
                    device_name: id_a.device_name.clone(),
                    protocol: super::protocol::PROTOCOL_VERSION,
                    spake_msg: msg_a,
                },
            )
            .await
            .expect("send request");
            let responder_msg = match read_message(&mut a).await.expect("accept") {
                Message::PairAccept { spake_msg } => spake_msg,
                other => panic!("expected accept, got {other:?}"),
            };
            let cipher = pairing::initiator_cipher(state_a, &responder_msg).expect("cipher");
            pairing::initiator_exchange(&mut a, &cipher, &id_a, &expected_uuid_for_initiator).await
        },
        async move {
            let request = read_message(&mut b).await.expect("request");
            let Message::PairRequest {
                spake_msg,
                device_uuid,
                ..
            } = request
            else {
                panic!("expected pair request");
            };
            let (msg_b, cipher) = pairing::responder_start(&code_b, &spake_msg).expect("start");
            pairing::responder_exchange(&mut b, &cipher, msg_b, &identity_b, &device_uuid).await
        },
    );
    let peer_from_a = initiator.expect("initiator outcome");
    responder.expect("responder outcome");
    assert_eq!(peer_from_a.device_uuid, expected_uuid);
    assert_eq!(peer_from_a.cert_der, vec![4, 5, 6]);

    // Wrong code: both sides must fail with a friendly error, not panic.
    let identity_a2 = IdentityExchange {
        cert_der: vec![7],
        ..identity_a
    };
    let identity_b2 = IdentityExchange {
        cert_der: vec![8],
        ..identity_b_for_second
    };
    let expected_uuid2 = identity_b2.device_uuid.clone();
    let (mut a2, mut b2) = tokio::io::duplex(64 * 1024);
    let (state_a2, msg_a2) = pairing::initiator_start("111111");
    let (initiator, responder) = tokio::join!(
        async move {
            send_message(
                &mut a2,
                &Message::PairRequest {
                    device_uuid: identity_a2.device_uuid.clone(),
                    device_name: identity_a2.device_name.clone(),
                    protocol: super::protocol::PROTOCOL_VERSION,
                    spake_msg: msg_a2,
                },
            )
            .await
            .expect("send");
            let responder_msg = match read_message(&mut a2).await.expect("accept") {
                Message::PairAccept { spake_msg } => spake_msg,
                other => panic!("expected accept, got {other:?}"),
            };
            let cipher = pairing::initiator_cipher(state_a2, &responder_msg).expect("cipher");
            pairing::initiator_exchange(&mut a2, &cipher, &identity_a2, &expected_uuid2).await
        },
        async move {
            let request = read_message(&mut b2).await.expect("request");
            let Message::PairRequest {
                spake_msg,
                device_uuid,
                ..
            } = request
            else {
                panic!("expected pair request");
            };
            // Responder uses a DIFFERENT code.
            let (msg_b, cipher) = pairing::responder_start("999999", &spake_msg).expect("start");
            pairing::responder_exchange(&mut b2, &cipher, msg_b, &identity_b2, &device_uuid).await
        },
    );
    assert!(initiator.is_err(), "initiator must fail on code mismatch");
    assert!(responder.is_err(), "responder must fail on code mismatch");
}

/// Registers the two test databases as paired peers of each other, mirroring
/// what `complete_pairing` does in production (the session driver reads the
/// peer row for cursor state, and the manager only dials paired peers).
fn pair_test_dbs(a: &DbHandle, b: &DbHandle, a_uuid: &str, b_uuid: &str) {
    {
        let conn = a.lock().expect("lock");
        sync_store::upsert_peer(&conn, b_uuid, "B", "fp-b").expect("upsert a->b");
    }
    {
        let conn = b.lock().expect("lock");
        sync_store::upsert_peer(&conn, a_uuid, "A", "fp-a").expect("upsert b->a");
    }
}
