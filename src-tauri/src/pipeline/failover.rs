//! Disk-backed dictation failover: crash/reboot survival for in-progress takes.
//!
//! Audio is stored as append-only i16le mono 16 kHz PCM plus an atomic JSON
//! sidecar. The sidecar `sample_count` is published only after the matching
//! PCM bytes have been flushed, and load clamps to the shorter of the two.

use super::gates::{MIN_RECORDING_MS, MIN_RECORDING_RMS};
use super::pill::{show_cancelled_pill, show_interrupted_pill};
use super::state::{
    lock_state, CaptureOrigin, CancelledCapture, SharedState, CANCEL_RESUME_WINDOW,
};
use super::{state, CapturedAudio};
use crate::media::audio::{self, DurableSink};
use chrono::{SecondsFormat, TimeZone, Utc};
use serde::{Deserialize, Serialize};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, Manager};

const SESSION_VERSION: u32 = 1;
const TARGET_RATE: u32 = 16_000;
const LIVE_DIR: &str = "live";
const COMMITTED_DIR: &str = "committed";
const SESSION_FILE: &str = "session.json";
const AUDIO_FILE: &str = "audio.pcm";
const TTL_SECS: i64 = 24 * 60 * 60;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FailoverKind {
    Recording,
    Cancelled,
    Processing,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SessionMeta {
    pub version: u32,
    pub id: String,
    pub kind: FailoverKind,
    pub started_at_unix: i64,
    pub sample_rate: u32,
    pub sample_count: u64,
    pub duration_ms: u64,
}

#[derive(Clone, Debug)]
pub struct LoadedTake {
    pub meta: SessionMeta,
    pub samples_16k: Vec<f32>,
}

impl LoadedTake {
    fn usable_samples(&self) -> u64 {
        self.samples_16k.len() as u64
    }

    fn duration_ms(&self) -> u64 {
        if self.meta.sample_rate == 0 {
            0
        } else {
            self.usable_samples() * 1000 / u64::from(self.meta.sample_rate)
        }
    }

    fn passes_gates(&self) -> bool {
        if self.duration_ms() < MIN_RECORDING_MS {
            return false;
        }
        audio::rms_f32(&self.samples_16k) >= MIN_RECORDING_RMS
    }

    fn is_fresh(&self, now_unix: i64) -> bool {
        now_unix.saturating_sub(self.meta.started_at_unix) < TTL_SECS
    }

    fn origin(&self) -> CaptureOrigin {
        match self.meta.kind {
            FailoverKind::Cancelled => CaptureOrigin::UserCancelled,
            FailoverKind::Recording | FailoverKind::Processing => CaptureOrigin::Interrupted,
        }
    }
}

#[derive(Clone, Serialize)]
pub struct CancelledCapturePayload {
    pub created_at: String,
    pub kind: String,
}

pub fn failover_dir() -> PathBuf {
    crate::app_data_dir().join("dictation-failover")
}

fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn f32_to_i16(s: f32) -> i16 {
    let v = if s.is_finite() { s.clamp(-1.0, 1.0) } else { 0.0 };
    (v * i16::MAX as f32) as i16
}

fn i16_to_f32(s: i16) -> f32 {
    s as f32 / i16::MAX as f32
}

fn slot_dir(root: &Path, live: bool) -> PathBuf {
    root.join(if live { LIVE_DIR } else { COMMITTED_DIR })
}

fn replace_file(from: &Path, to: &Path) -> std::io::Result<()> {
    let _ = fs::remove_file(to);
    match fs::rename(from, to) {
        Ok(()) => Ok(()),
        // Windows: access denied (5), sharing violation (32), already exists (183).
        Err(e) if matches!(e.raw_os_error(), Some(5 | 32 | 183)) => {
            fs::copy(from, to)?;
            let _ = fs::remove_file(from);
            Ok(())
        }
        Err(e) => Err(e),
    }
}

fn write_session_atomic(path: &Path, meta: &SessionMeta) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_vec_pretty(meta)?;
    let tmp = path.with_extension("json.tmp");
    {
        let mut f = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&tmp)?;
        f.write_all(&json)?;
        f.sync_all()?;
    }
    if let Err(e) = replace_file(&tmp, path) {
        let _ = fs::remove_file(&tmp);
        return Err(e.into());
    }
    Ok(())
}

