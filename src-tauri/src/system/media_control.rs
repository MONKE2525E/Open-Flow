#[cfg(any(test, windows))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum MediaPlaybackState {
    Closed,
    Opened,
    Changing,
    Stopped,
    Playing,
    Paused,
}

#[cfg(any(test, windows))]
pub(crate) fn should_pause_for_dictation(state: MediaPlaybackState) -> bool {
    matches!(state, MediaPlaybackState::Playing)
}

#[cfg(any(test, windows))]
pub(crate) fn should_resume_after_dictation(state: MediaPlaybackState) -> bool {
    !matches!(
        state,
        MediaPlaybackState::Stopped | MediaPlaybackState::Closed
    )
}

pub struct DictationMediaPauseGuard {
    generation: Option<u64>,
}

impl DictationMediaPauseGuard {
    pub fn new() -> Self {
        // Input transcription runs after the recording has already started
        // the pause. Keep that exact generation so an older async task cannot
        // end a newer recording's media pause when it eventually completes.
        Self {
            generation: platform::active_generation(),
        }
    }
}

impl Drop for DictationMediaPauseGuard {
    fn drop(&mut self) {
        if let Some(generation) = self.generation.take() {
            end_dictation_media_pause_if_current(generation);
        }
    }
}

#[cfg(any(test, windows))]
fn guard_owns_active_pause(is_active: bool, active_generation: u64, guard_generation: u64) -> bool {
    is_active && active_generation == guard_generation
}

#[cfg(windows)]
mod platform {
    use super::{should_pause_for_dictation, should_resume_after_dictation, MediaPlaybackState};
    use std::sync::Mutex;
    use windows::Media::Control::{
        GlobalSystemMediaTransportControlsSession,
        GlobalSystemMediaTransportControlsSessionManager,
        GlobalSystemMediaTransportControlsSessionPlaybackStatus,
    };

    struct PauseState {
        generation: u64,
        paused_sessions: Vec<PausedSession>,
        is_active: bool,
    }

    struct PausedSession {
        source_app_id: String,
        session: GlobalSystemMediaTransportControlsSession,
    }

    static PAUSE_STATE: Mutex<PauseState> = Mutex::new(PauseState {
        generation: 0,
        paused_sessions: Vec::new(),
        is_active: false,
    });

    /// WinRT session APIs require the calling thread to be in a COM apartment.
    /// `spawn_blocking` hands these closures a fresh thread-pool thread, which
    /// has no apartment by default, so each closure must init its own.
    struct ComGuard(bool);

    impl ComGuard {
        fn new() -> Self {
            let initialized = unsafe {
                windows::Win32::System::Com::CoInitializeEx(
                    None,
                    windows::Win32::System::Com::COINIT_MULTITHREADED,
                )
            }
            .is_ok();
            ComGuard(initialized)
        }
    }

    impl Drop for ComGuard {
        fn drop(&mut self) {
            if self.0 {
                unsafe {
                    windows::Win32::System::Com::CoUninitialize();
                }
            }
        }
    }

    pub fn begin() {
        let (generation, stale_sessions) = {
            let mut state = match PAUSE_STATE.lock() {
                Ok(state) => state,
                Err(poisoned) => {
                    log::warn!("media pause state lock was poisoned; recovering");
                    poisoned.into_inner()
                }
            };
            state.generation = state.generation.wrapping_add(1);
            state.is_active = true;
            let stale_sessions = std::mem::take(&mut state.paused_sessions);
            (state.generation, stale_sessions)
        };

        tauri::async_runtime::spawn_blocking(move || {
            let _com_guard = ComGuard::new();
            if !stale_sessions.is_empty() {
                log::debug!("media pause: restoring stale sessions before generation={generation}");
                restore_sessions(stale_sessions);
            }
            if let Err(err) = pause_playing_sessions(generation) {
                log::warn!("media pause: failed to inspect or pause sessions: {err}");
            }
        });
    }

    pub fn end() {
        let sessions = {
            let mut state = match PAUSE_STATE.lock() {
                Ok(state) => state,
                Err(poisoned) => {
                    log::warn!("media pause state lock was poisoned; recovering");
                    poisoned.into_inner()
                }
            };
            state.generation = state.generation.wrapping_add(1);
            state.is_active = false;
            std::mem::take(&mut state.paused_sessions)
        };

        if sessions.is_empty() {
            return;
        }

        tauri::async_runtime::spawn_blocking(move || {
            let _com_guard = ComGuard::new();
            restore_sessions(sessions)
        });
    }

    pub fn active_generation() -> Option<u64> {
        let state = match PAUSE_STATE.lock() {
            Ok(state) => state,
            Err(poisoned) => {
                log::warn!("media pause state lock was poisoned; recovering");
                poisoned.into_inner()
            }
        };
        state.is_active.then_some(state.generation)
    }

