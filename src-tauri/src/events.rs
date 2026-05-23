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
    Thinking {
        transcript: String,
        /// v0.4.x · AI 处理期间的实时活动（Claude CLI 的 thinking_delta / tool_use）。
        /// None = 还没活动（刚进 thinking）；Some = 正在思考/读文件/跑命令，气泡显示
        /// 这行让用户知道没卡死。其他后端没有细粒度事件就保持 None。
        #[serde(skip_serializing_if = "Option::is_none")]
        status: Option<String>,
    },
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
    /// v0.5 · 定时任务确认卡 —— backend 解析出 `[SCHEDULE]` 标记后弹给用户确认。
    /// 用户「确认」→ 前端调 create_schedule；「改一下」/ Esc → dismiss。
    #[serde(rename = "schedule-confirm")]
    ScheduleConfirm {
        title: String,
        action: String,
        schedule: crate::schedule::Schedule,
        /// 下一次触发时间（本地 RFC3339）—— 前端格式化成"明天 08:00"。
        #[serde(rename = "nextRun", skip_serializing_if = "Option::is_none")]
        next_run: Option<String>,
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

/// v0.4.x · Session 状态广播 —— pipeline 每次处理完一轮 / 用户开新对话 / 钉住切换时 emit。
/// 前端据此显示链条图标 + 「第 N 轮」+ 「📌 任务名」+ 软提示。payload: SessionState。
pub const EV_SESSION_STATE: &str = "session-state";

/// v0.5 · 定时任务执行完成 —— scheduler emit；前端浮一个轻气泡（不抢焦点、几秒自动消失）。
/// payload: ScheduleResultPayload。task_id 为空串 = 一次性"发现提示"（点展开打开任务窗）。
pub const EV_SCHEDULE_RESULT: &str = "schedule-result";

#[derive(Debug, Clone, Serialize)]
pub struct ScheduleResultPayload {
    #[serde(rename = "taskId")]
    pub task_id: String,
    pub title: String,
    pub summary: String,
    pub ok: bool,
}

/// v0.4+ · 桌宠撞到屏幕边缘 —— overlay 窗口被 clamp 顶住时 emit。
/// payload: { dir: "left"|"right"|"top"|"bottom" }（撞的是哪面墙）。
/// 前端给桌宠精灵加 `mc-bonk-{dir}` class 播一次"挤压回弹"动画。纯视觉，不动窗口。
pub const EV_EDGE_BONK: &str = "edge-bonk";

#[derive(Debug, Clone, Serialize)]
pub struct SessionState {
    /// 当前是否有可续上下文（决定显不显示链条图标）
    pub continuing: bool,
    /// 对话轮数（一问一答 = 1 轮）
    pub round: u32,
    /// 是否钉住任务模式
    pub pinned: bool,
    /// 钉住的任务标签（如 "deck.pptx"）
    #[serde(rename = "pinnedLabel", skip_serializing_if = "Option::is_none")]
    pub pinned_label: Option<String>,
    /// 是否该显示"隔了很久"软提示
    #[serde(rename = "softHint")]
    pub soft_hint: bool,
}

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
    /// v0.4.4 · 记忆首次透明告知：记忆默认开,第一次启动时一次性告知
    /// 「本地存、随时看/删」。marker file 兜底,弹过不再弹。
    MemoryIntro,
    /// v0.4.4 · 起名可发现性:还没起名的桌宠,一次性提示「给我起个名字?」+ CTA 开 picker。
    NameHint,
    /// v0.4.x · 主动记忆提醒:基于**真实**高频实体(项目/话题/文件/工具),偶尔(≥6h 冷却 +
    /// 自然停顿)来一句「还在忙 X 吗」。完全由 memory.db 真实数据驱动 —— 无数据不发,绝不编造。
    MemoryGlance,
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
            NudgeKind::MemoryIntro => "memory-intro",
            NudgeKind::NameHint    => "name-hint",
            NudgeKind::MemoryGlance => "memory-glance",
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