fn load_session(path: &Path) -> Option<SessionMeta> {
    let raw = fs::read_to_string(path).ok()?;
    let meta: SessionMeta = serde_json::from_str(&raw).ok()?;
    if meta.version != SESSION_VERSION || meta.sample_rate != TARGET_RATE {
        return None;
    }
    if meta.id.is_empty() {
        return None;
    }
    Some(meta)
}

/// Load a slot, clamping published `sample_count` to the PCM file length.
pub fn load_slot(root: &Path, live: bool) -> Option<LoadedTake> {
    let dir = slot_dir(root, live);
    let mut meta = load_session(&dir.join(SESSION_FILE))?;
    let pcm_path = dir.join(AUDIO_FILE);
    let mut file = File::open(&pcm_path).ok()?;
    let file_len = file.metadata().ok()?.len();
    let file_samples = file_len / 2;
    let usable = meta.sample_count.min(file_samples);
    if usable == 0 {
        return None;
    }
    let byte_len = (usable * 2) as usize;
    let mut buf = vec![0u8; byte_len];
    file.read_exact(&mut buf).ok()?;
    let mut samples = Vec::with_capacity(usable as usize);
    // Keep chunks_exact for the PCM pair decode; allow the clippy hint that prefers as_chunks.
    #[allow(clippy::manual_slice_chunks)]
    for chunk in buf.chunks_exact(2) {
        samples.push(i16_to_f32(i16::from_le_bytes(chunk.try_into().unwrap())));
    }
    meta.sample_count = usable;
    meta.duration_ms = usable * 1000 / u64::from(TARGET_RATE);
    Some(LoadedTake {
        meta,
        samples_16k: samples,
    })
}

pub fn delete_slot(root: &Path, live: bool) {
    let dir = slot_dir(root, live);
    let _ = fs::remove_file(dir.join(AUDIO_FILE));
    let _ = fs::remove_file(dir.join(SESSION_FILE));
    let _ = fs::remove_file(dir.join("session.json.tmp"));
    let _ = fs::remove_file(dir.join("audio.pcm.tmp"));
    let _ = fs::remove_dir(&dir);
}

pub fn delete_live(root: &Path) {
    delete_slot(root, true);
}

pub fn delete_committed(root: &Path) {
    delete_slot(root, false);
}

pub fn delete_all(root: &Path) {
    delete_live(root);
    delete_committed(root);
    let _ = fs::remove_dir(root);
}

pub fn abandon_live() {
    delete_live(&failover_dir());
}

pub fn discard_durable() {
    delete_all(&failover_dir());
}

fn samples_to_pcm(samples: &[f32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(samples.len() * 2);
    for &s in samples {
        out.extend_from_slice(&f32_to_i16(s).to_le_bytes());
    }
    out
}

fn write_slot(
    root: &Path,
    live: bool,
    id: &str,
    kind: FailoverKind,
    started_at_unix: i64,
    samples_16k: &[f32],
) -> anyhow::Result<()> {
    if samples_16k.is_empty() {
        anyhow::bail!("no samples to commit");
    }
    let dir = slot_dir(root, live);
    fs::create_dir_all(&dir)?;
    let pcm = samples_to_pcm(samples_16k);
    let tmp = dir.join("audio.pcm.tmp");
    let dest = dir.join(AUDIO_FILE);
    {
        let mut f = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&tmp)?;
        f.write_all(&pcm)?;
        f.sync_all()?;
    }
    replace_file(&tmp, &dest)?;
    let sample_count = samples_16k.len() as u64;
    let meta = SessionMeta {
        version: SESSION_VERSION,
        id: id.to_string(),
        kind,
        started_at_unix,
        sample_rate: TARGET_RATE,
        sample_count,
        duration_ms: sample_count * 1000 / u64::from(TARGET_RATE),
    };
    write_session_atomic(&dir.join(SESSION_FILE), &meta)?;
    Ok(())
}

