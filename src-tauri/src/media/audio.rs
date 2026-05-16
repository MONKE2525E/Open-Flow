use anyhow::{Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};

const DISPLAY_GAIN: f32 = 15.0;

fn lock_audio<'a, T>(mutex: &'a Mutex<T>, label: &str) -> Option<std::sync::MutexGuard<'a, T>> {
    match mutex.lock() {
        Ok(guard) => Some(guard),
        Err(_) => {
            log::error!("Audio {label} lock was poisoned");
            None
        }
    }
}

pub fn list_input_devices() -> Vec<String> {
    let host = cpal::default_host();
    host.input_devices()
        .map(|iter| iter.filter_map(|d| d.name().ok()).collect())
        .unwrap_or_default()
}

/// Processes RNNoise denoising incrementally during recording so the CPU cost
/// is spread across the hold duration rather than spiking on release.
/// Buffers incoming mono samples until a full 480-sample frame is ready,
/// then runs RNNoise immediately and emits the denoised output.
struct FrameDenoiser {
    state: Box<nnnoiseless::DenoiseState<'static>>,
    buf: Vec<f32>,
}

impl FrameDenoiser {
    fn new() -> Self {
        Self {
            state: nnnoiseless::DenoiseState::new(),
            buf: Vec::with_capacity(nnnoiseless::DenoiseState::FRAME_SIZE),
        }
    }

    fn push(&mut self, input: &[f32], out: &mut Vec<f32>) {
        const FRAME: usize = nnnoiseless::DenoiseState::FRAME_SIZE;
        let mut frame_in = [0.0f32; FRAME];
        let mut frame_out = [0.0f32; FRAME];

        for &s in input {
            self.buf.push(s);
            if self.buf.len() == FRAME {
                for (dst, &src) in frame_in.iter_mut().zip(&self.buf) {
                    *dst = src * 32767.0;
                }
                self.state.process_frame(&mut frame_out, &frame_in);
                for &s in &frame_out {
                    out.push((s / 32767.0).clamp(-1.0, 1.0));
                }
                self.buf.clear();
            }
        }
    }

    /// Flush any buffered samples that didn't fill a complete frame.
    fn flush(&mut self, out: &mut Vec<f32>) {
        if self.buf.is_empty() {
            return;
        }
        const FRAME: usize = nnnoiseless::DenoiseState::FRAME_SIZE;
        let mut frame_in = [0.0f32; FRAME];
        let mut frame_out = [0.0f32; FRAME];
        let len = self.buf.len();
        for (i, &s) in self.buf.iter().enumerate() {
            frame_in[i] = s * 32767.0;
        }
        self.state.process_frame(&mut frame_out, &frame_in);
        for &s in &frame_out[..len] {
            out.push((s / 32767.0).clamp(-1.0, 1.0));
        }
        self.buf.clear();
    }
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
    pub fn start(device_name: Option<String>, noise_reduction: bool, gain: f32) -> Result<Self> {
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
            // Mono samples stored here. Callbacks always mix to mono so the
            // channel count doesn't affect the stored data layout.
            let samples: Arc<Mutex<Vec<f32>>> = Arc::new(Mutex::new(Vec::new()));
            let samples_clone = Arc::clone(&samples);
            let level_cb = Arc::clone(&level_w);

            // FrameDenoiser lives behind a Mutex so both the callback closure
            // and the post-recording flush can reach it from the same thread.
            let denoiser: Option<Arc<Mutex<FrameDenoiser>>> = if noise_reduction {
                Some(Arc::new(Mutex::new(FrameDenoiser::new())))
            } else {
                None
            };
            let denoiser_cb = denoiser.as_ref().map(Arc::clone);

            let err_fn = |e| log::error!("Audio stream error: {e}");

            let stream = match config.sample_format() {
                cpal::SampleFormat::F32 => device.build_input_stream(
                    &config.into(),
                    move |data: &[f32], _| {
                        let display = (rms_f32(data) * DISPLAY_GAIN).min(1.0);
                        level_cb.store(display.to_bits(), Ordering::Relaxed);
                        let ch = channels as usize;
                        let Some(mut store) = lock_audio(&samples_clone, "sample buffer") else {
                            return;
                        };
                        if let Some(d) = &denoiser_cb {
                            let mono: Vec<f32> = if ch == 1 {
                                data.iter().map(|&s| (s * gain).clamp(-1.0, 1.0)).collect()
                            } else {
                                data.chunks(ch)
                                    .map(|frame| {
                                        frame
                                            .iter()
                                            .map(|&s| (s * gain).clamp(-1.0, 1.0))
                                            .sum::<f32>()
                                            / ch as f32
                                    })
                                    .collect()
                            };
                            if let Some(mut denoiser) = lock_audio(d, "denoiser") {
                                denoiser.push(&mono, &mut store);
                            }
                        } else if ch == 1 {
                            store.extend(data.iter().map(|&s| (s * gain).clamp(-1.0, 1.0)));
                        } else {
                            store.extend(data.chunks(ch).map(|frame| {
                                frame
                                    .iter()
                                    .map(|&s| (s * gain).clamp(-1.0, 1.0))
                                    .sum::<f32>()
                                    / ch as f32
                            }));
                        }
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
                        let ch = channels as usize;
                        let Some(mut store) = lock_audio(&samples_clone, "sample buffer") else {
                            return;
                        };
                        if let Some(d) = &denoiser_cb {
                            let mono: Vec<f32> = if ch == 1 {
                                floats
                                    .iter()
                                    .map(|&s| (s * gain).clamp(-1.0, 1.0))
                                    .collect()
                            } else {
                                floats
                                    .chunks(ch)
                                    .map(|frame| {
                                        frame
                                            .iter()
                                            .map(|&s| (s * gain).clamp(-1.0, 1.0))
                                            .sum::<f32>()
                                            / ch as f32
                                    })
                                    .collect()
                            };
                            if let Some(mut denoiser) = lock_audio(d, "denoiser") {
                                denoiser.push(&mono, &mut store);
                            }
                        } else if ch == 1 {
                            store.extend(floats.iter().map(|&s| (s * gain).clamp(-1.0, 1.0)));
                        } else {
                            store.extend(floats.chunks(ch).map(|frame| {
                                frame
                                    .iter()
                                    .map(|&s| (s * gain).clamp(-1.0, 1.0))
                                    .sum::<f32>()
                                    / ch as f32
                            }));
                        }
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

            let mut data = match lock_audio(&samples, "sample buffer") {
                Some(mut samples) => std::mem::take(&mut *samples),
                None => {
                    let _ = result_tx.send(Err(anyhow::anyhow!("Audio sample buffer unavailable")));
                    return;
                }
            };

            // Flush any samples sitting in the partial frame buffer.
            if let Some(d) = &denoiser {
                if let Some(mut denoiser) = lock_audio(d, "denoiser") {
                    denoiser.flush(&mut data);
                }
            }

            // data is now mono; duration is simply len / sample_rate.
            let dur_ms = data.len() as u64 * 1000 / sample_rate as u64;
            let overall_rms = rms_f32(&data);

            // Denoising already happened in real-time; just resample to 16 kHz.
            let (encode_data, encode_rate) = resample_to_16k(&data, sample_rate);

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
