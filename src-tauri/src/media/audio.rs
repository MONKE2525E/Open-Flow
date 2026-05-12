use anyhow::{Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc;

/// Amplify mic input before encoding for the transcription API.
const AUDIO_GAIN: f32 = 3.5;

/// Separate multiplier used only for the pill level indicator.
/// Much higher than AUDIO_GAIN so the bars respond to normal speech, not just loud taps.
const DISPLAY_GAIN: f32 = 15.0;

pub fn list_input_devices() -> Vec<String> {
    let host = cpal::default_host();
    host.input_devices()
        .map(|iter| iter.filter_map(|d| d.name().ok()).collect())
        .unwrap_or_default()
}

/// Blocking: records until `stop_tx` fires, returns WAV bytes + duration.
/// Runs on a dedicated thread so cpal's !Send stream stays contained.
pub struct RecordingSession {
    stop_tx: mpsc::SyncSender<()>,
    result_rx: mpsc::Receiver<Result<(Vec<u8>, u64)>>,
    /// Current RMS level of the (already-gained) input, as f32 bits.
    pub level: Arc<AtomicU32>,
    /// True while the audio thread is actively capturing.
    pub active: Arc<AtomicBool>,
}

impl RecordingSession {
    pub fn start(device_name: Option<String>) -> Result<Self> {
        let host = cpal::default_host();
        let device = if let Some(name) = device_name {
            host.input_devices()?
                .find(|d| d.name().map(|n| n == name).unwrap_or(false))
                .or_else(|| host.default_input_device())
                .context("No input device available")?
        } else {
            host.default_input_device().context("No input device available")?
        };
        let config = device.default_input_config()?;
        let sample_rate = config.sample_rate().0;
        let channels = config.channels();

        let (stop_tx, stop_rx) = mpsc::sync_channel::<()>(1);
        let (result_tx, result_rx) = mpsc::sync_channel(1);

        let level  = Arc::new(AtomicU32::new(0f32.to_bits()));
        let active = Arc::new(AtomicBool::new(true));

        let level_w  = Arc::clone(&level);
        let active_w = Arc::clone(&active);

        std::thread::spawn(move || {
            let samples: Arc<Mutex<Vec<f32>>> = Arc::new(Mutex::new(Vec::new()));
            let samples_clone = Arc::clone(&samples);
            let level_cb = Arc::clone(&level_w);

            let err_fn = |e| log::error!("Audio stream error: {e}");

            let stream = match config.sample_format() {
                cpal::SampleFormat::F32 => device.build_input_stream(
                    &config.into(),
                    move |data: &[f32], _| {
                        // Display level uses raw RMS × DISPLAY_GAIN, capped at 1.0
                        let display = (rms_f32(data) * DISPLAY_GAIN).min(1.0);
                        level_cb.store(display.to_bits(), Ordering::Relaxed);
                        // WAV samples use the API gain
                        let gained: Vec<f32> = data.iter()
                            .map(|&s| (s * AUDIO_GAIN).clamp(-1.0, 1.0))
                            .collect();
                        samples_clone.lock().unwrap().extend_from_slice(&gained);
                    },
                    err_fn,
                    None,
                ),
                cpal::SampleFormat::I16 => device.build_input_stream(
                    &config.into(),
                    move |data: &[i16], _| {
                        let floats: Vec<f32> = data.iter()
                            .map(|&s| s as f32 / i16::MAX as f32)
                            .collect();
                        let display = (rms_f32(&floats) * DISPLAY_GAIN).min(1.0);
                        level_cb.store(display.to_bits(), Ordering::Relaxed);
                        let gained: Vec<f32> = floats.iter()
                            .map(|&s| (s * AUDIO_GAIN).clamp(-1.0, 1.0))
                            .collect();
                        samples_clone.lock().unwrap().extend(gained);
                    },
                    err_fn,
                    None,
                ),
                fmt => {
                    let _ = result_tx.send(Err(anyhow::anyhow!("Unsupported sample format: {fmt:?}")));
                    return;
                }
            };

            let stream = match stream {
                Ok(s) => s,
                Err(e) => { let _ = result_tx.send(Err(e.into())); return; }
            };

            if let Err(e) = stream.play() {
                let _ = result_tx.send(Err(e.into()));
                return;
            }

            let _ = stop_rx.recv();
            drop(stream);

            active_w.store(false, Ordering::Relaxed);
            level_w.store(0f32.to_bits(), Ordering::Relaxed);

            let data = samples.lock().unwrap().clone();
            let dur_ms = data.len() as u64 * 1000
                / (sample_rate as u64 * channels as u64);

            // Downsample to 16kHz mono before encoding.
            // Speech only needs up to ~8kHz (Nyquist) and APIs transfer less data.
            // A 5s clip at 48kHz stereo is ~960KB; at 16kHz mono it's ~160KB.
            let (encode_data, encode_rate) = downsample_to_16k(&data, sample_rate, channels);

            let result = encode_wav(&encode_data, encode_rate, 1)
                .map(|wav| (wav, dur_ms));

            let _ = result_tx.send(result);
        });

        Ok(RecordingSession { stop_tx, result_rx, level, active })
    }

    pub fn stop(self) -> Result<(Vec<u8>, u64)> {
        let _ = self.stop_tx.send(());
        self.result_rx
            .recv()
            .context("Recording thread dropped channel")?
    }
}

/// Collapse multi-channel interleaved PCM to mono, then resample to 16kHz
/// using linear interpolation. 16kHz captures speech up to 8kHz (well above
/// the ~4kHz formant ceiling of human voice) while cutting file size ~6x
/// compared to a typical 48kHz stereo capture.
fn downsample_to_16k(data: &[f32], sample_rate: u32, channels: u16) -> (Vec<f32>, u32) {
    const TARGET: u32 = 16_000;

    // Mix down to mono
    let mono: Vec<f32> = if channels == 1 {
        data.to_vec()
    } else {
        let ch = channels as usize;
        data.chunks(ch)
            .map(|frame| frame.iter().sum::<f32>() / ch as f32)
            .collect()
    };

    if sample_rate == TARGET {
        return (mono, TARGET);
    }

    let ratio = sample_rate as f64 / TARGET as f64;
    let out_len = (mono.len() as f64 / ratio).ceil() as usize;
    let last = mono.len().saturating_sub(1);

    let resampled = (0..out_len)
        .map(|i| {
            let src = i as f64 * ratio;
            let lo = (src.floor() as usize).min(last);
            let hi = (lo + 1).min(last);
            let t = (src - src.floor()) as f32;
            mono[lo] * (1.0 - t) + mono[hi] * t
        })
        .collect();

    (resampled, TARGET)
}

fn rms_f32(data: &[f32]) -> f32 {
    if data.is_empty() { return 0.0; }
    (data.iter().map(|&s| s * s).sum::<f32>() / data.len() as f32).sqrt()
}

fn encode_wav(samples: &[f32], sample_rate: u32, channels: u16) -> Result<Vec<u8>> {
    if samples.is_empty() {
        anyhow::bail!("No audio captured");
    }
    let spec = hound::WavSpec {
        channels,
        sample_rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut buf = std::io::Cursor::new(Vec::new());
    let mut writer = hound::WavWriter::new(&mut buf, spec)?;
    for &s in samples {
        writer.write_sample((s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16)?;
    }
    writer.finalize()?;
    Ok(buf.into_inner())
}
