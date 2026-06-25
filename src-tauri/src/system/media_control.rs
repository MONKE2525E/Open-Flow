#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum MediaPlaybackState {
    Closed,
    Opened,
    Changing,
    Stopped,
    Playing,
    Paused,
}

pub(crate) fn should_pause_for_dictation(state: MediaPlaybackState) -> bool {
    matches!(state, MediaPlaybackState::Playing)
}

pub(crate) fn should_resume_after_dictation(state: MediaPlaybackState) -> bool {
    matches!(state, MediaPlaybackState::Paused)
}

pub struct DictationMediaPauseGuard {
    active: bool,
}

impl DictationMediaPauseGuard {
    pub fn new() -> Self {
        Self { active: true }
    }
}

impl Drop for DictationMediaPauseGuard {
    fn drop(&mut self) {
        if self.active {
            end_dictation_media_pause();
            self.active = false;
        }
    }
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
    }

    struct PausedSession {
        source_app_id: String,
        session: GlobalSystemMediaTransportControlsSession,
    }

    static PAUSE_STATE: Mutex<PauseState> = Mutex::new(PauseState {
        generation: 0,
        paused_sessions: Vec::new(),
    });

    pub fn begin() {
        let (generation, stale_sessions) = match PAUSE_STATE.lock() {
            Ok(mut state) => {
                state.generation = state.generation.wrapping_add(1);
                let stale_sessions = std::mem::take(&mut state.paused_sessions);
                (state.generation, stale_sessions)
            }
            Err(poisoned) => {
                log::warn!("media pause state lock was poisoned; recovering");
                let mut state = poisoned.into_inner();
                state.generation = state.generation.wrapping_add(1);
                let stale_sessions = std::mem::take(&mut state.paused_sessions);
                (state.generation, stale_sessions)
            }
        };

        tauri::async_runtime::spawn_blocking(move || {
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
        let sessions = match PAUSE_STATE.lock() {
            Ok(mut state) => {
                state.generation = state.generation.wrapping_add(1);
                std::mem::take(&mut state.paused_sessions)
            }
            Err(poisoned) => {
                log::warn!("media pause state lock was poisoned; recovering");
                let mut state = poisoned.into_inner();
                state.generation = state.generation.wrapping_add(1);
                std::mem::take(&mut state.paused_sessions)
            }
        };

        if sessions.is_empty() {
            return;
        }

        tauri::async_runtime::spawn_blocking(move || restore_sessions(sessions));
    }

    fn pause_playing_sessions(generation: u64) -> windows::core::Result<()> {
        let manager = GlobalSystemMediaTransportControlsSessionManager::RequestAsync()?.get()?;
        let sessions = manager.GetSessions()?;
        let total = sessions.Size().unwrap_or(0);
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
                            restore_session_if_still_paused(PausedSession {
                                source_app_id,
                                session,
                            });
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
        match PAUSE_STATE.lock() {
            Ok(state) => state.generation == generation,
            Err(poisoned) => poisoned.into_inner().generation == generation,
        }
    }

    fn store_paused_session(generation: u64, session: PausedSession) -> bool {
        match PAUSE_STATE.lock() {
            Ok(mut state) if state.generation == generation => {
                state.paused_sessions.push(session);
                true
            }
            Ok(_) => false,
            Err(poisoned) => {
                let mut state = poisoned.into_inner();
                if state.generation == generation {
                    state.paused_sessions.push(session);
                    true
                } else {
                    false
                }
            }
        }
    }

    fn restore_sessions(sessions: Vec<PausedSession>) {
        let total = sessions.len();
        let mut resumed = 0_usize;

        for session in sessions {
            if restore_session_if_still_paused(session) {
                resumed += 1;
            }
        }

        log::debug!("media pause: restore attempted={total} resumed={resumed}");
    }

    fn restore_session_if_still_paused(session: PausedSession) -> bool {
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
}

pub fn begin_dictation_media_pause() {
    platform::begin();
}

pub fn end_dictation_media_pause() {
    platform::end();
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
    fn only_still_paused_sessions_are_resumed() {
        assert!(should_resume_after_dictation(MediaPlaybackState::Paused));
        assert!(!should_resume_after_dictation(MediaPlaybackState::Playing));
        assert!(!should_resume_after_dictation(MediaPlaybackState::Stopped));
        assert!(!should_resume_after_dictation(MediaPlaybackState::Changing));
    }

    #[test]
    fn restore_requires_successful_pause_and_still_paused_session() {
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
            MediaPlaybackState::Playing
        ));
        assert!(!would_restore(
            MediaPlaybackState::Playing,
            true,
            MediaPlaybackState::Stopped
        ));
    }
}
