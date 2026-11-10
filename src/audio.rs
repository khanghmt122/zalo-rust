//! Audio I/O for VoIP calls.
//!
//! Phase 1 (this file): device enumeration + mic->speaker pass-through on a
//! dedicated thread (Send-safe handle).
//! Phase 2 (TODO): Opus encode/decode + RTP packetization over the call's UDP socket.
//! Phase 3 (TODO): jitter buffer, echo cancellation, mixing, device selection.

use anyhow::Result;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use std::thread::{self, JoinHandle};

/// Send-safe handle to a running audio session. Dropping it stops the thread.
pub struct AudioHandle {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl AudioHandle {
    /// Start mic->speaker pass-through on a dedicated thread.
    pub fn start() -> Result<Self> {
        let stop = Arc::new(AtomicBool::new(false));
        let stop_thread = stop.clone();
        let thread = thread::spawn(move || {
            if let Err(e) = run_pass_through(stop_thread.clone()) {
                eprintln!("audio thread error: {e}");
            }
        });
        Ok(Self {
            stop,
            thread: Some(thread),
        })
    }
}

impl Drop for AudioHandle {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

fn run_pass_through(stop: Arc<AtomicBool>) -> Result<()> {
    let host = cpal::default_host();
    let input_dev = host
        .default_input_device()
        .ok_or_else(|| anyhow::anyhow!("no default input device"))?;
    let output_dev = host
        .default_output_device()
        .ok_or_else(|| anyhow::anyhow!("no default output device"))?;

    let in_cfg = input_dev.default_input_config()?;
    let out_cfg = output_dev.default_output_config()?;

    let channels = in_cfg.channels() as usize;

    let ring: Arc<Mutex<Vec<f32>>> = Arc::new(Mutex::new(Vec::with_capacity(48000)));
    let ring_in = ring.clone();
    let ring_out = ring.clone();

    let input_stream = input_dev.build_input_stream(
        &in_cfg.into(),
        move |data: &[f32], _| {
            if let Ok(mut buf) = ring_in.lock() {
                buf.extend_from_slice(data);
                if buf.len() > 48000 * channels {
                    let drop = buf.len() - 48000 * channels;
                    buf.drain(0..drop);
                }
            }
        },
        |e| eprintln!("input stream error: {e}"),
        None,
    )?;
    input_stream.play()?;

    let output_stream = output_dev.build_output_stream(
        &out_cfg.into(),
        move |data: &mut [f32], _| {
            if let Ok(mut buf) = ring_out.lock() {
                let n = data.len().min(buf.len());
                for (i, slot) in data.iter_mut().enumerate() {
                    *slot = if i < n { buf[i] } else { 0.0 };
                }
                if n > 0 {
                    buf.drain(0..n);
                }
            } else {
                for s in data.iter_mut() {
                    *s = 0.0;
                }
            }
        },
        |e| eprintln!("output stream error: {e}"),
        None,
    )?;
    output_stream.play()?;

    while !stop.load(Ordering::Relaxed) {
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    drop(input_stream);
    drop(output_stream);
    Ok(())
}

/// List available input (microphone) device names.
pub fn list_input_devices() -> Result<Vec<String>> {
    let host = cpal::default_host();
    let mut out = Vec::new();
    for d in host.input_devices()? {
        if let Ok(name) = d.name() {
            out.push(name);
        }
    }
    Ok(out)
}

/// List available output (speaker) device names.
pub fn list_output_devices() -> Result<Vec<String>> {
    let host = cpal::default_host();
    let mut out = Vec::new();
    for d in host.output_devices()? {
        if let Ok(name) = d.name() {
            out.push(name);
        }
    }
    Ok(out)
}

/// Open the default mic and speaker and run pass-through for `seconds` (smoke test).
pub fn run_loopback(seconds: u64) -> Result<()> {
    let session = AudioHandle::start()?;
    std::thread::sleep(std::time::Duration::from_secs(seconds));
    drop(session);
    Ok(())
}

/// Convert 16-bit PCM mono samples into a standard 44-byte header WAV file buffer.
pub fn pcm_to_wav_bytes(samples: &[i16], sample_rate: u32, channels: u16) -> Vec<u8> {
    let bits_per_sample: u16 = 16;
    let byte_rate = sample_rate * channels as u32 * (bits_per_sample as u32 / 8);
    let block_align = channels * (bits_per_sample / 8);
    let data_len = samples.len() * 2;
    let riff_chunk_size = 36 + data_len;

    let mut buf = Vec::with_capacity(44 + data_len);
    // RIFF Header
    buf.extend_from_slice(b"RIFF");
    buf.extend_from_slice(&(riff_chunk_size as u32).to_le_bytes());
    buf.extend_from_slice(b"WAVE");

    // "fmt " subchunk
    buf.extend_from_slice(b"fmt ");
    buf.extend_from_slice(&16_u32.to_le_bytes()); // Subchunk1Size = 16 for PCM
    buf.extend_from_slice(&1_u16.to_le_bytes());  // AudioFormat = 1 (PCM)
    buf.extend_from_slice(&channels.to_le_bytes());
    buf.extend_from_slice(&sample_rate.to_le_bytes());
    buf.extend_from_slice(&byte_rate.to_le_bytes());
    buf.extend_from_slice(&block_align.to_le_bytes());
    buf.extend_from_slice(&bits_per_sample.to_le_bytes());

    // "data" subchunk
    buf.extend_from_slice(b"data");
    buf.extend_from_slice(&(data_len as u32).to_le_bytes());
    for &sample in samples {
        buf.extend_from_slice(&sample.to_le_bytes());
    }

    buf
}

/// Active voice message recorder capturing microphone audio.
pub struct VoiceRecorder {
    stop: Arc<AtomicBool>,
    samples: Arc<Mutex<Vec<i16>>>,
    sample_rate: u32,
    stream: Option<cpal::Stream>,
    start_time: std::time::Instant,
}

impl VoiceRecorder {
    /// Start recording from the default audio input device.
    pub fn start() -> Result<Self> {
        let host = cpal::default_host();
        let device = host
            .default_input_device()
            .ok_or_else(|| anyhow::anyhow!("Không tìm thấy microphone mặc định trên thiết bị"))?;
        let config = device.default_input_config()?;
        let sample_rate = config.sample_rate().0;
        let channels = config.channels();

        let stop = Arc::new(AtomicBool::new(false));
        let samples = Arc::new(Mutex::new(Vec::with_capacity(48000 * 5)));

        let samples_cb = samples.clone();
        let stop_cb = stop.clone();

        let err_fn = |err| eprintln!("Voice record error: {err}");

        let stream = match config.sample_format() {
            cpal::SampleFormat::F32 => device.build_input_stream(
                &config.into(),
                move |data: &[f32], _: &_| {
                    if stop_cb.load(Ordering::Relaxed) {
                        return;
                    }
                    if let Ok(mut buf) = samples_cb.lock() {
                        for chunk in data.chunks(channels as usize) {
                            let mono_sample = chunk.iter().copied().sum::<f32>() / (channels as f32);
                            let s16 = (mono_sample.clamp(-1.0, 1.0) * 32767.0) as i16;
                            buf.push(s16);
                        }
                    }
                },
                err_fn,
                None,
            )?,
            cpal::SampleFormat::I16 => device.build_input_stream(
                &config.into(),
                move |data: &[i16], _: &_| {
                    if stop_cb.load(Ordering::Relaxed) {
                        return;
                    }
                    if let Ok(mut buf) = samples_cb.lock() {
                        for chunk in data.chunks(channels as usize) {
                            let mono = chunk.iter().map(|&x| x as i32).sum::<i32>() / (channels as i32);
                            buf.push(mono as i16);
                        }
                    }
                },
                err_fn,
                None,
            )?,
            _ => anyhow::bail!("Định dạng âm thanh mic không được hỗ trợ"),
        };

        stream.play()?;

        Ok(Self {
            stop,
            samples,
            sample_rate,
            stream: Some(stream),
            start_time: std::time::Instant::now(),
        })
    }

    /// Elapsed time since recording started.
    pub fn duration(&self) -> std::time::Duration {
        self.start_time.elapsed()
    }

    /// Stop recording and encode collected samples to WAV bytes. Returns (bytes, duration_seconds).
    pub fn stop_and_encode_wav(mut self) -> Result<(Vec<u8>, f32)> {
        self.stop.store(true, Ordering::Relaxed);
        drop(self.stream.take());

        let samples = self.samples.lock().map_err(|_| anyhow::anyhow!("Lock error"))?.clone();
        if samples.is_empty() {
            anyhow::bail!("Chưa thu được âm thanh từ microphone");
        }
        let duration_secs = samples.len() as f32 / self.sample_rate as f32;
        let wav_bytes = pcm_to_wav_bytes(&samples, self.sample_rate, 1);
        Ok((wav_bytes, duration_secs))
    }
}