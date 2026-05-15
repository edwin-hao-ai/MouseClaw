//! Whisper transcription via whisper-rs (whisper.cpp Rust binding).
//!
//! v0.1.8: 用户可选模型大小（在 config / 托盘里切）
//!   - base   59MB  · 速度最快，中文质量一般（默认值，兜底）
//!   - small  190MB · 中文质量大跳，M-series 上仍 ~15× 实时（**推荐**）
//!   - medium 539MB · 接近 large 质量，速度还 OK
//!   - turbo  547MB · 实时性 + 准度最佳，但 RAM 占用接近 600MB 上限
//!
//! 切换模型 = 触发新一次后台下载（如缺失）+ 下次转写时重建 WhisperContext。

use std::path::PathBuf;
use std::sync::Mutex;
use anyhow::{anyhow, Context, Result};
use once_cell::sync::Lazy;
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

/// 当前 active 模型 —— 由 config 灌入 + 托盘切换更新。
/// 用 RwLock 不用 Mutex：读多写少（每次 transcribe 都要读）。
static ACTIVE_MODEL: Lazy<std::sync::RwLock<crate::config::WhisperModel>> =
    Lazy::new(|| std::sync::RwLock::new(crate::config::Config::load().whisper_model));

static CONTEXT: Lazy<Mutex<Option<(crate::config::WhisperModel, WhisperContext)>>> =
    Lazy::new(|| Mutex::new(None));

pub fn set_active_model(m: crate::config::WhisperModel) {
    let prev = *ACTIVE_MODEL.read().unwrap();
    if prev == m {
        return;
    }
    *ACTIVE_MODEL.write().unwrap() = m;
    // 丢掉缓存的 ctx —— 下次 transcribe 会重新加载新模型
    *CONTEXT.lock().unwrap() = None;
    println!("[mouseclaw] Whisper 模型切到 {:?}", m);
    // 如果新模型还没下载就 kick off
    kick_off_download_if_missing();
}

fn active_model() -> crate::config::WhisperModel {
    *ACTIVE_MODEL.read().unwrap()
}

fn model_path() -> Result<PathBuf> {
    model_path_for(active_model())
}

fn model_path_for(m: crate::config::WhisperModel) -> Result<PathBuf> {
    let home = std::env::var_os("HOME").ok_or_else(|| anyhow!("no HOME"))?;
    let p = PathBuf::from(home).join(".mouseclaw/models").join(m.filename());
    if !p.exists() {
        return Err(anyhow!(
            "Whisper {:?} model missing at {} — 后台正在下载，稍候再试 / 或手动跑：\n  curl -L -o {} {}",
            m, p.display(), p.display(), m.url()
        ));
    }
    Ok(p)
}

fn ensure_loaded() -> Result<()> {
    let target = active_model();
    let mut guard = CONTEXT.lock().unwrap();
    // 命中：模型匹配 → 复用
    if let Some((cached_model, _)) = guard.as_ref() {
        if *cached_model == target {
            return Ok(());
        }
    }
    // 否则重建
    let path = model_path_for(target)?;
    let ctx = WhisperContext::new_with_params(
        path.to_str().context("model path not UTF-8")?,
        WhisperContextParameters::default(),
    )
    .context("WhisperContext::new")?;
    *guard = Some((target, ctx));
    println!("[mouseclaw] Whisper ctx loaded ({:?}, {} MB on disk)",
             target, target.size_mb());
    Ok(())
}

/// Run Whisper on 16 kHz mono f32 samples and return the transcription text.
/// Forces Chinese language detection but accepts mixed English content.
pub fn transcribe(samples: &[f32]) -> Result<String> {
    ensure_loaded()?;
    let guard = CONTEXT.lock().unwrap();
    let (_, ctx) = guard.as_ref().ok_or_else(|| anyhow!("Whisper context not loaded"))?;

    let mut state = ctx.create_state().context("create_state")?;
    let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
    params.set_language(Some("zh"));
    params.set_translate(false);
    params.set_print_special(false);
    params.set_print_progress(false);
    params.set_print_realtime(false);
    params.set_print_timestamps(false);
    // Reasonable speed/quality default
    params.set_n_threads(4);

    state.full(params, samples).context("whisper full() failed")?;

    let n = state.full_n_segments().context("full_n_segments")?;
    let mut text = String::new();
    for i in 0..n {
        if let Ok(seg) = state.full_get_segment_text(i) {
            text.push_str(&seg);
        }
    }
    Ok(text.trim().to_string())
}

/// True iff the **currently selected** Whisper model is available on disk.
pub fn is_available() -> bool {
    model_path().is_ok()
}

/// Legacy const — 给 lib.rs 启动日志用，仍指向 base（兜底文件名）。
/// 新代码请走 `active_model().filename()` / `active_model().url()`。
pub const MODEL_FILENAME: &str = "ggml-base-q5_1.bin";
pub const MODEL_URL: &str =
    "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base-q5_1.bin";

/// State of the Whisper model: either ready or currently being fetched.
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

/// 当前 active 模型缺失就后台下载（用 curl，零依赖）。切换模型时也会再调一次。
pub fn kick_off_download_if_missing() {
    let target = active_model();
    if is_available() {
        *DOWNLOAD_STATE.lock().unwrap() = Some(ModelState::Ready);
        return;
    }
    *DOWNLOAD_STATE.lock().unwrap() = Some(ModelState::Downloading);
    println!(
        "[mouseclaw] Whisper {:?} 模型缺失，后台下载 {} (~{}MB)",
        target,
        target.url(),
        target.size_mb()
    );

    std::thread::spawn(move || {
        let dest = match dest_path_for(target) {
            Ok(p) => p,
            Err(e) => {
                *DOWNLOAD_STATE.lock().unwrap() =
                    Some(ModelState::Failed(format!("路径错误：{e}")));
                return;
            }
        };
        if let Some(parent) = dest.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let tmp = dest.with_extension("bin.tmp");
        let status = std::process::Command::new("curl")
            .args(["-fL", "--progress-bar", "-o"])
            .arg(&tmp)
            .arg(target.url())
            .status();
        match status {
            Ok(s) if s.success() => {
                if let Err(e) = std::fs::rename(&tmp, &dest) {
                    *DOWNLOAD_STATE.lock().unwrap() =
                        Some(ModelState::Failed(format!("rename: {e}")));
                    return;
                }
                println!("[mouseclaw] Whisper {:?} 下载完成 → {}", target, dest.display());
                *DOWNLOAD_STATE.lock().unwrap() = Some(ModelState::Ready);
            }
            Ok(s) => {
                *DOWNLOAD_STATE.lock().unwrap() =
                    Some(ModelState::Failed(format!("curl 退出 {s}")));
            }
            Err(e) => {
                *DOWNLOAD_STATE.lock().unwrap() =
                    Some(ModelState::Failed(format!("curl 启动失败: {e}")));
            }
        }
    });
}

fn dest_path_for(m: crate::config::WhisperModel) -> Result<PathBuf> {
    let home = std::env::var_os("HOME").ok_or_else(|| anyhow!("no HOME"))?;
    Ok(PathBuf::from(home).join(".mouseclaw/models").join(m.filename()))
}
