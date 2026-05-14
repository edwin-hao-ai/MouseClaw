//! Whisper transcription via whisper-rs (whisper.cpp Rust binding).
//!
//! Model: ggml-base-q5_1.bin (~57 MB, downloaded to ~/.mouseclaw/models/).
//! Configured for Chinese + English mixed input ("zh" auto-detects mixed CJK).
//!
//! The WhisperContext is loaded lazily once and cached for the rest of the
//! process lifetime — loading is ~200ms (mmap) but we don't want to pay it
//! per shortcut press.

use std::path::PathBuf;
use std::sync::Mutex;
use anyhow::{anyhow, Context, Result};
use once_cell::sync::Lazy;
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

static CONTEXT: Lazy<Mutex<Option<WhisperContext>>> = Lazy::new(|| Mutex::new(None));

fn model_path() -> Result<PathBuf> {
    let home = std::env::var_os("HOME").ok_or_else(|| anyhow!("no HOME"))?;
    let p = PathBuf::from(home).join(".mouseclaw/models/ggml-base-q5_1.bin");
    if !p.exists() {
        return Err(anyhow!(
            "Whisper model missing at {} — run setup or `curl -L -o ~/.mouseclaw/models/ggml-base-q5_1.bin https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base-q5_1.bin`",
            p.display()
        ));
    }
    Ok(p)
}

fn ensure_loaded() -> Result<()> {
    let mut guard = CONTEXT.lock().unwrap();
    if guard.is_none() {
        let path = model_path()?;
        let ctx = WhisperContext::new_with_params(
            path.to_str().context("model path not UTF-8")?,
            WhisperContextParameters::default(),
        )
        .context("WhisperContext::new")?;
        *guard = Some(ctx);
    }
    Ok(())
}

/// Run Whisper on 16 kHz mono f32 samples and return the transcription text.
/// Forces Chinese language detection but accepts mixed English content.
pub fn transcribe(samples: &[f32]) -> Result<String> {
    ensure_loaded()?;
    let guard = CONTEXT.lock().unwrap();
    let ctx = guard.as_ref().ok_or_else(|| anyhow!("Whisper context not loaded"))?;

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

/// True iff the Whisper model is available on disk.
pub fn is_available() -> bool {
    model_path().is_ok()
}

/// Whisper ggml-base-q5_1.bin SHA-256 checksum, captured at first download.
/// Used to verify integrity of bundled / re-downloaded model.
pub const MODEL_SHA256: &str =
    "422f1ae452ade6f30a004d7e5c6a43195e4433bc370bf23fac9cc591f01a8898";
pub const MODEL_URL: &str =
    "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base-q5_1.bin";
pub const MODEL_FILENAME: &str = "ggml-base-q5_1.bin";

/// State of the Whisper model: either ready or currently being fetched.
/// Wrap in a Mutex so the shortcut handler can check it and emit a helpful
/// bubble ("正在下载 Whisper 模型，57MB……") instead of a generic error.
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

/// On startup, check whether the Whisper model exists. If not, kick off a
/// background `curl` download to `~/.mouseclaw/models/ggml-base-q5_1.bin`.
/// Returns immediately; progress is tracked via DOWNLOAD_STATE.
///
/// This means first-launch users don't see an error on first shortcut press —
/// they see "正在下载 Whisper 模型…" and the pipeline retries on the next press.
pub fn kick_off_download_if_missing() {
    if is_available() {
        *DOWNLOAD_STATE.lock().unwrap() = Some(ModelState::Ready);
        return;
    }
    *DOWNLOAD_STATE.lock().unwrap() = Some(ModelState::Downloading);
    println!("[mouseclaw] Whisper 模型缺失，后台下载 {MODEL_URL} (~57MB)");

    std::thread::spawn(|| {
        let dest = match dest_path() {
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
        // Use curl — already present on every macOS install, no extra deps.
        let status = std::process::Command::new("curl")
            .args(["-fL", "--progress-bar", "-o"])
            .arg(&tmp)
            .arg(MODEL_URL)
            .status();
        match status {
            Ok(s) if s.success() => {
                if let Err(e) = std::fs::rename(&tmp, &dest) {
                    *DOWNLOAD_STATE.lock().unwrap() =
                        Some(ModelState::Failed(format!("rename: {e}")));
                    return;
                }
                println!("[mouseclaw] Whisper 模型下载完成 → {}", dest.display());
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

fn dest_path() -> Result<PathBuf> {
    let home = std::env::var_os("HOME").ok_or_else(|| anyhow!("no HOME"))?;
    Ok(PathBuf::from(home).join(".mouseclaw/models").join(MODEL_FILENAME))
}
