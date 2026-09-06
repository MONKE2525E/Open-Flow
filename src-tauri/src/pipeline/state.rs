use super::*;
use crate::core::window_geometry::WindowTarget;
use crate::pipeline::pill_position::PillPlacement;
use std::sync::atomic::AtomicU64;

pub(super) const RETRY_WINDOW: std::time::Duration = std::time::Duration::from_secs(600);

/// How long a cancelled recording's audio stays resumable — matches
/// `RETRY_WINDOW` so a missed pill notification (which auto-hides after 10s)
/// still leaves plenty of time to continue it from the Home history list.
pub(super) const CANCEL_RESUME_WINDOW: std::time::Duration = std::time::Duration::from_secs(600);

/// How long a failed paste's text stays available for the pill's Copy
/// button to pull back onto the clipboard.
pub(super) const PASTE_FAILURE_WINDOW: std::time::Duration = std::time::Duration::from_secs(300);

fn paste_failure_is_fresh(captured_at: std::time::Instant, now: std::time::Instant) -> bool {
    now.checked_duration_since(captured_at)
        .is_some_and(|age| age < PASTE_FAILURE_WINDOW)
}
// ---------- shared state ----------

/// Default pill window width (logical points) before the frontend reports the
/// real content width. Deliberately the *smallest* content width (the bare
/// recording capsule, matching PillApp.svelte's MIN_PILL_WINDOW_W) rather than
/// the historical fixed 380: the frontend widens the window within a frame of
/// mounting whenever it needs more, so starting small means the very first
/// reveal never flashes a 380px-wide transparent click-capture band around a
/// 72px pill.
pub const DEFAULT_PILL_WIDTH_POINTS: f64 = 96.0;

/// Default pill window height (logical points) — the bare 34px capsule plus
/// vertical shadow/entrance-transform margin (PillApp.svelte's
/// MIN_PILL_WINDOW_H). Grows when the profile label floats above the pill
/// (see `pill_height_points`).
pub const DEFAULT_PILL_HEIGHT_POINTS: f64 = 54.0;

pub struct AppState {
    pub lifecycle: DictationLifecycle,
    pub target: WindowTarget,
    pub pill_placement: Option<PillPlacement>,
    pub pill_placement_stale: bool,
    /// Width (logical points / CSS px) the pill window should be sized to —
    /// reported by the frontend from the measured visible content so the
    /// transparent click-capture zone around the pill stays as small as the
    /// pill itself (see `commands::recording::set_pill_size`). Used by the
    /// placement math so reveals and cross-monitor moves size to the content
    /// instead of snapping back to the old fixed 380px.
    pub pill_width_points: f64,
    /// Height (logical points) the pill window should be sized to. Tracks the
    /// profile label floating above the capsule; the window grows upward so
    /// the pill itself stays visually pinned.
    pub pill_height_points: f64,
    pub retry_capture: Option<RetryCapture>,
    pub cancelled_capture: Option<CancelledCapture>,
    pub paste_failure: Option<PasteFailure>,
    /// Id of the in-flight durable dictation, if any.
    pub failover_session_id: Option<String>,
    /// When true, the next durable start reuses `failover_session_id` (resume).
    pub failover_reuse_id: bool,
    /// Wall-clock start of the current durable take, used when committing.
    pub failover_started_at_unix: i64,
}

/// Single source of truth for what the app is currently doing with the
/// microphone/pipeline. Every transition happens under one lock acquisition
/// so there is never a moment where this reads as `Idle` between two phases
/// that are actually still busy (that gap is exactly what let a second
/// recording start concurrently with an in-flight one).
pub enum DictationLifecycle {
    Idle,
    /// Reserved the instant a recording is decided on, before the mic/session
    /// actually exists — closes the "looks idle but isn't" installation gap
    /// for both a normal fresh press and an interrupt's replacement
    /// recording.
    Starting {
        prepend_audio: Option<CapturedAudio>,
    },
    Recording {
        session: audio::RecordingSession,
        exclusive_mic_session_id: Option<u64>,
        handless: bool,
        handless_from_hold: bool,
        prepend_audio: Option<CapturedAudio>,
    },
    /// Session handed off for `session.stop()` (blocking: denoise flush,
    /// resample, encode) — audio isn't available yet. Exists so `lifecycle`
    /// never reads as `Idle` between "recording stopped" and "processing
    /// reserved."
    Stopping {
        generation: u64,
        // Not read back out of this variant — the caller of
        // `take_recording_for_stopping` already has its own copy for the
        // actual merge. Kept here so the state itself stays self-descriptive
        // (a resumed dictation is visible mid-stopping, not just mid-merge).
        #[allow(dead_code)]
        prepend_audio: Option<CapturedAudio>,
    },
    Processing(ActivePipeline),
    Finalizing {
        generation: u64,
    },
}

