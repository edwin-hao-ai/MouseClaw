//! 桌宠跟随鼠标光标 (v0.1.8)
//!
//! CLAUDE.md 之前的「不要让老鼠跟随鼠标」规则在 v0.1.8 解除 —— 用户明确要求
//! 召唤出来后桌宠跟随鼠标。但有重要边界：
//!
//! **仅在「没有气泡 / 没有面板」的状态跟随**：
//!   - listening（录音中）→ 跟随 ✓ (mouse 是唯一可见物，跟随像宠物)
//!   - idle (隐藏中)        → 不跟随 (窗口隐着)
//!   - thinking / reply / panel / mode-b / blocked → **冻结** (用户在读气泡内容)
//!
//! 否则用户每次想看回答都会被窗口飘走甩跑，体验更差。
//!
//! 实现：tokio 后台任务，30fps 轮询光标位置 → `overlay::reposition_to_cursor`，
//! 由 `AppState.follow_cursor: AtomicBool` 控制开关。

use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;
use tauri::AppHandle;

use crate::AppState;

/// 启动后台跟随任务 —— 进程启动时调一次，永久跑。
/// 实际跟随与否由 `state.follow_cursor` AtomicBool 控制（emit_view 时切换）。
pub fn spawn_follow_loop(app: AppHandle, state: Arc<AppState>) {
    tauri::async_runtime::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_millis(33)); // ~30 fps
        // first tick fires immediately; skip 一次让 state 稳定
        tick.tick().await;
        loop {
            tick.tick().await;
            if state.follow_cursor.load(Ordering::Relaxed) {
                crate::overlay::reposition_to_cursor(&app);
            }
        }
    });
}

/// 开启跟随 —— show_mouse 时叫，listening 状态保持。
pub fn enable(state: &Arc<AppState>) {
    state.follow_cursor.store(true, Ordering::Relaxed);
}

/// 关闭跟随 —— emit_view 进入 thinking/reply/panel/mode-b/blocked 时叫。
/// 让窗口停在最后位置，用户读气泡不被甩飞。
pub fn disable(state: &Arc<AppState>) {
    state.follow_cursor.store(false, Ordering::Relaxed);
}
