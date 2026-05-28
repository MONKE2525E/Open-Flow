use anyhow::{Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use crossbeam_queue::ArrayQueue;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::mpsc;
use std::sync::Arc;

const DISPLAY_GAIN: f32 = 15.0;
const AUDIO_QUEUE_CAPACITY_SAMPLES: usize = 320_000;
const WORKER_IDLE_SLEEP_MS: u64 = 2;

pub fn list_input_devices() -> Vec<String> {
    let host = cpal::default_host();
    host.input_devices()
        .map(|iter| iter.filter_map(|d| d.name().ok()).collect())
        .unwrap_or_default()
}

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
                for &n in &frame_out {
                    out.push((n / 32767.0).clamp(-1.0, 1.0));
                }
                self.buf.clear();
            }
        }
    }

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
        for &n in &frame_out[..len] {
            out.push((n / 32767.0).clamp(-1.0, 1.0));
        }
        self.buf.clear();
    }
}

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
        let channels = config.channels() as usize;

        let (stop_tx, stop_rx) = mpsc::sync_channel::<()>(1);
        let (result_tx, result_rx) = mpsc::sync_channel(1);
        let (ready_tx, ready_rx) = mpsc::sync_channel::<Result<()>>(1);

        let level = Arc::new(AtomicU32::new(0f32.to_bits()));
        let active = Arc::new(AtomicBool::new(true));

        let level_w = Arc::clone(&level);
        let active_w = Arc::clone(&active);

        std::thread::spawn(move || {
            let queue = Arc::new(ArrayQueue::<f32>::new(AUDIO_QUEUE_CAPACITY_SAMPLES));
            let dropped_samples = Arc::new(AtomicU64::new(0));
            let stop_processing = Arc::new(AtomicBool::new(false));

            let worker_queue = Arc::clone(&queue);
            let worker_stop = Arc::clone(&stop_processing);
            let worker = std::thread::spawn(move || {
                let mut processed = Vec::<f32>::new();
                let mut denoiser = if noise_reduction {
                    Some(FrameDenoiser::new())
                } else {
                    None
                };
                let mut one = [0.0f32; 1];

                loop {
                    let mut moved_any = false;
                    while let Some(sample) = worker_queue.pop() {
                        moved_any = true;
                        let adjusted = (sample * gain).clamp(-1.0, 1.0);
                        if let Some(d) = denoiser.as_mut() {
                            one[0] = adjusted;
                            d.push(&one, &mut processed);
                        } else {
                            processed.push(adjusted);
                        }
                    }

                    if worker_stop.load(Ordering::Relaxed) && worker_queue.is_empty() {
                        break;
                    }

                    if !moved_any {
                        std::thread::sleep(std::time::Duration::from_millis(WORKER_IDLE_SLEEP_MS));
                    }
                }

                if let Some(d) = denoiser.as_mut() {
                    d.flush(&mut processed);
                }

                processed
            });

            let level_cb = Arc::clone(&level_w);
            let queue_cb = Arc::clone(&queue);
            let dropped_cb = Arc::clone(&dropped_samples);
            let err_fn = |e| log::error!("Audio stream error: {e}");

            let stream = match config.sample_format() {
                cpal::SampleFormat::F32 => device.build_input_stream(
                    &config.into(),
                    move |data: &[f32], _| {
                        enqueue_f32_buffer(data, channels, &queue_cb, &dropped_cb, &level_cb)
                    },
                    err_fn,
                    None,
                ),
                cpal::SampleFormat::I16 => device.build_input_stream(
                    &config.into(),
                    move |data: &[i16], _| {
                        enqueue_i16_buffer(data, channels, &queue_cb, &dropped_cb, &level_cb)
                    },
                    err_fn,
                    None,
                ),
                fmt => {
                    let _ =
                        ready_tx.send(Err(anyhow::anyhow!("Unsupported sample format: {fmt:?}")));
                    return;
                }
            };

            let stream = match stream {
                Ok(s) => s,
                Err(e) => {
                    let _ = ready_tx.send(Err(e.into()));
                    return;
                }
            };

            if let Err(e) = stream.play() {
                let _ = ready_tx.send(Err(e.into()));
                return;
            }

            let _ = ready_tx.send(Ok(()));
            let _ = stop_rx.recv();
            drop(stream);

            active_w.store(false, Ordering::Relaxed);
            level_w.store(0f32.to_bits(), Ordering::Relaxed);
            stop_processing.store(true, Ordering::Relaxed);

            let data = match worker.join() {
                Ok(samples) => samples,
                Err(_) => {
                    let _ =
                        result_tx.send(Err(anyhow::anyhow!("Audio processing worker panicked")));
                    return;
                }
            };

            let dropped = dropped_samples.load(Ordering::Relaxed);
            if dropped > 0 {
                log::warn!("audio queue dropped {dropped} oldest samples due to backpressure");
            }

            let dur_ms = data.len() as u64 * 1000 / sample_rate as u64;
            let overall_rms = rms_f32(&data);
            let (encode_data, encode_rate) = resample_to_16k(&data, sample_rate);
            let result =
                encode_wav(&encode_data, encode_rate, 1).map(|wav| (wav, dur_ms, overall_rms));

            let _ = result_tx.send(result);
        });

        ready_rx
            .recv()
            .context("recording thread exited before signalling ready")??;

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