impl DictationLifecycle {
    pub fn is_idle(&self) -> bool {
        matches!(self, DictationLifecycle::Idle)
    }
    pub fn is_recording(&self) -> bool {
        matches!(self, DictationLifecycle::Recording { .. })
    }
    pub fn is_handless_recording(&self) -> bool {
        matches!(self, DictationLifecycle::Recording { handless: true, .. })
    }
}

/// Compact, stable name for the lifecycle variant — used by transition logs
/// so a session is reconstructable from the ring buffer alone.
pub(super) fn describe_lifecycle(lifecycle: &DictationLifecycle) -> &'static str {
    match lifecycle {
        DictationLifecycle::Idle => "idle",
        DictationLifecycle::Starting { .. } => "starting",
        DictationLifecycle::Recording { handless, .. } if *handless => "recording_handsfree",
        DictationLifecycle::Recording { .. } => "recording",
        DictationLifecycle::Stopping { .. } => "stopping",
        DictationLifecycle::Processing(_) => "processing",
        DictationLifecycle::Finalizing { .. } => "finalizing",
    }
}

pub struct ActivePipeline {
    pub generation: u64,
    pub cancel_tx: tokio::sync::watch::Sender<bool>,
    pub captured_audio: CapturedAudio,
    // Not currently read back out — the pipeline task that owns this
    // generation already has its own copy of `target`. Kept for state
    // completeness/observability.
    #[allow(dead_code)]
    pub target: WindowTarget,
}

pub type SharedState = Arc<Mutex<AppState>>;

#[derive(Clone)]
pub struct RetryCapture {
    pub audio: CapturedAudio,
    pub captured_at: std::time::Instant,
    pub target: WindowTarget,
    pub process_name: String,
    pub context_id: i64,
    pub profile: String,
    pub app_context: Option<String>,
    pub caps_lock_on: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaptureOrigin {
    UserCancelled,
    Interrupted,
}

impl CaptureOrigin {
    pub fn as_str(self) -> &'static str {
        match self {
            CaptureOrigin::UserCancelled => "cancelled",
            CaptureOrigin::Interrupted => "interrupted",
        }
    }
}

/// Audio from a recording the user cancelled, kept around briefly so the
/// pill's "Cancelled" state can offer a Continue button that resumes
/// recording (hands-free) with this audio prepended — see
/// `peek_cancelled_capture_if_fresh`/`clear_cancelled_capture` and
/// `commands::recording::resume_cancelled_capture`.
#[derive(Clone)]
pub struct CancelledCapture {
    pub audio: CapturedAudio,
    pub captured_at: std::time::Instant,
    pub id: String,
    pub origin: CaptureOrigin,
    pub created_at_rfc3339: String,
    pub started_at_unix: i64,
    // The dictation's original focus target, captured when that recording
    // started. Resuming must reuse this rather than re-capturing the
    // foreground window: by the time the user clicks Undo, the foreground
    // window is Verenu's own pill (it just received a real click), not the
    // app the user was dictating into — see resume_cancelled_capture, which
    // used to call WindowTarget::capture_foreground() itself and always hit
    // that self-target, tripping finalize.rs's self-inject guard and
    // clipboard-only fallback on every resume.
    pub target: WindowTarget,
}

/// Final injected text from a dictation whose paste couldn't be confirmed
/// (or definitely failed), kept around briefly so the pill's "Paste failed"
/// state can offer a Copy button — see
/// `commands::recording::copy_paste_failure_to_clipboard`.
#[derive(Clone)]
pub struct PasteFailure {
    pub text: String,
    pub captured_at: std::time::Instant,
}

pub(super) fn lock_state(state: &SharedState) -> anyhow::Result<MutexGuard<'_, AppState>> {
    state
        .lock()
        .map_err(|_| anyhow::anyhow!("Recording state lock was poisoned"))
}

static PIPELINE_GENERATION: AtomicU64 = AtomicU64::new(0);

fn next_pipeline_generation() -> u64 {
    // Starts from 1 so 0 can stay the hook's "not processing" sentinel.
    PIPELINE_GENERATION.fetch_add(1, Ordering::SeqCst) + 1
}

/// `Idle -> Starting { prepend_audio: None }`. Fails if anything is already
/// in progress. Used by a normal fresh press and by the manual
/// `commands/recording.rs` entry points, which — unlike the hotkey's own
/// Press/Release channel — run as independent Tauri command tasks and so
/// genuinely need this reservation to avoid racing the hotkey path.
pub fn reserve_starting(state: &SharedState) -> Result<(), String> {
    let mut st = lock_state(state).map_err(|e| e.to_string())?;
    if !st.lifecycle.is_idle() {
        return Err("Already recording".to_string());
    }
    log::debug!(
        "lifecycle: {} -> starting",
        describe_lifecycle(&st.lifecycle)
    );
    st.lifecycle = DictationLifecycle::Starting {
        prepend_audio: None,
    };
    Ok(())
}