pub fn write_committed(
    root: &Path,
    id: &str,
    kind: FailoverKind,
    started_at_unix: i64,
    samples_16k: &[f32],
) -> anyhow::Result<()> {
    write_slot(root, false, id, kind, started_at_unix, samples_16k)?;
    delete_live(root);
    log::info!(
        "failover: committed id_prefix={} kind={:?} samples={} duration_ms={}",
        id_prefix(id),
        kind,
        samples_16k.len(),
        samples_16k.len() as u64 * 1000 / u64::from(TARGET_RATE)
    );
    Ok(())
}

fn id_prefix(id: &str) -> &str {
    id.get(..8).unwrap_or(id)
}

/// Startup restore: pick live vs committed, delete the loser, return the winner.
pub fn restore_choice(root: &Path, now_unix: i64) -> Option<LoadedTake> {
    let live_raw = load_slot(root, true);
    let committed_raw = load_slot(root, false);

    let live = live_raw.filter(|t| {
        if t.is_fresh(now_unix) && t.passes_gates() {
            true
        } else {
            delete_live(root);
            false
        }
    });
    let committed = committed_raw.filter(|t| {
        if t.is_fresh(now_unix) && t.passes_gates() {
            true
        } else {
            delete_committed(root);
            false
        }
    });

    match (live, committed) {
        (Some(l), Some(c)) if l.meta.id == c.meta.id => {
            if c.meta.kind == FailoverKind::Processing {
                delete_live(root);
                Some(c)
            } else if l.usable_samples() >= c.usable_samples() {
                delete_committed(root);
                Some(l)
            } else {
                delete_live(root);
                Some(c)
            }
        }
        (Some(l), Some(c)) => {
            if l.duration_ms() >= MIN_RECORDING_MS {
                delete_committed(root);
                Some(l)
            } else {
                delete_live(root);
                Some(c)
            }
        }
        (Some(l), None) => Some(l),
        (None, Some(c)) => {
            delete_live(root);
            Some(c)
        }
        (None, None) => {
            delete_all(root);
            None
        }
    }
}

fn loaded_to_capture(take: LoadedTake) -> Option<CancelledCapture> {
    let duration_ms = take.duration_ms();
    let origin = take.origin();
    let started_at_unix = take.meta.started_at_unix;
    let id = take.meta.id.clone();
    let wav = audio::encode_wav(&take.samples_16k, TARGET_RATE, 1).ok()?;
    let created_at_rfc3339 = Utc
        .timestamp_opt(started_at_unix, 0)
        .single()
        .unwrap_or_else(Utc::now)
        .to_rfc3339_opts(SecondsFormat::Secs, true);
    Some(CancelledCapture {
        audio: CapturedAudio {
            wav: bytes::Bytes::from(wav),
            samples_16k: Arc::new(take.samples_16k),
            sample_rate: TARGET_RATE,
            duration_ms,
        },
        captured_at: Instant::now(),
        id,
        origin,
        created_at_rfc3339,
        started_at_unix,
    })
}

/// Load a surviving take into RAM. Does not show the pill (the watchdog does).
pub fn restore_into_state(state: &SharedState) {
    let Some(take) = restore_choice(&failover_dir(), now_unix()) else {
        return;
    };
    let origin = take.origin();
    let samples = take.usable_samples();
    let Some(capture) = loaded_to_capture(take) else {
        log::warn!("failover: restore encode failed samples={samples}");
        return;
    };
    match lock_state(state) {
        Ok(mut st) => {
            if !st.lifecycle.is_idle() {
                return;
            }
            log::info!(
                "failover: restored id_prefix={} origin={:?} samples={}",
                id_prefix(&capture.id),
                origin,
                samples
            );
            st.cancelled_capture = Some(capture);
        }
        Err(_) => log::warn!("failover: restore skipped (state lock poisoned)"),
    }
}

