//! The sync engine: turns the local change log into wire ops, applies remote
//! ops with last-writer-wins semantics, and drives a full sync session over an
//! established (already-authenticated) framed message stream.
//!
//! Conflict model, in one place:
//! - Every row change carries a stamp (wall-clock ms, origin device uuid,
//!   origin sequence). Stamps are totally ordered, so every device converges.
//! - Upsert vs upsert / upsert vs delete: higher stamp wins (LWW per record).
//! - Natural-key collisions (two devices created "the same" dictionary term,
//!   snippet trigger, or context name independently): the row with the higher
//!   stamp survives and the loser is hard-deleted on both sides. When the
//!   local row wins, an anti-entropy tombstone is logged for the remote uuid
//!   so the originating device removes its losing row too.
//! - Contexts sync as aggregates (row + targets + memberships); the whole
//!   aggregate is LWW, so two devices editing the same context concurrently
//!   converge on the later edit wholesale.
//! - Settings are LWW per key using `sync_setting_meta` stamps.
//! - Lifetime counters are summed per device (each dictation is counted by
//!   exactly the device it happened on), so totals merge without double-count.

use anyhow::{anyhow, Context as _, Result};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::data::{db, store};
use crate::DbHandle;

use super::protocol::{
    read_message, send_message, DeviceStatsDto, Hello, Message, OpsBatch, PullRequest,
    SettingRecord, StatsExchange, SyncOp, OPS_PER_BATCH, PROTOCOL_VERSION, SNAPSHOT_ROW_CHUNK,
};
use super::store as sync_store;
use super::store::SyncPeer;

/// Where a snapshot send has gotten to. Held by the sender across batches.
#[derive(Debug, Default, Clone, Copy)]
pub struct SnapshotProgress {
    /// 0 = dictionary, 1 = snippets, 2 = contexts, 3 = transcriptions,
    /// 4 = api_calls, 5 = done.
    pub stage: u8,
    pub last_id: i64,
    /// Sequence namespace for synthesized snapshot stamps. Snapshot rows are
    /// not written to the local log, so this must survive across batches.
    pub origin_seq: Option<i64>,
}

/// Settings keys that sync between paired devices. Everything else in
/// settings.json is device-local by design - see docs/lan-sync.md for the
/// full classification. API keys never appear here (they are not even stored
/// in settings.json; they live in the OS credential store).
pub const SYNCABLE_SETTINGS: &[&str] = &[
    store::TRANSCRIPTION_PROVIDER,
    store::TRANSCRIPTION_LANGUAGE,
    store::CLEANUP_PROVIDER,
    store::TRANSCRIPTION_MODEL,
    store::CLEANUP_MODEL,
    store::TRANSCRIPTION_MODELS_BY_PROVIDER,
    store::CLEANUP_MODELS_BY_PROVIDER,
    store::TRANSCRIPTION_DEFAULT_MODEL,
    store::CLEANUP_DEFAULT_MODEL,
    store::TRANSCRIPTION_FALLBACK_MODELS,
    store::CLEANUP_FALLBACK_MODELS,
    store::DUAL_TRANSCRIPTION_ENABLED,
    store::CLEANUP_ENABLED,
    store::DEFAULT_TONE,
    store::CLEANUP_INTENSITY,
    store::APP_CONTEXT_HINT,
    store::AUTO_LEARN_ENABLED,
    store::AUTO_LEARN_EVENT_MODE,
    // CONTEXTUAL_CAPS and AUTO_SPACING are legacy mirrors of this canonical
    // key. They are written together when CONTEXTUAL_FORMATTING changes and
    // must not be independently LWW-merged.
    store::CONTEXTUAL_FORMATTING,
    store::CLEANUP_PROMPT_OVERRIDE,
    store::VERENU_SERVICE_CHECKS_ENABLED,
];

pub(crate) const UNRESOLVED_APP_PREFIX: &str = "?::";

/// The environment the engine needs beyond the database. The Tauri manager
/// implements it against the real app; tests use an in-memory stand-in so the
/// whole session flow is testable without a running app.
pub trait SyncHost: Send + Sync {
    fn device_uuid(&self) -> String;
    fn device_name(&self) -> String;
    fn app_version(&self) -> String;
    /// Values + stamps for all [`SYNCABLE_SETTINGS`], from settings.json.
    fn settings_payload(&self) -> Result<Vec<SettingRecord>>;
    /// Persist a remote setting value and run its local side effects.
    fn apply_remote_setting(&self, key: &str, value: &serde_json::Value) -> Result<(), String>;
    /// Maps a peer's platform-specific app identifier to this device. `None`
    /// preserves the assignment as unresolved instead of binding the wrong app.
    fn resolve_app_target(&self, source: &str) -> Option<String> {
        Some(source.to_string())
    }
}

pub fn unresolved_app_target(source: &str) -> String {
    format!("{UNRESOLVED_APP_PREFIX}{}", source.trim_start_matches(UNRESOLVED_APP_PREFIX))
}