/// Reads the stashed cancelled capture (cloned) if present and fresh, without
/// consuming it — so a resume attempt that fails partway doesn't destroy the
/// resumable audio.
pub fn peek_cancelled_capture_if_fresh(state: &SharedState) -> Option<CancelledCapture> {
    let st = lock_state(state).ok()?;
    st.cancelled_capture.as_ref().and_then(|c| {
        if c.captured_at.elapsed() < CANCEL_RESUME_WINDOW {
            Some(c.clone())
        } else {
            None
        }
    })
}

/// Clears the stashed cancelled capture after it has been successfully
/// consumed by a resume.
pub fn clear_cancelled_capture(state: &SharedState) {
    if let Ok(mut st) = lock_state(state) {
        st.cancelled_capture = None;
    }
}

/// Expiry must release audio even if the user never invokes Retry or Resume.
/// Keep the small metadata record so existing expired-retry errors and durable
/// recovery bookkeeping retain their semantics. Other live owners keep their
/// Arc/Bytes handles, so an in-flight pipeline is unaffected.
pub fn release_expired_capture_audio(state: &SharedState) {
    fn release(audio: &mut CapturedAudio) {
        if !audio.wav.is_empty() || !audio.samples_16k.is_empty() {
            audio.wav = bytes::Bytes::new();
            audio.samples_16k = Arc::new(Vec::new());
        }
    }
    if let Ok(mut st) = lock_state(state) {
        if let Some(retry) = st.retry_capture.as_mut() {
            if retry.captured_at.elapsed() > RETRY_WINDOW {
                release(&mut retry.audio);
            }
        }
        if let Some(cancelled) = st.cancelled_capture.as_mut() {
            if cancelled.captured_at.elapsed() >= CANCEL_RESUME_WINDOW {
                release(&mut cancelled.audio);
            }
        }
    }
}

/// Attaches `audio` as the prepend audio of an already-reserved `Starting`
/// lifecycle (see `commands::recording::resume_cancelled_capture`).
pub fn set_starting_prepend_audio(state: &SharedState, audio: CapturedAudio) {
    if let Ok(mut st) = lock_state(state) {
        if let DictationLifecycle::Starting { prepend_audio } = &mut st.lifecycle {
            *prepend_audio = Some(audio);
        }
    }
}

/// `Processing(active) -> Starting { prepend_audio: Some(active.captured_audio) }`
/// in one step, so the old task loses ownership of its `ActivePipeline`
/// atomically with the replacement recording being reserved. Returns the
/// taken `ActivePipeline` (for signalling cancellation) if a pipeline was
/// actually in `Processing`; otherwise leaves `lifecycle` untouched and
/// returns `None`.
pub fn take_active_pipeline_for_interrupt(state: &SharedState) -> Option<ActivePipeline> {
    let mut st = lock_state(state).ok()?;
    match std::mem::replace(&mut st.lifecycle, DictationLifecycle::Idle) {
        DictationLifecycle::Processing(active) => {
            crate::core::hotkey::clear_processing_generation(active.generation);
            let prepend_audio = Some(active.captured_audio.clone());
            log::debug!(
                "lifecycle: processing -> starting gen={} (interrupt, audio prepended)",
                active.generation
            );
            st.lifecycle = DictationLifecycle::Starting { prepend_audio };
            Some(active)
        }
        other => {
            st.lifecycle = other;
            None
        }
    }
}

/// Takes `cancelled_capture`'s audio if present and still within
/// `CANCEL_RESUME_WINDOW`. Always clears the slot (a stale or already-taken
/// capture must not be resumable twice).
pub fn take_cancelled_capture_if_fresh(state: &SharedState) -> Option<CapturedAudio> {
    let mut st = lock_state(state).ok()?;
    st.cancelled_capture.take().and_then(|c| {
        if c.captured_at.elapsed() < CANCEL_RESUME_WINDOW {
            Some(c.audio)
        } else {
            None
        }
    })
}

/// Takes `paste_failure`'s text if present and still within
/// `PASTE_FAILURE_WINDOW`. Always clears the slot.
#[cfg_attr(not(test), allow(dead_code))]
pub fn take_paste_failure_if_fresh(state: &SharedState) -> Option<String> {
    take_paste_failure_if_fresh_at(state, std::time::Instant::now())
}

fn take_paste_failure_if_fresh_at(state: &SharedState, now: std::time::Instant) -> Option<String> {
    let mut st = lock_state(state).ok()?;
    st.paste_failure.take().and_then(|f| {
        if paste_failure_is_fresh(f.captured_at, now) {
            Some(f.text)
        } else {
            None
        }
    })
}

