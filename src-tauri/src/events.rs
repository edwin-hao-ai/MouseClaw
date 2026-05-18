//! IPC event payloads — must stay in sync with `src/types.ts` (`ViewKind`).
//! Renames here = broken UI. Always touch both files in the same commit.

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum ViewKind {
    Idle,
    Onboarding,
    Listening,
    /// v0.1.14 · 语音 IME 中（长按触发键说话）—— 跟 Listening 区分，桌宠用 type 态
    #[serde(rename = "voice-ime-listening")]
    VoiceImeListening,
    Thinking { transcript: String },
    Reply {
        transcript: String,
        reply: String,
        mode: ReplyMode,
        /// rename → camelCase 跟 src/types.ts 的 `insertText` 对齐
        /// （之前是 insert_text，TS 侧永远读到 undefined —— latent bug）
        #[serde(rename = "insertText", skip_serializing_if = "Option::is_none")]
        insert_text: Option<String>,
        /// true = Claude 还在流式输出中（气泡显示闪烁光标）；
        /// false = 最终回复（已确定 mode、可存 history）。
        streaming: bool,
    },
    Panel {
        #[serde(rename = "sessionId")]
        session_id: u64,
        turns: Vec<Turn>,
    },
    #[serde(rename = "mode-b-countdown")]
    ModeBCountdown {
        #[serde(rename = "insertText")]
        insert_text: String,
        remaining: u32,
    },
    #[serde(rename = "mode-b-inserting")]
    ModeBInserting {
        #[serde(rename = "insertText")]
        insert_text: String,
    },
    Blocked { reason: String },
}

#[derive(Debug, Clone, Copy, Serialize)]
pub enum ReplyMode {
    A,
    B,
}

#[derive(Debug, Clone, Serialize, serde::Deserialize)]
pub struct Turn {
    pub role: TurnRole,
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub streaming: Option<bool>,
}

#[derive(Debug, Clone, Copy, Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum TurnRole {
    User,
    Assistant,
}

pub const EV_VIEW_CHANGED: &str = "view-changed";

/// 桌宠皮肤切换事件。payload 是 SkinId 的 kebab-case 字符串（如 "lab"）。
/// 前端 App.tsx 订阅它实时更新 PixelMouse 的 skin prop —— 不重启就能换皮肤。
pub const EV_SKIN_CHANGED: &str = "skin-changed";

/// 语言切换事件 —— payload 是 lang id ("zh" / "en")。前端 i18n module 监听
/// 它来热切换所有 UI 文案，无需重启窗口。
pub const EV_LANG_CHANGED: &str = "lang-changed";

/// 主动提醒事件 (v0.1.27 P3) —— presence + nudge 引擎触发。
/// payload: NudgePayload。前端在 App.tsx 监听 → 渲染浮动 nudge bubble。
pub const EV_NUDGE: &str = "nudge";

/// 提醒类型 —— frontend 据此选 icon/文案/动画。
/// kebab-case 以便与前端 `NudgeKind` TS union 对齐。
#[derive(Debug, Clone, Copy, Serialize, serde::Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "kebab-case")]
pub enum NudgeKind {
    /// 久坐：90min 鼠标几乎没动 → 提醒站起来动一动
    Stretch,
    /// 卡壳：IDE 前台但 5min 没敲键 + 鼠标乱晃 → 主动喂 LLM 入口
    Stuck,
    /// 深夜：≥ 23:30 仍在活跃 → 劝睡
    LateNight,
}

impl NudgeKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            NudgeKind::Stretch   => "stretch",
            NudgeKind::Stuck     => "stuck",
            NudgeKind::LateNight => "late-night",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct NudgePayload {
    pub kind: NudgeKind,
    /// 本地化文案 —— Rust 端按 config.language 选 zh/en，前端直接展示
    pub message: String,
    /// 可选 CTA（按下后 frontend dispatch 命令；用于"卡壳→召唤"）
    #[serde(rename = "ctaLabel", skip_serializing_if = "Option::is_none")]
    pub cta_label: Option<String>,
    #[serde(rename = "ctaAction", skip_serializing_if = "Option::is_none")]
    pub cta_action: Option<String>,
}
