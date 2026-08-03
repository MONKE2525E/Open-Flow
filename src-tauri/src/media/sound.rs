//! Short synthesized audio cues for dictation start / stop / error / cancel.
//!
//! Each cue is built by additive synthesis into a small PCM buffer, then played
//! on a background thread that opens the default output device, plays the buffer,
//! and drops the stream. No shared state, no held device, no `Send`/lifetime
//! juggling. Playback is fire-and-forget and never panics the pipeline.
//!
//! The cues are designed to be *categorically* distinguishable, which (per UI
//! sound-design research — auditory icons / earcons) comes from **timbre, register
//! and gesture**, not pitch alone:
//! - Complete: a clean "soft" tone, higher register, rising — a plain "done".
//! - Error: a low, **buzzy** reed (odd harmonics, sustained), descending — "wrong".
//! - Start / Cancel: the same soft tone, kept unobtrusive.

use rodio::buffer::SamplesBuffer;
use std::f32::consts::{PI, TAU};
use std::sync::Arc;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::mpsc;

/// Which dictation transition the cue represents.
#[derive(Clone, Copy)]
pub enum SoundCue {
    /// Recording started (hotkey pressed) — a soft, airy rising arpeggio.
    Start,
    /// Recording finished (hotkey released) — a clean rising tone: "done".
    Stop,
    /// Processing failed — a low, buzzy descending "womp": "wrong".
    Error,
    /// Recording cancelled (Escape) — a quiet, soft two-note dismiss.
    Cancel,
}

const SAMPLE_RATE: u32 = 44_100;
const ATTACK_MS: f32 = 6.0; // smooth swell-in so the onset never clicks
const RELEASE_MS: f32 = 60.0; // release for sustained (buzz) voices
const AMPLITUDE: f32 = 0.30; // gentle, fixed master level

/// Delay before the start cue for a normal hold-to-talk press. Long enough that
/// the discarded first tap of a handsfree double-tap (held <200ms, then
/// cancelled) is suppressed before it sounds, while a genuine hold still chimes.
pub const START_CUE_NORMAL_DELAY_MS: u64 = 260;
/// Delay before the start cue when entering handsfree, so it lands after the
/// pill's entry animation rather than before it looks like it's listening.
pub const START_CUE_HANDSFREE_DELAY_MS: u64 = 220;

/// Generation counter for the pending start cue. Scheduling a new start cue or
/// cancelling supersedes any still-pending one, so the discarded first tap of a
/// handsfree double-tap never sounds and the cue can be held back cleanly.
static START_CUE_GEN: AtomicU64 = AtomicU64::new(0);
static SOUND_TX: OnceLock<mpsc::Sender<SoundCommand>> = OnceLock::new();
static VOLUME_SESSION: AtomicU64 = AtomicU64::new(0);
static SOUND_EFFECTS_VOLUME: AtomicU32 = AtomicU32::new(1.0f32.to_bits());

type AfterPlay = Box<dyn FnOnce() + Send + 'static>;

enum SoundCommand {
    Play {
        cue: SoundCue,
        generation: Option<u64>,
        after: Option<AfterPlay>,
    },
}

/// Instrument/voice a note is played with — the main lever for making cues
/// sound like different *kinds* of event rather than the same beep.
#[derive(Clone, Copy)]
enum Timbre {
    /// Near-sine, faintly warm — a clean, "normal" tone (start, complete, cancel).
    Soft,
    /// Reedy buzzer — odd harmonics, sustained, a "wrong" edge (error).
    Buzz,
}

/// One harmonic of a timbre: frequency `ratio` to the fundamental, relative
/// `gain`, and `decay_mult` (>1 = decays faster than the body, which gives bells
/// their bright-onset/mellow-tail shimmer). `decay_mult` is unused for sustained
/// voices.
struct Harmonic {
    ratio: f32,
    gain: f32,
    decay_mult: f32,
}

/// One note in a cue: fundamental `freq`, when it begins, how long it rings, a
/// relative `gain`, and the `timbre` it is voiced with.
struct Note {
    freq: f32,
    start_ms: f32,
    dur_ms: f32,
    gain: f32,
    timbre: Timbre,
}

/// Play the cue on a background thread. Returns immediately.
pub fn play(cue: SoundCue) {
    if sound_tx()
        .send(SoundCommand::Play {
            cue,
            generation: None,
            after: None,
        })
        .is_err()
    {
        log::debug!("sound cue failed: playback worker is unavailable");
    }
}

/// Set the master volume for future sound effects.
pub fn set_volume(volume: f32) {
    SOUND_EFFECTS_VOLUME.store(volume.clamp(0.0, 1.0).to_bits(), Ordering::Relaxed);
}

