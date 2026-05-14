//! User config persisted at `~/.mouseclaw/config.json`.
//!
//! Single source of truth for things the user picks once and rarely changes:
//! the global shortcut, default language hint, etc.

use std::path::PathBuf;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

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
}

impl Default for Config {
    fn default() -> Self {
        Self {
            shortcut: "Super+Shift+Space".into(),
            onboarded: false,
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
        serde_json::from_slice(&bytes).unwrap_or_default()
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

/// Map the Onboarding choice strings (matching ShortcutChoice in src/types.ts)
/// to canonical Tauri shortcut strings.
///
/// Native double-tap detection isn't supported by the plugin (requires CGEventTap)
/// so we approximate "double X" with a sensible 2-key combo that minimises clash
/// with system shortcuts.
pub fn choice_to_shortcut_str(choice: &str) -> &'static str {
    match choice {
        // "double-option" 用户期望双击 ⌥；近似为 ⌥⌘L（龙虾 lobster）
        "double-option" => "Alt+Super+KeyL",
        // 按住 Option：⌥+空格（Spotlight 是 ⌘+空格，正交不冲突）
        "hold-option"   => "Alt+Space",
        // 双击 Cmd：⌘+. (period) 是一个少用的组合，不易冲突
        "double-cmd"    => "Super+Period",
        // 按住 Cmd：⌘+⇧+M（M for MouseClaw）
        "hold-cmd"      => "Super+Shift+KeyM",
        // 默认 / 未识别：⌘+⇧+空格（Day 1 测试用）
        _               => "Super+Shift+Space",
    }
}
