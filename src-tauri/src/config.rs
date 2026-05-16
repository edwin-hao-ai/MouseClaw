//! User config persisted at `~/.mouseclaw/config.json`.
//!
//! Single source of truth for things the user picks once and rarely changes:
//! the global shortcut, default language hint, etc.

use std::path::PathBuf;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::backend::Backend;
use crate::skins::SkinId;

/// 可选 Whisper 模型 —— 用户在托盘 / config 切。
/// 体积 / 中文质量 / 速度的取舍详见 transcribe.rs 顶部注释。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WhisperModel {
    Base,
    Small,
    Medium,
    Turbo,
}

impl Default for WhisperModel {
    // 默认仍是 Base —— 59MB 首装最快。想要 90% 中文准度的用户去托盘
    // 「🎙️ 语音模型」自己切到 Small (190MB)，后台下载即可。
    fn default() -> Self { WhisperModel::Base }
}

impl WhisperModel {
    pub fn filename(&self) -> &'static str {
        match self {
            WhisperModel::Base   => "ggml-base-q5_1.bin",
            WhisperModel::Small  => "ggml-small-q5_1.bin",
            WhisperModel::Medium => "ggml-medium-q5_0.bin",
            WhisperModel::Turbo  => "ggml-large-v3-turbo-q5_0.bin",
        }
    }
    pub fn url(&self) -> &'static str {
        match self {
            WhisperModel::Base   => "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base-q5_1.bin",
            WhisperModel::Small  => "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small-q5_1.bin",
            WhisperModel::Medium => "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-medium-q5_0.bin",
            WhisperModel::Turbo  => "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3-turbo-q5_0.bin",
        }
    }
    /// 下载体积（MB，approx）—— 给 UI / log 用
    pub fn size_mb(&self) -> u32 {
        match self {
            WhisperModel::Base => 59, WhisperModel::Small => 190,
            WhisperModel::Medium => 539, WhisperModel::Turbo => 547,
        }
    }
    pub fn display_name(&self) -> &'static str {
        match self {
            WhisperModel::Base   => "⚡ Base (59MB · 最快 / 一般)",
            WhisperModel::Small  => "✨ Small (190MB · 推荐 · 中文好)",
            WhisperModel::Medium => "🎯 Medium (539MB · 接近 large)",
            WhisperModel::Turbo  => "🚀 Turbo (547MB · 最准 · 占 RAM)",
        }
    }
    pub fn as_str(&self) -> &'static str {
        match self {
            WhisperModel::Base => "base", WhisperModel::Small => "small",
            WhisperModel::Medium => "medium", WhisperModel::Turbo => "turbo",
        }
    }
    pub fn from_str(s: &str) -> Self {
        match s {
            "small" => WhisperModel::Small,
            "medium" => WhisperModel::Medium,
            "turbo" => WhisperModel::Turbo,
            _ => WhisperModel::Base, // 兜底 base —— 跟 Default 对齐
        }
    }
    pub fn all() -> &'static [WhisperModel] {
        &[WhisperModel::Base, WhisperModel::Small, WhisperModel::Medium, WhisperModel::Turbo]
    }
    pub fn tray_menu_id(&self) -> String { format!("whisper:{}", self.as_str()) }
}

