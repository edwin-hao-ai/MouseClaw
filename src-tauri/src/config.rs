//! User config persisted at `~/.mouseclaw/config.json`.
//!
//! Single source of truth for things the user picks once and rarely changes:
//! the global shortcut, default language hint, etc.

use std::path::PathBuf;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::backend::Backend;
use crate::skins::SkinId;

// v0.3 · WhisperModel + 模型切换器整套删除（Whisper 已退役）。sherpa-onnx zh-en
// 是唯一 ASR backend，模型打进 bundle，无需用户可选 model picker。

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
///   v8 → v9: 新增 language 字段（i18n · zh / en），默认 zh
///   v9 → v10: 新增 tidy_up_enabled（Typeless 套路），默认 false
///   v10 → v11: 新增 voice_ime_enabled（长按 fn 写到光标），默认 true
///   v11 → v12: 新增 voice_ime_trigger（可选 fn/option/control/right-*），默认 fn
///   v12 → v13: 新增 clipboard_paused（剪贴板暂停开关），默认 false
///   v13 → v14: 新增 workspace_path（AI 调用时的 cwd），默认 None
///   v14 → v15: 新增 pet_anchor（桌宠悬停位置，5 选项），默认 bottom-right
///   v15 → v16: Whisper 默认从 Base 升到 Small（中文更准）。
///              现有 config 上若仍是 Base，迁移时自动改 Small —— bundled Base
///              做 fallback，体验不变但准度大跳。
///   v16 → v17: Whisper 整套删除（whisper_model 字段 + WhisperModel enum）。
///              sherpa-onnx zh-en bundled in DMG，无 user-facing model picker。
pub const CURRENT_CONFIG_VERSION: u32 = 19;

/// 桌宠悬停位置 (v0.1.27) —— overlay 闲置时停哪儿打盹。
/// 召唤快捷键触发时仍然跑到光标位置工作，完事 auto-hide 后回到 anchor。
/// `Follow` = 跟随光标（不归位），费 CPU 更明显，留作高级选项。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PetAnchor {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
    Follow,
    /// v0.1.28 · 完全不显示桌宠（除被快捷键召唤 / nudge 触发时）。
    /// 给"我只想要静默工具，不要伴侣"的用户用。
    Hidden,
}

impl Default for PetAnchor {
    fn default() -> Self { PetAnchor::BottomRight }
}