/// Schedule the start cue after `delay_ms`. Claims a new generation, so any
/// previously scheduled start cue is superseded and will not sound. This is how
/// the discarded first tap of a handsfree double-tap is silenced and how the
/// handsfree cue is held back until its entry animation has played.
pub fn play_start_delayed(delay_ms: u64) {
    play_start_delayed_then(delay_ms, || {});
}

/// Schedule the start cue after `delay_ms`, then run `after` once that same cue
/// has either finished playing or failed. Superseded or cancelled generations
/// never invoke the callback.
pub fn play_start_delayed_then<F>(delay_ms: u64, after: F)
where
    F: FnOnce() + Send + 'static,
{
    let generation = START_CUE_GEN.fetch_add(1, Ordering::SeqCst).wrapping_add(1);
    tauri::async_runtime::spawn(async move {
        if delay_ms != 0 {
            tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
        }
        if START_CUE_GEN.load(Ordering::SeqCst) != generation {
            return; // superseded or cancelled
        }

        let after_cb = Box::new(after) as AfterPlay;
        if let Err(mpsc::SendError(SoundCommand::Play { after: Some(cb), .. })) =
            sound_tx().send(SoundCommand::Play {
                cue: SoundCue::Start,
                generation: Some(generation),
                after: Some(after_cb),
            })
        {
            log::debug!("sound cue failed: playback worker is unavailable");
            cb();
        }
    });
}

/// Cancel a pending (not-yet-played) start cue — e.g. when a quick tap is
/// discarded or the recording is escaped before the cue fires.
pub fn cancel_pending_start() {
    START_CUE_GEN.fetch_add(1, Ordering::SeqCst);
}

/// Mute only if the same volume session is still current after the call
/// returns. If an unmute raced in between, immediately undo the stale mute.
pub fn coordinated_mute(active: Arc<std::sync::atomic::AtomicBool>) {
    let session_id = VOLUME_SESSION
        .fetch_add(1, Ordering::SeqCst)
        .wrapping_add(1);
    tauri::async_runtime::spawn_blocking(move || {
        if !active.load(Ordering::Relaxed) || VOLUME_SESSION.load(Ordering::SeqCst) != session_id {
            return;
        }

        crate::system::volume::mute();

        if !active.load(Ordering::Relaxed) || VOLUME_SESSION.load(Ordering::SeqCst) != session_id {
            crate::system::volume::unmute();
        }
    });
}

/// Invalidate any pending coordinated mute before unmuting the system.
pub fn coordinated_unmute() {
    VOLUME_SESSION.fetch_add(1, Ordering::SeqCst);
    tauri::async_runtime::spawn_blocking(crate::system::volume::unmute);
}

fn sound_tx() -> &'static mpsc::Sender<SoundCommand> {
    SOUND_TX.get_or_init(|| {
        let (tx, rx) = mpsc::channel::<SoundCommand>();
        std::thread::spawn(move || sound_worker(rx));
        tx
    })
}

fn sound_worker(rx: mpsc::Receiver<SoundCommand>) {
    let mut output: Option<(rodio::OutputStream, rodio::OutputStreamHandle)> = None;
    let playback_id = Arc::new(AtomicU64::new(0));
    let mut current_sink: Option<Arc<rodio::Sink>> = None;

    loop {
        let command = match rx.recv_timeout(std::time::Duration::from_millis(250)) {
            Ok(command) => command,
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if current_sink.as_ref().is_some_and(|sink| sink.empty()) {
                    current_sink = None;
                    output = None;
                }
                continue;
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        };
        let SoundCommand::Play {
            cue,
            generation,
            after,
        } = command;

        if generation.is_some_and(|g| START_CUE_GEN.load(Ordering::SeqCst) != g) {
            continue;
        }

        let this_playback_id = playback_id.fetch_add(1, Ordering::SeqCst).wrapping_add(1);

        // Do not keep an idle device handle across cues. Releasing it here
        // lets the next cue reopen the current default output after a device
        // switch, while an actively playing sink still keeps its stream alive.
        if current_sink.as_ref().is_some_and(|sink| sink.empty()) {
            current_sink = None;
            output = None;
        }

        if let Some(sink) = current_sink.take() {
            sink.stop();
        }

        let sink = match play_with_cached_output(&mut output, cue) {
            Ok(sink) => Arc::new(sink),
            Err(e) => {
                log::debug!("sound cue failed: {e:?}");
                output = None;

                if generation.is_some_and(|g| START_CUE_GEN.load(Ordering::SeqCst) != g) {
                    continue;
                }

                if let Some(after) = after {
                    after();
                }
                continue;
            }
        };

        current_sink = Some(sink.clone());

        if generation.is_some_and(|g| START_CUE_GEN.load(Ordering::SeqCst) != g) {
            sink.stop();
            continue;
        }

        if let Some(after) = after {
            let playback_id = playback_id.clone();
            tauri::async_runtime::spawn_blocking(move || {
                sink.sleep_until_end();

                if playback_id.load(Ordering::SeqCst) != this_playback_id {
                    return;
                }
                if generation.is_some_and(|g| START_CUE_GEN.load(Ordering::SeqCst) != g) {
                    return;
                }

                after();
            });
        }
    }
}