fn resolve_context_targets_in_ops(host: &dyn SyncHost, ops: &[SyncOp]) -> Vec<SyncOp> {
    ops.iter()
        .cloned()
        .map(|mut op| {
            if op.table != "contexts" || op.op == "delete" {
                return op;
            }
            if let Some(targets) = op
                .payload
                .as_mut()
                .and_then(|payload| payload.get_mut("targets"))
                .and_then(serde_json::Value::as_array_mut)
            {
                for target in targets {
                    // Wire shape is either a bare string (a peer on an older
                    // build, or a legacy pre-platform-tag row) or a
                    // `{executable, platform}` object. Either way, rewrite
                    // `executable` to this device's own app identifier (or the
                    // `?::` unresolved marker) and stamp `platform` with this
                    // device's OS once resolved, since the rewritten string
                    // now follows this platform's naming convention.
                    let source = match target {
                        serde_json::Value::String(s) => Some(s.clone()),
                        serde_json::Value::Object(map) => {
                            map.get("executable").and_then(|v| v.as_str()).map(str::to_string)
                        }
                        _ => None,
                    };
                    let Some(source) = source else { continue };
                    let resolved = host
                        .resolve_app_target(&source)
                        .unwrap_or_else(|| unresolved_app_target(&source));
                    let platform = if resolved.starts_with(UNRESOLVED_APP_PREFIX) {
                        None
                    } else {
                        db::current_platform_tag()
                    };
                    *target = serde_json::json!({ "executable": resolved, "platform": platform });
                }
            }
            op
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Payload row types (the JSON shapes that travel inside SyncOp payloads)
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, Deserialize)]
pub struct DictionaryRow {
    pub term: String,
    #[serde(default)]
    pub mistake: Option<String>,
    #[serde(default)]
    pub auto_learned: bool,
    #[serde(default)]
    pub correction_count: i64,
    #[serde(default = "default_confidence_tier")]
    pub confidence_tier: String,
    #[serde(default)]
    pub last_seen_at: Option<String>,
    pub created_at: String,
}

fn default_confidence_tier() -> String {
    "low".to_string()
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SnippetRow {
    pub trigger: String,
    pub expansion: String,
    #[serde(default)]
    pub instructions: String,
    #[serde(default)]
    pub use_count: i64,
    pub created_at: String,
}

/// A synced exe target, carrying which OS assigned it. `#[serde(untagged)]`
/// so a peer still running a pre-platform-tagging build (bare string wire
/// format) deserializes fine, just with `platform: None`.
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(untagged)]
pub enum TargetEntry {
    Legacy(String),
    Tagged {
        executable: String,
        #[serde(default)]
        platform: Option<String>,
    },
}

impl TargetEntry {
    fn executable(&self) -> &str {
        match self {
            TargetEntry::Legacy(exe) => exe,
            TargetEntry::Tagged { executable, .. } => executable,
        }
    }

    fn platform(&self) -> Option<&str> {
        match self {
            TargetEntry::Legacy(_) => None,
            TargetEntry::Tagged { platform, .. } => platform.as_deref(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ContextAggregate {
    pub name: String,
    #[serde(default)]
    pub is_everywhere: bool,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub tone: Option<String>,
    #[serde(default)]
    pub cleanup_intensity: Option<String>,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub custom_instructions: Option<String>,
    #[serde(default)]
    pub contextual_formatting_disabled: bool,
    #[serde(default)]
    pub pinned_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    #[serde(default)]
    pub targets: Vec<TargetEntry>,
    #[serde(default)]
    pub websites: Vec<String>,
    #[serde(default)]
    pub dictionary_uuids: Vec<String>,
    #[serde(default)]
    pub snippet_uuids: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TranscriptionRow {
    pub raw_text: String,
    pub clean_text: String,
    #[serde(default)]
    pub words: i64,
    #[serde(default)]
    pub spoken_words: Option<i64>,
    #[serde(default)]
    pub duration_ms: i64,
    #[serde(default)]
    pub api_used: String,
    #[serde(default)]
    pub app_name: Option<String>,
    #[serde(default)]
    pub context_uuid: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ApiCallRow {
    #[serde(default)]
    pub transcription_uuid: Option<String>,
    pub model: String,
    pub provider: String,
    pub task: String,
    #[serde(default)]
    pub audio_ms: i64,
    #[serde(default)]
    pub input_chars: i64,
    #[serde(default)]
    pub output_chars: i64,
    pub created_at: String,
}

// ---------------------------------------------------------------------------
// Apply bookkeeping
// ---------------------------------------------------------------------------

/// What changed when a batch was applied, for frontend refresh events.
#[derive(Debug, Default, Clone)]
pub struct ApplySummary {
    pub dictionary: bool,
    pub snippets: bool,
    pub contexts: bool,
    pub history: bool,
    pub settings: bool,
    pub stats: bool,
    pub applied: usize,
    pub skipped: usize,
}

impl ApplySummary {
    pub fn touched_tables(&self) -> Vec<&'static str> {
        let mut tables = Vec::new();
        if self.dictionary {
            tables.push("dictionary");
        }
        if self.snippets {
            tables.push("snippets");
        }
        if self.contexts {
            tables.push("contexts");
        }
        if self.history {
            tables.push("history");
        }
        if self.settings {
            tables.push("settings");
        }
        if self.stats {
            tables.push("stats");
        }
        tables
    }
}

/// Holds the `sync_state.applying` flag for the duration of an apply, resetting
/// it even when an apply errors out. While the flag is set the change-capture
/// triggers stay silent; the engine logs applied ops itself with the remote's
/// original stamp so peers can dedup exactly (no echo amplification).
struct ApplyingGuard<'a>(&'a Connection);

impl<'a> ApplyingGuard<'a> {
    fn new(conn: &'a Connection) -> Result<Self> {
        sync_store::set_sync_applying(conn, true)?;
        Ok(Self(conn))
    }
}

impl Drop for ApplyingGuard<'_> {
    fn drop(&mut self) {
        if let Err(err) = sync_store::set_sync_applying(self.0, false) {
            log::error!("sync: failed to clear applying flag: {err}");
        }
    }
}

/// Logs an op that originated on THIS device (local mutation follow-ups like
/// anti-entropy tombstones and context cascades) with a fresh stamp.
pub fn append_self_log(
    conn: &Connection,
    table_name: &str,
    row_uuid: &str,
    op: &str,
) -> Result<()> {
    let Some(origin) = sync_store::self_uuid(conn)? else {
        return Ok(());
    };
    let origin_seq: i64 = conn.query_row(
        "SELECT COALESCE(MAX(origin_seq), 0) + 1 FROM sync_log WHERE origin = ?1",
        params![origin],
        |r| r.get(0),
    )?;
    let ts = sync_store::now_ms();
    sync_store::append_op(
        conn,
        &SyncOp {
            table: table_name.to_string(),
            row_uuid: row_uuid.to_string(),
            op: op.to_string(),
            ts_ms: ts,
            origin: origin.clone(),
            origin_seq,
            payload: None,
        },
        ts,
        &origin,
        origin_seq,
    )
}

fn latest_stamp(conn: &Connection, table: &str, row_uuid: &str) -> Result<Option<(i64, String, i64)>> {
    sync_store::latest_op_stamp(conn, table, row_uuid)
}

// ---------------------------------------------------------------------------
// Send side: resolving log entries and snapshots into wire ops
// ---------------------------------------------------------------------------

fn dictionary_payload(conn: &Connection, uuid: &str) -> Result<Option<serde_json::Value>> {
    let row = conn
        .query_row(
            "SELECT term, mistake, auto_learned, correction_count, confidence_tier, last_seen_at, created_at
             FROM dictionary WHERE uuid = ?1",
            params![uuid],
            |r| {
                Ok(DictionaryRow {
                    term: r.get(0)?,
                    mistake: r.get(1)?,
                    auto_learned: r.get::<_, i64>(2)? != 0,
                    correction_count: r.get(3)?,
                    confidence_tier: r.get(4)?,
                    last_seen_at: r.get(5)?,
                    created_at: r.get(6)?,
                })
            },
        )
        .optional()?;
    Ok(row.map(|row| serde_json::to_value(row).expect("serialize dictionary row")))
}

fn snippet_payload(conn: &Connection, uuid: &str) -> Result<Option<serde_json::Value>> {
    let row = conn
        .query_row(
            "SELECT trigger, expansion, instructions, use_count, created_at
             FROM snippets WHERE uuid = ?1",
            params![uuid],
            |r| {
                Ok(SnippetRow {
                    trigger: r.get(0)?,
                    expansion: r.get(1)?,
                    instructions: r.get(2)?,
                    use_count: r.get(3)?,
                    created_at: r.get(4)?,
                })
            },
        )
        .optional()?;
    Ok(row.map(|row| serde_json::to_value(row).expect("serialize snippet row")))
}

fn context_aggregate(conn: &Connection, uuid: &str) -> Result<Option<serde_json::Value>> {
    let Some(context_id) = conn
        .query_row("SELECT id FROM contexts WHERE uuid = ?1", params![uuid], |r| {
            r.get::<_, i64>(0)
        })
        .optional()?
    else {
        return Ok(None);
    };
    let row = conn
        .query_row(
            "SELECT name, is_everywhere, icon, tone, cleanup_intensity, color, custom_instructions,
                    contextual_formatting_disabled, pinned_at, created_at, updated_at
             FROM contexts WHERE id = ?1",
            params![context_id],
            |r| {
                Ok(ContextAggregate {
                    name: r.get(0)?,
                    is_everywhere: r.get::<_, i64>(1)? != 0,
                    icon: r.get(2)?,
                    tone: r.get(3)?,
                    cleanup_intensity: r.get(4)?,
                    color: r.get(5)?,
                    custom_instructions: r.get(6)?,
                    contextual_formatting_disabled: r.get::<_, i64>(7)? != 0,
                    pinned_at: r.get(8)?,
                    created_at: r.get(9)?,
                    updated_at: r.get(10)?,
                    targets: Vec::new(),
                    websites: Vec::new(),
                    dictionary_uuids: Vec::new(),
                    snippet_uuids: Vec::new(),
                })
            },
        )
        .optional()?;
    let mut aggregate = match row {
        Some(aggregate) => aggregate,
        None => return Ok(None),
    };
    let mut stmt = conn.prepare(
        "SELECT executable, platform FROM context_targets WHERE context_id = ?1 ORDER BY executable",
    )?;
    aggregate.targets = stmt
        .query_map(params![context_id], |r| {
            Ok(TargetEntry::Tagged {
                executable: r.get(0)?,
                platform: r.get(1)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut stmt = conn.prepare(
        "SELECT domain FROM context_website_targets WHERE context_id = ?1 ORDER BY domain",
    )?;
    aggregate.websites = stmt
        .query_map(params![context_id], |r| r.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut stmt = conn.prepare(
        "SELECT d.uuid FROM dictionary_contexts dc JOIN dictionary d ON d.id = dc.dictionary_id
         WHERE dc.context_id = ?1 ORDER BY d.id",
    )?;
    aggregate.dictionary_uuids = stmt
        .query_map(params![context_id], |r| r.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut stmt = conn.prepare(
        "SELECT s.uuid FROM snippet_contexts sc JOIN snippets s ON s.id = sc.snippet_id
         WHERE sc.context_id = ?1 ORDER BY s.id",
    )?;
    aggregate.snippet_uuids = stmt
        .query_map(params![context_id], |r| r.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(Some(
        serde_json::to_value(aggregate).expect("serialize context aggregate"),
    ))
}

fn transcription_payload(conn: &Connection, uuid: &str) -> Result<Option<serde_json::Value>> {
    let row = conn
        .query_row(
            "SELECT t.raw_text, t.clean_text, t.words, t.spoken_words, t.duration_ms, t.api_used,
                    t.app_name, c.uuid, t.created_at
             FROM transcriptions t LEFT JOIN contexts c ON c.id = t.context_id
             WHERE t.uuid = ?1",
            params![uuid],
            |r| {
                Ok(TranscriptionRow {
                    raw_text: r.get(0)?,
                    clean_text: r.get(1)?,
                    words: r.get(2)?,
                    spoken_words: r.get(3)?,
                    duration_ms: r.get(4)?,
                    api_used: r.get(5)?,
                    app_name: r.get(6)?,
                    context_uuid: r.get(7)?,
                    created_at: r.get(8)?,
                })
            },
        )
        .optional()?;
    Ok(row.map(|row| serde_json::to_value(row).expect("serialize transcription row")))
}

fn api_call_payload(conn: &Connection, uuid: &str) -> Result<Option<serde_json::Value>> {
    let row = conn
        .query_row(
            "SELECT t.uuid, a.model, a.provider, a.task, a.audio_ms, a.input_chars, a.output_chars, a.created_at
             FROM api_calls a LEFT JOIN transcriptions t ON t.id = a.transcription_id
             WHERE a.uuid = ?1",
            params![uuid],
            |r| {
                Ok(ApiCallRow {
                    transcription_uuid: r.get(0)?,
                    model: r.get(1)?,
                    provider: r.get(2)?,
                    task: r.get(3)?,
                    audio_ms: r.get(4)?,
                    input_chars: r.get(5)?,
                    output_chars: r.get(6)?,
                    created_at: r.get(7)?,
                })
            },
        )
        .optional()?;
    Ok(row.map(|row| serde_json::to_value(row).expect("serialize api call row")))
}

/// Resolves a collapsed log entry into a full wire op. History is append-only
/// for sync purposes: retention pruning is device-local, so old transcription
/// delete tombstones from pre-v22 databases must never be sent to peers.
fn resolve_entry(conn: &Connection, entry: &sync_store::LogEntry) -> Result<Option<SyncOp>> {
    if entry.table_name == "transcriptions" && entry.op == "delete" {
        return Ok(None);
    }
    let payload = match entry.table_name.as_str() {
        "dictionary" if entry.op == "upsert" => dictionary_payload(conn, &entry.row_uuid)?,
        "snippets" if entry.op == "upsert" => snippet_payload(conn, &entry.row_uuid)?,
        "contexts" if entry.op == "upsert" => context_aggregate(conn, &entry.row_uuid)?,
        "transcriptions" if entry.op == "upsert" => transcription_payload(conn, &entry.row_uuid)?,
        "api_calls" if entry.op == "upsert" => api_call_payload(conn, &entry.row_uuid)?,
        _ => None,
    };
    let op = if entry.op == "upsert" && payload.is_none() {
        "delete"
    } else {
        &entry.op
    };
    Ok(Some(SyncOp {
        table: entry.table_name.clone(),
        row_uuid: entry.row_uuid.clone(),
        op: op.to_string(),
        ts_ms: entry.ts_ms,
        origin: entry.origin.clone(),
        origin_seq: entry.origin_seq,
        payload,
    }))
}

/// Collects the next batch of ops to send. `snapshot` sends the full current
/// state (new pairing or a peer whose cursor fell out of our retained log);
/// otherwise it sends the collapsed changes after `since_seq`. Returns the ops
/// plus the cursor to report: 0 while more batches follow, else the final log
/// position.
pub fn collect_ops(
    conn: &Connection,
    since_seq: i64,
    snapshot: bool,
    limit: usize,
    progress: &mut SnapshotProgress,
) -> Result<(Vec<SyncOp>, i64, bool)> {
    let limit = limit.max(1) as i64;
    if snapshot {
        // Full state for a new/rejoining peer. Every table is keyset-paginated
        // so the wire batch and sender memory stay bounded on big libraries.
        let mut ops = Vec::new();
        let now = sync_store::now_ms();
        let origin = sync_store::self_uuid(conn)?.unwrap_or_default();
        let mut origin_seq = match progress.origin_seq {
            Some(seq) => seq,
            None => {
                let seq: i64 = conn.query_row(
                    "SELECT COALESCE(MAX(origin_seq), 0) FROM sync_log WHERE origin = ?1",
                    params![origin],
                    |r| r.get(0),
                )?;
                progress.origin_seq = Some(seq);
                seq
            }
        };
        let mut push = |table: &str,
                        uuid: String,
                        payload: Option<serde_json::Value>,
                        stamp: Option<(i64, String, i64)>,
                        ops: &mut Vec<SyncOp>| {
            let (ts_ms, op_origin, op_seq) = stamp.unwrap_or_else(|| {
                origin_seq += 1;
                (now, origin.clone(), origin_seq)
            });
            if op_origin == origin {
                origin_seq = origin_seq.max(op_seq);
            }
            ops.push(SyncOp {
                table: table.to_string(),
                row_uuid: uuid,
                op: "upsert".to_string(),
                ts_ms,
                origin: op_origin,
                origin_seq: op_seq,
                payload,
            });
        };

        while (ops.len() as i64) < limit && progress.stage <= 4 {
            let capacity = limit - ops.len() as i64;
            let chunk = capacity.min(SNAPSHOT_ROW_CHUNK);
            let stage = progress.stage;
            let table = match stage {
                0 => "dictionary",
                1 => "snippets",
                2 => "contexts",
                3 => "transcriptions",
                4 => "api_calls",
                _ => unreachable!("snapshot stage is complete"),
            };
            let mut stmt = conn.prepare(&format!(
                "SELECT id, uuid FROM {table} WHERE id > ?1 ORDER BY id LIMIT ?2"
            ))?;
            let rows = stmt
                .query_map(params![progress.last_id, chunk], |r| {
                    Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            let fetched = rows.len() as i64;
            for (id, uuid) in rows {
                progress.last_id = id;
                let payload = match table {
                    "dictionary" => dictionary_payload(conn, &uuid)?,
                    "snippets" => snippet_payload(conn, &uuid)?,
                    "contexts" => context_aggregate(conn, &uuid)?,
                    "transcriptions" => transcription_payload(conn, &uuid)?,
                    "api_calls" => api_call_payload(conn, &uuid)?,
                    _ => unreachable!("snapshot table is complete"),
                };
                let stamp = latest_stamp(conn, table, &uuid)?;
                push(table, uuid, payload, stamp, &mut ops);
            }
            if fetched < chunk {
                // This table is exhausted; move to the next stage.
                progress.stage += 1;
                progress.last_id = 0;
                if progress.stage > 4 {
                    progress.origin_seq = Some(origin_seq);
                    let cursor = sync_store::max_log_seq(conn)?;
                    return Ok((ops, cursor, true));
                }
            } else {
                // Batch full; more of this table remains.
                progress.origin_seq = Some(origin_seq);
                return Ok((ops, 0, false));
            }
        }
        if progress.stage <= 4 {
            // Capacity exhausted mid-stream.
            progress.origin_seq = Some(origin_seq);
            return Ok((ops, 0, false));
        }
        progress.origin_seq = Some(origin_seq);
        let cursor = sync_store::max_log_seq(conn)?;
        return Ok((ops, cursor, true));
    }

    let entries = sync_store::changes_since(conn, since_seq, limit)?;
    let mut ops = Vec::with_capacity(entries.len());
    for entry in &entries {
        if let Some(op) = resolve_entry(conn, entry)? {
            ops.push(op);
        }
    }
    let done = entries.len() < limit as usize;
    let cursor = if done {
        sync_store::max_log_seq(conn)?
    } else {
        entries
            .last()
            .map(|entry| entry.seq)
            .unwrap_or(since_seq)
    };
    Ok((ops, cursor, done))
}

// ---------------------------------------------------------------------------
// Apply side
// ---------------------------------------------------------------------------

/// Result of one incoming op.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Applied {
    Yes,
    Skipped,
}

/// Applies a batch of remote ops inside one applying-guard window. Idempotent:
/// re-applying an already-known op is a no-op, so retries and duplicate
/// deliveries never create duplicate rows.
pub fn apply_ops(conn: &Connection, ops: &[SyncOp]) -> Result<ApplySummary> {
    let _guard = ApplyingGuard::new(conn)?;
    let mut summary = ApplySummary::default();
    // Apply in dependency order: content first, then contexts (which reference
    // content uuids), then history (which references contexts).
    for op in ops {
        match op.table.as_str() {
            "dictionary" => {
                if apply_dictionary_op(conn, op)? == Applied::Yes {
                    summary.dictionary = true;
                    summary.applied += 1;
                } else {
                    summary.skipped += 1;
                }
            }
            "snippets" => {
                if apply_snippet_op(conn, op)? == Applied::Yes {
                    summary.snippets = true;
                    summary.applied += 1;
                } else {
                    summary.skipped += 1;
                }
            }
            "contexts" => {
                if apply_context_op(conn, op)? == Applied::Yes {
                    summary.contexts = true;
                    summary.applied += 1;
                } else {
                    summary.skipped += 1;
                }
            }
            "transcriptions" => {
                if apply_transcription_op(conn, op)? == Applied::Yes {
                    summary.history = true;
                    summary.applied += 1;
                } else {
                    summary.skipped += 1;
                }
            }
            "api_calls" => {
                if apply_api_call_op(conn, op)? == Applied::Yes {
                    summary.history = true;
                    summary.applied += 1;
                } else {
                    summary.skipped += 1;
                }
            }
            other => {
                log::warn!("sync: ignoring op for unknown table {other:?}");
                summary.skipped += 1;
            }
        }
    }
    Ok(summary)
}

/// Logs a successfully applied remote op with its original stamp, so peers
/// that pull from us see the same op with the same identity (dedup works) and
/// our own LWW comparisons stay consistent.
fn log_applied(conn: &Connection, op: &SyncOp) -> Result<()> {
    sync_store::append_op(conn, op, op.ts_ms, &op.origin, op.origin_seq)
}

/// When a local row wins a natural-key collision, the remote's losing row must
/// eventually vanish everywhere. Log an anti-entropy tombstone for it.
fn log_anti_entropy_delete(conn: &Connection, table: &str, row_uuid: &str) -> Result<()> {
    append_self_log(conn, table, row_uuid, "delete")
}

/// Contexts whose junction rows referenced a dictionary/snippet row that is
/// about to be hard-deleted. Their aggregates change, so they are re-logged
/// (the cascade delete of the junction rows is trigger-suppressed here).
fn log_contexts_referencing_dictionary(conn: &Connection, dictionary_uuid: &str) -> Result<()> {
    let ids: Vec<i64> = {
        let mut stmt = conn.prepare(
            "SELECT dc.context_id FROM dictionary_contexts dc
             JOIN dictionary d ON d.id = dc.dictionary_id WHERE d.uuid = ?1",
        )?;
        let collected = stmt
            .query_map(params![dictionary_uuid], |r| r.get(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        collected
    };
    log_context_upserts(conn, &ids)
}

fn log_contexts_referencing_snippet(conn: &Connection, snippet_uuid: &str) -> Result<()> {
    let ids: Vec<i64> = {
        let mut stmt = conn.prepare(
            "SELECT sc.context_id FROM snippet_contexts sc
             JOIN snippets s ON s.id = sc.snippet_id WHERE s.uuid = ?1",
        )?;
        let collected = stmt
            .query_map(params![snippet_uuid], |r| r.get(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        collected
    };
    log_context_upserts(conn, &ids)
}

fn log_context_upserts(conn: &Connection, context_ids: &[i64]) -> Result<()> {
    for context_id in context_ids {
        let uuid: Option<String> = conn
            .query_row(
                "SELECT uuid FROM contexts WHERE id = ?1",
                params![context_id],
                |r| r.get(0),
            )
            .optional()?;
        if let Some(uuid) = uuid {
            append_self_log(conn, "contexts", &uuid, "upsert")?;
        }
    }
    Ok(())
}

fn apply_dictionary_op(conn: &Connection, op: &SyncOp) -> Result<Applied> {
    if op.is_delete() {
        return apply_simple_delete(conn, op, "dictionary", "dictionary");
    }
    if let Some(stamp) = latest_stamp(conn, "dictionary", &op.row_uuid)? {
        if !op.newer_than(&stamp) {
            return Ok(Applied::Skipped);
        }
    }
    let row: DictionaryRow = serde_json::from_value(
        op.payload
            .clone()
            .ok_or_else(|| anyhow!("dictionary upsert missing payload"))?,
    )
    .context("invalid dictionary payload")?;

    let insert = |conn: &Connection| -> rusqlite::Result<usize> {
        conn.execute(
            "INSERT INTO dictionary (uuid, term, mistake, auto_learned, correction_count, confidence_tier, last_seen_at, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(uuid) DO UPDATE SET
               term = excluded.term, mistake = excluded.mistake,
               auto_learned = excluded.auto_learned,
               correction_count = excluded.correction_count,
               confidence_tier = excluded.confidence_tier,
               last_seen_at = excluded.last_seen_at",
            params![
                op.row_uuid,
                row.term,
                row.mistake,
                row.auto_learned as i64,
                row.correction_count,
                row.confidence_tier,
                row.last_seen_at,
                row.created_at
            ],
        )
    };
    apply_with_natural_key_resolution(
        conn,
        op,
        "dictionary",
        insert,
        &|conn| conflicting_uuid(conn, "dictionary", "term", &row.term, &op.row_uuid),
    )
}

fn apply_snippet_op(conn: &Connection, op: &SyncOp) -> Result<Applied> {
    if op.is_delete() {
        return apply_simple_delete(conn, op, "snippets", "snippets");
    }
    if let Some(stamp) = latest_stamp(conn, "snippets", &op.row_uuid)? {
        if !op.newer_than(&stamp) {
            return Ok(Applied::Skipped);
        }
    }
    let row: SnippetRow = serde_json::from_value(
        op.payload
            .clone()
            .ok_or_else(|| anyhow!("snippet upsert missing payload"))?,
    )
    .context("invalid snippet payload")?;

    let insert = |conn: &Connection| -> rusqlite::Result<usize> {
        conn.execute(
            "INSERT INTO snippets (uuid, trigger, expansion, instructions, use_count, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(uuid) DO UPDATE SET
               trigger = excluded.trigger, expansion = excluded.expansion,
               instructions = excluded.instructions, use_count = excluded.use_count",
            params![
                op.row_uuid,
                row.trigger,
                row.expansion,
                row.instructions,
                row.use_count,
                row.created_at
            ],
        )
    };
    apply_with_natural_key_resolution(
        conn,
        op,
        "snippets",
        insert,
        &|conn| conflicting_uuid(conn, "snippets", "trigger", &row.trigger, &op.row_uuid),
    )
}

/// Shared upsert flow with natural-key collision resolution. `insert` writes
/// the row (upsert by uuid); `find_conflict` returns the uuid of a DIFFERENT
/// local row holding the same natural key. Context membership cascades are
/// logged before deleting dictionary/snippet losers.
fn apply_with_natural_key_resolution(
    conn: &Connection,
    op: &SyncOp,
    table: &str,
    insert: impl Fn(&Connection) -> rusqlite::Result<usize>,
    find_conflict: &dyn Fn(&Connection) -> Result<Option<String>>,
) -> Result<Applied> {
    match insert(conn) {
        Ok(_) => {
            log_applied(conn, op)?;
            Ok(Applied::Yes)
        }
        Err(rusqlite::Error::SqliteFailure(err, _))
            if err.code == rusqlite::ErrorCode::ConstraintViolation =>
        {
            // Another local row owns the same natural key. Deterministic
            // winner: the higher stamp.
            let Some(conflict_uuid) = find_conflict(conn)? else {
                return Err(anyhow!(
                    "sync: {table} constraint failed but no conflicting row found for {}",
                    op.row_uuid
                ));
            };
            let conflict_stamp = latest_stamp(conn, table, &conflict_uuid)?
                .unwrap_or((0, String::new(), 0));
            if op.newer_than(&conflict_stamp) {
                // Remote row wins: remove the local loser (with its cascades)
                // and retry the insert.
                if table == "dictionary" {
                    log_contexts_referencing_dictionary(conn, &conflict_uuid)?;
                } else if table == "snippets" {
                    log_contexts_referencing_snippet(conn, &conflict_uuid)?;
                }
                conn.execute(
                    &format!("DELETE FROM {table} WHERE uuid = ?1"),
                    params![conflict_uuid],
                )?;
                insert(conn).map_err(|e| anyhow!("sync: retry insert failed: {e}"))?;
                log_applied(conn, op)?;
                Ok(Applied::Yes)
            } else {
                // Local row wins: tell the peer (eventually) to drop its loser.
                log_anti_entropy_delete(conn, table, &op.row_uuid)?;
                Ok(Applied::Skipped)
            }
        }
        Err(err) => Err(anyhow!("sync: {table} upsert failed: {err}")),
    }
}

/// Finds a row with the same natural key but a different uuid, if any.
fn conflicting_uuid(
    conn: &Connection,
    table: &str,
    column: &str,
    value: &str,
    exclude_uuid: &str,
) -> Result<Option<String>> {
    let collate = if table == "contexts" { "COLLATE NOCASE" } else { "" };
    let context_scope = if table == "contexts" {
        " AND is_everywhere = 0"
    } else {
        ""
    };
    let uuid: Option<String> = conn
        .query_row(
            &format!(
                "SELECT uuid FROM {table} WHERE {column} = ?1 {collate}{context_scope} AND uuid != ?2 LIMIT 1"
            ),
            params![value, exclude_uuid],
            |r| r.get(0),
        )
        .optional()?;
    Ok(uuid)
}

fn apply_simple_delete(
    conn: &Connection,
    op: &SyncOp,
    table: &str,
    log_table: &str,
) -> Result<Applied> {
    if let Some(stamp) = latest_stamp(conn, log_table, &op.row_uuid)? {
        if !op.newer_than(&stamp) {
            return Ok(Applied::Skipped);
        }
    }
    // Capture cascades before the delete (the trigger-suppressed cascade would
    // otherwise never be logged).
    if table == "dictionary" {
        log_contexts_referencing_dictionary(conn, &op.row_uuid)?;
    } else if table == "snippets" {
        log_contexts_referencing_snippet(conn, &op.row_uuid)?;
    }
    let deleted = conn.execute(
        &format!("DELETE FROM {table} WHERE uuid = ?1"),
        params![op.row_uuid],
    )?;
    log_applied(conn, op)?;
    if deleted == 0 {
        log::debug!("sync: delete for absent {} row {}", table, op.row_uuid);
    }
    Ok(Applied::Yes)
}

fn apply_context_op(conn: &Connection, op: &SyncOp) -> Result<Applied> {
    if op.is_delete() {
        return apply_context_delete(conn, op);
    }
    if let Some(stamp) = latest_stamp(conn, "contexts", &op.row_uuid)? {
        if !op.newer_than(&stamp) {
            return Ok(Applied::Skipped);
        }
    }
    let aggregate: ContextAggregate = serde_json::from_value(
        op.payload
            .clone()
            .ok_or_else(|| anyhow!("context upsert missing payload"))?,
    )
    .context("invalid context payload")?;

    if aggregate.is_everywhere {
        apply_everywhere_aggregate(conn, &aggregate)?;
        log_applied(conn, op)?;
        return Ok(Applied::Yes);
    }

    let insert = |conn: &Connection| -> rusqlite::Result<usize> {
        conn.execute(
            "INSERT INTO contexts (uuid, name, is_everywhere, icon, tone, cleanup_intensity, color,
                                   custom_instructions, contextual_formatting_disabled, pinned_at,
                                   created_at, updated_at)
             VALUES (?1, ?2, 0, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
             ON CONFLICT(uuid) DO UPDATE SET
               name = excluded.name, icon = excluded.icon, tone = excluded.tone,
               cleanup_intensity = excluded.cleanup_intensity, color = excluded.color,
               custom_instructions = excluded.custom_instructions,
               contextual_formatting_disabled = excluded.contextual_formatting_disabled,
               pinned_at = excluded.pinned_at, updated_at = excluded.updated_at",
            params![
                op.row_uuid,
                aggregate.name,
                aggregate.icon,
                aggregate.tone,
                aggregate.cleanup_intensity,
                aggregate.color,
                aggregate.custom_instructions,
                aggregate.contextual_formatting_disabled as i64,
                aggregate.pinned_at,
                aggregate.created_at,
                aggregate.updated_at
            ],
        )
    };
    // Name conflicts resolve like any natural key, EXCEPT the loser is a
    // context: deleting it must reuse the app's delete semantics (junctions
    // move to Everywhere), not a bare DELETE.
    match insert(conn) {
        Ok(_) => {}
        Err(rusqlite::Error::SqliteFailure(err, _))
            if err.code == rusqlite::ErrorCode::ConstraintViolation =>
        {
            let Some(conflict_uuid) =
                conflicting_uuid(conn, "contexts", "name", &aggregate.name, &op.row_uuid)?
            else {
                return Err(anyhow!(
                    "sync: context name conflict without conflicting row ({})",
                    aggregate.name
                ));
            };
            let conflict_stamp =
                latest_stamp(conn, "contexts", &conflict_uuid)?.unwrap_or((0, String::new(), 0));
            if op.newer_than(&conflict_stamp) {
                delete_context_by_uuid(conn, &conflict_uuid)?;
                insert(conn).map_err(|e| anyhow!("sync: context retry insert failed: {e}"))?;
            } else {
                log_anti_entropy_delete(conn, "contexts", &op.row_uuid)?;
                return Ok(Applied::Skipped);
            }
        }
        Err(err) => return Err(anyhow!("sync: context upsert failed: {err}")),
    }

    let context_id: i64 = conn.query_row(
        "SELECT id FROM contexts WHERE uuid = ?1",
        params![op.row_uuid],
        |r| r.get(0),
    )?;
    reconcile_context_children(conn, context_id, &aggregate)?;
    log_applied(conn, op)?;
    Ok(Applied::Yes)
}

/// Applies the Everywhere aggregate: style/name edits sync, the row itself is
/// never created or deleted, and targets are impossible by construction.
fn apply_everywhere_aggregate(conn: &Connection, aggregate: &ContextAggregate) -> Result<()> {
    let everywhere_id = db::ensure_everywhere_context_conn(conn)?;
    // A rename can still collide with a user context's name; keep the local
    // name on collision rather than deleting a user context over it.
    let rename = conn.execute(
        "UPDATE contexts SET name = ?1, updated_at = ?2 WHERE id = ?3",
        params![aggregate.name, aggregate.updated_at, everywhere_id],
    );
    if let Err(err) = rename {
        log::warn!("sync: skipping Everywhere rename: {err}");
    }
    conn.execute(
        "UPDATE contexts SET icon = ?1, tone = ?2, cleanup_intensity = ?3, color = ?4,
                custom_instructions = ?5, contextual_formatting_disabled = ?6, pinned_at = ?7
         WHERE id = ?8",
        params![
            aggregate.icon,
            aggregate.tone,
            aggregate.cleanup_intensity,
            aggregate.color,
            aggregate.custom_instructions,
            aggregate.contextual_formatting_disabled as i64,
            aggregate.pinned_at,
            everywhere_id
        ],
    )?;
    reconcile_context_members(conn, everywhere_id, aggregate)?;
    Ok(())
}

fn delete_context_by_uuid(conn: &Connection, uuid: &str) -> Result<()> {
    let Some(context_id) = conn
        .query_row("SELECT id FROM contexts WHERE uuid = ?1", params![uuid], |r| {
            r.get::<_, i64>(0)
        })
        .optional()?
    else {
        return Ok(());
    };
    // Reuse the app's delete semantics: junction rows move to Everywhere so
    // vocabulary is never orphaned. Trigger-suppressed; the delete op itself
    // is logged by the caller.
    db::delete_context_conn(conn, context_id)?;
    Ok(())
}

fn apply_context_delete(conn: &Connection, op: &SyncOp) -> Result<Applied> {
    if let Some(stamp) = latest_stamp(conn, "contexts", &op.row_uuid)? {
        if !op.newer_than(&stamp) {
            return Ok(Applied::Skipped);
        }
    }
    delete_context_by_uuid(conn, &op.row_uuid)?;
    log_applied(conn, op)?;
    Ok(Applied::Yes)
}

fn reconcile_context_children(
    conn: &Connection,
    context_id: i64,
    aggregate: &ContextAggregate,
) -> Result<()> {
    // Exe targets: single-owner by design; assign moves them, extras are
    // removed — EXCEPT a row this device already resolved for its own
    // platform (a real installed app, not the "?::" unresolved marker) is
    // sticky: `resolve_context_targets_in_ops` reruns app matching on every
    // incoming op and always stamps the result with this device's own
    // platform tag, so without this guard a stale/failed re-match on a later
    // sync would silently delete a target the user (or an earlier successful
    // match) already pinned correctly on this device. A genuinely
    // cross-device removal only reaches this device through its own local
    // `remove_context_target` call, never through this reconcile path, so
    // protecting sticky rows here never blocks a real local delete.
    let my_platform = db::current_platform_tag();
    let sticky_executables: std::collections::HashSet<String> = conn
        .prepare(
            "SELECT executable FROM context_targets
             WHERE context_id = ?1 AND platform IS ?2 AND executable NOT LIKE '?::%'",
        )?
        .query_map(params![context_id, my_platform], |r| r.get::<_, String>(0))?
        .map(|result| result.map(|executable| executable.trim().to_lowercase()))
        .collect::<rusqlite::Result<_>>()?;
    for entry in &aggregate.targets {
        let normalized = entry.executable().trim().to_lowercase();
        if normalized.is_empty() || sticky_executables.contains(&normalized) {
            continue;
        }
        conn.execute(
            "INSERT INTO context_targets (context_id, executable, platform) VALUES (?1, ?2, ?3)
             ON CONFLICT(executable) DO UPDATE SET context_id = excluded.context_id, platform = excluded.platform",
            params![context_id, normalized, entry.platform()],
        )?;
    }
    let target_executables: Vec<String> = aggregate
        .targets
        .iter()
        .map(|entry| entry.executable().trim().to_lowercase())
        .filter(|executable| !executable.is_empty())
        .chain(sticky_executables.iter().cloned())
        .collect();
    remove_missing(
        conn,
        "context_targets",
        "executable",
        context_id,
        &target_executables,
    )?;
    let normalized_websites = aggregate
        .websites
        .iter()
        .map(|domain| normalize_domain(domain))
        .collect::<Vec<_>>();
    for normalized in &normalized_websites {
        if normalized.is_empty() {
            continue;
        }
        conn.execute(
            "INSERT INTO context_website_targets (context_id, domain) VALUES (?1, ?2)
             ON CONFLICT(domain) DO UPDATE SET context_id = excluded.context_id",
            params![context_id, normalized],
        )?;
    }
    remove_missing(
        conn,
        "context_website_targets",
        "domain",
        context_id,
        &normalized_websites,
    )?;
    reconcile_context_members(conn, context_id, aggregate)?;
    Ok(())
}

/// Junction membership reconcile: add everything in the payload that resolves
/// locally, remove everything currently present that the payload lacks.
fn reconcile_context_members(
    conn: &Connection,
    context_id: i64,
    aggregate: &ContextAggregate,
) -> Result<()> {
    let mut resolved_dictionary: Vec<i64> = Vec::with_capacity(aggregate.dictionary_uuids.len());
    {
        let mut stmt = conn.prepare("SELECT id FROM dictionary WHERE uuid = ?1")?;
        for uuid in &aggregate.dictionary_uuids {
            if let Some(id) = stmt
                .query_row(params![uuid], |r| r.get::<_, i64>(0))
                .optional()?
            {
                resolved_dictionary.push(id);
            }
        }
    }
    for dictionary_id in &resolved_dictionary {
        conn.execute(
            "INSERT OR IGNORE INTO dictionary_contexts (context_id, dictionary_id) VALUES (?1, ?2)",
            params![context_id, dictionary_id],
        )?;
    }
    delete_members_not_in(
        conn,
        "dictionary_contexts",
        "dictionary_id",
        context_id,
        &resolved_dictionary,
    )?;

    let mut resolved_snippets: Vec<i64> = Vec::with_capacity(aggregate.snippet_uuids.len());
    {
        let mut stmt = conn.prepare("SELECT id FROM snippets WHERE uuid = ?1")?;
        for uuid in &aggregate.snippet_uuids {
            if let Some(id) = stmt
                .query_row(params![uuid], |r| r.get::<_, i64>(0))
                .optional()?
            {
                resolved_snippets.push(id);
            }
        }
    }
    for snippet_id in &resolved_snippets {
        conn.execute(
            "INSERT OR IGNORE INTO snippet_contexts (context_id, snippet_id) VALUES (?1, ?2)",
            params![context_id, snippet_id],
        )?;
    }
    delete_members_not_in(
        conn,
        "snippet_contexts",
        "snippet_id",
        context_id,
        &resolved_snippets,
    )?;
    Ok(())
}

fn delete_members_not_in(
    conn: &Connection,
    table: &str,
    column: &str,
    context_id: i64,
    keep_ids: &[i64],
) -> Result<()> {
    // json_each keeps a dynamic NOT IN list to a single bound parameter.
    let keep_json = serde_json::to_string(keep_ids)?;
    let n = conn.execute(
        &format!(
            "DELETE FROM {table}
             WHERE context_id = ?1
               AND {column} NOT IN (SELECT value FROM json_each(?2))"
        ),
        params![context_id, keep_json],
    )?;
    if n > 0 {
        log::debug!("sync: pruned {n} stale rows from {table} for context {context_id}");
    }
    Ok(())
}

fn remove_missing(
    conn: &Connection,
    table: &str,
    column: &str,
    context_id: i64,
    keep_values: &[String],
) -> Result<()> {
    let keep_json = serde_json::to_string(
        &keep_values
            .iter()
            .map(|s| s.trim().to_lowercase())
            .collect::<Vec<_>>(),
    )?;
    let n = conn.execute(
        &format!(
            "DELETE FROM {table}
             WHERE context_id = ?1
               AND {column} NOT IN (SELECT value FROM json_each(?2))"
        ),
        params![context_id, keep_json],
    )?;
    if n > 0 {
        log::debug!("sync: pruned {n} stale rows from {table} for context {context_id}");
    }
    Ok(())
}

fn normalize_domain(domain: &str) -> String {
    let trimmed = domain.trim().to_lowercase();
    trimmed
        .trim_start_matches("http://")
        .trim_start_matches("https://")
        .trim_start_matches("www.")
        .split('/')
        .next()
        .unwrap_or("")
        .to_string()
}

fn apply_transcription_op(conn: &Connection, op: &SyncOp) -> Result<Applied> {
    if op.is_delete() {
        // Transcription deletion is retention cleanup, and retention is
        // intentionally device-local. A peer must never erase a row merely
        // because another device has a shorter local retention window.
        return Ok(Applied::Skipped);
    }
    if let Some(stamp) = latest_stamp(conn, "transcriptions", &op.row_uuid)? {
        if !op.newer_than(&stamp) {
            return Ok(Applied::Skipped);
        }
    }
    let row: TranscriptionRow = serde_json::from_value(
        op.payload
            .clone()
            .ok_or_else(|| anyhow!("transcription upsert missing payload"))?,
    )
    .context("invalid transcription payload")?;
    let context_id: Option<i64> = match &row.context_uuid {
        Some(context_uuid) => conn
            .query_row(
                "SELECT id FROM contexts WHERE uuid = ?1",
                params![context_uuid],
                |r| r.get(0),
            )
            .optional()?,
        None => None,
    };
    // Raw insert on purpose: lifetime counters must NOT be bumped here - the
    // dictation was already counted by the device it happened on, and its
    // counters arrive through the stats exchange.
    let inserted = conn.execute(
        "INSERT OR IGNORE INTO transcriptions
           (uuid, raw_text, clean_text, words, spoken_words, duration_ms, api_used, app_name, context_id, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        params![
            op.row_uuid,
            row.raw_text,
            row.clean_text,
            row.words,
            row.spoken_words,
            row.duration_ms,
            row.api_used,
            row.app_name,
            context_id,
            row.created_at
        ],
    )?;
    if inserted > 0 {
        log_applied(conn, op)?;
        Ok(Applied::Yes)
    } else {
        // Row already existed with an older stamp (e.g. pre-uuid history).
        // Don't log: our newer local version stays authoritative.
        Ok(Applied::Skipped)
    }
}

fn apply_api_call_op(conn: &Connection, op: &SyncOp) -> Result<Applied> {
    if op.is_delete() {
        return apply_simple_delete(conn, op, "api_calls", "api_calls");
    }
    if let Some(stamp) = latest_stamp(conn, "api_calls", &op.row_uuid)? {
        if !op.newer_than(&stamp) {
            return Ok(Applied::Skipped);
        }
    }
    let row: ApiCallRow = serde_json::from_value(
        op.payload
            .clone()
            .ok_or_else(|| anyhow!("api_call upsert missing payload"))?,
    )
    .context("invalid api call payload")?;
    let Some(transcription_uuid) = row.transcription_uuid.as_deref() else {
        log::warn!("sync: skipping api call {} without a transcription", op.row_uuid);
        return Ok(Applied::Skipped);
    };
    let Some(transcription_id) = conn
        .query_row(
            "SELECT id FROM transcriptions WHERE uuid = ?1",
            params![transcription_uuid],
            |r| r.get::<_, i64>(0),
        )
        .optional()?
    else {
        log::warn!(
            "sync: skipping api call {} with missing transcription {}",
            op.row_uuid,
            transcription_uuid
        );
        return Ok(Applied::Skipped);
    };
    let inserted = conn.execute(
        "INSERT OR IGNORE INTO api_calls
           (uuid, transcription_id, model, provider, task, audio_ms, input_chars, output_chars, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            op.row_uuid,
            transcription_id,
            row.model,
            row.provider,
            row.task,
            row.audio_ms,
            row.input_chars,
            row.output_chars,
            row.created_at
        ],
    )?;
    if inserted > 0 {
        log_applied(conn, op)?;
        Ok(Applied::Yes)
    } else {
        Ok(Applied::Skipped)
    }
}

// ---------------------------------------------------------------------------
// Meta exchange (stats + settings)
// ---------------------------------------------------------------------------

pub fn build_stats_exchange(conn: &Connection, self_uuid: &str) -> Result<StatsExchange> {
    let (total_words, dictionary_fixes) = conn
        .query_row(
            "SELECT COALESCE(total_words, 0), COALESCE(dictionary_fixes, 0)
             FROM lifetime_stats WHERE id = 1",
            [],
            |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)),
        )
        .optional()?
        .unwrap_or((0, 0));
    let remote_stats = sync_store::list_remote_stats(conn)?
        .into_iter()
        .map(|s| DeviceStatsDto {
            device_id: s.device_id,
            total_words: s.total_words,
            dictionary_fixes: s.dictionary_fixes,
        })
        .collect();
    Ok(StatsExchange {
        self_stats: DeviceStatsDto {
            device_id: self_uuid.to_string(),
            total_words,
            dictionary_fixes,
        },
        remote_stats,
    })
}

/// Applies the peer's stats exchange: their own counters and everything they
/// know about third devices. Rows for ourselves are ignored (our own
/// `lifetime_stats` row is authoritative for this device).
pub fn apply_stats_exchange(conn: &Connection, stats: &StatsExchange, peer_uuid: &str) -> Result<()> {
    let self_uuid = sync_store::self_uuid(conn)?.unwrap_or_default();
    let mut incoming: Vec<DeviceStatsDto> = Vec::with_capacity(stats.remote_stats.len() + 1);
    incoming.push(stats.self_stats.clone());
    incoming.extend(stats.remote_stats.iter().cloned());
    for dto in incoming {
        if dto.device_id == self_uuid || dto.device_id.is_empty() {
            continue;
        }
        let _ = peer_uuid; // peer's own row arrives as dto.self_stats
        sync_store::upsert_remote_stats(
            conn,
            &super::store::DeviceStats {
                device_id: dto.device_id,
                total_words: dto.total_words,
                dictionary_fixes: dto.dictionary_fixes,
            },
        )?;
    }
    Ok(())
}

/// Applies the peer's settings with per-key LWW. An unstamped local key uses
/// the zero stamp until it is changed or a remote value is accepted, so an
/// incoming stamped value can win the first exchange.
pub fn apply_settings_exchange(
    conn: &Connection,
    host: &dyn SyncHost,
    settings: &[SettingRecord],
) -> Result<usize> {
    let mut applied = 0;
    for record in settings {
        if !SYNCABLE_SETTINGS.contains(&record.key.as_str()) {
            continue;
        }
        let local = sync_store::get_setting_stamp(conn, &record.key)?;
        let local_stamp = local
            .map(|s| (s.ts_ms, s.origin))
            .unwrap_or((0, String::new()));
        if (record.ts_ms, record.origin.as_str()) <= (local_stamp.0, local_stamp.1.as_str()) {
            continue;
        }
        match host.apply_remote_setting(&record.key, &record.value) {
            Ok(()) => {
                sync_store::set_setting_stamp(conn, &record.key, record.ts_ms, &record.origin)?;
                applied += 1;
            }
            Err(err) => {
                log::warn!("sync: failed to apply setting {}: {err}", record.key);
            }
        }
    }
    Ok(applied)
}

/// Stamps a local settings change so it wins LWW against peers from now on.
/// Called from the save_setting command path for syncable keys.
pub fn record_local_setting_change(conn: &Connection, key: &str) -> Result<()> {
    if !SYNCABLE_SETTINGS.contains(&key) {
        return Ok(());
    }
    sync_store::set_setting_stamp(conn, key, sync_store::now_ms(), &sync_store::self_uuid(conn)?.unwrap_or_default())
}

// ---------------------------------------------------------------------------
// Session driver
// ---------------------------------------------------------------------------

/// What a completed session did, for status events.
#[derive(Debug, Default)]
pub struct SessionSummary {
    pub applied: ApplySummary,
    pub settings_applied: usize,
}

fn hello(uuid: &str, name: &str, app_version: &str) -> Hello {
    Hello {
        device_uuid: uuid.to_string(),
        device_name: name.to_string(),
        protocol: PROTOCOL_VERSION,
        app_version: app_version.to_string(),
    }
}

/// Runs one full sync session over an authenticated stream. Both sides call
/// this; the phases run in a fixed order (initiator pulls, initiator serves,
/// then the roles flip), so the sequential message flow cannot deadlock:
///
/// 1. Hello exchange (initiator sends first).
/// 2. Meta exchange - stats + settings, initiator sends first.
/// 3. Initiator pulls from the responder (request/batches/acks).
/// 4. Responder pulls from the initiator.
/// 5. Each side sends SyncDone and drains the peer's.
///
/// The caller must already have verified the peer: TLS is up, the `Hello`
/// uuid maps to a paired peer, and the certificate fingerprint matches the
/// pin recorded at pairing time.
pub async fn run_session<S>(
    db: &DbHandle,
    host: &dyn SyncHost,
    stream: &mut S,
    initiator: bool,
    peer: &SyncPeer,
) -> Result<SessionSummary>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    run_session_after_hello(db, host, stream, initiator, peer, None).await
}

/// Responder entry point when the connection dispatcher already consumed and
/// authenticated the initial Hello frame.
pub async fn run_session_after_hello<S>(
    db: &DbHandle,
    host: &dyn SyncHost,
    stream: &mut S,
    initiator: bool,
    peer: &SyncPeer,
    remote_hello: Option<Hello>,
) -> Result<SessionSummary>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    use super::protocol::{read_message, send_message};

    // 1. Hello exchange.
    if initiator {
        send_message(
            stream,
            &Message::Hello(hello(
                &host.device_uuid(),
                &host.device_name(),
                &host.app_version(),
            )),
        )
        .await?;
        match read_message(stream).await? {
            Message::HelloAck(remote) => check_hello(&remote)?,
            Message::Error { message } => return Err(anyhow!("peer error: {message}")),
            other => return Err(anyhow!("expected HelloAck, got {other:?}")),
        }
    } else {
        if let Some(remote) = remote_hello {
            check_hello(&remote)?;
        } else {
            match read_message(stream).await? {
                Message::Hello(remote) => check_hello(&remote)?,
                Message::Error { message } => return Err(anyhow!("peer error: {message}")),
                other => return Err(anyhow!("expected Hello, got {other:?}")),
            }
        }
        send_message(
            stream,
            &Message::HelloAck(hello(
                &host.device_uuid(),
                &host.device_name(),
                &host.app_version(),
            )),
        )
        .await?;
    }

    let mut summary = SessionSummary::default();

    // 2. Meta exchange (stats + settings), initiator first. The DB lock is
    // never held across an await (the sync DB is shared with the pipeline).
    let stats = {
        let conn = lock(db)?;
        build_stats_exchange(&conn, &host.device_uuid())?
    };
    // ManagerHost reads setting stamps from this same database. Calling it
    // while holding `conn` deadlocks because std::sync::Mutex is not reentrant.
    let meta = Message::Meta {
        stats,
        settings: host.settings_payload()?,
    };
    if initiator {
        send_message(stream, &meta).await?;
    }
    let remote_meta = match read_message(stream).await? {
        Message::Meta { stats, settings } => (stats, settings),
        Message::Error { message } => return Err(anyhow!("peer error: {message}")),
        other => return Err(anyhow!("expected Meta, got {other:?}")),
    };
    if !initiator {
        send_message(stream, &meta).await?;
    }
    {
        let conn = lock(db)?;
        apply_stats_exchange(&conn, &remote_meta.0, peer.device_uuid.as_str())?;
        summary.settings_applied = apply_settings_exchange(&conn, host, &remote_meta.1)?;
    }
    if summary.settings_applied > 0 {
        summary.applied.settings = true;
    }
    summary.applied.stats = true;

    // 3 + 4. Pull both directions in fixed order.
    let mut pulled = ApplySummary::default();
    if initiator {
        pulled.merge(pull_from_peer(db, host, stream, peer).await?);
        serve_peer_pulls(db, stream, peer).await?;
    } else {
        serve_peer_pulls(db, stream, peer).await?;
        pulled.merge(pull_from_peer(db, host, stream, peer).await?);
    }
    summary.applied.merge(pulled);

    // 5. Done: announce and drain the peer's announcement (or their close).
    let _ = send_message(stream, &Message::SyncDone).await;
    match read_message(stream).await {
        Ok(Message::SyncDone) | Err(_) => {}
        Ok(_) => {}
    }
    Ok(summary)
}

fn check_hello(remote: &Hello) -> Result<()> {
    if remote.protocol != PROTOCOL_VERSION {
        return Err(anyhow!(
            "peer runs sync protocol v{} but this device speaks v{}",
            remote.protocol,
            PROTOCOL_VERSION
        ));
    }
    Ok(())
}

fn lock(db: &DbHandle) -> Result<std::sync::MutexGuard<'_, Connection>> {
    db.lock()
        .map_err(|_| anyhow!("database lock was poisoned"))
}

/// Puller side: request deltas, apply batches until the sender reports a
/// final cursor. A SyncDone from the peer mid-pull is a clean terminator.
async fn pull_from_peer<S>(
    db: &DbHandle,
    host: &dyn SyncHost,
    stream: &mut S,
    peer: &SyncPeer,
) -> Result<ApplySummary>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    let mut summary = ApplySummary::default();
    // The pull position is OUR record of the peer's log (recv_cursor) - the
    // send_cursor on the same row tracks the opposite direction.
    let since = {
        let conn = lock(db)?;
        sync_store::peer_recv_cursor(&conn, &peer.device_uuid)?
    };
    let snapshot = since == 0;
    send_message(
        stream,
        &Message::PullRequest(PullRequest {
            since_seq: since,
            snapshot,
        }),
    )
    .await?;

    loop {
        match read_message(stream).await? {
            Message::Ops(batch) => {
                let resolved_ops = resolve_context_targets_in_ops(host, &batch.ops);
                let applied = {
                    let conn = lock(db)?;
                    apply_ops(&conn, &resolved_ops)?
                };
                summary.merge(applied);
                let acked = batch.cursor;
                send_message(stream, &Message::Ack { seq: acked }).await?;
                if batch.done {
                    // Final batch: remember the position so the next session
                    // pulls only fresh changes.
                    let conn = lock(db)?;
                    sync_store::set_peer_recv_cursor(&conn, &peer.device_uuid, acked)?;
                    break;
                }
            }
            Message::SyncDone => break,
            Message::Error { message } => return Err(anyhow!("peer error: {message}")),
            other => return Err(anyhow!("unexpected message during pull: {other:?}")),
        }
    }
    Ok(summary)
}

/// Sender side: serve the peer's pull request with batches until it acks the
/// final cursor. Returns when the peer's pull is complete - it will then start
/// its own pull.
async fn serve_peer_pulls<S>(db: &DbHandle, stream: &mut S, peer: &SyncPeer) -> Result<()>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    match super::protocol::read_message(stream).await? {
        Message::PullRequest(request) => {
            serve_one_pull(db, stream, peer, request.since_seq, request.snapshot).await
        }
        Message::SyncDone => Ok(()),
        Message::Error { message } => Err(anyhow!("peer error: {message}")),
        other => Err(anyhow!("expected PullRequest, got {other:?}")),
    }
}

async fn serve_one_pull<S>(
    db: &DbHandle,
    stream: &mut S,
    peer: &SyncPeer,
    since_seq: i64,
    requested_snapshot: bool,
) -> Result<()>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    let snapshot = if requested_snapshot {
        true
    } else {
        let conn = lock(db)?;
        sync_store::needs_snapshot_for(&conn, &peer.device_uuid, since_seq)?
    };
    let mut progress = SnapshotProgress::default();
    let mut next_since_seq = since_seq;
    loop {
        let (ops, cursor, done) = {
            let conn = lock(db)?;
            collect_ops(&conn, next_since_seq, snapshot, OPS_PER_BATCH, &mut progress)?
        };
        let final_cursor = cursor;
        send_message(
            stream,
            &Message::Ops(OpsBatch {
                ops,
                cursor: final_cursor,
                done,
                snapshot,
            }),
        )
        .await?;
        match read_message(stream).await? {
            Message::Ack { seq } => {
                let conn = lock(db)?;
                sync_store::set_peer_send_position(&conn, &peer.device_uuid, seq, false)?;
            }
            Message::Error { message } => return Err(anyhow!("peer error: {message}")),
            other => return Err(anyhow!("unexpected message during serve: {other:?}")),
        }
        if !snapshot && !done {
            next_since_seq = cursor;
        }
        if done {
            break;
        }
    }
    Ok(())
}

impl ApplySummary {
    fn merge(&mut self, other: ApplySummary) {
        self.dictionary |= other.dictionary;
        self.snippets |= other.snippets;
        self.contexts |= other.contexts;
        self.history |= other.history;
        self.settings |= other.settings;
        self.stats |= other.stats;
        self.applied += other.applied;
        self.skipped += other.skipped;
    }
}