impl PetAnchor {
    pub fn as_str(&self) -> &'static str {
        match self {
            PetAnchor::TopLeft     => "top-left",
            PetAnchor::TopRight    => "top-right",
            PetAnchor::BottomLeft  => "bottom-left",
            PetAnchor::BottomRight => "bottom-right",
            PetAnchor::Follow      => "follow",
            PetAnchor::Hidden      => "hidden",
        }
    }
    pub fn from_str(s: &str) -> Self {
        match s {
            "top-left"     => PetAnchor::TopLeft,
            "top-right"    => PetAnchor::TopRight,
            "bottom-left"  => PetAnchor::BottomLeft,
            "follow"       => PetAnchor::Follow,
            "hidden"       => PetAnchor::Hidden,
            _              => PetAnchor::BottomRight, // 默认兜底
        }
    }
    pub fn all() -> &'static [PetAnchor] {
        &[PetAnchor::TopLeft, PetAnchor::TopRight,
          PetAnchor::BottomLeft, PetAnchor::BottomRight,
          PetAnchor::Follow, PetAnchor::Hidden]
    }
    pub fn tray_menu_id(&self) -> String { format!("anchor:{}", self.as_str()) }
    /// 闲置时是否要让 overlay 持续可见地停在那个角落。
    /// 4 个角 = true（桌宠"住"在角落）
    /// Follow = false（由 cursor_follow 接管显示）
    /// Hidden = false（彻底隐身，召唤时才出现）
    pub fn pin_visible_when_idle(&self) -> bool {
        matches!(self,
            PetAnchor::TopLeft  | PetAnchor::TopRight |
            PetAnchor::BottomLeft | PetAnchor::BottomRight)
    }
}

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
    // whisper_model removed in v0.3 — sherpa zh-en is now the sole bundled ASR.
    // Old configs with this field deserialize fine (serde ignores unknown fields).
    /// UI 语言（i18n · zh / en）。默认 zh —— 与原默认中文体验对齐。
    /// 加新语言：枚举改成 string + 前端 LANGUAGES 表加项即可，Rust 这边不卡。
    #[serde(default = "default_language")]
    pub language: String,
    /// v0.4.0 P0 起锁死 "zh-en" —— sherpa Zipformer 本身就是双语联合训练，
    /// 让用户二选一反而损失体感（混说就崩）。字段保留是为了向后兼容，
    /// 老 config 里的 "zh" / "en" 都会被 transcribe_stream 直接忽略。
    /// 不同于上面 `language`（UI 文案 i18n），voice_lang 只决定**模型**。
    #[serde(default = "default_voice_lang")]
    pub voice_lang: String,
    /// v0.4.0 P1 · 内置程序员词表是否注入 sherpa hotwords contextual biasing。
    /// 默认开 —— 装上即享受 "useEffect / Tauri / Claude" 等 80+ 程序员高频词
    /// 的识别提升。用户托盘里能关。详见 `vocab.rs`。
    #[serde(default = "default_vocab_builtin_enabled")]
    pub vocab_builtin_enabled: bool,
    /// v0.4.0 · 首次使用引导是否已完成 —— 模型下载完后桌宠主动跳出来教用户用一次。
    /// 用户完成或跳过都设 true；PetMenu 的「📖 教我用 MouseClaw」可强制重启。
    #[serde(default)]
    pub firstrun_tour_done: bool,
    // tidy_up_enabled removed in v0.3.4 — LLM polish deleted (slow + costly,
    // violates voice-typing's speed-first value). Old configs silently ignore
    // the field via serde's default unknown-field behavior.
    /// v0.3.6 · 用户拖动桌宠到屏幕某处后保存的窗口坐标 (logical pt, top-left origin)。
    /// Some 时优先于 pet_anchor —— 让"拖到这"压过 anchor。
    /// 用户在托盘 anchor 子菜单点任一选项 → 自动清空回到 None。
    #[serde(default)]
    pub pet_custom_position: Option<(f64, f64)>,
    /// v0.4.0 · 桌宠开口说话 —— AI 回复完成时用 macOS `say` 朗读
    /// 默认关 = 安静。开启从托盘「🔊 桌宠开口说话」即可。
    #[serde(default)]
    pub tts_enabled: bool,
    /// v0.4.x · 桌宠音效 —— 程序化 chiptune（睡觉鼾声 / 完成提示音 / 撞墙等）。
    /// 前端 petAudio 用 Web Audio 实时合成，按皮肤物种换嗓音；这俩字段只是开关 + 音量。
    /// 默认开 + 音量 0.45（很轻）—— 用户在托盘「🔉 桌宠音效」一键关。
    /// 不 bump CURRENT_CONFIG_VERSION：serde default 兜底，老 config 无痛升级、不重走 Onboarding。
    #[serde(default = "default_sfx_enabled")]
    pub sfx_enabled: bool,
    #[serde(default = "default_sfx_volume")]
    pub sfx_volume: f32,
    /// 长按 fn → 语音输入到光标（v0.1.11 voice IME）
    /// 默认开 —— 用户长按 fn 才触发，短按 fn 仍走 macOS 原生行为
    #[serde(default = "default_voice_ime")]
    pub voice_ime_enabled: bool,
    /// voice IME 的触发键 —— v0.1.12 可在 Onboarding/托盘选
    /// 取值："fn" / "option" / "control" / "right-shift" / "right-command" / "right-option"
    /// 默认 fn —— 兼容老 config
    #[serde(default = "default_voice_ime_trigger")]
    pub voice_ime_trigger: String,
    /// 剪贴板捕获暂停开关 —— v0.1.13 隐私强化
    /// 默认 false（开启捕获）。用户点托盘「⏸️ 暂停剪贴板记录」时为 true。
    #[serde(default)]
    pub clipboard_paused: bool,
    /// 工作区路径 (v0.1.21) —— AI 调用时以此为 cwd，让 Claude 知道在哪个项目里。
    /// 用户在托盘「📁 设置工作区…」选；默认 None = 走 macOS 系统默认 cwd（home）
    #[serde(default)]
    pub workspace_path: Option<String>,
    /// 开机自启动 (v0.1.26) —— 登录 Mac 时自动启动 MouseClaw，住在菜单栏
    /// 真值与 LaunchAgent / SMAppService 的实际状态在启动时双向同步：
    /// 用户在系统设置里手动关掉 → 下次启动回写 config 为 false
    /// 默认 true —— 菜单栏常驻应用的用户期待
    #[serde(default = "default_autostart")]
    pub autostart: bool,
    /// 桌宠悬停位置 (v0.1.27) —— overlay 闲置时停在屏幕哪个角落。
    /// 默认 bottom-right —— 不挡视线、最远离 menubar 和 dock。
    #[serde(default)]
    pub pet_anchor: PetAnchor,
    /// Set to true the first time the user completes Onboarding. Until then,
    /// the app doesn't register a global shortcut — clicking the tray or
    /// launching the app re-opens the Onboarding window instead.
    #[serde(default)]
    pub onboarded: bool,
    /// v0.5 · 开场调皮入场动画（launch entrance）是否已经"炸"过一次。
    /// false → 下次启动播 **loud** 入场（探头→横冲→张望→跑去角落），播完置 true，
    ///   **一生一次**（首次安装那次）。
    /// true → 之后每日 `--minimized` 自启播 **subtle**（角落伸懒腰），手动冷启播
    ///   **medium**（窜到角落+开心蹦）。详见 `entrance.rs`。
    /// 老 config 缺这个字段 → serde default false → 老用户升级后会看到一次 loud 入场
    ///   （等价"第一次见到新版桌宠"，符合预期）。
    #[serde(default)]
    pub seen_entrance: bool,
    /// v0.5 · 是否已经给过一次性"定时任务发现提示"。从没建过定时任务的用户，
    /// 启动几分钟后桌宠会轻轻提一句怎么用，提过即置 true（只提一次，不打扰）。
    #[serde(default)]
    pub seen_schedule_hint: bool,
    /// Schema version. Saved configs older than CURRENT_CONFIG_VERSION get
    /// treated as not-onboarded so the user re-picks a shortcut.
    /// Pre-versioned configs default to 1 (the legacy schema).
    #[serde(default = "legacy_version")]
    pub version: u32,
}

