//! SQLite access for sync bookkeeping: peers, the change log, remote lifetime
//! counters, and settings sync timestamps. Schema lives in `data/db/schema.rs`
//! (v19); this module is the only place that touches those tables.

use anyhow::Result;
use rusqlite::{params, Connection, OptionalExtension};

use super::protocol::SyncOp;

/// Row of `sync_peers` - a paired, trusted device.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SyncPeer {
    pub device_uuid: String,
    pub name: String,
    pub cert_fp: String,
    pub added_at: String,
    pub last_sync_at: Option<String>,
    pub send_cursor: i64,
    pub needs_snapshot: bool,
    pub last_error: Option<String>,
}

/// One device's lifetime counters as reported by that device.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DeviceStats {
    pub device_id: String,
    pub total_words: i64,
    pub dictionary_fixes: i64,
}

pub fn set_sync_applying(conn: &Connection, applying: bool) -> Result<()> {
    conn.execute(
        "UPDATE sync_state SET applying = ?1",
        params![if applying { 1 } else { 0 }],
    )?;
    Ok(())
}

pub fn self_uuid(conn: &Connection) -> Result<Option<String>> {
    let uuid: Option<String> = conn
        .query_row("SELECT uuid FROM sync_identity LIMIT 1", [], |r| r.get(0))
        .optional()?;
    Ok(uuid)
}

pub fn ensure_self_identity(conn: &Connection, uuid: &str, name: &str) -> Result<()> {
    let existing = self_uuid(conn)?;
    match existing {
        Some(existing) if existing == uuid => {
            conn.execute("UPDATE sync_identity SET name = ?1", params![name])?;
        }
        // A changed uuid (keychain wiped and regenerated) keeps the row single:
        // old log entries keep their original origin, which stays valid history.
        _ => {
            conn.execute("DELETE FROM sync_identity", [])?;
            conn.execute(
                "INSERT INTO sync_identity (uuid, name) VALUES (?1, ?2)",
                params![uuid, name],
            )?;
        }
    }
    Ok(())
}