    pub fn end_if_current(generation: u64) {
        let sessions = {
            let mut state = match PAUSE_STATE.lock() {
                Ok(state) => state,
                Err(poisoned) => {
                    log::warn!("media pause state lock was poisoned; recovering");
                    poisoned.into_inner()
                }
            };
            if !super::guard_owns_active_pause(state.is_active, state.generation, generation) {
                return;
            }
            state.generation = state.generation.wrapping_add(1);
            state.is_active = false;
            std::mem::take(&mut state.paused_sessions)
        };

        if sessions.is_empty() {
            return;
        }

        tauri::async_runtime::spawn_blocking(move || {
            let _com_guard = ComGuard::new();
            restore_sessions(sessions)
        });
    }

    fn pause_playing_sessions(generation: u64) -> windows::core::Result<()> {
        let manager = GlobalSystemMediaTransportControlsSessionManager::RequestAsync()?.get()?;
        let sessions = manager.GetSessions()?;
        let total = sessions.Size()?;
        let mut inspected = 0_u32;
        let mut paused = 0_u32;

        for index in 0..total {
            if !is_current_generation(generation) {
                break;
            }

            let session = match sessions.GetAt(index) {
                Ok(session) => session,
                Err(err) => {
                    log::debug!("media pause: failed to read session index={index}: {err}");
                    continue;
                }
            };
            inspected += 1;

            let playback_state = match playback_state(&session) {
                Ok(state) => state,
                Err(err) => {
                    log::debug!("media pause: failed to read playback state: {err}");
                    continue;
                }
            };
            if !should_pause_for_dictation(playback_state) {
                continue;
            }

            let source_app_id = session
                .SourceAppUserModelId()
                .map(|value| value.to_string_lossy())
                .unwrap_or_else(|_| "unknown".to_string());

            match session.TryPauseAsync() {
                Ok(operation) => match operation.get() {
                    Ok(true) => {
                        paused += 1;
                        let restore_immediately = !store_paused_session(
                            generation,
                            PausedSession {
                                source_app_id: source_app_id.clone(),
                                session: session.clone(),
                            },
                        );
                        log::debug!("media pause: paused source_app_id={source_app_id}");

                        if restore_immediately {
                            // The session was just told to pause moments ago; its
                            // playback status may not have caught up to `Paused`
                            // yet, so skip the staleness check that gates a normal
                            // end-of-dictation restore and resume unconditionally.
                            restore_session(
                                PausedSession {
                                    source_app_id,
                                    session,
                                },
                                true,
                            );
                        }
                    }
                    Ok(false) => {
                        log::debug!(
                            "media pause: session rejected pause source_app_id={source_app_id}"
                        );
                    }
                    Err(err) => {
                        log::debug!(
                                "media pause: pause command failed source_app_id={source_app_id}: {err}"
                            );
                    }
                },
                Err(err) => {
                    log::debug!(
                        "media pause: failed to create pause command source_app_id={source_app_id}: {err}"
                    );
                }
            }
        }

        log::debug!("media pause: inspected={inspected} paused={paused}");
        Ok(())
    }

    fn is_current_generation(generation: u64) -> bool {
        let guard = match PAUSE_STATE.lock() {
            Ok(state) => state,
            Err(poisoned) => {
                log::warn!("media pause state lock was poisoned; recovering");
                poisoned.into_inner()
            }
        };
        guard.generation == generation
    }

    fn store_paused_session(generation: u64, session: PausedSession) -> bool {
        let mut state = match PAUSE_STATE.lock() {
            Ok(state) => state,
            Err(poisoned) => {
                log::warn!("media pause state lock was poisoned; recovering");
                poisoned.into_inner()
            }
        };
        // A late-arriving TryPauseAsync confirmation from an older generation
        // can resolve after a newer dictation session has already started.
        // As long as some dictation is still active, hand the session off to
        // it instead of force-resuming media a still-running session needs
        // kept paused.
        if state.is_active && state.generation >= generation {
            state.paused_sessions.push(session);
            true
        } else {
            false
        }
    }

    fn restore_sessions(sessions: Vec<PausedSession>) {
        let total = sessions.len();
        let mut resumed = 0_usize;

        for session in sessions {
            if restore_session(session, false) {
                resumed += 1;
            }
        }

        log::debug!("media pause: restore attempted={total} resumed={resumed}");
    }

