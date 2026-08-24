use crate::core::text_context::SentenceContext;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectionState {
    CollapsedCaret,
    NonCollapsedSelection,
    Unknown,
}

impl SelectionState {
    pub fn as_str(self) -> &'static str {
        match self {
            SelectionState::CollapsedCaret => "collapsed_caret",
            SelectionState::NonCollapsedSelection => "non_collapsed_selection",
            SelectionState::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum ContextProbeSource {
    CaretLocal,
    EmptyField,
    AmbiguousSelection,
    PermissionMissing,
    UnsupportedControl,
    Unavailable,
}

impl ContextProbeSource {
    pub fn as_str(self) -> &'static str {
        match self {
            ContextProbeSource::CaretLocal => "caret_local",
            ContextProbeSource::EmptyField => "empty_field",
            ContextProbeSource::AmbiguousSelection => "ambiguous_selection",
            ContextProbeSource::PermissionMissing => "permission_missing",
            ContextProbeSource::UnsupportedControl => "unsupported_control",
            ContextProbeSource::Unavailable => "unavailable",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InjectionContextProbe {
    pub context: SentenceContext,
    pub source: ContextProbeSource,
    pub context_tail: String,
    /// Text immediately after the caret, or after the end of a selection.
    pub context_head: String,
    /// Whether an empty or non-empty left edge was confirmed by the platform.
    pub left_reliable: bool,
    /// Whether an empty or non-empty right edge was confirmed by the platform.
    pub right_reliable: bool,
    pub selection_state: SelectionState,
    pub control_identity_hash: String,
    pub control_type: String,
    /// Owning process id for validating that the probe still belongs to the
    /// window captured when dictation started.
    pub target_id: usize,
}

impl InjectionContextProbe {
    pub fn unavailable(source: ContextProbeSource, control_type: impl Into<String>) -> Self {
        Self {
            context: SentenceContext::Unknown,
            source,
            context_tail: String::new(),
            context_head: String::new(),
            left_reliable: false,
            right_reliable: false,
            selection_state: SelectionState::Unknown,
            control_identity_hash: source.as_str().to_string(),
            control_type: control_type.into(),
            target_id: 0,
        }
    }
}

pub(crate) fn stable_metadata_hash(parts: &[&str]) -> String {
    const FNV_OFFSET_BASIS: u64 = 0xcbf29ce484222325;
    const FNV_PRIME: u64 = 0x100000001b3;

    let mut hash = FNV_OFFSET_BASIS;
    for part in parts {
        for byte in part.as_bytes() {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(FNV_PRIME);
        }
        hash ^= u64::from(b'|');
        hash = hash.wrapping_mul(FNV_PRIME);
    }

    format!("{hash:016x}")
}

pub(crate) fn resolve_context_from_tail(
    field_empty: bool,
    caret_context: Option<&str>,
) -> SentenceContext {
    if field_empty {
        SentenceContext::NewSentence
    } else if let Some(text) = caret_context {
        crate::core::text_context::classify_context_tail(text)
    } else {
        SentenceContext::Unknown
    }
}

#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn describe_selection_state(range_seen: bool, range_collapsed: bool) -> SelectionState {
    if range_seen && range_collapsed {
        SelectionState::CollapsedCaret
    } else if range_seen {
        SelectionState::NonCollapsedSelection
    } else {
        SelectionState::Unknown
    }
}

#[cfg(target_os = "macos")]
const MACOS_PROBE_TIMEOUT_MS: u64 = 120;

#[cfg(target_os = "macos")]
static BASE_INSTANT: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();

#[cfg(target_os = "macos")]
fn get_monotonic_ms() -> u64 {
    let base = *BASE_INSTANT.get_or_init(std::time::Instant::now);
    (std::time::Instant::now()
        .duration_since(base)
        .as_millis()
        .min(u64::MAX as u128) as u64)
        .saturating_add(1)
}

#[cfg(target_os = "macos")]
static MACOS_PROBE_START_TIME: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

#[cfg(target_os = "macos")]
struct MacosProbeGuard {
    start_time: u64,
}

#[cfg(target_os = "macos")]
impl MacosProbeGuard {
    fn acquire() -> Option<Self> {
        use std::sync::atomic::Ordering;

        let now = get_monotonic_ms();

        // Retry loop: if a concurrent probe is released between our load and the
        // CAS, the CAS fails with the new (zero) value; loop to re-evaluate
        // instead of incorrectly reporting busy.
        let mut active_time = MACOS_PROBE_START_TIME.load(Ordering::SeqCst);
        loop {
            if active_time != 0 {
                // Block if the active probe hasn't timed out. Also block when
                // now < active_time: a thread with a later timestamp already
                // acquired the guard between our load and CAS — treat it as live.
                if now < active_time || now - active_time < 2000 {
                    return None;
                }
            }

            match MACOS_PROBE_START_TIME.compare_exchange(
                active_time,
                now,
                Ordering::SeqCst,
                Ordering::SeqCst,
            ) {
                Ok(_) => return Some(Self { start_time: now }),
                Err(actual) => active_time = actual,
            }
        }
    }
}

#[cfg(target_os = "macos")]
impl Drop for MacosProbeGuard {
    fn drop(&mut self) {
        use std::sync::atomic::Ordering;
        let _ = MACOS_PROBE_START_TIME.compare_exchange(
            self.start_time,
            0,
            Ordering::SeqCst,
            Ordering::SeqCst,
        );
    }
}

pub async fn read_injection_context_probe() -> InjectionContextProbe {
    #[cfg(windows)]
    {
        crate::api::auto_learn::read_injection_context_probe()
    }

    #[cfg(target_os = "macos")]
    {
        let Some(guard) = MacosProbeGuard::acquire() else {
            return InjectionContextProbe::unavailable(
                ContextProbeSource::Unavailable,
                "probe_busy",
            );
        };

        // Move the guard into the background task thread. This holds the active lock
        // state while the AX call runs, preventing re-entrant AX calls on short timeouts.
        let task = tokio::task::spawn_blocking(move || {
            let _guard = guard;
            crate::core::context_probe_macos::read_injection_context_probe_sync()
        });

        match tokio::time::timeout(
            tokio::time::Duration::from_millis(MACOS_PROBE_TIMEOUT_MS),
            task,
        )
        .await
        {
            Ok(Ok(probe)) => probe,
            Ok(Err(err)) => {
                log::debug!("context probe join failed: {err}");
                InjectionContextProbe::unavailable(
                    ContextProbeSource::Unavailable,
                    "probe_join_failed",
                )
            }
            Err(_) => {
                log::debug!("context probe timed out after {MACOS_PROBE_TIMEOUT_MS}ms");
                InjectionContextProbe::unavailable(ContextProbeSource::Unavailable, "probe_timeout")
            }
        }
    }

    #[cfg(not(any(windows, target_os = "macos")))]
    {
        InjectionContextProbe::unavailable(ContextProbeSource::Unavailable, "unavailable")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_field_resolves_to_new_sentence() {
        assert_eq!(
            resolve_context_from_tail(true, None),
            SentenceContext::NewSentence
        );
    }
}