fn enqueue_f32_buffer(
    data: &[f32],
    channels: usize,
    queue: &ArrayQueue<f32>,
    dropped: &AtomicU64,
    level: &AtomicU32,
) {
    if data.is_empty() {
        level.store(0f32.to_bits(), Ordering::Relaxed);
        return;
    }

    let mut sum = 0.0f32;
    let mut count = 0usize;
    if channels <= 1 {
        for &raw in data {
            let mono = raw.clamp(-1.0, 1.0);
            sum += mono * mono;
            count += 1;
            push_overwriting_oldest(queue, dropped, mono);
        }
    } else {
        for frame in data.chunks(channels) {
            let mono = (frame.iter().copied().sum::<f32>() / frame.len() as f32).clamp(-1.0, 1.0);
            sum += mono * mono;
            count += 1;
            push_overwriting_oldest(queue, dropped, mono);
        }
    }

    let rms = if count == 0 {
        0.0
    } else {
        (sum / count as f32).sqrt()
    };
    let display = (rms * DISPLAY_GAIN).min(1.0);
    level.store(display.to_bits(), Ordering::Relaxed);
}

fn enqueue_i16_buffer(
    data: &[i16],
    channels: usize,
    queue: &ArrayQueue<f32>,
    dropped: &AtomicU64,
    level: &AtomicU32,
) {
    if data.is_empty() {
        level.store(0f32.to_bits(), Ordering::Relaxed);
        return;
    }

    let mut sum = 0.0f32;
    let mut count = 0usize;
    if channels <= 1 {
        for &raw in data {
            let mono = (raw as f32 / i16::MAX as f32).clamp(-1.0, 1.0);
            sum += mono * mono;
            count += 1;
            push_overwriting_oldest(queue, dropped, mono);
        }
    } else {
        for frame in data.chunks(channels) {
            let sum_raw: i64 = frame.iter().map(|&sample| sample as i64).sum();
            let mono =
                (sum_raw as f32 / (frame.len() as f32 * i16::MAX as f32)).clamp(-1.0, 1.0);
            sum += mono * mono;
            count += 1;
            push_overwriting_oldest(queue, dropped, mono);
        }
    }

    let rms = if count == 0 {
        0.0
    } else {
        (sum / count as f32).sqrt()
    };
    let display = (rms * DISPLAY_GAIN).min(1.0);
    level.store(display.to_bits(), Ordering::Relaxed);
}

fn push_overwriting_oldest(queue: &ArrayQueue<f32>, dropped: &AtomicU64, sample: f32) {
    if queue.push(sample).is_err() {
        let _ = queue.pop();
        let _ = queue.push(sample);
        dropped.fetch_add(1, Ordering::Relaxed);
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

#[cfg(test)]
mod tests {
    use super::{enqueue_i16_buffer, push_overwriting_oldest};
    use crossbeam_queue::ArrayQueue;
    use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};

    #[test]
    fn push_overwrite_drops_oldest_when_queue_is_full() {
        let q = ArrayQueue::<f32>::new(4);
        let dropped = AtomicU64::new(0);

        for i in 0..10 {
            push_overwriting_oldest(&q, &dropped, i as f32);
        }

        let mut out = Vec::new();
        while let Some(v) = q.pop() {
            out.push(v);
        }

        assert_eq!(dropped.load(Ordering::Relaxed), 6);
        assert_eq!(out, vec![6.0, 7.0, 8.0, 9.0]);
    }

    #[test]
    fn enqueue_i16_multichannel_sums_raw_before_normalizing() {
        let q = ArrayQueue::<f32>::new(8);
        let dropped = AtomicU64::new(0);
        let level = AtomicU32::new(0f32.to_bits());
        let data = [i16::MAX, i16::MAX, 0, 0];

        enqueue_i16_buffer(&data, 2, &q, &dropped, &level);

        let first = q.pop().expect("first sample");
        let second = q.pop().expect("second sample");
        assert!((first - 1.0).abs() < 1e-6);
        assert!(second.abs() < 1e-6);
        assert_eq!(dropped.load(Ordering::Relaxed), 0);
    }
}