/// Bump this whenever shortcut choices / config schema change in a way that
/// invalidates user's saved choice. Old configs auto-trigger re-Onboarding.
///   v1 → v2: Onboarding 选项从 4 个双击/按住 改成 2 个按住
///   v2 → v3: 弃用 Alt+Space（macOS 上 ⌥+空格 会打出字符，全局热键抢不到）
///   v3 → v4: 弃用 Control+Super+Space（注释改了但代码没改 —— 见 v5）
///   v4 → v5: 真正弃用 Control+Super+Space —— ⌃⌘空格 是 macOS「表情与符号」
///            系统快捷键。换成 ⌘⇧空格 / ⌘⇧M（macOS 默认未占用）
///   v5 → v6: 新增多后端选择（backend 字段）—— Onboarding 多一步选 AI 后端
///   v6 → v7: 新增桌宠皮肤（skin 字段）—— Onboarding 多一步选老鼠风格
///   v7 → v8: 新增可选 Whisper 模型（whisper_model 字段，默认 small）
pub const CURRENT_CONFIG_VERSION: u32 = 8;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Canonical shortcut string, e.g. "Super+Shift+Space" / "Alt+Space".
    /// Parseable by tauri_plugin_global_shortcut::Shortcut::from_str.
    pub shortcut: String,
    /// 用户选的 AI 后端（Claude Code CLI / Codex CLI / OpenClaw CLI）。
    #[serde(default)]
    pub backend: Backend,
    /// 用户选的桌宠皮肤（6 款老鼠风格）。
    #[serde(default)]
    pub skin: SkinId,
    /// 用户选的 Whisper 转写模型（默认 Small —— 中文质量好且不超 RAM 上限）。
    #[serde(default)]
    pub whisper_model: WhisperModel,
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
            shortcut: "Super+Shift+Space".into(),
            backend: Backend::default(),
            skin: SkinId::default(),
            whisper_model: WhisperModel::default(),
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
/// **踩过的坑（按时间）：**
///   - ❌ Alt+Space (⌥空格)     → macOS 打出不间断空格字符，热键抢不到
///   - ❌ 单 modifier + 字母     → 容易被 app 内快捷键吃
///   - ❌ Control+Super+Space   → ⌃⌘空格 是 macOS「表情与符号」系统快捷键
///   - ✅ Super+Shift+Space     → ⌘⇧空格，macOS 默认未占用
///   - ✅ Super+Shift+KeyM      → ⌘⇧M，macOS 默认未占用
///
/// 注册仍可能失败（用户装了 Alfred/Raycast 等占了键）—— lib.rs setup() 里
/// register 失败会 log + 重弹 Onboarding 让用户换。
///
/// Tauri Shortcut 字符串：`Super`=⌘ `Alt`=⌥ `Control`=⌃ `Shift`=⇧；
/// 字母 `KeyA`..`KeyZ`；空格 `Space`。
pub fn choice_to_shortcut_str(choice: &str) -> &'static str {
    match choice {
        // 推荐：⌘⇧空格
        "double-option" | "hold-option" | "cmd-shift-space" => "Super+Shift+Space",
        // 备选：⌘⇧M (M for MouseClaw)
        "double-cmd" | "hold-cmd" | "cmd-shift-m" => "Super+Shift+KeyM",
        // 默认 fallback
        _ => "Super+Shift+Space",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;
    use tauri_plugin_global_shortcut::Shortcut;

    #[test]
    fn all_choice_strings_parse_as_valid_shortcuts() {
        for c in ["hold-option", "hold-cmd", "double-option", "double-cmd", "garbage"] {
            let s = choice_to_shortcut_str(c);
            assert!(
                Shortcut::from_str(s).is_ok(),
                "choice {c:?} → {s:?} 必须是合法快捷键"
            );
        }
    }

    #[test]
    fn unknown_choice_falls_back_to_default() {
        assert_eq!(choice_to_shortcut_str("whatever"), "Super+Shift+Space");
    }

    #[test]
    fn default_config_is_not_onboarded_and_current_version() {
        let c = Config::default();
        assert!(!c.onboarded);
        assert_eq!(c.version, CURRENT_CONFIG_VERSION);
        assert_eq!(c.backend, Backend::ClaudeCli);
        assert_eq!(c.skin, SkinId::Classic);
        // 默认 Base —— 体积最小（59MB），首装最快。
        assert_eq!(c.whisper_model, WhisperModel::Base);
    }

    #[test]
    fn whisper_models_round_trip_and_have_distinct_files() {
        let all = WhisperModel::all();
        assert_eq!(all.len(), 4);
        let mut files = std::collections::HashSet::new();
        for m in all {
            assert_eq!(WhisperModel::from_str(m.as_str()), *m, "{:?} round-trip", m);
            assert!(files.insert(m.filename()), "{:?} filename 重复", m);
            assert!(m.url().contains("huggingface.co"), "{:?} url 必须指向 hf", m);
            assert!(m.size_mb() > 0);
        }
    }

    #[test]
    fn whisper_unknown_falls_back_to_base() {
        assert_eq!(WhisperModel::from_str(""), WhisperModel::Base);
        assert_eq!(WhisperModel::from_str("nope"), WhisperModel::Base);
    }

    #[test]
    fn legacy_config_json_deserializes_with_defaults() {
        // 旧 config 没有 backend / version 字段 —— serde default 兜底
        let json = r#"{"shortcut":"Super+Shift+Space","onboarded":true}"#;
        let cfg: Config = serde_json::from_str(json).unwrap();
        assert_eq!(cfg.backend, Backend::ClaudeCli);
        assert_eq!(cfg.version, legacy_version());
    }
}