/// Reads `paste_failure`'s text if present and still within
/// `PASTE_FAILURE_WINDOW`, without clearing the slot — so a transient
/// clipboard failure can be retried instead of permanently losing the text.
pub fn peek_paste_failure_if_fresh(state: &SharedState) -> Option<String> {
    let st = lock_state(state).ok()?;
    st.paste_failure.as_ref().and_then(|f| {
        if paste_failure_is_fresh(f.captured_at, std::time::Instant::now()) {
            Some(f.text.clone())
        } else {
            None
        }
    })
}

/// Clears the `paste_failure` slot after it has been successfully handled.
pub fn clear_paste_failure(state: &SharedState) {
    if let Ok(mut st) = lock_state(state) {
        st.paste_failure = None;
    }
}

/// `Processing(active) -> Idle`, discarding the audio entirely (Escape
/// cancels outright, never appends). Returns the taken `ActivePipeline` (for
/// signalling cancellation) if one was present.
pub fn take_active_pipeline_for_escape(state: &SharedState) -> Option<ActivePipeline> {
    let mut st = lock_state(state).ok()?;
    match std::mem::replace(&mut st.lifecycle, DictationLifecycle::Idle) {
        DictationLifecycle::Processing(active) => {
            crate::core::hotkey::clear_processing_generation(active.generation);
            log::debug!(
                "lifecycle: processing -> idle gen={} (escape, audio discarded)",
                active.generation
            );
            Some(active)
        }
        other => {
            st.lifecycle = other;
            None
        }
    }
}

/// `Recording { .. } -> Idle`, discarding handless/prepend_audio state.
/// Used by plain cancel paths that never go through the transcribe/finalize
/// pipeline: the in-app mic button, calibration, a discarded quick-tap, or
/// Escape while still actively recording (pre-`Release`).
pub fn take_recording_plain(state: &SharedState) -> Option<(audio::RecordingSession, Option<u64>)> {
    let mut st = lock_state(state).ok()?;
    match std::mem::replace(&mut st.lifecycle, DictationLifecycle::Idle) {
        DictationLifecycle::Recording {
            session,
            exclusive_mic_session_id,
            ..
        } => {
            log::info!("lifecycle: recording -> idle (plain cancel/discard)");
            Some((session, exclusive_mic_session_id))
        }
        DictationLifecycle::Starting { .. } => {
            // Cancelling a start must consume the reservation atomically. If
            // the microphone task has not installed Recording yet, it will
            // observe Idle and stop the session instead of resurrecting it.
            None
        }
        other => {
            st.lifecycle = other;
            None
        }
    }
}

/// Cancels an in-flight start without touching any other lifecycle. This is
/// used by stop commands that can arrive before the microphone task finishes
/// opening the device.
pub fn cancel_starting_reservation(state: &SharedState) -> bool {
    let Ok(mut st) = lock_state(state) else {
        return false;
    };
    if matches!(st.lifecycle, DictationLifecycle::Starting { .. }) {
        log::info!("lifecycle: starting -> idle (start reservation cancelled)");
        st.lifecycle = DictationLifecycle::Idle;
        true
    } else {
        false
    }
}

/// `Recording { handless: false, .. } -> Recording { handless: true, .. }`.
/// Used when Space converts an active hold-to-talk session into hands-free
/// without stopping or restarting the microphone session.
pub fn promote_recording_to_handless(state: &SharedState) -> bool {
    let Ok(mut st) = lock_state(state) else {
        return false;
    };
    if let DictationLifecycle::Recording {
        handless,
        handless_from_hold,
        ..
    } = &mut st.lifecycle
    {
        if !*handless {
            log::info!("lifecycle: recording -> recording_handsfree (Space conversion)");
            *handless = true;
            *handless_from_hold = true;
            return true;
        }
    }
    false
}

/// Clears the conversion marker when the user begins a deliberate new
/// hands-free stop gesture. This keeps the original hold release suppressed
/// without disabling normal hands-free stopping afterward.
pub fn clear_handless_hold_marker(state: &SharedState) {
    if let Ok(mut st) = lock_state(state) {
        if let DictationLifecycle::Recording {
            handless: true,
            handless_from_hold,
            ..
        } = &mut st.lifecycle
        {
            *handless_from_hold = false;
        }
    }
}

/// Consumes one stale release/cancel generated by the original hold after
/// Space has already converted that recording to hands-free.
pub fn consume_handless_hold_stop(state: &SharedState) -> bool {
    let Ok(mut st) = lock_state(state) else {
        return false;
    };
    if let DictationLifecycle::Recording {
        handless: true,
        handless_from_hold,
        ..
    } = &mut st.lifecycle
    {
        if *handless_from_hold {
            *handless_from_hold = false;
            return true;
        }
    }
    false
}

/// Session/target/mic-id/generation/prepend-audio needed to finish stopping
/// a recording and capturing its audio.
pub(super) type StoppingHandoff = (
    audio::RecordingSession,
    WindowTarget,
    Option<u64>,
    u64,
    Option<CapturedAudio>,
);