fn legacy_version() -> u32 { 1 }
fn default_language() -> String { "zh".into() }
fn default_voice_lang() -> String { "zh-en".into() }
fn default_vocab_builtin_enabled() -> bool { true }
// LLM tidy 默认**关** —— Claude CLI 调用每次 +3-8s，对 AI 召唤流程是过度优化。
// 只有写到光标的语音 IME 场景值得开（精修文本，用户看到的就是它）。
// 用户托盘菜单可一键开。
// default_tidy_up removed in v0.3.4 alongside LLM polish
fn default_voice_ime() -> bool { true }
fn default_voice_ime_trigger() -> String { "fn".into() }
fn default_autostart() -> bool { true }
fn default_sfx_enabled() -> bool { true }
fn default_sfx_volume() -> f32 { 0.45 }

impl Default for Config {
    fn default() -> Self {
        Self {
            shortcut: "Super+Shift+Space".into(),
            backend: Backend::default(),
            skin: SkinId::default(),
            language: default_language(),
            voice_lang: default_voice_lang(),
            vocab_builtin_enabled: default_vocab_builtin_enabled(),
            firstrun_tour_done: false,
            voice_ime_enabled: default_voice_ime(),
            voice_ime_trigger: default_voice_ime_trigger(),
            clipboard_paused: false,
            workspace_path: None,
            autostart: default_autostart(),
            pet_anchor: PetAnchor::default(),
            pet_custom_position: None,
            tts_enabled: false,
            sfx_enabled: default_sfx_enabled(),
            sfx_volume: default_sfx_volume(),
            onboarded: false,
            seen_entrance: false,
            seen_schedule_hint: false,
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
            // v16 → v17 · Whisper 整套删除，sherpa-zh-en bundled in DMG.
            // 老 config 的 whisper_model 字段自然被忽略（serde skips unknown）。
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
        // v0.3 · WhisperModel deleted — sherpa-zh-en is sole ASR, bundled in DMG.
    }

    #[test]
    fn pet_anchor_round_trips_and_defaults_bottom_right() {
        assert_eq!(PetAnchor::default(), PetAnchor::BottomRight);
        for a in PetAnchor::all() {
            assert_eq!(PetAnchor::from_str(a.as_str()), *a, "{:?} round-trip", a);
        }
        assert_eq!(PetAnchor::from_str("garbage"), PetAnchor::BottomRight);
        // visibility: 4 corners pin; follow + hidden don't
        assert!(PetAnchor::BottomRight.pin_visible_when_idle());
        assert!(PetAnchor::TopLeft.pin_visible_when_idle());
        assert!(!PetAnchor::Follow.pin_visible_when_idle());
        assert!(!PetAnchor::Hidden.pin_visible_when_idle());
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
