//! Streaming ASR via sherpa-onnx + Zipformer-transducer (v0.2).
//!
//! Replaces Whisper batch transcription (transcribe.rs is being deleted).
//! Designed for real-time "边说边出字" — partials emit every ~200ms while
//! user holds the shortcut, final pulled on release.
//!
//! ### Model
//!   - sherpa-onnx-streaming-zipformer-bilingual-zh-en-2023-02-20
//!   - ~180 MB on disk (encoder int8 + decoder + joiner int8 + tokens)
//!   - Auto-downloaded from huggingface on first launch into
//!     `~/.mouseclaw/models/sherpa-zh-en/`
//!   - WER on CommonVoice-zh comparable to Whisper Small (8-12%)
//!
//! ### Resource discipline
//!   - `OnlineRecognizer` cached behind once_cell::Lazy + Mutex
//!   - Loaded on first transcribe (cold start ~500ms)
//!   - Streams (`OnlineStream`) created per-utterance, dropped after finalize
//!
//! ### Privacy
//!   - All inference on-device via onnxruntime CPU/CoreML
//!   - No network calls at runtime (model download only happens once, opt-in)

use std::path::PathBuf;
use std::sync::Mutex;
use anyhow::{anyhow, Context, Result};
use once_cell::sync::Lazy;
use sherpa_onnx::{OnlineRecognizer, OnlineRecognizerConfig, OnlineStream};

/// Model relative dir under ~/.mouseclaw/models/
pub const MODEL_DIR: &str = "sherpa-zh-en";
/// HuggingFace download URL base
const MODEL_HF_BASE: &str =
    "https://huggingface.co/csukuangfj/sherpa-onnx-streaming-zipformer-bilingual-zh-en-2023-02-20/resolve/main";
/// Files we need from the HF repo
pub const MODEL_FILES: &[&str] = &[
    "encoder-epoch-99-avg-1.int8.onnx",
    "decoder-epoch-99-avg-1.onnx",
    "joiner-epoch-99-avg-1.int8.onnx",
    "tokens.txt",
];

/// Cached recognizer —— first transcribe loads it, subsequent ones reuse.
static RECOGNIZER: Lazy<Mutex<Option<OnlineRecognizer>>> = Lazy::new(|| Mutex::new(None));

/// Track download state so UI can show progress.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModelState {
    Ready,
    Downloading,
    Failed(String),
}
static DOWNLOAD_STATE: Lazy<Mutex<Option<ModelState>>> = Lazy::new(|| Mutex::new(None));

pub fn current_state() -> Option<ModelState> {
    DOWNLOAD_STATE.lock().unwrap().clone()
}

fn models_dir() -> Result<PathBuf> {
    let home = std::env::var_os("HOME").ok_or_else(|| anyhow!("HOME not set"))?;
    Ok(PathBuf::from(home).join(".mouseclaw/models").join(MODEL_DIR))
}

/// True iff all 4 required files exist under ~/.mouseclaw/models/sherpa-zh-en/
pub fn is_ready() -> bool {
    let dir = match models_dir() { Ok(d) => d, Err(_) => return false };
    MODEL_FILES.iter().all(|f| dir.join(f).exists())
}

/// 启动时调一次：
///   1. 已就绪 → 直接 Ready
///   2. App bundle 里有打包模型 → 同步拷贝到 ~/.mouseclaw/models/sherpa-zh-en/
///      ≈ 几百 ms 文件复制，instant Ready，**用户首启就能说话**
///   3. 都没有（dev `cargo run` 或第一次升级）→ 后台 curl 下载（180MB · 几分钟）
pub fn kick_off_download_if_missing() {
    if is_ready() {
        *DOWNLOAD_STATE.lock().unwrap() = Some(ModelState::Ready);
        return;
    }
    // v0.3 · App bundle 兜底 —— DMG 里打包了完整模型，第一次启动直接拷贝
    if try_seed_from_bundle().unwrap_or(false) {
        *DOWNLOAD_STATE.lock().unwrap() = Some(ModelState::Ready);
        println!("[mouseclaw] 🎤 sherpa zh-en 从 bundle seed 到 ~/.mouseclaw/models/sherpa-zh-en/");
        return;
    }
    *DOWNLOAD_STATE.lock().unwrap() = Some(ModelState::Downloading);
    println!("[mouseclaw] 🎤 sherpa zh-en 模型缺失（dev 模式？），后台下载 (~180MB total)");

    std::thread::spawn(|| {
        let dir = match models_dir() {
            Ok(d) => d,
            Err(e) => {
                *DOWNLOAD_STATE.lock().unwrap() = Some(ModelState::Failed(e.to_string()));
                return;
            }
        };
        if let Err(e) = std::fs::create_dir_all(&dir) {
            *DOWNLOAD_STATE.lock().unwrap() =
                Some(ModelState::Failed(format!("mkdir: {e}")));
            return;
        }
        for file in MODEL_FILES {
            let dest = dir.join(file);
            if dest.exists() { continue; }
            let tmp  = dest.with_extension("part");
            let url  = format!("{MODEL_HF_BASE}/{file}");
            println!("[mouseclaw] curl {url}");
            let status = std::process::Command::new("curl")
                .args(["-fL", "--progress-bar", "-o"])
                .arg(&tmp)
                .arg(&url)
                .status();
            match status {
                Ok(s) if s.success() => {
                    if let Err(e) = std::fs::rename(&tmp, &dest) {
                        *DOWNLOAD_STATE.lock().unwrap() =
                            Some(ModelState::Failed(format!("rename {file}: {e}")));
                        return;
                    }
                }
                Ok(s) => {
                    *DOWNLOAD_STATE.lock().unwrap() =
                        Some(ModelState::Failed(format!("curl {file} exited {s}")));
                    return;
                }
                Err(e) => {
                    *DOWNLOAD_STATE.lock().unwrap() =
                        Some(ModelState::Failed(format!("curl {file} spawn: {e}")));
                    return;
                }
            }
        }
        println!("[mouseclaw] 🎤 sherpa zh-en download complete → {}", dir.display());
        *DOWNLOAD_STATE.lock().unwrap() = Some(ModelState::Ready);
    });
}

