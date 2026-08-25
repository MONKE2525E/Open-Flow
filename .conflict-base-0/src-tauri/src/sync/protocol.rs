//! Wire protocol for Verenu LAN sync: length-prefixed JSON messages over a
//! mutually-authenticated TLS stream. Boring on purpose - every message is a
//! serde struct, framed with a 4-byte big-endian length prefix.

use anyhow::{anyhow, Result, Context};
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

/// Bump when the message set or op payloads change incompatibly. Devices on
/// different versions refuse to sync with a clear error instead of corrupting
/// each other's data.
pub const PROTOCOL_VERSION: u32 = 1;

/// Hard cap on one framed message. Batches are chunked well below this; the
/// cap exists so a hostile peer can't make us allocate unbounded memory.
pub const MAX_MESSAGE_BYTES: u32 = 64 * 1024 * 1024;

/// Ops are sent in chunks of this many rows (transcription text makes rows
/// large; this keeps each message comfortably under the frame cap).
pub const OPS_PER_BATCH: usize = 400;

/// Snapshot rows per history chunk (see the engine's `collect_ops`).
pub const SNAPSHOT_ROW_CHUNK: i64 = 400;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Hello {
    pub device_uuid: String,
    pub device_name: String,
    pub protocol: u32,
    pub app_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatsExchange {
    /// This device's own lifetime counters (the `lifetime_stats` row).
    pub self_stats: DeviceStatsDto,
    /// Everything this device knows about *other* devices' counters.
    pub remote_stats: Vec<DeviceStatsDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceStatsDto {
    pub device_id: String,
    pub total_words: i64,
    pub dictionary_fixes: i64,
}

/// One synced setting: the value plus the LWW stamp from `sync_setting_meta`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SettingRecord {
    pub key: String,
    pub value: serde_json::Value,
    pub ts_ms: i64,
    pub origin: String,
}

/// A single change: one row upsert/delete (or a context aggregate upsert).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncOp {
    pub table: String,
    pub row_uuid: String,
    pub op: String,
    pub ts_ms: i64,
    pub origin: String,
    pub origin_seq: i64,
    /// JSON payload for upserts (shape depends on `table`); absent for deletes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payload: Option<serde_json::Value>,
}

impl SyncOp {
    pub fn table_name(&self) -> &str {
        &self.table
    }

    pub fn op_name(&self) -> &str {
        &self.op
    }

    pub fn is_delete(&self) -> bool {
        self.op == "delete"
    }

    /// Deterministic total order for last-writer-wins: (timestamp, origin,
    /// origin seq). Two devices comparing the same pair of ops always pick the
    /// same winner regardless of application order.
    pub fn stamp(&self) -> (i64, &str, i64) {
        (self.ts_ms, self.origin.as_str(), self.origin_seq)
    }

    pub fn newer_than(&self, other: &(i64, String, i64)) -> bool {
        self.stamp() > (other.0, other.1.as_str(), other.2)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PullRequest {
    pub since_seq: i64,
    pub snapshot: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpsBatch {
    pub ops: Vec<SyncOp>,
    /// Sender's log position after this batch (valid when `done`).
    pub cursor: i64,
    /// True when this is the last batch of the pull.
    pub done: bool,
    pub snapshot: bool,
}

/// Everything that can travel over the wire after the TLS handshake.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Message {
    Hello(Hello),
    HelloAck(Hello),
    Meta {
        stats: StatsExchange,
        settings: Vec<SettingRecord>,
    },
    PullRequest(PullRequest),
    Ops(OpsBatch),
    Ack {
        seq: i64,
    },
    SyncDone,
    /// Best-effort notification that the sender removed us as a peer.
    Unpair {
        device_uuid: String,
    },
    Error {
        message: String,
    },
    // ---- pairing (only before trust exists) ----
    PairRequest {
        device_uuid: String,
        device_name: String,
        protocol: u32,
        /// Initiator's SPAKE2 message, exchanged in the clear by design -
        /// PAKE messages are safe over an unauthenticated channel.
        spake_msg: Vec<u8>,
    },
    /// Responder agrees and sends its SPAKE2 message (user typed the code).
    PairAccept {
        spake_msg: Vec<u8>,
    },
    /// Initiator's SPAKE2 message, or the encrypted identity exchange.
    PairVerify {
        ciphertext: Vec<u8>,
        nonce: Vec<u8>,
    },
    PairComplete,
    PairReject {
        reason: String,
    },
    PairBusy,
}

pub async fn send_message<W: AsyncWrite + Unpin>(writer: &mut W, message: &Message) -> Result<()> {
    let body = serde_json::to_vec(message).context("encode message")?;
    let len = u32::try_from(body.len()).map_err(|_| anyhow!("message too large"))?;
    if len > MAX_MESSAGE_BYTES {
        return Err(anyhow!("message too large: {len} bytes"));
    }
    writer.write_all(&len.to_be_bytes()).await?;
    writer.write_all(&body).await?;
    writer.flush().await?;
    Ok(())
}

pub async fn read_message<R: AsyncRead + Unpin>(reader: &mut R) -> Result<Message> {
    let mut len_bytes = [0u8; 4];
    reader.read_exact(&mut len_bytes).await?;
    let len = u32::from_be_bytes(len_bytes);
    if len > MAX_MESSAGE_BYTES {
        return Err(anyhow!("peer sent oversized message: {len} bytes"));
    }
    let mut body = vec![0u8; len as usize];
    reader.read_exact(&mut body).await?;
    let message: Message =
        serde_json::from_slice(&body).context("decode message from peer")?;
    Ok(message)
}