pub fn list_peers(conn: &Connection) -> Result<Vec<SyncPeer>> {
    let mut stmt = conn.prepare(
        "SELECT device_uuid, name, cert_fp, added_at, last_sync_at, send_cursor, needs_snapshot, last_error
         FROM sync_peers ORDER BY added_at ASC",
    )?;
    let rows = stmt
        .query_map([], |r| {
            Ok(SyncPeer {
                device_uuid: r.get(0)?,
                name: r.get(1)?,
                cert_fp: r.get(2)?,
                added_at: r.get(3)?,
                last_sync_at: r.get(4)?,
                send_cursor: r.get(5)?,
                needs_snapshot: r.get::<_, i64>(6)? != 0,
                last_error: r.get(7)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

pub fn get_peer(conn: &Connection, device_uuid: &str) -> Result<Option<SyncPeer>> {
    let mut stmt = conn.prepare(
        "SELECT device_uuid, name, cert_fp, added_at, last_sync_at, send_cursor, needs_snapshot, last_error
         FROM sync_peers WHERE device_uuid = ?1",
    )?;
    let row = stmt
        .query_row(params![device_uuid], |r| {
            Ok(SyncPeer {
                device_uuid: r.get(0)?,
                name: r.get(1)?,
                cert_fp: r.get(2)?,
                added_at: r.get(3)?,
                last_sync_at: r.get(4)?,
                send_cursor: r.get(5)?,
                needs_snapshot: r.get::<_, i64>(6)? != 0,
                last_error: r.get(7)?,
            })
        })
        .optional()?;
    Ok(row)
}

/// Inserts or updates a paired device. `cert_fp` is the hex SHA-256 of the
/// peer's certificate DER - the pin every future connection is checked against.
pub fn upsert_peer(
    conn: &Connection,
    device_uuid: &str,
    name: &str,
    cert_fp: &str,
) -> Result<()> {
    conn.execute(
        "INSERT INTO sync_peers (device_uuid, name, cert_fp, needs_snapshot)
         VALUES (?1, ?2, ?3, 1)
         ON CONFLICT(device_uuid) DO UPDATE SET
           name = excluded.name,
           cert_fp = excluded.cert_fp,
           needs_snapshot = 1,
           send_cursor = 0,
           recv_cursor = 0,
           last_error = NULL",
        params![device_uuid, name, cert_fp],
    )?;
    Ok(())
}

pub fn remove_peer(conn: &Connection, device_uuid: &str) -> Result<bool> {
    let n = conn.execute(
        "DELETE FROM sync_peers WHERE device_uuid = ?1",
        params![device_uuid],
    )?;
    Ok(n > 0)
}

pub fn mark_peer_synced(conn: &Connection, device_uuid: &str, send_cursor: i64) -> Result<()> {
    conn.execute(
        "UPDATE sync_peers
         SET last_sync_at = datetime('now'), send_cursor = MAX(send_cursor, ?2), last_error = NULL
         WHERE device_uuid = ?1",
        params![device_uuid, send_cursor],
    )?;
    Ok(())
}

pub fn mark_peer_error(conn: &Connection, device_uuid: &str, error: &str) -> Result<()> {
    conn.execute(
        "UPDATE sync_peers SET last_error = ?2 WHERE device_uuid = ?1",
        params![device_uuid, truncate_error(error)],
    )?;
    Ok(())
}

fn truncate_error(error: &str) -> String {
    error.chars().take(300).collect()
}

/// Highest log position this device has sent (and had acknowledged) to the
/// peer, and whether the peer still needs a full snapshot (new pairing, or its
/// cursor points before our oldest retained log entry).
pub fn peer_send_position(conn: &Connection, device_uuid: &str) -> Result<(i64, bool)> {
    // A missing row (peer removed mid-session, or a defensive caller) means
    // "start over from a snapshot" rather than a hard error - the trust check
    // happened before the session started.
    let row = conn
        .query_row(
            "SELECT send_cursor, needs_snapshot FROM sync_peers WHERE device_uuid = ?1",
            params![device_uuid],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?
        .unwrap_or((0, 1));
    Ok((row.0, row.1 != 0))
}

/// Decides whether a pull from `device_uuid` must be a full snapshot: either
/// the peer was just paired, or its cursor predates our oldest retained log
/// entry (it was offline longer than the log's retention window).
pub fn needs_snapshot_for(conn: &Connection, device_uuid: &str, since_seq: i64) -> Result<bool> {
    let (_, peer_flag) = peer_send_position(conn, device_uuid)?;
    if peer_flag {
        return Ok(true);
    }
    match oldest_log_seq(conn)? {
        Some(oldest) => Ok(since_seq < oldest.saturating_sub(1)),
        None => Ok(false),
    }
}

pub fn set_peer_send_position(
    conn: &Connection,
    device_uuid: &str,
    send_cursor: i64,
    needs_snapshot: bool,
) -> Result<()> {
    conn.execute(
        "UPDATE sync_peers SET send_cursor = ?2, needs_snapshot = ?3 WHERE device_uuid = ?1",
        params![device_uuid, send_cursor, if needs_snapshot { 1 } else { 0 }],
    )?;
    Ok(())
}

/// Highest position of THE PEER'S log this device has pulled and applied.
/// Kept separate from `send_cursor` on purpose: serving a pull advances what
/// we've sent, pulling advances what we've received, and conflating the two
/// made each session skip the peer's oldest changes.
pub fn peer_recv_cursor(conn: &Connection, device_uuid: &str) -> Result<i64> {
    let cursor: i64 = conn
        .query_row(
            "SELECT recv_cursor FROM sync_peers WHERE device_uuid = ?1",
            params![device_uuid],
            |r| r.get(0),
        )
        .optional()?
        .unwrap_or(0);
    Ok(cursor)
}

pub fn set_peer_recv_cursor(conn: &Connection, device_uuid: &str, cursor: i64) -> Result<()> {
    conn.execute(
        "UPDATE sync_peers
         SET recv_cursor = MAX(recv_cursor, ?2), needs_snapshot = 0
         WHERE device_uuid = ?1",
        params![device_uuid, cursor],
    )?;
    Ok(())
}

/// Oldest sequence number still present in the change log (None = empty log).
fn oldest_log_seq(conn: &Connection) -> Result<Option<i64>> {
    let seq: Option<i64> = conn
        .query_row("SELECT MIN(seq) FROM sync_log", [], |r| r.get(0))
        .optional()?
        .flatten();
    Ok(seq)
}

/// Latest log entry recorded for a row, used as the local LWW stamp.
pub fn latest_op_stamp(
    conn: &Connection,
    table_name: &str,
    row_uuid: &str,
) -> Result<Option<(i64, String, i64)>> {
    let row = conn
        .query_row(
            "SELECT ts_ms, origin, origin_seq FROM sync_log
             WHERE table_name = ?1 AND row_uuid = ?2
             ORDER BY seq DESC LIMIT 1",
            params![table_name, row_uuid],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()?;
    Ok(row)
}

/// Appends an op to the local change log, preserving the original timestamp
/// and origin so peers can dedup and order it deterministically.
pub fn append_op(
    conn: &Connection,
    op: &SyncOp,
    ts_ms: i64,
    origin: &str,
    origin_seq: i64,
) -> Result<()> {
    conn.execute(
        "INSERT INTO sync_log (table_name, row_uuid, op, ts_ms, origin, origin_seq)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![op.table_name(), op.row_uuid, op.op_name(), ts_ms, origin, origin_seq],
    )?;
    Ok(())
}

/// One collapsed change-log entry: the latest op for a (table, row uuid).
#[derive(Debug, Clone)]
pub struct LogEntry {
    #[allow(dead_code)]
    pub seq: i64,
    pub table_name: String,
    pub row_uuid: String,
    pub op: String,
    pub ts_ms: i64,
    pub origin: String,
    pub origin_seq: i64,
}

/// Latest op per (table, row uuid) with `seq > after_seq`, ordered by seq.
/// SQLite's bare-column-with-MAX behavior returns the columns of the max-seq
/// row within each group.
pub fn changes_since(conn: &Connection, after_seq: i64, limit: i64) -> Result<Vec<LogEntry>> {
    let mut stmt = conn.prepare(
        "SELECT seq, table_name, row_uuid, op, ts_ms, origin, origin_seq
         FROM (SELECT seq, table_name, row_uuid, op, ts_ms, origin, origin_seq,
                      MAX(seq) OVER (PARTITION BY table_name, row_uuid) AS max_seq
               FROM sync_log WHERE seq > ?1)
         WHERE seq = max_seq
         ORDER BY seq ASC
         LIMIT ?2",
    )?;
    let rows = stmt
        .query_map(params![after_seq, limit], |r| {
            Ok(LogEntry {
                seq: r.get(0)?,
                table_name: r.get(1)?,
                row_uuid: r.get(2)?,
                op: r.get(3)?,
                ts_ms: r.get(4)?,
                origin: r.get(5)?,
                origin_seq: r.get(6)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

pub fn max_log_seq(conn: &Connection) -> Result<i64> {
    let seq: i64 = conn.query_row(
        "SELECT COALESCE(MAX(seq), 0) FROM sync_log",
        [],
        |r| r.get(0),
    )?;
    Ok(seq)
}

/// Drops change-log history that every paired device has acknowledged, always
/// keeping the newest entry per row (the LWW stamp the apply path compares
/// against). Entries a peer hasn't caught up to are kept so it can still pull
/// its delta; if it stays away too long it falls back to a full snapshot.
pub fn compact_log(conn: &Connection) -> Result<usize> {
    let min_cursor: Option<i64> = conn
        .query_row(
            "SELECT MIN(send_cursor) FROM sync_peers",
            [],
            |r| r.get(0),
        )
        .optional()?
        .flatten();
    let Some(min_cursor) = min_cursor else {
        return conn
            .execute(
                "DELETE FROM sync_log
                 WHERE seq NOT IN (SELECT MAX(seq) FROM sync_log GROUP BY table_name, row_uuid)",
                [],
            )
            .map_err(Into::into);
    };
    let n = conn.execute(
        "DELETE FROM sync_log
         WHERE seq <= ?1
           AND seq NOT IN (SELECT MAX(seq) FROM sync_log GROUP BY table_name, row_uuid)",
        params![min_cursor],
    )?;
    Ok(n)
}

pub fn list_remote_stats(conn: &Connection) -> Result<Vec<DeviceStats>> {
    let mut stmt = conn.prepare(
        "SELECT device_id, total_words, dictionary_fixes FROM sync_remote_stats",
    )?;
    let rows = stmt
        .query_map([], |r| {
            Ok(DeviceStats {
                device_id: r.get(0)?,
                total_words: r.get(1)?,
                dictionary_fixes: r.get(2)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

pub fn upsert_remote_stats(conn: &Connection, stats: &DeviceStats) -> Result<()> {
    conn.execute(
        "INSERT INTO sync_remote_stats (device_id, total_words, dictionary_fixes, updated_at)
         VALUES (?1, ?2, ?3, datetime('now'))
         ON CONFLICT(device_id) DO UPDATE SET
           total_words = excluded.total_words,
           dictionary_fixes = excluded.dictionary_fixes,
           updated_at = excluded.updated_at",
        params![stats.device_id, stats.total_words, stats.dictionary_fixes],
    )?;
    Ok(())
}

pub fn remove_remote_stats(conn: &Connection, device_id: &str) -> Result<()> {
    conn.execute(
        "DELETE FROM sync_remote_stats WHERE device_id = ?1",
        params![device_id],
    )?;
    Ok(())
}

/// Effective lifetime totals: this device's own counters plus every synced
/// peer's counters. Each dictation is counted exactly once (by the device it
/// happened on), so the sum never double-counts. Asserted by the merge tests.
#[cfg(test)]
pub fn effective_lifetime_totals(conn: &Connection) -> Result<(i64, i64)> {
    let row: (i64, i64) = conn.query_row(
        "SELECT COALESCE((SELECT total_words FROM lifetime_stats WHERE id = 1), 0)
              + COALESCE((SELECT SUM(total_words) FROM sync_remote_stats), 0),
                COALESCE((SELECT dictionary_fixes FROM lifetime_stats WHERE id = 1), 0)
              + COALESCE((SELECT SUM(dictionary_fixes) FROM sync_remote_stats), 0)",
        [],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    Ok(row)
}

/// Per-key sync stamps for synced settings (settings.json itself stays the
/// value store - this table only carries the LWW metadata).
#[derive(Debug, Clone)]
pub struct SettingStamp {
    pub ts_ms: i64,
    pub origin: String,
}

pub fn list_setting_stamps(conn: &Connection) -> Result<Vec<(String, SettingStamp)>> {
    let mut stmt = conn.prepare("SELECT key, ts_ms, origin FROM sync_setting_meta")?;
    let rows = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                SettingStamp { ts_ms: r.get(1)?, origin: r.get(2)? },
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

pub fn get_setting_stamp(conn: &Connection, key: &str) -> Result<Option<SettingStamp>> {
    let row = conn
        .query_row(
            "SELECT ts_ms, origin FROM sync_setting_meta WHERE key = ?1",
            params![key],
            |r| Ok(SettingStamp { ts_ms: r.get(0)?, origin: r.get(1)? }),
        )
        .optional()?;
    Ok(row)
}

pub fn set_setting_stamp(conn: &Connection, key: &str, ts_ms: i64, origin: &str) -> Result<()> {
    conn.execute(
        "INSERT INTO sync_setting_meta (key, ts_ms, origin) VALUES (?1, ?2, ?3)
         ON CONFLICT(key) DO UPDATE SET ts_ms = excluded.ts_ms, origin = excluded.origin",
        params![key, ts_ms, origin],
    )?;
    Ok(())
}

/// Called once per sync session on both sides: seeds stamps for every synced
/// key that doesn't have one yet, so the first session compares real
/// timestamps instead of treating every existing value as ancient.
pub fn seed_setting_stamps(conn: &Connection, origin: &str, keys: &[String]) -> Result<()> {
    let now_ms = now_ms();
    for key in keys {
        conn.execute(
            "INSERT OR IGNORE INTO sync_setting_meta (key, ts_ms, origin) VALUES (?1, ?2, ?3)",
            params![key, now_ms, origin],
        )?;
    }
    Ok(())
}

/// Wall-clock milliseconds since the Unix epoch (UTC), matching what the
/// change-capture triggers write into `sync_log.ts_ms`.
pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

