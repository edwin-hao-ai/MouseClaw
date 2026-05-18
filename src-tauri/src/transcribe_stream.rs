//! Streaming ASR via sherpa-onnx + Zipformer-transducer (v0.2 · WIP).
//!
//! Replacement for `transcribe.rs` (Whisper batch). Designed for real-time
//! "边说边出字" UX — partials emit every ~200ms while user holds the
//! shortcut, final pulled on release.
//!
//! ### Model
//!   - sherpa-onnx-streaming-zipformer-bilingual-zh-en-2023-02-20
//!   - ~180 MB on disk (encoder + decoder + joiner + tokens)
//!   - Auto-downloaded from huggingface on first launch into
//!     `~/.mouseclaw/models/sherpa-zh-en/`
//!   - WER on CommonVoice-zh / LibriSpeech-test-clean comparable to
//!     Whisper Small (8-12% Chinese WER)
//!
//! ### Resource discipline
//!   - Model loads lazily on first transcribe call (cold start ~500ms)
//!   - Idle-unloaded after `IDLE_UNLOAD_SECS` of no activity (TODO Day 3)
//!   - One `OnlineRecognizer` cached; streams created per-utterance
//!
//! ### Day 1 status (this file)
//!   - Skeleton + types
//!   - Stub `init()` / `stream_chunk` / `finalize`
//!   - NOT YET wired into pipeline.rs — that's Day 3
//!   - NOT YET downloading model — that's Day 2

use std::path::PathBuf;
use std::sync::Mutex;
use anyhow::{anyhow, Context, Result};
use once_cell::sync::Lazy;
use sherpa_onnx::{OnlineRecognizer, OnlineRecognizerConfig};

/// Model relative dir under ~/.mouseclaw/models/
pub const MODEL_DIR: &str = "sherpa-zh-en";
/// HuggingFace download URL prefix —— Day 2 用 curl 抓
pub const MODEL_HF_BASE: &str =
    "https://huggingface.co/csukuangfj/sherpa-onnx-streaming-zipformer-bilingual-zh-en-2023-02-20/resolve/main";
/// 单个模型包预期 4 个文件（来自上面的 HF repo）
pub const MODEL_FILES: &[&str] = &[
    "encoder-epoch-99-avg-1.int8.onnx",
    "decoder-epoch-99-avg-1.onnx",
    "joiner-epoch-99-avg-1.int8.onnx",
    "tokens.txt",
];

/// 缓存的 recognizer —— 第一次 transcribe 时加载，后续复用。
/// 用 Mutex 不用 RwLock：sherpa OnlineRecognizer 内部就有锁。
static RECOGNIZER: Lazy<Mutex<Option<OnlineRecognizer>>> = Lazy::new(|| Mutex::new(None));

fn models_dir() -> Result<PathBuf> {
    let home = std::env::var_os("HOME").ok_or_else(|| anyhow!("HOME not set"))?;
    Ok(PathBuf::from(home).join(".mouseclaw/models").join(MODEL_DIR))
}

/// 检查 4 个 model 文件齐不齐。Day 2 加 download_if_missing。
pub fn is_ready() -> bool {
    let dir = match models_dir() { Ok(d) => d, Err(_) => return false };
    MODEL_FILES.iter().all(|f| dir.join(f).exists())
}

/// 第一次 transcribe 时延迟加载 recognizer。
fn ensure_loaded() -> Result<()> {
    let mut guard = RECOGNIZER.lock().unwrap();
    if guard.is_some() { return Ok(()); }

    let dir = models_dir()?;
    if !is_ready() {
        return Err(anyhow!(
            "sherpa zh-en model missing under {} — Day 2 会接 auto-download",
            dir.display()
        ));
    }

    let mut config = OnlineRecognizerConfig::default();
    config.model_config.transducer.encoder = Some(
        dir.join(MODEL_FILES[0]).to_string_lossy().into_owned(),
    );
    config.model_config.transducer.decoder = Some(
        dir.join(MODEL_FILES[1]).to_string_lossy().into_owned(),
    );
    config.model_config.transducer.joiner = Some(
        dir.join(MODEL_FILES[2]).to_string_lossy().into_owned(),
    );
    config.model_config.tokens = Some(
        dir.join(MODEL_FILES[3]).to_string_lossy().into_owned(),
    );
    config.enable_endpoint = true;
    config.decoding_method = Some("greedy_search".into());

    let rec = OnlineRecognizer::create(&config)
        .ok_or_else(|| anyhow!("OnlineRecognizer::create returned None"))?;
    *guard = Some(rec);
    println!("[mouseclaw] 🎤 sherpa streaming recognizer loaded");
    Ok(())
}

/// 流式转录的一次 session —— 持有 stream，外部拿 partial / final。
/// Day 3 pipeline.rs 录音循环里：每 ~200ms accept_waveform 一次，
/// 然后 partial() 拿当前文字 emit 到前端。
pub struct StreamSession {
    // sherpa::OnlineStream is created per utterance from the cached recognizer
    // Real type comes from the sherpa-onnx crate — stub here to verify compile
    _placeholder: (),
}

impl StreamSession {
    pub fn new() -> Result<Self> {
        ensure_loaded()?;
        // TODO Day 3: let stream = RECOGNIZER.lock().unwrap()
        //                  .as_ref().unwrap().create_stream();
        Ok(Self { _placeholder: () })
    }

    /// 喂 16kHz mono f32 samples chunk (16-bit PCM 也行，要 normalize)
    pub fn accept(&mut self, _samples: &[f32]) -> Result<()> {
        // TODO Day 3: stream.accept_waveform(16000, samples);
        //             while recognizer.is_ready(&stream) { recognizer.decode(&stream); }
        Ok(())
    }

    /// 当前 partial transcript（增量调用，UI 边说边出用）
    pub fn partial(&self) -> String {
        // TODO Day 3: recognizer.get_result(&stream).text
        String::new()
    }

    /// 用户松开快捷键后调一次 —— 拿最终文本
    pub fn finalize(self) -> Result<String> {
        // TODO Day 3: stream.input_finished();
        //             while is_ready { decode }
        //             let r = recognizer.get_result(&stream); Ok(r.text)
        Ok(String::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_dir_path_uses_home() {
        // Just verify it doesn't panic; actual path content depends on HOME.
        if let Ok(dir) = models_dir() {
            assert!(dir.ends_with(MODEL_DIR));
        }
    }

    #[test]
    fn model_files_count_is_four() {
        assert_eq!(MODEL_FILES.len(), 4);
    }

    #[test]
    fn is_ready_returns_bool_doesnt_panic() {
        let _ = is_ready();
    }
}
