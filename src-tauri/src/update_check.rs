//! 自动版本检查（v0.1.24）
//!
//! 启动后台拉一次 `https://edwin-hao-ai.github.io/MouseClaw/version.json`：
//! - latest 比当前 Cargo.toml 版本新 → emit `view-changed` Blocked 提示用户去下载
//! - 24h 只查一次，存 `~/.mouseclaw/last_update_check`
//! - 网络失败静默忽略（不打扰）

use anyhow::{Context, Result};
use serde::Deserialize;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, Manager};

const VERSION_URL: &str = "https://edwin-hao-ai.github.io/MouseClaw/version.json";
/// 24 小时
const CHECK_INTERVAL_SECS: u64 = 24 * 60 * 60;
/// 当前打包版本 —— Cargo.toml 改了就改这里
const CURRENT: &str = env!("CARGO_PKG_VERSION");

#[derive(Deserialize, Debug, Clone)]
pub struct VersionInfo {
    pub latest: String,
    #[serde(default)]
    pub dmg_url: Option<String>,
    #[serde(default)]
    pub release_url: Option<String>,
    #[serde(default)]
    pub notes_zh: Option<String>,
    #[serde(default)]
    pub notes_en: Option<String>,
}

/// 进程启动 60s 后跑一次（让 Onboarding / 主流程先稳）
pub fn spawn(app: AppHandle) {
    std::thread::Builder::new()
        .name("mouseclaw-update-check".into())
        .spawn(move || {
            std::thread::sleep(Duration::from_secs(60));
            if let Err(e) = check_once(&app) {
                eprintln!("[mouseclaw] update_check: {e}");
            }
        })
        .ok();
}

fn check_once(app: &AppHandle) -> Result<()> {
    let stamp = last_check_stamp().unwrap_or(0);
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    if now.saturating_sub(stamp) < CHECK_INTERVAL_SECS {
        return Ok(()); // 24h 内查过了
    }

    // ureq 是同步的，没装；用 curl 兜底
    let out = std::process::Command::new("curl")
        .args(["-fsSL", "--max-time", "8", VERSION_URL])
        .output()
        .context("curl failed")?;
    if !out.status.success() {
        anyhow::bail!("curl exit {}", out.status);
    }
    let body = String::from_utf8(out.stdout).context("not utf-8")?;
    let info: VersionInfo = serde_json::from_str(&body).context("parse json")?;

    // 写时间戳（即使比较失败也别下次再压网络）
    let _ = save_check_stamp(now);

    if !is_newer(&info.latest, CURRENT) {
        println!(
            "[mouseclaw] 🆗 version check: {CURRENT} is up to date (latest={})",
            info.latest
        );
        return Ok(());
    }

    println!(
        "[mouseclaw] 🆕 update available: {CURRENT} → {}",
        info.latest
    );
    let lang = crate::config::Config::load().language;
    let zh = lang != "en";
    let body_text = if zh {
        format!(
            "🆕 有新版本 v{} 可下载（当前 v{CURRENT}）。\n\n{}\n\n点托盘菜单「ℹ️ 关于 MouseClaw」里的下载链接，或直接：{}",
            info.latest,
            info.notes_zh.unwrap_or_default(),
            info.release_url.as_deref().unwrap_or("https://github.com/edwin-hao-ai/MouseClaw/releases/latest")
        )
    } else {
        format!(
            "🆕 New version v{} available (you're on v{CURRENT}).\n\n{}\n\nClick the menubar 🦞 → About to grab the link, or: {}",
            info.latest,
            info.notes_en.unwrap_or_default(),
            info.release_url.as_deref().unwrap_or("https://github.com/edwin-hao-ai/MouseClaw/releases/latest")
        )
    };
    // 用 Reply view 而不是 Blocked —— 不打断用户主流程，等他下次召唤就看到
    for (_, w) in app.webview_windows() {
        let _ = w.emit(
            crate::events::EV_VIEW_CHANGED,
            serde_json::json!({
                "kind": "reply",
                "transcript": "update available",
                "reply": body_text,
                "mode": "A",
                "streaming": false,
            }),
        );
    }
    Ok(())
}

fn last_check_stamp() -> Option<u64> {
    let path = stamp_path()?;
    std::fs::read_to_string(&path).ok()?.trim().parse().ok()
}

fn save_check_stamp(now: u64) -> Result<()> {
    let path = stamp_path().context("no HOME")?;
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    std::fs::write(&path, format!("{now}"))?;
    Ok(())
}

fn stamp_path() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    Some(PathBuf::from(home).join(".mouseclaw/last_update_check"))
}

/// "0.1.24" > "0.1.23"
fn is_newer(a: &str, b: &str) -> bool {
    let pa: Vec<u32> = a.split('.').filter_map(|p| p.parse().ok()).collect();
    let pb: Vec<u32> = b.split('.').filter_map(|p| p.parse().ok()).collect();
    for i in 0..pa.len().max(pb.len()) {
        let x = pa.get(i).copied().unwrap_or(0);
        let y = pb.get(i).copied().unwrap_or(0);
        if x > y {
            return true;
        }
        if x < y {
            return false;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn is_newer_basic() {
        assert!(is_newer("0.1.24", "0.1.23"));
        assert!(is_newer("0.2.0", "0.1.99"));
        assert!(is_newer("1.0.0", "0.9.9"));
        assert!(!is_newer("0.1.23", "0.1.23"));
        assert!(!is_newer("0.1.22", "0.1.23"));
        // 不同长度
        assert!(is_newer("0.1.24", "0.1"));
        assert!(!is_newer("0.1", "0.1.0"));
    }
}
