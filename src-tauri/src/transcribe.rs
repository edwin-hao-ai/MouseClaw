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