/// Lazy-load the OnlineRecognizer on first call.
fn ensure_loaded() -> Result<()> {
    let mut guard = RECOGNIZER.lock().unwrap();
    if guard.is_some() { return Ok(()); }

    let dir = models_dir()?;
    if !is_ready() {
        return Err(anyhow!(
            "sherpa zh-en model not ready under {} — wait for download",
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
    // v0.3.1 · push-to-talk 模式不要 endpoint 自动切段 —— 用户主动控制开始/结束
    // 如果开 endpoint，sherpa 在停顿时会自动 commit 一段并 reset stream，
    // 之后 get_result 只返回新段的文字（旧段被丢掉），用户感觉"流式断了"。
    config.enable_endpoint = false;
    config.decoding_method = Some("greedy_search".into());

    let rec = OnlineRecognizer::create(&config)
        .ok_or_else(|| anyhow!("OnlineRecognizer::create returned None"))?;
    *guard = Some(rec);
    println!("[mouseclaw] 🎤 sherpa streaming recognizer loaded");
    Ok(())
}

/// One streaming utterance —— created on press, fed via accept(), final on
/// release. Drops the underlying OnlineStream when done.
///
/// Decoding loop pattern (from sherpa-onnx docs):
///   accept_waveform → while is_ready { decode } → get_result
pub struct StreamSession {
    stream: OnlineStream,
}

impl StreamSession {
    pub fn new() -> Result<Self> {
        ensure_loaded()?;
        let stream = {
            let g = RECOGNIZER.lock().unwrap();
            g.as_ref().unwrap().create_stream()
        };
        Ok(Self { stream })
    }

    /// Feed a chunk of 16 kHz mono f32 samples. Triggers internal decode.
    /// Safe to call from a tokio task every 200ms.
    pub fn accept(&mut self, samples: &[f32]) {
        if samples.is_empty() { return; }
        self.stream.accept_waveform(16_000, samples);
        let g = RECOGNIZER.lock().unwrap();
        let rec = g.as_ref().unwrap();
        while rec.is_ready(&self.stream) {
            rec.decode(&self.stream);
        }
    }

    /// Current best-effort partial transcript. Cheap to call.
    pub fn partial(&self) -> String {
        let g = RECOGNIZER.lock().unwrap();
        match g.as_ref().unwrap().get_result(&self.stream) {
            Some(r) => r.text,
            None    => String::new(),
        }
    }

    /// Called once on shortcut release — flush trailing audio + return final.
    pub fn finalize(self) -> Result<String> {
        self.stream.input_finished();
        let g = RECOGNIZER.lock().unwrap();
        let rec = g.as_ref().unwrap();
        while rec.is_ready(&self.stream) {
            rec.decode(&self.stream);
        }
        let text = rec.get_result(&self.stream)
            .map(|r| r.text)
            .unwrap_or_default();
        Ok(text)
    }
}

/// 找 bundle 里的 sherpa-zh-en 资源。Tauri 安装到
/// `MouseClaw.app/Contents/Resources/models/sherpa-zh-en/<file>`。
/// 当前 exe 在 `MouseClaw.app/Contents/MacOS/mouseclaw`，资源在
/// `current_exe()/../../Resources/models/sherpa-zh-en/`。
/// Dev `cargo run` 走 `CARGO_MANIFEST_DIR/resources/sherpa-zh-en/` fallback。
fn bundle_resource_dir() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let bundled = exe.parent()?.parent()?.join("Resources").join("models").join(MODEL_DIR);
    if MODEL_FILES.iter().all(|f| bundled.join(f).exists()) {
        return Some(bundled);
    }
    // dev fallback —— 仓库里 src-tauri/resources/sherpa-zh-en/
    let dev = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("resources")
        .join(MODEL_DIR);
    if MODEL_FILES.iter().all(|f| dev.join(f).exists()) {
        return Some(dev);
    }
    None
}

/// 第一次启动从 app bundle 拷模型到 ~/.mouseclaw/models/sherpa-zh-en/。
/// 成功返回 true，资源不存在返回 false（dev 模式 / 未打包）。
fn try_seed_from_bundle() -> Result<bool> {
    let Some(src_dir) = bundle_resource_dir() else { return Ok(false); };
    let dest_dir = models_dir()?;
    std::fs::create_dir_all(&dest_dir)?;
    for file in MODEL_FILES {
        let src = src_dir.join(file);
        let dest = dest_dir.join(file);
        if dest.exists() { continue; }
        std::fs::copy(&src, &dest)
            .with_context(|| format!("seed copy {}", file))?;
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_dir_path_uses_home() {
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

    #[test]
    fn current_state_doesnt_panic_before_kickoff() {
        let _ = current_state();
    }
}
