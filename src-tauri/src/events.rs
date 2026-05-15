//! IPC event payloads — must stay in sync with `src/types.ts` (`ViewKind`).
//! Renames here = broken UI. Always touch both files in the same commit.

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum ViewKind {
    Idle,
    Onboarding,
    Listening,
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
    Panel { session_id: u64, turns: Vec<Turn> },
    #[serde(rename = "mode-b-countdown")]
    ModeBCountdown { insert_text: String, remaining: u32 },
    #[serde(rename = "mode-b-inserting")]
    ModeBInserting { insert_text: String },
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