/// After the pill window can be shown, surface a restored take.
pub fn offer_restored_capture_pill(app: &AppHandle) -> bool {
    let Some(state) = app.try_state::<SharedState>() else {
        return false;
    };
    let Some(capture) = state::peek_cancelled_capture_if_fresh(state.inner()) else {
        return false;
    };
    if capture.captured_at.elapsed() >= CANCEL_RESUME_WINDOW {
        return false;
    }
    emit_cancelled_payload(app, &capture.created_at_rfc3339, capture.origin.as_str());
    match capture.origin {
        CaptureOrigin::UserCancelled => show_cancelled_pill(app),
        CaptureOrigin::Interrupted => show_interrupted_pill(app),
    }
    true
}

pub fn emit_cancelled_payload(app: &AppHandle, created_at: &str, kind: &str) {
    app.emit(
        "verenu:cancelled-capture",
        CancelledCapturePayload {
            created_at: created_at.to_string(),
            kind: kind.to_string(),
        },
    )
    .ok();
}

pub fn commit_capture(audio: &CapturedAudio, id: &str, kind: FailoverKind, started_at_unix: i64) {
    if let Err(e) = write_committed(
        &failover_dir(),
        id,
        kind,
        started_at_unix,
        &audio.samples_16k,
    ) {
        log::warn!(
            "failover: commit failed id_prefix={} samples={}: {e}",
            id_prefix(id),
            audio.samples_16k.len()
        );
    }
}

pub fn retire_committed() {
    delete_committed(&failover_dir());
}

/// Incremental live writer. PCM is appended and synced before `sample_count`
/// is published in the sidecar.
pub struct LiveWriter {
    root: PathBuf,
    file: File,
    meta: SessionMeta,
    native_tail: Vec<f32>,
    native_rate: u32,
    src_pos: f64,
    last_checkpoint: Instant,
    superseded: bool,
    app: Option<AppHandle>,
    state: Option<SharedState>,
}

