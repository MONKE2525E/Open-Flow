//! Per-input-device adaptive voice-detection sensitivity.
//!
//! Replaces the old manual microphone-calibration wizard. Each input device
//! gets one learned scalar, persisted in `settings.json` under
//! [`store::VAD_DEVICE_PROFILES`], and the detector uses it to scale its
//! acceptance thresholds (see `media::vad`).
//!
//! **Sensitivity semantics: higher = more sensitive = easier to accept
//! speech.** `media::vad::analyze_speech` divides its acceptance thresholds by
//! this value, so 2.0 halves them and 0.5 doubles them. The default leans
//! deliberately toward the sensitive end — missing a whisper is much worse
//! than occasionally transcribing an empty clip.
//!
//! This is a small local adaptive-control loop, not a learning pipeline. It
//! only moves on two unambiguous events:
//!
//! * a confirmed empty dictation (VAD accepted, STT succeeded, no speech in
//!   the output, and the waveform itself corroborates "nothing was there") —
//!   *small* nudge toward less sensitive, and only after
//!   [`EMPTY_STREAK_TO_ADJUST`] of them in a row;
//! * a confirmed Skip-VAD recovery (VAD rejected, the user retried the same
//!   audio with VAD bypassed, and a real transcription came back) — a *larger*
//!   single-event nudge toward more sensitive.
//!
//! Anything ambiguous — an STT/network error, a cancelled dictation, a
//! Skip-VAD retry that also produced nothing, a VAD that failed to load —
//! trains nothing at all.

use crate::data::store;
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;

/// Hard bounds. Repeated unusual events can walk the value to an edge but
/// never past it, so the detector can never be trained into uselessness.
pub const MIN_SENSITIVITY: f32 = 0.6;
pub const MAX_SENSITIVITY: f32 = 2.5;

/// Starting point for an unknown device: already more sensitive than neutral,
/// because a brand-new device has to work for a whisperer with zero setup.
pub const DEFAULT_SENSITIVITY: f32 = 1.3;

/// A confirmed missed utterance is direct evidence of a false negative, so it
/// corrects hard and immediately.
pub const RECOVERY_STEP: f32 = 0.25;

/// A confirmed empty is weaker evidence (the user may simply have pressed the
/// hotkey by accident), so it corrects gently *and* only after a run of them.
/// The streak requirement is also the anti-oscillation guard: alternating
/// empty/recovery events can never ping-pong the value, because each recovery
/// clears the streak before it ever reaches the adjustment point.
pub const EMPTY_STEP: f32 = 0.15;
pub const EMPTY_STREAK_TO_ADJUST: u32 = 3;

// The two invariants the whole design rests on: a new device leans sensitive,
// and a confirmed missed utterance corrects harder than a confirmed empty.
const _: () = assert!(DEFAULT_SENSITIVITY > 1.0);
const _: () = assert!(RECOVERY_STEP > EMPTY_STEP);

/// Key used for "whatever the OS default input is" — the device setting is
/// `None` until the user explicitly picks a microphone.
pub const DEFAULT_DEVICE_KEY: &str = "__system_default__";

