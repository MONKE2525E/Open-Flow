use anyhow::{Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};

const AUDIO_GAIN: f32 = 3.5;
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
    result_rx: mpsc::Receiver<Result<(Vec<u8>, u64, f32)>>,
    pub level: Arc<AtomicU32>,
    pub active: Arc<AtomicBool>,
}

impl RecordingSession {
    pub fn start(device_name: Option<String>, noise_reduction: bool) -> Result<Self> {
        let host = cpal::default_host();
        let device = if let Some(name) = device_name {
            host.input_devices()?
                .find(|d| d.name().map(|n| n == name).unwrap_or(false))
                .or_else(|| host.default_input_device())
                .context("No input device available")?
        } else {
            host.default_input_device()
                .context("No input device available")?
        };
        let config = device.default_input_config()?;
        let sample_rate = config.sample_rate().0;
        let channels = config.channels();

        let (stop_tx, stop_rx) = mpsc::sync_channel::<()>(1);
        let (result_tx, result_rx) = mpsc::sync_channel(1);

        let level = Arc::new(AtomicU32::new(0f32.to_bits()));
        let active = Arc::new(AtomicBool::new(true));

        let level_w = Arc::clone(&level);
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
                        let display = (rms_f32(data) * DISPLAY_GAIN).min(1.0);
                        level_cb.store(display.to_bits(), Ordering::Relaxed);
                        let gained: Vec<f32> = data
                            .iter()
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
                        let floats: Vec<f32> =
                            data.iter().map(|&s| s as f32 / i16::MAX as f32).collect();
                        let display = (rms_f32(&floats) * DISPLAY_GAIN).min(1.0);
                        level_cb.store(display.to_bits(), Ordering::Relaxed);
                        let gained: Vec<f32> = floats
                            .iter()
                            .map(|&s| (s * AUDIO_GAIN).clamp(-1.0, 1.0))
                            .collect();
                        samples_clone.lock().unwrap().extend(gained);
                    },
                    err_fn,
                    None,
                ),
                fmt => {
                    let _ =
                        result_tx.send(Err(anyhow::anyhow!("Unsupported sample format: {fmt:?}")));
                    return;
                }
            };

            let stream = match stream {
                Ok(s) => s,
                Err(e) => {
                    let _ = result_tx.send(Err(e.into()));
                    return;
                }
            };

            if let Err(e) = stream.play() {
                let _ = result_tx.send(Err(e.into()));
                return;
            }

            let _ = stop_rx.recv();
            drop(stream);

            active_w.store(false, Ordering::Relaxed);
            level_w.store(0f32.to_bits(), Ordering::Relaxed);

            let data = std::mem::take(&mut *samples.lock().unwrap());
            let dur_ms = data.len() as u64 * 1000 / (sample_rate as u64 * channels as u64);

            // RMS on post-gain samples; divide out the gain to get an
            // approximation of the original mic level for silence detection.
            let overall_rms = rms_f32(&data) / AUDIO_GAIN;

            // Denoise then resample, or go straight to resample.
            let (encode_data, encode_rate) = if noise_reduction {
                let mono = mix_to_mono(&data, channels);
                let denoised = denoise_mono(&mono);
                resample_to_16k(&denoised, sample_rate)
            } else {
                downsample_to_16k(&data, sample_rate, channels)
            };

            let result =
                encode_wav(&encode_data, encode_rate, 1).map(|wav| (wav, dur_ms, overall_rms));

            let _ = result_tx.send(result);
        });

        Ok(RecordingSession {
            stop_tx,
            result_rx,
            level,
            active,
        })
    }

    pub fn stop(self) -> Result<(Vec<u8>, u64, f32)> {
        let _ = self.stop_tx.send(());
        self.result_rx
            .recv()
            .context("Recording thread dropped channel")?
    }
}

/// Apply RNNoise-based noise suppression to a mono f32 buffer at native rate.
/// nnnoiseless expects samples in the i16 amplitude range [-32768, 32767].
/// Processes in 480-sample frames (10 ms at 48 kHz); the last partial frame
/// is zero-padded. Output is rescaled back to [-1, 1].
fn denoise_mono(samples: &[f32]) -> Vec<f32> {
    use nnnoiseless::DenoiseState;
    const FRAME: usize = DenoiseState::FRAME_SIZE; // 480

    if samples.is_empty() {
        return Vec::new();
    }

    let mut state = DenoiseState::new();
    let mut out = Vec::with_capacity(samples.len());
    let mut frame_in = [0.0f32; FRAME];
    let mut frame_out = [0.0f32; FRAME];

    let mut i = 0;
    while i < samples.len() {
        let end = (i + FRAME).min(samples.len());
        let chunk = &samples[i..end];
        let chunk_len = chunk.len();

        // Scale [-1,1] → i16 range for nnnoiseless
        for (dst, &src) in frame_in[..chunk_len].iter_mut().zip(chunk) {
            *dst = src * 32767.0;
        }
        frame_in[chunk_len..].fill(0.0);

        state.process_frame(&mut frame_out, &frame_in);

        // Scale back to [-1,1]
        for &s in &frame_out[..chunk_len] {
            out.push((s / 32767.0).clamp(-1.0, 1.0));
        }

        i += FRAME;
    }

    out
}

fn mix_to_mono(data: &[f32], channels: u16) -> Vec<f32> {
    if channels == 1 {
        return data.to_vec();
    }
    let ch = channels as usize;
    data.chunks(ch)
        .map(|frame| frame.iter().sum::<f32>() / ch as f32)
        .collect()
}

fn resample_to_16k(mono: &[f32], sample_rate: u32) -> (Vec<f32>, u32) {
    const TARGET: u32 = 16_000;
    if sample_rate == TARGET {
        return (mono.to_vec(), TARGET);
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

/// Collapse multi-channel interleaved PCM to mono, then resample to 16 kHz.
fn downsample_to_16k(data: &[f32], sample_rate: u32, channels: u16) -> (Vec<f32>, u32) {
    let mono = mix_to_mono(data, channels);
    resample_to_16k(&mono, sample_rate)
}

fn rms_f32(data: &[f32]) -> f32 {
    if data.is_empty() {
        return 0.0;
    }
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