/// `Recording { .. } -> Stopping { generation, prepend_audio }`, atomically.
/// Returns the session/target/mic-id/generation/prepend-audio needed to
/// finish stopping and capturing audio, or `None` if nothing was recording
/// (recording never started or was already consumed).
pub(super) fn take_recording_for_stopping(state: &SharedState) -> Option<StoppingHandoff> {
    let mut st = match lock_state(state) {
        Ok(st) => st,
        Err(e) => {
            log::error!("recording state: {e}");
            return None;
        }
    };
    let target = st.target;
    match std::mem::replace(&mut st.lifecycle, DictationLifecycle::Idle) {
        DictationLifecycle::Recording {
            session,
            exclusive_mic_session_id,
            handless: _,
            prepend_audio,
            ..
        } => {
            let generation = next_pipeline_generation();
            log::info!("lifecycle: recording -> stopping gen={generation}");
            st.lifecycle = DictationLifecycle::Stopping {
                generation,
                prepend_audio: prepend_audio.clone(),
            };
            Some((
                session,
                target,
                exclusive_mic_session_id,
                generation,
                prepend_audio,
            ))
        }
        DictationLifecycle::Starting { .. } => {
            // A release can race the microphone opening. Consume the start
            // reservation so the late opener cannot install a recording after
            // the user has already released the hotkey.
            None
        }
        other => {
            st.lifecycle = other;
            None
        }
    }
}

/// `Stopping { generation, .. } -> Idle`, only if still owned by `generation`
/// (a stale/superseded task must leave `lifecycle` untouched). Used on
/// quality-gate rejection.
pub(super) fn leave_stopping_if_owned(state: &SharedState, generation: u64) -> bool {
    let Ok(mut st) = lock_state(state) else {
        return false;
    };
    let owns = matches!(
        &st.lifecycle,
        DictationLifecycle::Stopping { generation: g, .. } if *g == generation
    );
    if owns {
        log::info!("lifecycle: stopping -> idle gen={generation} (rejected/discarded)");
        st.lifecycle = DictationLifecycle::Idle;
    }
    owns
}

/// `Stopping { generation, .. } -> Processing(active)`, only if still owned.
/// Also arms the hook's Escape gate for this generation.
pub(super) fn install_processing(
    state: &SharedState,
    generation: u64,
    active: ActivePipeline,
) -> bool {
    let Ok(mut st) = lock_state(state) else {
        return false;
    };
    let owns = matches!(
        &st.lifecycle,
        DictationLifecycle::Stopping { generation: g, .. } if *g == generation
    );
    if owns {
        log::info!("lifecycle: stopping -> processing gen={generation}");
        st.lifecycle = DictationLifecycle::Processing(active);
        crate::core::hotkey::set_processing_generation(generation);
    }
    owns
}

/// `Processing(active) -> Finalizing { generation }`, only if still owned —
/// the hard ownership check / point of no return. Also clears the hook's
/// Escape gate for this generation immediately (finalization can no longer
/// be cancelled, so Escape has nothing left to do here).
pub(super) fn enter_finalizing(state: &SharedState, generation: u64) -> bool {
    let Ok(mut st) = lock_state(state) else {
        return false;
    };
    let owns =
        matches!(&st.lifecycle, DictationLifecycle::Processing(a) if a.generation == generation);
    if owns {
        log::info!("lifecycle: processing -> finalizing gen={generation}");
        st.lifecycle = DictationLifecycle::Finalizing { generation };
        crate::core::hotkey::clear_processing_generation(generation);
    }
    owns
}

/// `Processing(active) -> Idle`, only if still owned. Used by every
/// non-happy-path exit from the processing stage (cancelled, failed) so
/// `lifecycle`/the hook's processing flag are never left stuck reporting
/// busy.
pub(super) fn leave_processing_if_owned(state: &SharedState, generation: u64) -> bool {
    let Ok(mut st) = lock_state(state) else {
        return false;
    };
    let owns =
        matches!(&st.lifecycle, DictationLifecycle::Processing(a) if a.generation == generation);
    if owns {
        log::info!("lifecycle: processing -> idle gen={generation} (failed/cancelled)");
        st.lifecycle = DictationLifecycle::Idle;
        crate::core::hotkey::clear_processing_generation(generation);
    }
    owns
}

/// `Finalizing { generation } -> Idle`, only if still owned (always true in
/// practice — nothing else can occupy `Finalizing` for this generation).
pub(super) fn leave_finalizing(state: &SharedState, generation: u64) {
    let Ok(mut st) = lock_state(state) else {
        return;
    };
    if matches!(&st.lifecycle, DictationLifecycle::Finalizing { generation: g } if *g == generation)
    {
        log::info!("lifecycle: finalizing -> idle gen={generation}");
        st.lifecycle = DictationLifecycle::Idle;
    }
}