fn play_with_cached_output(
    output: &mut Option<(rodio::OutputStream, rodio::OutputStreamHandle)>,
    cue: SoundCue,
) -> anyhow::Result<rodio::Sink> {
    if output.is_none() {
        *output = Some(rodio::OutputStream::try_default()?);
    }

    let sink_result = {
        let (_, handle) = output
            .as_ref()
            .expect("cached output stream must exist before creating a sink");
        rodio::Sink::try_new(handle)
    };

    let sink = match sink_result {
        Ok(sink) => sink,
        Err(_) => {
            *output = Some(rodio::OutputStream::try_default()?);
            let (_, handle) = output
                .as_ref()
                .expect("cached output stream must exist after reinitialization");
            rodio::Sink::try_new(handle)?
        }
    };

    let volume = f32::from_bits(SOUND_EFFECTS_VOLUME.load(Ordering::Relaxed));
    sink.append(render(notes(cue), volume));
    Ok(sink)
}

fn harmonics(timbre: Timbre) -> &'static [Harmonic] {
    match timbre {
        // Near-sine with a whisper of octave for warmth.
        Timbre::Soft => &[
            Harmonic {
                ratio: 1.0,
                gain: 1.0,
                decay_mult: 1.0,
            },
            Harmonic {
                ratio: 2.0,
                gain: 0.08,
                decay_mult: 1.6,
            },
        ],
        // Odd harmonics for a reedy "buzzer" edge, but softened: dropped the
        // bright 7th, eased the 3rd/5th so it reads as "wrong" without being harsh.
        Timbre::Buzz => &[
            Harmonic {
                ratio: 1.0,
                gain: 1.0,
                decay_mult: 1.0,
            },
            Harmonic {
                ratio: 2.0,
                gain: 0.20,
                decay_mult: 1.0,
            },
            Harmonic {
                ratio: 3.0,
                gain: 0.34,
                decay_mult: 1.0,
            },
            Harmonic {
                ratio: 5.0,
                gain: 0.12,
                decay_mult: 1.0,
            },
        ],
    }
}

/// Sustained voices hold at full level (attack → sustain → release) like a
/// buzzer; plucked voices decay exponentially like a struck bell.
fn is_sustained(timbre: Timbre) -> bool {
    matches!(timbre, Timbre::Buzz)
}

fn notes(cue: SoundCue) -> &'static [Note] {
    match cue {
        // Begin listening: a soft, airy rising arpeggio — anticipatory, light.
        SoundCue::Start => &[
            Note {
                freq: 523.25,
                start_ms: 0.0,
                dur_ms: 220.0,
                gain: 1.0,
                timbre: Timbre::Soft,
            }, // C5
            Note {
                freq: 659.25,
                start_ms: 60.0,
                dur_ms: 240.0,
                gain: 1.05,
                timbre: Timbre::Soft,
            }, // E5
            Note {
                freq: 783.99,
                start_ms: 120.0,
                dur_ms: 340.0,
                gain: 1.1,
                timbre: Timbre::Soft,
            }, // G5
        ],
        // Completed: a plain, clean rising tone (perfect fifth) — a simple "done",
        // no bell shimmer.
        SoundCue::Stop => &[
            Note {
                freq: 587.33,
                start_ms: 0.0,
                dur_ms: 200.0,
                gain: 1.0,
                timbre: Timbre::Soft,
            }, // D5
            Note {
                freq: 880.00,
                start_ms: 90.0,
                dur_ms: 360.0,
                gain: 1.05,
                timbre: Timbre::Soft,
            }, // A5
        ],
        // Failure: a low, buzzy descending "womp" — shorter and a touch softer,
        // low register + reedy edge still read clearly as "wrong".
        SoundCue::Error => &[
            Note {
                freq: 220.00,
                start_ms: 0.0,
                dur_ms: 150.0,
                gain: 0.6,
                timbre: Timbre::Buzz,
            }, // A3
            Note {
                freq: 174.61,
                start_ms: 165.0,
                dur_ms: 240.0,
                gain: 0.6,
                timbre: Timbre::Buzz,
            }, // F3
        ],
        // Cancelled: a quiet, soft, neutral two-note dismiss.
        SoundCue::Cancel => &[
            Note {
                freq: 392.00,
                start_ms: 0.0,
                dur_ms: 150.0,
                gain: 0.5,
                timbre: Timbre::Soft,
            }, // G4
            Note {
                freq: 293.66,
                start_ms: 70.0,
                dur_ms: 220.0,
                gain: 0.5,
                timbre: Timbre::Soft,
            }, // D4
        ],
    }
}

