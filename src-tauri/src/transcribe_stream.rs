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
use tauri::AppHandle;

/// Model relative dir under ~/.mouseclaw/models/ —— **中文模型**
pub const MODEL_DIR: &str = "sherpa-zh-en";
/// 中文模型文件名（4 个）—— 用于 ensure_loaded / bundle seed 拼路径
pub const MODEL_FILES: &[&str] = &[
    "encoder-epoch-99-avg-1.int8.onnx",
    "decoder-epoch-99-avg-1.onnx",
    "joiner-epoch-99-avg-1.int8.onnx",
    "tokens.txt",
];

/// v0.4.0 P0：锁死 zh-en 双语模型 —— 中英混说零切换。
/// 旧 config.voice_lang ("zh" / "en") 保留字段做向后兼容但**被忽略**。
/// 原因：sherpa zh-en Zipformer 本身双语联合训练，比单语模型在 code-switch
/// 场景体感强很多；强制让用户二选一是历史包袱，UX 上是个 bug。
pub fn active_lang() -> String { "zh-en".into() }
fn is_en() -> bool { false }
pub fn active_model_dir() -> &'static str { MODEL_DIR }
pub fn active_model_files() -> &'static [&'static str] { MODEL_FILES }
pub fn active_spec() -> crate::model_downloader::ModelSpec {
    crate::model_downloader::zh_en_spec()
}

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
    Ok(PathBuf::from(home).join(".mouseclaw/models").join(active_model_dir()))
}

/// True iff active 语言模型的所有文件齐了（字节校验由 ModelSpec::is_ready 做）
pub fn is_ready() -> bool { active_spec().is_ready() }

/// 启动时调一次 (v0.4.0)：
///   1. active 语言模型已就绪 → Ready，跳过
///   2. App bundle 里仍有打包中文模型（dev / 老 DMG）→ seed 兜底
///   3. 都没有 → 后台 `model_downloader::download(active_spec())`
pub fn kick_off_download_if_missing(app: AppHandle) {
    let spec = active_spec();
    let was_ready = spec.is_ready();
    if was_ready {
        *DOWNLOAD_STATE.lock().unwrap() = Some(ModelState::Ready);
    } else {
        // dev / 老 DMG bundle 兜底（仅中文 —— 英文模型从来没塞过 bundle）
        if !is_en() && try_seed_from_bundle().unwrap_or(false) && spec.is_ready() {
            *DOWNLOAD_STATE.lock().unwrap() = Some(ModelState::Ready);
            println!("[mouseclaw] 🎤 sherpa zh-en 从 bundle seed");
        } else {
            *DOWNLOAD_STATE.lock().unwrap() = Some(ModelState::Downloading);
            println!("[mouseclaw] 🎤 {} 后台下载", spec.display);
        }
    }

    // 永远走一遍 download() —— ready 时它会立即 emit "ok"（给 DownloaderView），
    // 不 ready 时正常跑下载链路。两路统一减少 bug。
    let display = spec.display.to_string();
    tauri::async_runtime::spawn(async move {
        match crate::model_downloader::download(app, spec).await {
            Ok(()) => {
                *DOWNLOAD_STATE.lock().unwrap() = Some(ModelState::Ready);
                if !was_ready {
                    println!("[mouseclaw] 🎤 {display} 下载完成");
                }
            }
            Err(e) => {
                *DOWNLOAD_STATE.lock().unwrap() =
                    Some(ModelState::Failed(e.to_string()));
                eprintln!("[mouseclaw] 🎤 {display} 下载失败: {e:#}");
            }
        }
    });
}

/// Lazy-load the OnlineRecognizer on first call.
fn ensure_loaded() -> Result<()> {
    let mut guard = RECOGNIZER.lock().unwrap();
    if guard.is_some() { return Ok(()); }

    let dir = models_dir()?;
    if !is_ready() {
        return Err(anyhow!(
            "sherpa model not ready under {} — wait for download",
            dir.display()
        ));
    }

    let files = active_model_files();
    let mut config = OnlineRecognizerConfig::default();
    config.model_config.transducer.encoder = Some(
        dir.join(files[0]).to_string_lossy().into_owned(),
    );
    config.model_config.transducer.decoder = Some(
        dir.join(files[1]).to_string_lossy().into_owned(),
    );
    config.model_config.transducer.joiner = Some(
        dir.join(files[2]).to_string_lossy().into_owned(),
    );
    config.model_config.tokens = Some(
        dir.join(files[3]).to_string_lossy().into_owned(),
    );
    // v0.3.1 · push-to-talk 模式不要 endpoint 自动切段 —— 用户主动控制开始/结束
    // 如果开 endpoint，sherpa 在停顿时会自动 commit 一段并 reset stream，
    // 之后 get_result 只返回新段的文字（旧段被丢掉），用户感觉"流式断了"。
    config.enable_endpoint = false;

    // v0.4.0 P1 · 术语库（hotwords contextual biasing）
    // 1. 重新生成 active.txt（合并内置 + 用户词表）
    // 2. 如果非空，注入 hotwords_file 并切到 modified_beam_search
    //    （greedy_search 不支持 hotwords — sherpa 文档明确说明）
    // 3. 词表空 → 退回 greedy（性能更优一点点）
    let cfg = crate::config::Config::load();
    let active_count = crate::vocab::regenerate_active(cfg.vocab_builtin_enabled)
        .unwrap_or(0);
    let active_path = crate::vocab::active_file_path().ok();
    if active_count > 0 && active_path.as_ref().map(|p| p.exists()).unwrap_or(false) {
        config.decoding_method = Some("modified_beam_search".into());
        config.max_active_paths = 4;
        config.hotwords_file = active_path.map(|p| p.to_string_lossy().into_owned());
        config.hotwords_score = crate::vocab::DEFAULT_HOTWORDS_SCORE;
        println!(
            "[mouseclaw] 🎤 hotwords loaded: {} entries, score={}",
            active_count, crate::vocab::DEFAULT_HOTWORDS_SCORE
        );
    } else {
        config.decoding_method = Some("greedy_search".into());
    }

    let rec = OnlineRecognizer::create(&config)
        .ok_or_else(|| anyhow!("OnlineRecognizer::create returned None"))?;
    *guard = Some(rec);
    println!("[mouseclaw] 🎤 sherpa streaming recognizer loaded");
    Ok(())
}

/// v0.4.0 P1 · 强制下次 transcribe 重建 recognizer ——
/// 用于词表/术语库改了之后让 sherpa 重读 hotwords_file。
/// 当前正在录音的 stream 不受影响（trade-off，OK）。
pub fn invalidate_recognizer() {
    let mut guard = RECOGNIZER.lock().unwrap();
    if guard.is_some() {
        println!("[mouseclaw] 🎤 recognizer invalidated (vocab reload)");
        *guard = None;
    }
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
    // dev fallback —— 仅 debug build。release 二进制不走，否则开发机上 release
    // DMG 会读到 CARGO_MANIFEST_DIR/resources/ 里残留的开发期模型，导致下载链路
    // 永远不被触发（用户实测 2026-05-19）
    #[cfg(debug_assertions)]
    {
        let dev = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("resources")
            .join(MODEL_DIR);
        if MODEL_FILES.iter().all(|f| dev.join(f).exists()) {
            return Some(dev);
        }
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