    fn restore_session(session: PausedSession, force: bool) -> bool {
        if !force {
            match playback_state(&session.session) {
                Ok(state) if should_resume_after_dictation(state) => {}
                Ok(_) => return false,
                Err(err) => {
                    log::debug!(
                        "media pause: failed to read restore state source_app_id={}: {err}",
                        session.source_app_id
                    );
                    return false;
                }
            }
        }

        match session.session.TryPlayAsync() {
            Ok(operation) => match operation.get() {
                Ok(true) => {
                    log::debug!(
                        "media pause: resumed source_app_id={}",
                        session.source_app_id
                    );
                    true
                }
                Ok(false) => {
                    log::debug!(
                        "media pause: session rejected resume source_app_id={}",
                        session.source_app_id
                    );
                    false
                }
                Err(err) => {
                    log::debug!(
                        "media pause: resume command failed source_app_id={}: {err}",
                        session.source_app_id
                    );
                    false
                }
            },
            Err(err) => {
                log::debug!(
                    "media pause: failed to create resume command source_app_id={}: {err}",
                    session.source_app_id
                );
                false
            }
        }
    }

    fn playback_state(
        session: &GlobalSystemMediaTransportControlsSession,
    ) -> windows::core::Result<MediaPlaybackState> {
        let status = session.GetPlaybackInfo()?.PlaybackStatus()?;
        Ok(match status {
            GlobalSystemMediaTransportControlsSessionPlaybackStatus::Closed => {
                MediaPlaybackState::Closed
            }
            GlobalSystemMediaTransportControlsSessionPlaybackStatus::Opened => {
                MediaPlaybackState::Opened
            }
            GlobalSystemMediaTransportControlsSessionPlaybackStatus::Changing => {
                MediaPlaybackState::Changing
            }
            GlobalSystemMediaTransportControlsSessionPlaybackStatus::Stopped => {
                MediaPlaybackState::Stopped
            }
            GlobalSystemMediaTransportControlsSessionPlaybackStatus::Playing => {
                MediaPlaybackState::Playing
            }
            GlobalSystemMediaTransportControlsSessionPlaybackStatus::Paused => {
                MediaPlaybackState::Paused
            }
            _ => MediaPlaybackState::Stopped,
        })
    }
}

#[cfg(not(windows))]
mod platform {
    pub fn begin() {}
    pub fn end() {}
    pub fn active_generation() -> Option<u64> {
        None
    }
    pub fn end_if_current(_generation: u64) {}
}

pub fn begin_dictation_media_pause() {
    platform::begin();
}

pub fn end_dictation_media_pause() {
    platform::end();
}

fn end_dictation_media_pause_if_current(generation: u64) {
    platform::end_if_current(generation);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_playing_sessions_are_paused() {
        assert!(should_pause_for_dictation(MediaPlaybackState::Playing));
        assert!(!should_pause_for_dictation(MediaPlaybackState::Paused));
        assert!(!should_pause_for_dictation(MediaPlaybackState::Stopped));
        assert!(!should_pause_for_dictation(MediaPlaybackState::Closed));
    }

    #[test]
    fn older_guard_cannot_end_a_newer_media_pause() {
        assert!(guard_owns_active_pause(true, 7, 7));
        assert!(!guard_owns_active_pause(true, 8, 7));
        assert!(!guard_owns_active_pause(false, 7, 7));
    }

    #[test]
    fn resume_is_skipped_only_for_terminal_sessions() {
        // Anything short of Stopped/Closed is resumed: state can lag behind
        // an actual pause (or the session can drift to Playing/Changing by
        // the time we check), so gating strictly on Paused risked leaving
        // media paused forever. Resuming an already-playing session is a
        // harmless no-op, not a fight with a user who resumed it manually.
        assert!(should_resume_after_dictation(MediaPlaybackState::Paused));
        assert!(should_resume_after_dictation(MediaPlaybackState::Playing));
        assert!(should_resume_after_dictation(MediaPlaybackState::Changing));
        assert!(should_resume_after_dictation(MediaPlaybackState::Opened));
        assert!(!should_resume_after_dictation(MediaPlaybackState::Stopped));
        assert!(!should_resume_after_dictation(MediaPlaybackState::Closed));
    }

    #[test]
    fn restore_requires_successful_pause_and_non_terminal_session_state() {
        fn would_restore(
            initial_state: MediaPlaybackState,
            pause_succeeded: bool,
            restore_state: MediaPlaybackState,
        ) -> bool {
            let stored = should_pause_for_dictation(initial_state) && pause_succeeded;
            stored && should_resume_after_dictation(restore_state)
        }

        assert!(would_restore(
            MediaPlaybackState::Playing,
            true,
            MediaPlaybackState::Paused
        ));
        // A session that drifted back to Playing by restore time is still
        // resumed - TryPlayAsync on an already-playing session is a no-op.
        assert!(would_restore(
            MediaPlaybackState::Playing,
            true,
            MediaPlaybackState::Playing
        ));
        assert!(!would_restore(
            MediaPlaybackState::Paused,
            true,
            MediaPlaybackState::Paused
        ));
        assert!(!would_restore(
            MediaPlaybackState::Playing,
            false,
            MediaPlaybackState::Paused
        ));
        assert!(!would_restore(
            MediaPlaybackState::Playing,
            true,
            MediaPlaybackState::Stopped
        ));
    }
}