fn render(notes: &[Note], volume: f32) -> SamplesBuffer<f32> {
    SamplesBuffer::new(1, SAMPLE_RATE, render_samples(notes, volume))
}

fn render_samples(notes: &[Note], volume: f32) -> Vec<f32> {
    let sr = SAMPLE_RATE as f32;
    let total_ms = notes
        .iter()
        .map(|n| n.start_ms + n.dur_ms)
        .fold(0.0_f32, f32::max);
    let total = ((total_ms / 1000.0) * sr).ceil() as usize + 1;
    let mut buf = vec![0.0_f32; total];

    let attack = (ATTACK_MS / 1000.0) * sr;
    for note in notes {
        let start = ((note.start_ms / 1000.0) * sr) as usize;
        let len = ((note.dur_ms / 1000.0) * sr) as usize;
        if len == 0 {
            continue;
        }
        let lenf = len as f32;
        let sustained = is_sustained(note.timbre);
        // Plucked bodies decay to roughly -60 dB by the note's end.
        let base_decay = 6.9 / lenf;
        let release = ((RELEASE_MS / 1000.0) * sr).min(lenf * 0.5);

        for h in harmonics(note.timbre) {
            let freq = note.freq * h.ratio;
            if freq > 18_000.0 {
                continue; // skip inaudible/aliasing partials
            }
            let amp = AMPLITUDE * note.gain * h.gain * volume;
            let h_decay = base_decay * h.decay_mult;
            let w = TAU * freq / sr;
            let decay_step = (-h_decay).exp();
            let mut decay_env = (-(attack.ceil() - attack) * h_decay).exp();
            for i in 0..len {
                let idx = start + i;
                if idx >= total {
                    break;
                }
                let n = i as f32;
                let env = if n < attack {
                    0.5 - 0.5 * (PI * n / attack).cos()
                } else if sustained {
                    let tail = lenf - n;
                    if tail < release {
                        0.5 - 0.5 * (PI * tail / release).cos()
                    } else {
                        1.0
                    }
                } else {
                    let current = decay_env;
                    decay_env *= decay_step;
                    current
                };
                buf[idx] += (w * n).sin() * env * amp;
            }
        }
    }

    // Guard against summed overlap/harmonic clipping.
    for s in &mut buf {
        *s = s.clamp(-1.0, 1.0);
    }
    buf
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Audition the cues without launching the app: renders each to a 16-bit WAV
    /// in `$VERENU_CUE_OUT` (or the cwd). Ignored by default; run explicitly:
    ///   cargo test -p verenu --manifest-path src-tauri/Cargo.toml \
    ///     render_cue_previews -- --ignored --nocapture
    #[test]
    #[ignore]
    fn render_cue_previews() {
        let out_dir = std::env::var("VERENU_CUE_OUT").unwrap_or_else(|_| ".".to_string());
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: SAMPLE_RATE,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        for (name, cue) in [
            ("start", SoundCue::Start),
            ("stop", SoundCue::Stop),
            ("error", SoundCue::Error),
            ("cancel", SoundCue::Cancel),
        ] {
            let path = format!("{out_dir}/cue_{name}.wav");
            let mut writer = hound::WavWriter::create(&path, spec).expect("create wav");
            for s in render_samples(notes(cue), 1.0) {
                let v = (s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
                writer.write_sample(v).expect("write sample");
            }
            writer.finalize().expect("finalize wav");
            eprintln!("wrote {path}");
        }
    }

    #[test]
    fn render_volume_scales_generated_samples() {
        let full = render_samples(notes(SoundCue::Start), 1.0);
        let half = render_samples(notes(SoundCue::Start), 0.5);
        let full_peak = full.iter().map(|sample| sample.abs()).fold(0.0, f32::max);
        let half_peak = half.iter().map(|sample| sample.abs()).fold(0.0, f32::max);

        assert!(full_peak > 0.0);
        assert!((half_peak / full_peak - 0.5).abs() < 0.001);
        assert!(render_samples(notes(SoundCue::Start), 0.0)
            .iter()
            .all(|sample| *sample == 0.0));
    }
}