impl LiveWriter {
    pub fn open(
        root: PathBuf,
        id: String,
        prepend_16k: Option<&[f32]>,
        app: Option<AppHandle>,
        state: Option<SharedState>,
    ) -> anyhow::Result<Self> {
        let dir = slot_dir(&root, true);
        fs::create_dir_all(&dir)?;
        let pcm_path = dir.join(AUDIO_FILE);
        let _ = fs::remove_file(&pcm_path);
        let mut file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&pcm_path)?;
        let started_at_unix = now_unix();
        let mut meta = SessionMeta {
            version: SESSION_VERSION,
            id,
            kind: FailoverKind::Recording,
            started_at_unix,
            sample_rate: TARGET_RATE,
            sample_count: 0,
            duration_ms: 0,
        };
        if let Some(prepend) = prepend_16k {
            if !prepend.is_empty() {
                let pcm = samples_to_pcm(prepend);
                file.write_all(&pcm)?;
                file.flush()?;
                file.sync_data()?;
                meta.sample_count = prepend.len() as u64;
                meta.duration_ms = meta.sample_count * 1000 / u64::from(TARGET_RATE);
            }
        }
        write_session_atomic(&dir.join(SESSION_FILE), &meta)?;
        Ok(Self {
            root,
            file,
            meta,
            native_tail: Vec::new(),
            native_rate: 0,
            src_pos: 0.0,
            last_checkpoint: Instant::now(),
            superseded: false,
            app,
            state,
        })
    }

    fn convert_native(&mut self, finish: bool) -> Vec<f32> {
        if self.native_rate == 0 || self.native_tail.is_empty() {
            if finish {
                self.native_tail.clear();
                self.src_pos = 0.0;
            }
            return Vec::new();
        }
        if self.native_rate == TARGET_RATE {
            let out = std::mem::take(&mut self.native_tail);
            self.src_pos = 0.0;
            return out;
        }
        let ratio = f64::from(self.native_rate) / f64::from(TARGET_RATE);
        let last = self.native_tail.len().saturating_sub(1);
        let mut out = Vec::new();
        loop {
            let lo = self.src_pos.floor() as usize;
            let hi = lo + 1;
            if !finish && hi >= self.native_tail.len() {
                break;
            }
            if lo > last {
                break;
            }
            let hi = hi.min(last);
            let t = (self.src_pos - lo as f64) as f32;
            out.push(self.native_tail[lo] * (1.0 - t) + self.native_tail[hi] * t);
            self.src_pos += ratio;
            if finish && lo >= last {
                break;
            }
        }
        if finish {
            self.native_tail.clear();
            self.src_pos = 0.0;
        } else {
            let drop = self.src_pos.floor() as usize;
            if drop > 0 {
                let drop = drop.min(self.native_tail.len());
                self.native_tail.drain(..drop);
                self.src_pos -= drop as f64;
                if self.src_pos < 0.0 {
                    self.src_pos = 0.0;
                }
            }
        }
        out
    }

    fn checkpoint(&mut self, finish: bool) {
        let samples = self.convert_native(finish);
        if samples.is_empty() && !finish {
            return;
        }
        if !samples.is_empty() {
            let pcm = samples_to_pcm(&samples);
            if let Err(e) = self.file.write_all(&pcm).and_then(|_| self.file.flush()) {
                log::warn!("failover: live pcm write failed: {e}");
                return;
            }
            if let Err(e) = self.file.sync_data() {
                log::warn!("failover: live pcm sync failed: {e}");
                return;
            }
            self.meta.sample_count += samples.len() as u64;
            self.meta.duration_ms = self.meta.sample_count * 1000 / u64::from(TARGET_RATE);
            let session_path = slot_dir(&self.root, true).join(SESSION_FILE);
            if let Err(e) = write_session_atomic(&session_path, &self.meta) {
                log::warn!("failover: live sidecar write failed: {e}");
            }
        }
        self.last_checkpoint = Instant::now();
        self.maybe_supersede();
    }

    fn maybe_supersede(&mut self) {
        if self.superseded || self.meta.duration_ms < MIN_RECORDING_MS {
            return;
        }
        self.superseded = true;
        delete_committed(&self.root);
        if let Some(state) = &self.state {
            if let Ok(mut st) = lock_state(state) {
                let current_id = self.meta.id.as_str();
                let stale = st
                    .cancelled_capture
                    .as_ref()
                    .is_some_and(|c| c.id != current_id);
                if stale {
                    st.cancelled_capture = None;
                    if let Some(app) = &self.app {
                        state::emit_cancelled_capture_cleared(app);
                    }
                }
            }
        }
        log::debug!(
            "failover: live superseded committed id_prefix={} duration_ms={}",
            id_prefix(&self.meta.id),
            self.meta.duration_ms
        );
    }
}

impl DurableSink for LiveWriter {
    fn extend(&mut self, native_samples: &[f32], native_rate: u32) {
        if native_samples.is_empty() {
            return;
        }
        if self.native_rate == 0 {
            self.native_rate = native_rate;
        }
        self.native_tail.extend_from_slice(native_samples);
        let one_sec = self.native_rate.max(1) as usize;
        if self.native_tail.len() >= one_sec
            || self.last_checkpoint.elapsed() >= Duration::from_secs(1)
        {
            self.checkpoint(false);
        }
    }

    fn finish(&mut self) {
        self.checkpoint(true);
        let _ = self.file.flush();
        let _ = self.file.sync_all();
    }
}

pub fn open_live_writer(
    id: String,
    prepend_16k: Option<&[f32]>,
    app: &AppHandle,
    state: &SharedState,
) -> Option<Box<dyn DurableSink>> {
    match LiveWriter::open(
        failover_dir(),
        id,
        prepend_16k,
        Some(app.clone()),
        Some(state.clone()),
    ) {
        Ok(w) => Some(Box::new(w)),
        Err(e) => {
            log::warn!("failover: live writer open failed: {e}");
            None
        }
    }
}

pub fn new_session_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

