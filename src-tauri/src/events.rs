//! IPC event payloads — must stay in sync with `src/types.ts` (`ViewKind`).
//! Renames here = broken UI. Always touch both files in the same commit.

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum ViewKind {
    Idle,
    Onboarding,
    Listening {
        /// v0.2 · 边说边出字 —— sherpa streaming 实时 partial。
        /// 空串时前端隐藏 partial 显示（首字延迟内）。
        #[serde(default)]
        partial: String,
    },
    /// v0.1.14 · 语音 IME 中（长按触发键说话）—— 跟 Listening 区分，桌宠用 type 态
    /// v0.3.8 · 加 partial：桌宠气泡实时显示 sherpa 流式文本（写入光标延后到 fn 松开
    /// 后一次完成，避开 macOS fn 键的焦点抢盗问题）
    #[serde(rename = "voice-ime-listening")]
    VoiceImeListening {
        #[serde(default)]
        partial: String,
    },
    /// v0.4 · 用户拖文件 hover 在桌宠上但还没 drop —— 张嘴等接收
    #[serde(rename = "feed-waiting")]
    FeedWaiting,
    /// v0.4 · 已吞下文件 —— 录音听用户问问题
    #[serde(rename = "feed-listening")]
    FeedListening {
        /// 已吃下的文件名（用于气泡上展示）
        files: Vec<String>,
        /// 当前 sherpa partial（边说边出字）
        #[serde(default)]
        partial: String,
    },
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
    /// v0.4.0 · 语音转写完成后的确认阶段 —— 防止误识别浪费 token。
    /// 3 秒倒数自动发；Esc 取消；Enter 立即发；点击气泡进入编辑模式。
    #[serde(rename = "voice-confirm")]
    VoiceConfirm {
        transcript: String,
        remaining: u32,
    },
    /// v0.4.0 · 首次使用引导 · 5 步流程（欢迎 → 准备网页 → 教召唤 → 教 fn → 庆祝）
    #[serde(rename = "tour-step")]
    TourStep { step: u32 },
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

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
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
    /// v0.1.32 · 喝水：连续高强度敲键 ≥ 20min → 喝口水歇一下
    Water,
    /// v0.1.32 · 早安：每天首次解锁屏幕 → 老鼠伸懒腰打招呼
    GoodMorning,
    /// v0.1.32 · 午餐：12:00-13:30 久坐 → 提醒午饭啦
    Lunch,
    /// v0.4.x · 老用户升级提示：装上 browser use / office use CLI 才能完整动手。
    /// 一次性（marker file 兜底），不走 presence 心跳，由启动检测直接 emit。
    LearnedCli,
    /// v0.4+ · 连续工作：长时间高强度敲键（≥ 2h 窗口持续活跃）+ **当下出现自然停顿**
    /// → 趁停顿提醒休息眼睛 / 起来走走。刻意在停顿时发，不打断心流。
    LongFocus,
}

impl NudgeKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            NudgeKind::Stretch     => "stretch",
            NudgeKind::Stuck       => "stuck",
            NudgeKind::LateNight   => "late-night",
            NudgeKind::Water       => "water",
            NudgeKind::GoodMorning => "good-morning",
            NudgeKind::Lunch       => "lunch",
            NudgeKind::LearnedCli  => "learned-cli",
            NudgeKind::LongFocus   => "long-focus",
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