/// Stable per-device key. The device name is all cpal gives us, and it is what
/// `store::AudioConfig::device` already stores, so it is also what a device
/// switch actually changes.
pub fn device_key(device: Option<&str>) -> String {
    match device.map(str::trim).filter(|name| !name.is_empty()) {
        Some(name) => name.to_string(),
        None => DEFAULT_DEVICE_KEY.to_string(),
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DeviceProfile {
    pub sensitivity: f32,
    /// Consecutive confirmed-empty dictations not yet converted into an
    /// adjustment. Reset by any adjustment in either direction.
    pub empty_streak: u32,
}

impl Default for DeviceProfile {
    fn default() -> Self {
        Self {
            sensitivity: DEFAULT_SENSITIVITY,
            empty_streak: 0,
        }
    }
}

/// The two events that are strong enough to move a profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Feedback {
    /// VAD accepted, transcription succeeded, output had no speech, and the
    /// audio itself corroborates that it was genuinely empty.
    ConfirmedEmpty,
    /// VAD rejected, the user hit Skip VAD on the same audio, and the
    /// bypassed transcription produced real speech.
    ConfirmedRecovery,
}

/// Pure state transition — the whole adaptive algorithm, isolated so it can be
/// tested without a Tauri app handle or a settings file.
pub fn next_profile(current: DeviceProfile, feedback: Feedback) -> DeviceProfile {
    match feedback {
        Feedback::ConfirmedRecovery => DeviceProfile {
            sensitivity: clamp_sensitivity(current.sensitivity + RECOVERY_STEP),
            empty_streak: 0,
        },
        Feedback::ConfirmedEmpty => {
            let streak = current.empty_streak.saturating_add(1);
            if streak < EMPTY_STREAK_TO_ADJUST {
                DeviceProfile {
                    sensitivity: current.sensitivity,
                    empty_streak: streak,
                }
            } else {
                DeviceProfile {
                    sensitivity: clamp_sensitivity(current.sensitivity - EMPTY_STEP),
                    empty_streak: 0,
                }
            }
        }
    }
}

pub fn clamp_sensitivity(value: f32) -> f32 {
    if !value.is_finite() {
        return DEFAULT_SENSITIVITY;
    }
    value.clamp(MIN_SENSITIVITY, MAX_SENSITIVITY)
}

/// Parses the persisted map, dropping anything malformed rather than failing.
/// A corrupt or outdated entry must degrade to the safe default, never to an
/// unusable detector.
pub fn parse_profiles(stored: Option<&Value>) -> BTreeMap<String, DeviceProfile> {
    let mut profiles = BTreeMap::new();
    let Some(object) = stored.and_then(Value::as_object) else {
        return profiles;
    };
    for (device, entry) in object {
        if device.trim().is_empty() {
            continue;
        }
        let Some(entry) = entry.as_object() else {
            continue;
        };
        // A stored value outside the bounds (an older build's range, a hand
        // edit) is clamped, not discarded — the direction it was learned in is
        // still information.
        let Some(sensitivity) = entry
            .get("sensitivity")
            .and_then(Value::as_f64)
            .map(|value| value as f32)
            .filter(|value| value.is_finite())
        else {
            continue;
        };
        let empty_streak = entry
            .get("emptyStreak")
            .and_then(Value::as_u64)
            .unwrap_or(0)
            .min(EMPTY_STREAK_TO_ADJUST as u64) as u32;
        profiles.insert(
            device.clone(),
            DeviceProfile {
                sensitivity: clamp_sensitivity(sensitivity),
                empty_streak,
            },
        );
    }
    profiles
}

fn serialize_profiles(profiles: &BTreeMap<String, DeviceProfile>) -> Value {
    let mut map = Map::new();
    for (device, profile) in profiles {
        map.insert(
            device.clone(),
            json!({
                "sensitivity": profile.sensitivity,
                "emptyStreak": profile.empty_streak,
            }),
        );
    }
    Value::Object(map)
}

/// Learned profile for `device`, or the whisper-friendly default when this
/// device has never been seen. Deliberately does *not* seed from the retired
/// `mic_gain` calibration value: `media::vad::gain_leniency_scale` already
/// consumes `mic_gain`, so seeding from it too would count the same signal
/// twice. The old calibration data is retired, and gain stays what it always
/// was — an audio-path knob, not a detection threshold.
pub fn profile_for(snapshot: &store::SettingsSnapshot, device: Option<&str>) -> DeviceProfile {
    parse_profiles(snapshot.get(store::VAD_DEVICE_PROFILES))
        .get(&device_key(device))
        .copied()
        .unwrap_or_default()
}

/// Convenience for the hot path, which only needs the scalar.
pub fn sensitivity_for(snapshot: &store::SettingsSnapshot, device: Option<&str>) -> f32 {
    profile_for(snapshot, device).sensitivity
}

/// Applies `feedback` to `device`'s profile and persists the result.
///
/// Returns `Some((old, new))` when the sensitivity actually moved, so the
/// caller can log the change; `None` when the event was absorbed into the
/// empty streak without moving the value, or when persistence failed.
pub fn record_feedback(
    app: &tauri::AppHandle,
    device: Option<&str>,
    feedback: Feedback,
) -> Option<(f32, f32)> {
    let handle = match store::settings_handle(app) {
        Ok(handle) => handle,
        Err(error) => {
            log::warn!("vad: could not open settings to record feedback: {error}");
            return None;
        }
    };
    let key = device_key(device);
    let mut profiles = parse_profiles(handle.get(store::VAD_DEVICE_PROFILES).as_ref());
    let current = profiles.get(&key).copied().unwrap_or_default();
    let updated = next_profile(current, feedback);
    profiles.insert(key.clone(), updated);

    if let Err(error) = handle.save_value(store::VAD_DEVICE_PROFILES, serialize_profiles(&profiles))
    {
        log::warn!("vad: could not persist learned sensitivity: {error}");
        return None;
    }

    if (updated.sensitivity - current.sensitivity).abs() < f32::EPSILON {
        log::debug!(
            "vad: feedback={feedback:?} device_known={} recorded without adjustment (empty_streak={}/{})",
            key != DEFAULT_DEVICE_KEY,
            updated.empty_streak,
            EMPTY_STREAK_TO_ADJUST
        );
        return None;
    }
    Some((current.sensitivity, updated.sensitivity))
}

/// Clears every learned profile, restoring the automatic defaults. Touches
/// nothing but this one key — mic gain, device selection and noise reduction
/// are unrelated audio settings and stay exactly as they were.
pub fn reset(app: &tauri::AppHandle) -> Result<(), String> {
    let handle = store::settings_handle(app)?;
    handle.delete(store::VAD_DEVICE_PROFILES)?;
    handle.save()?;
    log::info!("vad: learned sensitivity reset for all input devices");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profile(sensitivity: f32, empty_streak: u32) -> DeviceProfile {
        DeviceProfile {
            sensitivity,
            empty_streak,
        }
    }

    #[test]
    fn new_device_starts_whisper_friendly() {
        assert_eq!(DeviceProfile::default().sensitivity, DEFAULT_SENSITIVITY);
        assert_eq!(DeviceProfile::default().empty_streak, 0);
    }

    #[test]
    fn confirmed_recovery_raises_sensitivity_immediately() {
        let next = next_profile(profile(1.0, 0), Feedback::ConfirmedRecovery);
        assert!((next.sensitivity - 1.25).abs() < 1e-6);
    }

    #[test]
    fn a_single_confirmed_empty_only_accumulates_evidence() {
        let next = next_profile(profile(1.3, 0), Feedback::ConfirmedEmpty);
        assert_eq!(next.sensitivity, 1.3);
        assert_eq!(next.empty_streak, 1);
    }

    #[test]
    fn confirmed_empties_lower_sensitivity_only_after_a_streak() {
        let mut current = profile(1.3, 0);
        for _ in 0..EMPTY_STREAK_TO_ADJUST - 1 {
            current = next_profile(current, Feedback::ConfirmedEmpty);
            assert_eq!(current.sensitivity, 1.3);
        }
        current = next_profile(current, Feedback::ConfirmedEmpty);
        assert!((current.sensitivity - (1.3 - EMPTY_STEP)).abs() < 1e-6);
        assert_eq!(current.empty_streak, 0);
    }

    #[test]
    fn alternating_events_cannot_oscillate() {
        // Empty, recovery, empty, recovery… the streak never reaches the
        // adjustment point, so only the recoveries move the value.
        let mut current = profile(1.0, 0);
        for _ in 0..6 {
            current = next_profile(current, Feedback::ConfirmedEmpty);
            current = next_profile(current, Feedback::ConfirmedRecovery);
        }
        assert_eq!(current.sensitivity, MAX_SENSITIVITY);
        assert_eq!(current.empty_streak, 0);
    }

    #[test]
    fn adjustments_respect_hard_bounds() {
        let mut current = profile(MAX_SENSITIVITY, 0);
        for _ in 0..20 {
            current = next_profile(current, Feedback::ConfirmedRecovery);
        }
        assert_eq!(current.sensitivity, MAX_SENSITIVITY);

        let mut current = profile(MIN_SENSITIVITY, 0);
        for _ in 0..60 {
            current = next_profile(current, Feedback::ConfirmedEmpty);
        }
        assert_eq!(current.sensitivity, MIN_SENSITIVITY);
    }

    #[test]
    fn device_key_folds_unset_and_blank_to_the_system_default() {
        assert_eq!(device_key(None), DEFAULT_DEVICE_KEY);
        assert_eq!(device_key(Some("   ")), DEFAULT_DEVICE_KEY);
        assert_eq!(device_key(Some(" Blue Yeti ")), "Blue Yeti");
    }

    #[test]
    fn profiles_are_isolated_per_device_and_restored_on_switch() {
        let mut profiles: BTreeMap<String, DeviceProfile> = BTreeMap::new();
        profiles.insert("Blue Yeti".into(), profile(1.0, 0));
        profiles.insert("AirPods".into(), profile(2.0, 1));

        // Adjusting one device leaves the other untouched...
        let yeti = next_profile(profiles["Blue Yeti"], Feedback::ConfirmedRecovery);
        profiles.insert("Blue Yeti".into(), yeti);
        assert_eq!(profiles["AirPods"], profile(2.0, 1));

        // ...and a round-trip through storage restores both.
        let restored = parse_profiles(Some(&serialize_profiles(&profiles)));
        assert_eq!(restored["Blue Yeti"].sensitivity, 1.25);
        assert_eq!(restored["AirPods"], profile(2.0, 1));
    }

    #[test]
    fn unknown_device_falls_back_to_the_default_profile() {
        let profiles = parse_profiles(Some(&json!({ "Blue Yeti": { "sensitivity": 1.0 } })));
        assert!(!profiles.contains_key("AirPods"));
        assert_eq!(DeviceProfile::default().sensitivity, DEFAULT_SENSITIVITY);
    }

    #[test]
    fn invalid_persisted_state_fails_safe() {
        assert!(parse_profiles(None).is_empty());
        assert!(parse_profiles(Some(&json!("not an object"))).is_empty());
        assert!(parse_profiles(Some(&json!({ "Mic": 3.0 }))).is_empty());
        assert!(parse_profiles(Some(&json!({ "Mic": { "sensitivity": "loud" } }))).is_empty());
        assert!(parse_profiles(Some(&json!({ "": { "sensitivity": 1.0 } }))).is_empty());

        // Out-of-range and non-finite values clamp instead of poisoning the map.
        let clamped = parse_profiles(Some(&json!({
            "Hot": { "sensitivity": 99.0 },
            "Cold": { "sensitivity": -4.0 },
        })));
        assert_eq!(clamped["Hot"].sensitivity, MAX_SENSITIVITY);
        assert_eq!(clamped["Cold"].sensitivity, MIN_SENSITIVITY);
        assert_eq!(clamp_sensitivity(f32::NAN), DEFAULT_SENSITIVITY);
    }

    #[test]
    fn reset_restores_defaults_for_every_device() {
        // `reset` deletes the key; parsing an absent key yields no profiles,
        // and every lookup then resolves to the default.
        let profiles = parse_profiles(None);
        assert_eq!(
            profiles
                .get(&device_key(Some("Blue Yeti")))
                .copied()
                .unwrap_or_default(),
            DeviceProfile::default()
        );
    }
}
