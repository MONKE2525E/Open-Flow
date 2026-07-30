use super::*;
use crate::core::window_geometry::WindowTarget;
use crate::pipeline::pill_position::PillPlacement;
use std::sync::atomic::AtomicU64;

pub(super) const RETRY_WINDOW: std::time::Duration = std::time::Duration::from_secs(600);
// ---------- shared state ----------

pub struct AppState {
    pub lifecycle: DictationLifecycle,
    pub target: WindowTarget,
    pub pill_placement: Option<PillPlacement>,
    pub pill_placement_stale: bool,
    pub retry_capture: Option<RetryCapture>,
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
    pub profile: String,
    pub app_context: Option<String>,
    pub caps_lock_on: bool,
}

pub(super) fn lock_state(state: &SharedState) -> anyhow::Result<MutexGuard<'_, AppState>> {
    state
        .lock()
        .map_err(|_| anyhow::anyhow!("Recording state lock was poisoned"))
}

/// Whether nothing is currently recording/processing/finalizing. Used by
/// entry points that must not race the primary hotkey-driven pipeline
/// (e.g. `retry_transcription`, the in-app mic button's manual commands).
pub fn is_idle(state: &SharedState) -> bool {
    lock_state(state)
        .map(|st| st.lifecycle.is_idle())
        .unwrap_or(false)
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
    st.lifecycle = DictationLifecycle::Starting {
        prepend_audio: None,
    };
    Ok(())
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
            st.lifecycle = DictationLifecycle::Starting { prepend_audio };
            Some(active)
        }
        other => {
            st.lifecycle = other;
            None
        }
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
        } => Some((session, exclusive_mic_session_id)),
        other => {
            st.lifecycle = other;
            None
        }
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
            retry_capture: None,
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
    fn is_idle_reflects_lifecycle() {
        let state = fresh_state();
        assert!(is_idle(&state));
        reserve_starting(&state).unwrap();
        assert!(!is_idle(&state));
    }
}