pub(super) fn emit_pipeline_failed(app: &AppHandle) {
    app.emit(
        "verenu:pipeline-failed",
        Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true),
    )
    .ok();
}

/// Tells any open window (Home's history list in particular) that a
/// recording was just cancelled and its audio is resumable for
/// `CANCEL_RESUME_WINDOW`.
pub(super) fn emit_cancelled_capture(app: &AppHandle, capture: &CancelledCapture) {
    super::failover::emit_cancelled_payload(
        app,
        &capture.created_at_rfc3339,
        capture.origin.as_str(),
    );
}

/// Tells any open window that the current cancelled capture is gone —
/// resumed, explicitly dismissed, or expired — so a stale offer (pill toast
/// or Home banner) doesn't linger past its usefulness.
pub fn emit_cancelled_capture_cleared(app: &AppHandle) {
    app.emit("verenu:cancelled-capture-cleared", ()).ok();
}

/// Tells the frontend to re-poll provider status immediately rather than
/// waiting for its next 5-minute interval, because a pipeline call just
/// failed in a way that looks provider-side (quota or a retryable
/// timeout/429/5xx) rather than a local/config problem. The frontend
/// re-fetches the same filtered status it already polls periodically, so
/// this only surfaces something if the status API independently confirms an
/// issue with a provider the user actually has selected.
pub(super) fn emit_provider_recheck(app: &AppHandle) {
    app.emit("verenu:recheck-provider-status", ()).ok();
}

/// Tells the frontend to re-check connectivity immediately because a pipeline
/// call just failed with a connection error. The frontend re-runs its
/// `check_connectivity` poll so the persistent "No internet connection" toast
/// appears right away instead of on the next 60s interval.
pub(super) fn emit_connectivity_recheck(app: &AppHandle) {
    app.emit("verenu:recheck-connectivity", ()).ok();
}

