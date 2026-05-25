//! Offline ASR via sherpa-onnx + SenseVoice (v0.7).
//!
//! 取代流式 zipformer 做听写 —— 中英混说质量高一档、**自带标点 + ITN**（use_itn）、
//! 多语种（zh/en/ja/ko/yue）。离线：按住录音缓冲 16k 音频 → 松手一次性转写。
//!
//! ### Model
//!   - sherpa-onnx-sense-voice-zh-en-ja-ko-yue-2024-07-17 (int8)
//!   - model.int8.onnx ~228MB + tokens.txt ~308KB
//!   - 下到 `~/.mouseclaw/models/sense-voice/`
//!
//! ### 为什么离线
//!   SenseVoice 是非流式模型（喂整段音频一次出结果）。听写本来就是松手才落字
//!   （流式 partial 只是 cosmetic 预览），故离线不影响打字流程，只是录音中显示
//!   "听写中…" 而非粗略实时预览。SenseVoice 推理快（数秒音频 ~1.5s），松手→出字跟手。

use std::path::PathBuf;
use std::sync::Mutex;
use anyhow::{anyhow, Result};
use once_cell::sync::Lazy;
use sherpa_onnx::{OfflineRecognizer, OfflineRecognizerConfig, OfflineSenseVoiceModelConfig};
use tauri::AppHandle;

/// 复用流式那边的下载状态枚举，避免重复定义。
pub use crate::transcribe_stream::ModelState;

pub const MODEL_DIR: &str = "sense-voice";
pub const MODEL_FILES: &[&str] = &["model.int8.onnx", "tokens.txt"];

pub fn active_spec() -> crate::model_downloader::ModelSpec {
    crate::model_downloader::sense_voice_spec()
}

/// Cached offline recognizer —— 首次 transcribe 加载，后续复用。
static RECOGNIZER: Lazy<Mutex<Option<OfflineRecognizer>>> = Lazy::new(|| Mutex::new(None));
static DOWNLOAD_STATE: Lazy<Mutex<Option<ModelState>>> = Lazy::new(|| Mutex::new(None));

pub fn current_state() -> Option<ModelState> {
    DOWNLOAD_STATE.lock().unwrap().clone()
}

fn models_dir() -> Result<PathBuf> {
    let home = std::env::var_os("HOME").ok_or_else(|| anyhow!("HOME not set"))?;
    Ok(PathBuf::from(home).join(".mouseclaw/models").join(MODEL_DIR))
}

/// 所有文件齐了（字节校验由 ModelSpec::is_ready 做）。
pub fn is_ready() -> bool { active_spec().is_ready() }

/// Lazy-load the OfflineRecognizer on first call.
fn ensure_loaded() -> Result<()> {
    let mut guard = RECOGNIZER.lock().unwrap();
    if guard.is_some() { return Ok(()); }
    let dir = models_dir()?;
    if !is_ready() {
        return Err(anyhow!(
            "SenseVoice model not ready under {} — wait for download",
            dir.display()
        ));
    }
    let mut config = OfflineRecognizerConfig::default();
    config.model_config.num_threads = crate::transcribe_stream::pick_inference_threads();
    config.model_config.sense_voice = OfflineSenseVoiceModelConfig {
        model: Some(dir.join("model.int8.onnx").to_string_lossy().into_owned()),
        language: Some("auto".into()),
        // use_itn=true → 原生标点 + 逆文本归一（「三点半」→「3:30」），不再需要单独标点模型。
        use_itn: true,
    };
    config.model_config.tokens = Some(
        dir.join("tokens.txt").to_string_lossy().into_owned(),
    );
    let rec = OfflineRecognizer::create(&config)
        .ok_or_else(|| anyhow!("OfflineRecognizer::create returned None"))?;
    *guard = Some(rec);
    println!("[mouseclaw] 🎤 SenseVoice offline recognizer loaded");
    Ok(())
}

/// 一次性转写整段 16k 单声道 f32 → 文本（自带标点）。CPU 密集同步调用，放专用线程跑。
pub fn transcribe(samples: &[f32]) -> Result<String> {
    ensure_loaded()?;
    let g = RECOGNIZER.lock().unwrap();
    let rec = g.as_ref().ok_or_else(|| anyhow!("recognizer not loaded"))?;
    let stream = rec.create_stream();
    stream.accept_waveform(16_000, samples);
    rec.decode(&stream);
    Ok(stream.get_result().map(|r| r.text).unwrap_or_default())
}

/// 后台预热（建 recognizer），避免首次松手时同步加载卡住。
pub fn preload() {
    std::thread::spawn(|| {
        if let Err(e) = ensure_loaded() {
            eprintln!("[mouseclaw] 🎤 SenseVoice preload skipped: {e}");
        }
    });
}

/// 启动时调一次：就绪→预热；缺→后台下载。
pub fn kick_off_download_if_missing(app: AppHandle) {
    let spec = active_spec();
    let was_ready = spec.is_ready();
    if was_ready {
        *DOWNLOAD_STATE.lock().unwrap() = Some(ModelState::Ready);
        preload();
    } else {
        *DOWNLOAD_STATE.lock().unwrap() = Some(ModelState::Downloading);
        println!("[mouseclaw] 🎤 {} 后台下载", spec.display);
    }
    let display = spec.display.to_string();
    tauri::async_runtime::spawn(async move {
        match crate::model_downloader::download(app, spec).await {
            Ok(()) => {
                *DOWNLOAD_STATE.lock().unwrap() = Some(ModelState::Ready);
                if !was_ready {
                    println!("[mouseclaw] 🎤 {display} 下载完成");
                    preload();
                }
            }
            Err(e) => {
                *DOWNLOAD_STATE.lock().unwrap() = Some(ModelState::Failed(e.to_string()));
                eprintln!("[mouseclaw] 🎤 {display} 下载失败: {e:#}");
            }
        }
    });
}
