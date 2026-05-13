//! 截屏：调 macOS 自带的 `screencapture` CLI 抓主屏完整画面。
//! Day 1 简化策略——抓主屏整张；Day 2+ 再考虑「只抓活动窗口」。
//! 用户原话拒绝了 300×300 局部："如果是针对一个网站呢，300*300啥也看不明白"。

use std::path::PathBuf;
use anyhow::{bail, Context, Result};

/// 截当前主屏，落盘到 `/tmp/mouseclaw-frame-{timestamp}.png`，返回路径。
///
/// `-x` 静音（不出快门声音）
/// `-o` 不包含窗口阴影
/// 不加 `-i` / `-W`：那些是交互式的，会卡住 daemon。
pub async fn capture_main_screen() -> Result<PathBuf> {
    let ts = chrono::Local::now().format("%Y%m%d-%H%M%S-%3f").to_string();
    let path = std::env::temp_dir().join(format!("mouseclaw-frame-{ts}.png"));

    let status = tokio::process::Command::new("screencapture")
        .args(["-x", "-o"])
        .arg(&path)
        .status()
        .await
        .context("failed to spawn `screencapture`")?;

    if !status.success() {
        bail!("`screencapture` exited with status {status}");
    }
    if !path.exists() {
        bail!("screencapture reported success but no file at {}", path.display());
    }
    Ok(path)
}
