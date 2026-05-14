//! User config persisted at `~/.mouseclaw/config.json`.
//!
//! Single source of truth for things the user picks once and rarely changes:
//! the global shortcut, default language hint, etc.

use std::path::PathBuf;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// Bump this whenever shortcut choices / config schema change in a way that
/// invalidates user's saved choice. Old configs auto-trigger re-Onboarding.
///   v1 → v2: Onboarding 选项从 4 个双击/按住 改成 2 个按住
///   v2 → v3: 弃用 Alt+Space（macOS 上 ⌥+空格 会打出字符，全局热键抢不到）
///   v3 → v4: 弃用 Control+Super+Space —— ⌃⌘空格 是 macOS「表情与符号」
///            系统快捷键，被系统抢走。换成 ⌘⇧空格 / ⌘⇧M
pub const CURRENT_CONFIG_VERSION: u32 = 4;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Canonical shortcut string, e.g. "Super+Shift+Space" / "Alt+Space".
    /// Parseable by tauri_plugin_global_shortcut::Shortcut::from_str.
    pub shortcut: String,
    /// Set to true the first time the user completes Onboarding. Until then,
    /// the app doesn't register a global shortcut — clicking the tray or
    /// launching the app re-opens the Onboarding window instead.
    #[serde(default)]
    pub onboarded: bool,
    /// Schema version. Saved configs older than CURRENT_CONFIG_VERSION get
    /// treated as not-onboarded so the user re-picks a shortcut.
    /// Pre-versioned configs default to 1 (the legacy schema).
    #[serde(default = "legacy_version")]
    pub version: u32,
}

fn legacy_version() -> u32 { 1 }

impl Default for Config {
    fn default() -> Self {
        Self {
            shortcut: "Control+Super+Space".into(),
            onboarded: false,
            version: CURRENT_CONFIG_VERSION,
        }
    }
}

fn config_path() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .map(|h| PathBuf::from(h).join(".mouseclaw/config.json"))
}

impl Config {
    pub fn load() -> Self {
        let Some(path) = config_path() else { return Self::default(); };
        let Ok(bytes) = std::fs::read(&path) else { return Self::default(); };
        let mut cfg: Self = serde_json::from_slice(&bytes).unwrap_or_default();
        // Schema migration: older configs had different shortcut choices.
        // Force re-Onboarding so the user picks a current valid combo.
        if cfg.version < CURRENT_CONFIG_VERSION {
            eprintln!(
                "[mouseclaw] config schema v{} < v{} → 重新走 Onboarding 让你选新快捷键",
                cfg.version, CURRENT_CONFIG_VERSION
            );
            cfg.onboarded = false;
            cfg.version = CURRENT_CONFIG_VERSION;
        }
        cfg
    }

    pub fn save(&self) -> Result<()> {
        let path = config_path().context("HOME not set")?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        let json = serde_json::to_vec_pretty(self)?;
        std::fs::write(&path, json).with_context(|| format!("write {}", path.display()))?;
        Ok(())
    }
}

/// Map Onboarding choice strings to canonical Tauri shortcut strings.
///
/// v0.1.5+: push-to-talk 语义——按住录音，松开发送。
///
/// **重要**：组合键不能产生字符，否则全局热键抢不到 / 被输入法吃掉：
///   - ❌ Alt+Space (⌥空格) → macOS 打出不间断空格
///   - ❌ 任何 单 modifier + 字母 → 容易被 app 内快捷键吃
///   - ✅ Control+Cmd+Space / Control+Cmd+M → 三键组合，不产生字符，
///        macOS 默认没占用，全局热键能稳定抢到
///
/// Tauri Shortcut 字符串：`Super`=⌘ `Alt`=⌥ `Control`=⌃ `Shift`=⇧；
/// 字母 `KeyA`..`KeyZ`；空格 `Space`。
pub fn choice_to_shortcut_str(choice: &str) -> &'static str {
    match choice {
        // 推荐：⌃⌘空格 — 三键组合不产生字符，macOS 默认空闲
        "double-option" | "hold-option" | "ctrl-cmd-space" => "Control+Super+Space",
        // 备选：⌃⌘M (M for MouseClaw)
        "double-cmd" | "hold-cmd" | "ctrl-cmd-m" => "Control+Super+KeyM",
        // 默认 fallback
        _ => "Control+Super+Space",
    }
}