pub fn flush_on_exit(app: &AppHandle) {
    let Some(state) = app.try_state::<SharedState>() else {
        return;
    };
    let (id, started) = match lock_state(state.inner()) {
        Ok(st) => (
            st.failover_session_id.clone(),
            if st.failover_started_at_unix != 0 {
                st.failover_started_at_unix
            } else {
                now_unix()
            },
        ),
        Err(_) => return,
    };
    let Some((session, mic_id)) = state::take_recording_plain(state.inner()) else {
        return;
    };
    match session.stop() {
        Ok(result) => {
            if let Some(id) = id {
                if result.duration_ms >= MIN_RECORDING_MS {
                    let samples = result.samples_16k;
                    let _ = write_committed(
                        &failover_dir(),
                        &id,
                        FailoverKind::Recording,
                        started,
                        &samples,
                    );
                }
            }
        }
        Err(e) => log::warn!("failover: exit flush stop failed: {e}"),
    }
    if let Some(session_id) = mic_id {
        crate::system::volume::release_mic(session_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_root() -> PathBuf {
        let p = std::env::temp_dir().join(format!("verenu-failover-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&p).unwrap();
        p
    }

    fn tone(samples: usize, amp: f32) -> Vec<f32> {
        (0..samples)
            .map(|i| amp * ((i as f32) * 0.1).sin())
            .collect()
    }

    fn loud_ms(ms: u64) -> Vec<f32> {
        tone((TARGET_RATE as u64 * ms / 1000) as usize, 0.4)
    }

    #[test]
    fn round_trip_write_read() {
        let root = test_root();
        let samples = loud_ms(800);
        write_committed(&root, "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee", FailoverKind::Cancelled, 1_700_000_000, &samples)
            .unwrap();
        let loaded = load_slot(&root, false).unwrap();
        assert_eq!(loaded.samples_16k.len(), samples.len());
        assert!(loaded.passes_gates());
        delete_all(&root);
    }

    #[test]
    fn pcm_ahead_of_sidecar_uses_published_count() {
        let root = test_root();
        let samples = loud_ms(800);
        write_committed(&root, "id-published", FailoverKind::Recording, now_unix(), &samples).unwrap();
        let pcm_path = slot_dir(&root, false).join(AUDIO_FILE);
        let extra = samples_to_pcm(&loud_ms(200));
        let mut f = OpenOptions::new().append(true).open(&pcm_path).unwrap();
        f.write_all(&extra).unwrap();
        f.sync_all().unwrap();
        drop(f);
        let loaded = load_slot(&root, false).unwrap();
        assert_eq!(loaded.samples_16k.len(), samples.len());
        delete_all(&root);
    }

    #[test]
    fn sidecar_ahead_of_pcm_clamps_to_file() {
        let root = test_root();
        let samples = loud_ms(800);
        write_committed(&root, "id-clamp", FailoverKind::Recording, now_unix(), &samples).unwrap();
        let mut meta = load_session(&slot_dir(&root, false).join(SESSION_FILE)).unwrap();
        meta.sample_count = samples.len() as u64 + 4000;
        write_session_atomic(&slot_dir(&root, false).join(SESSION_FILE), &meta).unwrap();
        let loaded = load_slot(&root, false).unwrap();
        assert_eq!(loaded.samples_16k.len(), samples.len());
        delete_all(&root);
    }

    #[test]
    fn odd_trailing_byte_is_dropped() {
        let root = test_root();
        let samples = loud_ms(800);
        write_committed(&root, "id-odd", FailoverKind::Recording, now_unix(), &samples).unwrap();
        let pcm_path = slot_dir(&root, false).join(AUDIO_FILE);
        let mut f = OpenOptions::new().append(true).open(&pcm_path).unwrap();
        f.write_all(&[0x7f]).unwrap();
        f.sync_all().unwrap();
        drop(f);
        let loaded = load_slot(&root, false).unwrap();
        assert_eq!(loaded.samples_16k.len(), samples.len());
        delete_all(&root);
    }

    #[test]
    fn expired_sidecar_is_not_restored() {
        let root = test_root();
        let samples = loud_ms(800);
        write_committed(&root, "id-old", FailoverKind::Cancelled, 1_000, &samples).unwrap();
        assert!(restore_choice(&root, 1_000 + TTL_SECS + 10).is_none());
        assert!(load_slot(&root, false).is_none());
        delete_all(&root);
    }

    #[test]
    fn processing_committed_beats_same_id_live() {
        let root = test_root();
        let committed = loud_ms(1200);
        let live = loud_ms(2000);
        let t = now_unix();
        write_committed(&root, "same-id", FailoverKind::Processing, t, &committed).unwrap();
        write_slot(
            &root,
            true,
            "same-id",
            FailoverKind::Recording,
            t,
            &live,
        )
        .unwrap();
        let restored = restore_choice(&root, t).unwrap();
        assert_eq!(restored.meta.kind, FailoverKind::Processing);
        assert_eq!(restored.samples_16k.len(), committed.len());
        delete_all(&root);
    }

    #[test]
    fn short_new_live_keeps_old_committed() {
        let root = test_root();
        let old = loud_ms(1200);
        write_committed(&root, "old-id", FailoverKind::Cancelled, now_unix(), &old).unwrap();
        let short = loud_ms(200);
        let mut w = LiveWriter::open(root.clone(), "new-id".into(), Some(&short), None, None).unwrap();
        w.finish();
        drop(w);
        // 200ms fails gates, so restore_choice drops live and keeps committed.
        let restored = restore_choice(&root, now_unix()).unwrap();
        assert_eq!(restored.meta.id, "old-id");
        delete_all(&root);
    }

    #[test]
    fn long_new_live_wins_over_committed() {
        let root = test_root();
        let old = loud_ms(1200);
        write_committed(&root, "old-id", FailoverKind::Cancelled, now_unix(), &old).unwrap();
        let neu = loud_ms(900);
        let mut w = LiveWriter::open(root.clone(), "new-id".into(), Some(&neu), None, None).unwrap();
        w.finish();
        drop(w);
        let restored = restore_choice(&root, now_unix()).unwrap();
        assert_eq!(restored.meta.id, "new-id");
        assert!(load_slot(&root, false).is_none());
        delete_all(&root);
    }

    #[test]
    fn live_writer_publish_after_sync() {
        let root = test_root();
        let mut w = LiveWriter::open(root.clone(), "live-1".into(), None, None, None).unwrap();
        w.extend(&loud_ms(1000), TARGET_RATE);
        w.finish();
        drop(w);
        let loaded = load_slot(&root, true).unwrap();
        assert!(loaded.samples_16k.len() >= 15_000);
        delete_all(&root);
    }

    #[test]
    fn resume_seed_without_supersede_keeps_committed() {
        let root = test_root();
        let original = loud_ms(1000);
        let t = now_unix();
        write_committed(&root, "resume-id", FailoverKind::Cancelled, t, &original).unwrap();
        write_slot(
            &root,
            true,
            "resume-id",
            FailoverKind::Recording,
            t,
            &original,
        )
        .unwrap();
        assert!(load_slot(&root, false).is_some());
        assert!(load_slot(&root, true).is_some());
        let restored = restore_choice(&root, t).unwrap();
        assert_eq!(restored.meta.id, "resume-id");
        delete_all(&root);
    }

    #[test]
    fn crash_before_seed_keeps_committed() {
        let root = test_root();
        let original = loud_ms(1000);
        write_committed(&root, "resume-id", FailoverKind::Cancelled, now_unix(), &original).unwrap();
        let restored = restore_choice(&root, now_unix()).unwrap();
        assert_eq!(restored.meta.id, "resume-id");
        assert_eq!(restored.meta.kind, FailoverKind::Cancelled);
        delete_all(&root);
    }

    #[test]
    fn too_quiet_is_not_restored() {
        let root = test_root();
        let quiet = vec![0.0f32; (TARGET_RATE as u64 * 800 / 1000) as usize];
        write_committed(&root, "quiet", FailoverKind::Cancelled, now_unix(), &quiet).unwrap();
        assert!(restore_choice(&root, now_unix()).is_none());
        delete_all(&root);
    }
}
