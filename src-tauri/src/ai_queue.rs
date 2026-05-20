//! AI 任务串行队列（v0.4+）
//!
//! 用户决策（2026-05-20）：「AI 任务排队逐个做完，听写永远即时不排队」。
//!
//! 桌宠是一只 —— 一次只做一件 AI 工作。共用一把 tokio 锁串行：
//!   - reactive action（清理 / 翻译 / 解释 / 回信，clipboard_action.rs）
//!   - 主 pipeline（语音 / 截图 → AI，pipeline.rs）
//!
//! **不走这里的**：语音输入法 fn 听写（本地 sherpa ASR，纯打字，永远即时）。
//!
//! 用法：
//! ```ignore
//! let _ticket = ai_queue::acquire().await; // 排队等轮到自己；期间桌宠显示"忙碌"
//! // ... 调 AI（ask_streaming / ask_text_only）...
//! // _ticket drop（函数返回 / panic / future 取消）→ 释放锁 + 忙碌计数 -1 → 下一个进来
//! ```
//!
//! 为什么不做"拒绝新任务"或"并行"：
//!   - 拒绝 → 用户要手动重试，烦
//!   - 并行 → 多个 claude 子进程 + 本地 ASR 抢 CPU → 卡顿（用户实测反馈）
//!   排队是"按了就一定会做、且不卡"的平衡点。

use once_cell::sync::Lazy;
use std::sync::Arc;
use tokio::sync::Mutex;

/// 全局串行锁。permits = 1（Mutex 本质），保证一次只有一个 AI 任务在跑。
static AI_LOCK: Lazy<Arc<Mutex<()>>> = Lazy::new(|| Arc::new(Mutex::new(())));

/// 持有期间 = "我正在用 AI"。drop 时释放锁 + 忙碌计数 -1。
pub struct Ticket {
    _permit: tokio::sync::OwnedMutexGuard<()>,
    _busy: crate::reactive::TaskGuard,
}

/// 排队拿锁。前面有任务就 await 等它完成。
///
/// 先 +1 忙碌计数（含"排队等待中"的任务，让桌宠在排队阶段就显示忙），
/// 再 await 锁。拿到后返回 Ticket，调用方持有它跑 AI，作用域结束自动释放。
pub async fn acquire() -> Ticket {
    let busy = crate::reactive::TaskGuard::start(); // 计数 +1（排队中也算忙）
    let permit = AI_LOCK.clone().lock_owned().await; // 等轮到自己
    Ticket { _permit: permit, _busy: busy }
}

/// 当前是否有 AI 任务在跑 / 排队（诊断用）。
pub fn is_busy() -> bool {
    crate::reactive::bg_task_count() > 0
}
