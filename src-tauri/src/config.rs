//! User config persisted at `~/.mouseclaw/config.json`.
//!
//! Single source of truth for things the user picks once and rarely changes:
//! the global shortcut, default language hint, etc.

use std::path::PathBuf;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// Bump this whenever shortcut choices / config schema change in a way that
/// invalidates user's saved choice. Old configs auto-trigger re-Onboarding.
pub const CURRENT_CONFIG_VERSION: u32 = 2;

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
            shortcut: "Alt+Space".into(),
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
/// v0.1.5+: 全部改成 push-to-talk 语义——按住录音，松开发送。所以这些
/// 组合键得是用户能舒服「按住」的，不是双击/连点。
///
/// 测试过的 Tauri Shortcut 字符串格式：用 `Super` = ⌘、`Alt` = ⌥、`Control` = ⌃、
/// `Shift` = ⇧；字母用 `KeyA`...`KeyZ`；空格 `Space`、句号 `Period` 等。
pub fn choice_to_shortcut_str(choice: &str) -> &'static str {
    match choice {
        // 推荐：⌥+Space — 单手好按、不撞 Spotlight (⌘Space)
        "double-option" | "hold-option" => "Alt+Space",
        // 备选：⌘+⇧+Space — 之前的默认值
        "double-cmd"    | "hold-cmd"    => "Super+Shift+Space",
        // 默认 fallback
        _ => "Alt+Space",
    }
}
