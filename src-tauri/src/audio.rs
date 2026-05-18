//! Microphone capture via cpal.
//!
//! `cpal::Stream` is `!Send`, so we own it on a dedicated worker thread and
//! communicate via channels. `Recorder` (the public handle) is Send+Sync and
//! can live in Tauri's `manage()` state.
//!
//! Toggle pattern:
//!   1st press → `Recorder::start()` spawns the audio thread + cpal stream
//!   2nd press → `stop_and_take()` joins the thread and returns 16 kHz samples
//!
//! Resampling: naive nearest-neighbor (good for speech, no extra deps).

use std::sync::{mpsc, Arc, Mutex};
use anyhow::{anyhow, bail, Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

pub const WHISPER_SR: u32 = 16_000;
pub const MAX_SECONDS: usize = 600;

pub struct Recorder {
    stop_tx: mpsc::SyncSender<()>,
    /// Captured samples come back here after `stop_tx.send(())`.
    samples_rx: mpsc::Receiver<Result<Vec<f32>, String>>,
    /// Joined to ensure the audio thread cleans up before we drop.
    handle: Option<std::thread::JoinHandle<()>>,
    pub source_rate: u32,
    /// v0.2 · Shared with audio thread —— streaming consumer can drain it
    /// mid-recording for partial transcription. None until init succeeds.
    samples_buf: Arc<Mutex<Vec<f32>>>,
}

impl Recorder {
    pub fn start() -> Result<Self> {
        let (stop_tx, stop_rx) = mpsc::sync_channel::<()>(1);
        let (samples_tx, samples_rx) = mpsc::channel();
        let (init_tx, init_rx) = mpsc::sync_channel::<Result<u32, String>>(1);
        // v0.2 · share samples buffer between audio thread + streaming consumer
        let samples_buf_shared: Arc<Mutex<Vec<f32>>> =
            Arc::new(Mutex::new(Vec::with_capacity(48_000 * 5)));
        let samples_buf_for_thread = samples_buf_shared.clone();

        let handle = std::thread::spawn(move || {
            // ── Set up cpal on this thread ──────────────────────────────
            let host = cpal::default_host();
            let device = match host.default_input_device() {
                Some(d) => d,
                None => {
                    let _ = init_tx.send(Err("no default input device".into()));
                    return;
                }
            };
            let cfg = match device.default_input_config() {
                Ok(c) => c,
                Err(e) => {
                    let _ = init_tx.send(Err(format!("default_input_config: {e}")));
                    return;
                }
            };
            let source_rate = cfg.sample_rate().0;
            let channels = cfg.channels() as usize;
            // v0.2 · samples_buf moved to outer scope, accessed via the shared Arc
            let samples_buf = samples_buf_for_thread;
            let buf_writer = samples_buf.clone();
            let err_fn = |e| eprintln!("[mouseclaw] audio stream err: {e}");

            let stream_result = match cfg.sample_format() {
                cpal::SampleFormat::F32 => device.build_input_stream(
                    &cfg.into(),
                    move |data: &[f32], _| append_mono(&buf_writer, data, channels, |s| *s),
                    err_fn,
                    None,
                ),
                cpal::SampleFormat::I16 => device.build_input_stream(
                    &cfg.into(),
                    move |data: &[i16], _| {
                        append_mono(&buf_writer, data, channels, |s| (*s as f32) / i16::MAX as f32)
                    },
                    err_fn,
                    None,
                ),
                cpal::SampleFormat::U16 => device.build_input_stream(
                    &cfg.into(),
                    move |data: &[u16], _| {
                        append_mono(&buf_writer, data, channels, |s| {
                            (*s as f32 - 32768.0) / 32768.0
                        })
                    },
                    err_fn,
                    None,
                ),
                fmt => {
                    let _ = init_tx.send(Err(format!("unsupported sample format: {fmt:?}")));
                    return;
                }
            };
            let stream = match stream_result {
                Ok(s) => s,
                Err(e) => {
                    let _ = init_tx.send(Err(format!("build_input_stream: {e}")));
                    return;
                }
            };
            if let Err(e) = stream.play() {
                let _ = init_tx.send(Err(format!("stream.play: {e}")));
                return;
            }

            // Signal that we're recording successfully + send source_rate
            let _ = init_tx.send(Ok(source_rate));

            // Block until the controlling thread asks us to stop
            let _ = stop_rx.recv();

            // Drop the stream to halt capture.
            // v0.3 · 不在这儿 drain samples_buf —— 上层 Recorder::stop_drain_remaining_16k
            // 会在 join 后自己读完整 buffer。Channel path 保留是兼容旧 API（已无调用方），
            // 发个空信号让等待方不会卡死。
            drop(stream);
            let _ = samples_tx.send(Ok(Vec::new()));
        });

        let source_rate = init_rx
            .recv()
            .context("audio thread initialisation channel closed")?
            .map_err(|e| anyhow!(e))?;

        Ok(Self {
            stop_tx,
            samples_rx,
            handle: Some(handle),
            source_rate,
            samples_buf: samples_buf_shared,
        })
    }

    /// v0.2 · 流式消费：拉走当前累积样本，重采样到 16kHz mono，返回。
    /// 录音继续。给 transcribe_stream 200ms 一次的轮询用。
    /// **注意**：这会清空 buffer —— 之后 `stop_and_take` 拿不到这部分了。
    /// 用 streaming 模式时**只**用这个 + `stop_silent_remaining`，不要混用 stop_and_take。
    pub fn drain_resampled_16k(&self) -> Vec<f32> {
        let pcm = match self.samples_buf.lock() {
            Ok(mut g) => std::mem::take(&mut *g),
            Err(_) => return Vec::new(),
        };
        if pcm.is_empty() { return pcm; }
        resample_to_16k(&pcm, self.source_rate)
    }

    /// v0.2 · streaming 模式下停止录音并 drain 最后一批样本（已 resample）。
    /// 不走 samples_rx channel（avoid hang if no samples buffered there）。
    pub fn stop_drain_remaining_16k(mut self) -> Result<Vec<f32>> {
        let _ = self.stop_tx.send(());
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
        // After join the audio thread has finished writing; safe to drain.
        let pcm = match self.samples_buf.lock() {
            Ok(g) => g.clone(),
            Err(_) => Vec::new(),
        };
        // Drop unread samples_rx (audio thread may have queued a final batch
        // that we don't need — we already drained the shared buffer).
        let _ = self.samples_rx.try_recv();
        Ok(resample_to_16k(&pcm, self.source_rate))
    }

    // stop_and_take removed in v0.3 — callers migrated to stop_drain_remaining_16k.
}

fn append_mono<S, F: Fn(&S) -> f32>(
    buf: &Arc<Mutex<Vec<f32>>>,
    data: &[S],
    channels: usize,
    to_f32: F,
) {
    let Ok(mut g) = buf.lock() else { return };
    if channels <= 1 {
        g.extend(data.iter().map(|s| to_f32(s)));
    } else {
        for frame in data.chunks_exact(channels) {
            let mut sum = 0.0_f32;
            for s in frame {
                sum += to_f32(s);
            }
            g.push(sum / channels as f32);
        }
    }
}

fn resample_to_16k(input: &[f32], src_rate: u32) -> Vec<f32> {
    if src_rate == WHISPER_SR {
        return input.to_vec();
    }
    let ratio = src_rate as f64 / WHISPER_SR as f64;
    let n_out = (input.len() as f64 / ratio).round() as usize;
    let mut out = Vec::with_capacity(n_out);
    for i in 0..n_out {
        let idx = (i as f64 * ratio) as usize;
        if idx < input.len() {
            out.push(input[idx]);
        }
    }
    out
}
