//! 中英文标点补全 (v0.3.6)
//!
//! sherpa-onnx CT-Transformer 标点模型 —— 给纯文本补 `。，？！`。
//! 100% 本地推理（onnxruntime CPU + CoreML），~10ms/句，0 token 成本。
//!
//! ## 为什么需要这个模块
//! sherpa Zipformer streaming ASR 训练数据是裸文本，输出**没有标点**。
//! 之前 Whisper Small 自带标点，迁移后用户立刻反馈"标点没了"。
//! LLM polish 走过一遍但慢且费钱 —— 用户明确否决（语：「语音打字 = 节省时间」）。
//! 这个本地小模型补回标点，零成本零等待 = 唯一正解。
//!
//! ## Model
//!   - sherpa-onnx-punct-ct-transformer-zh-en-vocab272727-2024-04-12-int8
//!   - 72 MB 量化版 · 双语 zh-en · 加 。，？
//!   - bundle 进 DMG 的 `Resources/models/sherpa-punct/model.int8.onnx`
//!   - 首次启动 try_seed_from_bundle 拷到 `~/.mouseclaw/models/sherpa-punct/`
//!
//! ## 失败兜底
//!   - 模型 load 失败（理论上不该发生）→ 返回原文，主流程不挂
//!   - add_punctuation 内部错误 → 返回原文
//!   - 调用方拿到结果直接用即可，无需 Result 处理

use std::path::PathBuf;
use std::sync::Mutex;
use anyhow::{anyhow, Context, Result};
use once_cell::sync::Lazy;
use sherpa_onnx::{OfflinePunctuation, OfflinePunctuationConfig};

pub const MODEL_DIR: &str = "sherpa-punct";
pub const MODEL_FILE: &str = "model.int8.onnx";

/// 缓存的 punctuator —— 首次调用时加载，后续复用。
static PUNCTUATOR: Lazy<Mutex<Option<OfflinePunctuation>>> = Lazy::new(|| Mutex::new(None));

fn model_dir() -> Result<PathBuf> {
    let home = std::env::var_os("HOME").ok_or_else(|| anyhow!("HOME not set"))?;
    Ok(PathBuf::from(home).join(".mouseclaw/models").join(MODEL_DIR))
}

fn model_path() -> Result<PathBuf> {
    Ok(model_dir()?.join(MODEL_FILE))
}

pub fn is_ready() -> bool {
    model_path().ok().map(|p| p.exists()).unwrap_or(false)
}

/// 首次启动从 app bundle 拷到 ~/.mouseclaw/models/sherpa-punct/。
/// 同 transcribe_stream::try_seed_from_bundle 的模式。
pub fn try_seed_from_bundle() -> Result<bool> {
    let src = match bundle_resource_path() {
        Some(p) => p,
        None => return Ok(false),
    };
    let dest_dir = model_dir()?;
    let dest = dest_dir.join(MODEL_FILE);
    if dest.exists() { return Ok(true); }
    std::fs::create_dir_all(&dest_dir)?;
    std::fs::copy(&src, &dest).with_context(|| format!("seed copy {MODEL_FILE}"))?;
    println!("[mouseclaw] 🎯 sherpa-punct seeded from bundle → {}", dest.display());
    Ok(true)
}

/// v0.4.0 · 标点模型按需下载 —— 启动时调一次。
/// 优先 bundle seed（dev / 老 DMG），缺失则走 model_downloader（GitHub 国内代理 →
/// ghfast → github）。失败不阻塞主流程（add_punctuation 自带 fallback 返回原文）。
pub fn kick_off_download_if_missing(app: tauri::AppHandle) {
    // bundle seed 兜底（dev / 老 DMG）—— 但永远走一遍 download()，让 DownloaderView 能拿到 "ok"
    let _ = try_seed_from_bundle();
    if !is_ready() {
        println!("[mouseclaw] 🎯 sherpa-punct 后台下载 (~62MB tarball)");
    }
    tauri::async_runtime::spawn(async move {
        let spec = crate::model_downloader::punct_spec();
        if let Err(e) = crate::model_downloader::download(app, spec).await {
            eprintln!("[mouseclaw] 🎯 sherpa-punct 下载失败: {e:#}（add_punctuation 会 fallback 到原文）");
        }
    });
}

/// app bundle 里的资源路径：`<App.app>/Contents/Resources/models/sherpa-punct/`
fn bundle_resource_path() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let bundled = exe.parent()?.parent()?
        .join("Resources").join("models").join(MODEL_DIR).join(MODEL_FILE);
    if bundled.exists() { return Some(bundled); }
    // dev fallback —— 仅 debug build（参考 transcribe_stream 同名注释）
    #[cfg(debug_assertions)]
    {
        let dev = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("resources").join(MODEL_DIR).join(MODEL_FILE);
        if dev.exists() { return Some(dev); }
    }
    None
}

fn ensure_loaded() -> Result<()> {
    let mut guard = PUNCTUATOR.lock().unwrap();
    if guard.is_some() { return Ok(()); }
    let path = model_path()?;
    if !path.exists() {
        return Err(anyhow!("punct model not ready under {}", path.display()));
    }
    let mut config = OfflinePunctuationConfig::default();
    config.model.ct_transformer = Some(path.to_string_lossy().into_owned());
    // v0.4.2 · 性能：offline 标点是整句一次性推理，多线程收益比流式更明显。
    // 默认 num_threads=1 → 提到一半核（clamp 2..=4）。provider 仍 cpu（同 ASR 理由）。
    let threads = crate::transcribe_stream::pick_inference_threads();
    config.model.num_threads = threads;
    println!("[mouseclaw] 🎯 punctuation num_threads={threads}");
    let p = OfflinePunctuation::create(&config)
        .ok_or_else(|| anyhow!("OfflinePunctuation::create returned None"))?;
    *guard = Some(p);
    println!("[mouseclaw] 🎯 punctuation model loaded");
    Ok(())
}

/// 给文本补标点 —— 主入口。失败 / 不可用时静默返回原文（不阻断主流程）。
///
/// 调用契约：调用方拿到的 String 永远可用：
///   - 如果模型 OK → 带标点的版本
///   - 模型不在 / 模型挂了 → 原文（毫秒级 fallback）
///
/// 速度：~10ms / 短句，主线程跑也可以；长句仍建议在 spawn_blocking 里跑。
pub fn add_punctuation(text: &str) -> String {
    let trimmed = text.trim();
    if trimmed.is_empty() { return text.to_string(); }
    // 已经有终止标点了就不再走模型（粗判：避免短句 "你好" 被加成 "你好。"）
    // 实际上有标点的输入再过模型也安全，但省一次推理
    if let Some(last) = trimmed.chars().last() {
        if "。.!?！？".contains(last) {
            return text.to_string();
        }
    }
    if ensure_loaded().is_err() { return text.to_string(); }
    let guard = PUNCTUATOR.lock().unwrap();
    let Some(p) = guard.as_ref() else { return text.to_string(); };
    match p.add_punctuation(trimmed) {
        Some(out) if !out.trim().is_empty() => out,
        _ => text.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_path_uses_home() {
        if let Ok(p) = model_path() {
            assert!(p.ends_with(MODEL_FILE));
        }
    }

    #[test]
    fn add_punctuation_safe_on_empty() {
        assert_eq!(add_punctuation(""), "");
        assert_eq!(add_punctuation("   "), "   ");
    }

    #[test]
    fn add_punctuation_skips_text_already_ending_with_terminator() {
        // 已有 。 不应再处理
        let s = "你好。";
        assert_eq!(add_punctuation(s), s);
    }
}