/// Returns true if our own process currently owns the foreground window.
/// Catches the case where the user opened the Verenu main window while
/// transcribing — if we tried to Ctrl+V / Cmd+V in that state the paste would
/// land in our own WebView and silently disappear.
pub(super) fn foreground_is_own_process() -> bool {
    #[cfg(windows)]
    unsafe {
        use windows::Win32::UI::WindowsAndMessaging::{
            GetForegroundWindow, GetWindowThreadProcessId,
        };
        let hwnd = GetForegroundWindow();
        if hwnd.0.is_null() {
            return false;
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        pid == std::process::id()
    }
    #[cfg(not(windows))]
    {
        false
    }
}

/// Returns true if `hwnd` belongs to our own process.
/// Catches the case where recording was started while Verenu itself had focus.
#[cfg_attr(not(windows), allow(unused_variables))]
pub(super) fn hwnd_is_own_process(hwnd: usize) -> bool {
    if hwnd == 0 {
        return false;
    }
    #[cfg(windows)]
    unsafe {
        use windows::Win32::Foundation::HWND;
        use windows::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId;
        let mut pid = 0u32;
        GetWindowThreadProcessId(HWND(hwnd as *mut _), Some(&mut pid));
        pid == std::process::id()
    }
    #[cfg(not(windows))]
    {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh_state() -> SharedState {
        Arc::new(Mutex::new(AppState {
            lifecycle: DictationLifecycle::Idle,
            target: WindowTarget::default(),
            pill_placement: None,
            pill_placement_stale: false,
            pill_width_points: DEFAULT_PILL_WIDTH_POINTS,
            pill_height_points: DEFAULT_PILL_HEIGHT_POINTS,
            retry_capture: None,
            cancelled_capture: None,
            paste_failure: None,
            failover_session_id: None,
            failover_reuse_id: false,
            failover_started_at_unix: 0,
        }))
    }

    fn fake_audio(duration_ms: u64) -> CapturedAudio {
        CapturedAudio {
            wav: bytes::Bytes::new(),
            samples_16k: Arc::new(vec![0.0; 16]),
            sample_rate: 16_000,
            duration_ms,
        }
    }

    fn fake_active(generation: u64) -> ActivePipeline {
        let (cancel_tx, _rx) = tokio::sync::watch::channel(false);
        ActivePipeline {
            generation,
            cancel_tx,
            captured_audio: fake_audio(1000),
            target: WindowTarget::default(),
        }
    }

    #[test]
    fn reserve_starting_fails_unless_idle() {
        let state = fresh_state();
        assert!(reserve_starting(&state).is_ok());
        // Already Starting now — a second reservation must fail.
        assert!(reserve_starting(&state).is_err());
    }

    #[test]
    fn peek_cancelled_capture_does_not_consume() {
        let state = fresh_state();
        {
            let mut st = lock_state(&state).unwrap();
            st.cancelled_capture = Some(CancelledCapture {
                audio: fake_audio(500),
                captured_at: std::time::Instant::now(),
                id: "test-id".into(),
                origin: CaptureOrigin::UserCancelled,
                created_at_rfc3339: "2026-01-01T00:00:00Z".into(),
                started_at_unix: 0,
                target: WindowTarget::default(),
            });
        }
        // Peeking clones — the slot must survive for the later clear.
        assert_eq!(
            peek_cancelled_capture_if_fresh(&state)
                .unwrap()
                .audio
                .duration_ms,
            500
        );
        assert!(lock_state(&state).unwrap().cancelled_capture.is_some());
    }

    #[test]
    fn clear_cancelled_capture_removes_the_slot() {
        let state = fresh_state();
        {
            let mut st = lock_state(&state).unwrap();
            st.cancelled_capture = Some(CancelledCapture {
                audio: fake_audio(500),
                captured_at: std::time::Instant::now(),
                id: "test-id".into(),
                origin: CaptureOrigin::UserCancelled,
                created_at_rfc3339: "2026-01-01T00:00:00Z".into(),
                started_at_unix: 0,
                target: WindowTarget::default(),
            });
        }
        clear_cancelled_capture(&state);
        assert!(lock_state(&state).unwrap().cancelled_capture.is_none());
    }

    #[test]
    fn expired_audio_is_released_without_consuming_fresh_or_live_owners() {
        let state = fresh_state();
        let audio = fake_audio(500);
        let live_owner = audio.samples_16k.clone();
        {
            let mut st = lock_state(&state).unwrap();
            st.cancelled_capture = Some(CancelledCapture {
                audio: audio.clone(),
                captured_at: std::time::Instant::now(),
                id: "expiry-test".into(),
                origin: CaptureOrigin::UserCancelled,
                created_at_rfc3339: "2026-01-01T00:00:00Z".into(),
                started_at_unix: 0,
                target: WindowTarget::default(),
            });
            st.retry_capture = Some(RetryCapture {
                audio,
                captured_at: std::time::Instant::now()
                    - RETRY_WINDOW
                    - std::time::Duration::from_secs(1),
                target: WindowTarget::default(),
                process_name: String::new(),
                context_id: 1,
                profile: String::new(),
                app_context: None,
                caps_lock_on: false,
            });
        }
        release_expired_capture_audio(&state);
        {
            let mut st = lock_state(&state).unwrap();
            assert!(st
                .retry_capture
                .as_ref()
                .unwrap()
                .audio
                .samples_16k
                .is_empty());
            let capture = st.cancelled_capture.as_mut().unwrap();
            assert_eq!(capture.audio.samples_16k.len(), 16);
            capture.captured_at = std::time::Instant::now() - CANCEL_RESUME_WINDOW;
        }
        release_expired_capture_audio(&state);
        assert_eq!(Arc::strong_count(&live_owner), 1);
        assert_eq!(live_owner.len(), 16);
        assert!(peek_cancelled_capture_if_fresh(&state).is_none());
        assert!(lock_state(&state).unwrap().retry_capture.is_some());
    }

    #[test]
    fn set_starting_prepend_audio_attaches_to_reserved_start() {
        let state = fresh_state();
        reserve_starting(&state).unwrap();
        set_starting_prepend_audio(&state, fake_audio(500));
        match &lock_state(&state).unwrap().lifecycle {
            DictationLifecycle::Starting { prepend_audio } => {
                assert_eq!(prepend_audio.as_ref().unwrap().duration_ms, 500)
            }
            _ => panic!("expected Starting with prepend_audio"),
        };
    }

    #[test]
    fn take_paste_failure_if_fresh_returns_text_once() {
        let state = fresh_state();
        {
            let mut st = lock_state(&state).unwrap();
            st.paste_failure = Some(PasteFailure {
                text: "hello world".to_string(),
                captured_at: std::time::Instant::now(),
            });
        }
        assert_eq!(
            take_paste_failure_if_fresh(&state),
            Some("hello world".to_string())
        );
        // Already taken — a second call must return None.
        assert_eq!(take_paste_failure_if_fresh(&state), None);
    }

    #[test]
    fn take_paste_failure_if_fresh_expires_after_window() {
        let state = fresh_state();
        let captured_at = std::time::Instant::now();
        let now = captured_at + PASTE_FAILURE_WINDOW + std::time::Duration::from_secs(1);
        {
            let mut st = lock_state(&state).unwrap();
            st.paste_failure = Some(PasteFailure {
                text: "stale".to_string(),
                captured_at,
            });
        }
        assert_eq!(take_paste_failure_if_fresh_at(&state, now), None);
    }

    #[test]
    fn take_recording_plain_cancels_a_pending_start() {
        let state = fresh_state();
        reserve_starting(&state).unwrap();

        assert!(take_recording_plain(&state).is_none());
        assert!(lock_state(&state).unwrap().lifecycle.is_idle());
    }

    #[test]
    fn take_recording_for_stopping_cancels_a_pending_start() {
        let state = fresh_state();
        reserve_starting(&state).unwrap();

        assert!(take_recording_for_stopping(&state).is_none());
        assert!(lock_state(&state).unwrap().lifecycle.is_idle());
    }

    #[test]
    fn cancel_starting_reservation_does_not_touch_other_lifecycle() {
        let state = fresh_state();
        assert!(!cancel_starting_reservation(&state));
        assert!(lock_state(&state).unwrap().lifecycle.is_idle());

        reserve_starting(&state).unwrap();
        assert!(cancel_starting_reservation(&state));
        assert!(lock_state(&state).unwrap().lifecycle.is_idle());
    }

    #[test]
    fn interrupt_takes_processing_and_installs_starting_with_prepend_audio() {
        let state = fresh_state();
        {
            let mut st = lock_state(&state).unwrap();
            st.lifecycle = DictationLifecycle::Processing(fake_active(1));
        }
        let taken = take_active_pipeline_for_interrupt(&state);
        assert!(taken.is_some());
        let st = lock_state(&state).unwrap();
        match &st.lifecycle {
            DictationLifecycle::Starting { prepend_audio } => {
                assert!(prepend_audio.is_some());
            }
            _ => panic!("expected Starting with prepend_audio"),
        }
    }

    #[test]
    fn interrupt_is_a_noop_outside_processing() {
        let state = fresh_state();
        assert!(take_active_pipeline_for_interrupt(&state).is_none());
        let st = lock_state(&state).unwrap();
        assert!(st.lifecycle.is_idle());
    }

    #[test]
    fn escape_takes_processing_and_discards_audio_into_idle() {
        let state = fresh_state();
        {
            let mut st = lock_state(&state).unwrap();
            st.lifecycle = DictationLifecycle::Processing(fake_active(1));
        }
        let taken = take_active_pipeline_for_escape(&state);
        assert!(taken.is_some());
        let st = lock_state(&state).unwrap();
        assert!(st.lifecycle.is_idle());
    }

    #[test]
    fn install_processing_rejects_stale_generation() {
        let state = fresh_state();
        {
            let mut st = lock_state(&state).unwrap();
            st.lifecycle = DictationLifecycle::Stopping {
                generation: 5,
                prepend_audio: None,
            };
        }
        // A stale task for generation 4 must not be able to install itself
        // over the current (5) reservation.
        assert!(!install_processing(&state, 4, fake_active(4)));
        let st = lock_state(&state).unwrap();
        assert!(matches!(
            &st.lifecycle,
            DictationLifecycle::Stopping { generation: 5, .. }
        ));
    }

    #[test]
    fn install_processing_succeeds_for_matching_generation() {
        let state = fresh_state();
        {
            let mut st = lock_state(&state).unwrap();
            st.lifecycle = DictationLifecycle::Stopping {
                generation: 7,
                prepend_audio: None,
            };
        }
        assert!(install_processing(&state, 7, fake_active(7)));
        let st = lock_state(&state).unwrap();
        assert!(matches!(&st.lifecycle, DictationLifecycle::Processing(a) if a.generation == 7));
    }

    #[test]
    fn enter_finalizing_and_leave_processing_are_generation_checked() {
        let state = fresh_state();
        {
            let mut st = lock_state(&state).unwrap();
            st.lifecycle = DictationLifecycle::Processing(fake_active(9));
        }
        // A superseded task's cleanup for the wrong generation must be a no-op.
        assert!(!leave_processing_if_owned(&state, 8));
        assert!(!enter_finalizing(&state, 8));
        {
            let st = lock_state(&state).unwrap();
            assert!(
                matches!(&st.lifecycle, DictationLifecycle::Processing(a) if a.generation == 9)
            );
        }
        // The owning generation succeeds and transitions to Finalizing.
        assert!(enter_finalizing(&state, 9));
        let st = lock_state(&state).unwrap();
        assert!(matches!(
            &st.lifecycle,
            DictationLifecycle::Finalizing { generation: 9 }
        ));
    }

    #[test]
    fn leave_finalizing_only_clears_matching_generation() {
        let state = fresh_state();
        {
            let mut st = lock_state(&state).unwrap();
            st.lifecycle = DictationLifecycle::Finalizing { generation: 3 };
        }
        leave_finalizing(&state, 2);
        {
            let st = lock_state(&state).unwrap();
            assert!(matches!(
                &st.lifecycle,
                DictationLifecycle::Finalizing { generation: 3 }
            ));
        }
        leave_finalizing(&state, 3);
        let st = lock_state(&state).unwrap();
        assert!(st.lifecycle.is_idle());
    }

    #[test]
    fn lifecycle_idle_reflects_state() {
        let state = fresh_state();
        assert!(lock_state(&state).unwrap().lifecycle.is_idle());
        reserve_starting(&state).unwrap();
        assert!(!lock_state(&state).unwrap().lifecycle.is_idle());
    }
}
