//! v0.5 · 开场调皮入场动画（launch entrance）。
//!
//! 设计原型：`docs/prototypes/launch-entrance-20260521.html`（拍板后 1:1 移植）。
//!
//! 一打开 app 让桌宠在桌面上做一段调皮的出场吸引注意力，然后默默回到角落待命。
//! 按启动场景**分三档**（用户拍板「按场景分档」2026-05-21）：
//!
//! | 档 | 触发 | 行为 |
//! |----|------|------|
//! | **Loud**   | 首次安装（onboarding 完成重启后 · `!seen_entrance`） | 边缘探头→横冲中央→急刹→张望蹦跶→跑去 anchor 角落坐下。**一生一次**（播完 `seen_entrance=true`）。 |
//! | **Medium** | 手动冷启（`onboarded` 且非 `--minimized`） | 从近边窜到 anchor + 一个开心蹦。每次手动开都演。 |
//! | **Subtle** | 每日开机自启（`--minimized`） | 直接在角落淡入 + 伸懒腰。不横穿、不抢注意力。 |
//!
//! ## 两层协作
//! - **窗口位置**（本模块 · Rust）：overlay 窗口本体沿路径滑动（`set_position` 逐帧），
//!   复用 `feed_flow::run_to_cursor_then_follow` 的成熟模式。横穿期间窗口扩到
//!   `EXPANDED_SIZE`（同 listening，桌宠有空间蹦），落地后 `emit_view(Idle)` 缩回 compact。
//!   **绝不**把窗口撑成全屏（会重演 f253762 的 WebKit backing 遮挡 / 桌面图标点不动）。
//! - **桌宠精灵**（前端 CSS）：跑腿 / 急刹 / 蹦跶 / 抖耳甩尾由 `.mouse-wrap.mc-entrance-*`
//!   驱动，9 款皮肤一视同仁（蛙鼓气 / cyber 天线 / 狐大尾摆）。本模块 emit `entrance-phase`
//!   事件切换前端精灵动画。**先 emit phase 再改窗口尺寸** —— 给前端时间关掉 useAdaptiveOverlay
//!   （否则自适应 hook 会把横穿窗口缩掉，跟 set_position 抢尺寸）。
//!
//! ## 可打断（硬规则）
//! 任意阶段，用户按召唤快捷键 / 拖文件 → `overlay::show_mouse` / `feed_flow::on_drag_enter`
//! 调 [`abort`] bump `entrance_gen`。本模块每帧 / 每次 sleep 后自检 gen 变了立刻停 + emit
//! `done`（清掉前端精灵 class），把窗口让给打断者，不再自己归位。
//!
//! ## 无障碍
//! 前端启动上报 `prefers-reduced-motion` → `AppState.reduced_motion`。reduced 时跳过所有
//! 横穿 / 蹦跶，桌宠直接出现在 anchor（前端 CSS 也对 `mc-entrance-*` 降级为无动画）。

use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

use tauri::{AppHandle, Emitter, LogicalPosition, LogicalSize, Manager};

use crate::config::{Config, PetAnchor};
use crate::events::ViewKind;
use crate::overlay_size::{COMPACT_SIZE, EXPANDED_SIZE};
use crate::AppState;

/// 前端 App.tsx `listen("entrance-phase")` 是契约 —— 改名要同步前端 types.ts。
pub const EV_ENTRANCE: &str = "entrance-phase";

/// 桌宠中心距 overlay 窗口底部的像素（同 `overlay_size::PET_BOTTOM_OFFSET`）。
/// 用来在「桌宠中心屏幕坐标」↔「窗口左上角」之间换算。
const PET_FROM_BOTTOM: f64 = 40.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tier {
    Loud,
    Medium,
    Subtle,
}

impl Tier {
    fn as_str(self) -> &'static str {
        match self {
            Tier::Loud => "loud",
            Tier::Medium => "medium",
            Tier::Subtle => "subtle",
        }
    }
}

#[derive(serde::Serialize, Clone)]
struct EntrancePayload {
    phase: &'static str,
    tier: &'static str,
}

// ── 触发判定（纯函数，可单测）─────────────────────────────────────

/// 给定 config + 是否 `--minimized` 启动，决定该播哪一档（None = 不播）。
///
/// 规则：
///   - 没 onboarded → None（首次安装走 Onboarding，loud 等重启后那次再播）
///   - anchor 不常驻可见（Follow / Hidden）→ None（用户选了"别老待着"，尊重之）
///   - `!seen_entrance` → Loud（一生一次）
///   - `--minimized`（开机自启）→ Subtle
///   - 否则（手动冷启）→ Medium
pub fn decide_tier(cfg: &Config, minimized: bool) -> Option<Tier> {
    if !cfg.onboarded {
        return None;
    }
    if !cfg.pet_anchor.pin_visible_when_idle() {
        return None;
    }
    if !cfg.seen_entrance {
        Some(Tier::Loud)
    } else if minimized {
        Some(Tier::Subtle)
    } else {
        Some(Tier::Medium)
    }
}

// ── 几何换算（纯函数，可单测）────────────────────────────────────

/// 桌宠中心屏幕坐标 → 给定尺寸 overlay 窗口的左上角（逻辑像素，top-left origin）。
/// 桌宠水平居中、距窗口底 `PET_FROM_BOTTOM`。
fn win_for_pet_center(pcx: f64, pcy: f64, size: f64) -> (f64, f64) {
    (pcx - size * 0.5, pcy - (size - PET_FROM_BOTTOM))
}

/// 桌宠"归位角落"的中心屏幕坐标 —— compact 窗口停在 anchor 角时桌宠中心落在哪。
/// 复用 anchor::corner_position 的同一套数学，保证入场终点 == idle 静默落点（无突跳）。
fn home_pet_center(anchor: PetAnchor, sx: f64, sy: f64, sw: f64, sh: f64) -> (f64, f64) {
    let (tlx, tly) = crate::anchor::corner_position(
        anchor, sx, sy, sw, sh, COMPACT_SIZE, COMPACT_SIZE, crate::anchor::ANCHOR_PADDING,
    );
    (tlx + COMPACT_SIZE * 0.5, tly + COMPACT_SIZE - PET_FROM_BOTTOM)
}

/// anchor 在屏幕右半边？（决定从哪条边入场：右侧家 → 左边冲进来，反之亦然）
fn anchor_on_right(anchor: PetAnchor) -> bool {
    matches!(anchor, PetAnchor::TopRight | PetAnchor::BottomRight)
}

fn ease_out_cubic(t: f64) -> f64 {
    1.0 - (1.0 - t).powi(3)
}
fn ease_in_out_cubic(t: f64) -> f64 {
    if t < 0.5 {
        4.0 * t * t * t
    } else {
        1.0 - (-2.0 * t + 2.0).powi(3) / 2.0
    }
}

// ── 启动入口 ────────────────────────────────────────────────────

/// lib.rs setup() 在 `cfg.onboarded` 分支里调一次。内部判定档位 + spawn 异步播放。
/// 不阻塞启动。延迟 ~700ms 起跑，给前端 webview 挂载 + 上报 reduced-motion 的时间。
pub fn maybe_play_on_launch(app: AppHandle, cfg: &Config, minimized: bool) {
    let Some(tier) = decide_tier(cfg, minimized) else { return };
    let anchor = cfg.pet_anchor;
    let Some(state) = app.try_state::<Arc<AppState>>().map(|s| s.inner().clone()) else { return };
    println!("[mouseclaw] 🎬 launch entrance queued: {} (anchor={})", tier.as_str(), anchor.as_str());
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_millis(700)).await;
        play(app.clone(), state, tier, anchor).await;
        if matches!(tier, Tier::Loud) {
            // 炸档一生一次 —— 播完（或被打断也算见过）置标记。
            let mut c = Config::load();
            if !c.seen_entrance {
                c.seen_entrance = true;
                if let Err(e) = c.save() {
                    eprintln!("[mouseclaw] entrance: save seen_entrance failed: {e}");
                }
            }
        }
    });
}

/// 外部打断 —— bump entrance_gen 让进行中的 play() 下一帧自检失败立刻停。
/// overlay::show_mouse / show_mouse_at_anchor / feed_flow::on_drag_enter 调它。
pub fn abort(state: &Arc<AppState>) {
    state.entrance_gen.fetch_add(1, Ordering::SeqCst);
}

// ── 播放编排 ────────────────────────────────────────────────────

fn alive(state: &Arc<AppState>, my_gen: u64) -> bool {
    state.entrance_gen.load(Ordering::SeqCst) == my_gen
}

fn emit_phase(app: &AppHandle, tier: Tier, phase: &'static str) {
    let _ = app.emit_to("mouse", EV_ENTRANCE, EntrancePayload { phase, tier: tier.as_str() });
}

/// sleep + 自检 gen。返回 false = 被打断（调用方应 emit done + return）。
async fn nap(state: &Arc<AppState>, my_gen: u64, ms: u64) -> bool {
    tokio::time::sleep(Duration::from_millis(ms)).await;
    alive(state, my_gen)
}

/// 仅设置窗口位置（主线程）。
fn set_pos(app: &AppHandle, x: f64, y: f64) {
    let app2 = app.clone();
    let _ = app.run_on_main_thread(move || {
        if let Some(w) = app2.get_webview_window("mouse") {
            let _ = w.set_position(LogicalPosition::new(x, y));
        }
    });
}

/// 设置窗口尺寸 + 位置 + 显示（主线程）。
fn set_win(app: &AppHandle, x: f64, y: f64, size: f64) {
    let app2 = app.clone();
    let _ = app.run_on_main_thread(move || {
        if let Some(w) = app2.get_webview_window("mouse") {
            let _ = w.set_size(LogicalSize::new(size, size));
            let _ = w.set_position(LogicalPosition::new(x, y));
            let _ = w.show();
            let _ = w.set_always_on_top(true);
        }
    });
}

/// 主线程读主屏 visible frame（已扣 menubar/dock）。拿不到 → None。
async fn read_frame(app: &AppHandle) -> Option<(f64, f64, f64, f64)> {
    let app_t = app.clone();
    tokio::task::spawn_blocking(move || {
        let (tx, rx) = std::sync::mpsc::sync_channel(1);
        let _ = app_t.run_on_main_thread(move || {
            let _ = tx.send(crate::overlay_size::visible_frame_top_left());
        });
        rx.recv_timeout(Duration::from_millis(200)).ok().flatten()
    })
    .await
    .ok()
    .flatten()
}

/// 横穿一段：在 `dur_ms` 内把桌宠中心从 `from` 插值到 `to`（窗口尺寸 = EXPANDED）。
/// 每帧自检 gen。返回 false = 中途被打断。
async fn travel(
    app: &AppHandle,
    state: &Arc<AppState>,
    my_gen: u64,
    from: (f64, f64),
    to: (f64, f64),
    dur_ms: u64,
    ease: fn(f64) -> f64,
) -> bool {
    let frames = (dur_ms / 16).max(1); // ~60 fps
    let frame_ms = (dur_ms / frames).max(8);
    for i in 1..=frames {
        if !alive(state, my_gen) {
            return false;
        }
        let t = i as f64 / frames as f64;
        let e = ease(t);
        let pcx = from.0 + (to.0 - from.0) * e;
        let pcy = from.1 + (to.1 - from.1) * e;
        let (wx, wy) = win_for_pet_center(pcx, pcy, EXPANDED_SIZE);
        set_pos(app, wx, wy);
        tokio::time::sleep(Duration::from_millis(frame_ms)).await;
    }
    alive(state, my_gen)
}

/// 收尾：清前端精灵 + 把窗口缩回 compact 停到 anchor 角（== hide_overlay 的归位流程）。
async fn settle(app: &AppHandle, anchor: PetAnchor, tier: Tier) {
    emit_phase(app, tier, "done"); // 前端清 mc-entrance-* → 回 idle 64px 桌宠
    tokio::time::sleep(Duration::from_millis(60)).await; // 让前端先换回 64 再缩窗口
    crate::overlay::emit_view(app, &ViewKind::Idle); // shrink 320→80 + has_ui=false
    if anchor.pin_visible_when_idle() {
        crate::anchor::apply_idle_anchor(app, anchor);
    }
}

/// 被打断时的清理：只清前端精灵，不动窗口（让打断者 —— show_mouse 等 —— 接管位置）。
fn abort_cleanup(app: &AppHandle, tier: Tier) {
    emit_phase(app, tier, "done");
}

pub async fn play(app: AppHandle, state: Arc<AppState>, tier: Tier, anchor: PetAnchor) {
    // 认领本次 run（也顺带打断任何更早的 run）
    let my_gen = state.entrance_gen.fetch_add(1, Ordering::SeqCst) + 1;
    // 入场独占窗口位置 —— 关掉 cursor_follow 免得抢 set_position
    crate::cursor_follow::disable(&state);

    let Some((sx, sy, sw, sh)) = read_frame(&app).await else {
        // 拿不到屏幕几何 → 老老实实把桌宠放角落，别冒险横穿
        settle(&app, anchor, tier).await;
        return;
    };
    let home = home_pet_center(anchor, sx, sy, sw, sh);
    let reduced = state.reduced_motion.load(Ordering::Relaxed);

    match tier {
        Tier::Subtle => {
            // 角落淡入 + 伸懒腰（无横穿）。先 settle 到 compact 角落（窗口缩好），
            // 再 emit stretch 让前端在 64px 桌宠上播一次伸懒腰。
            crate::overlay::emit_view(&app, &ViewKind::Idle);
            if anchor.pin_visible_when_idle() {
                crate::anchor::apply_idle_anchor(&app, anchor);
            }
            if reduced {
                return; // 直接出现，不伸懒腰
            }
            if !nap(&state, my_gen, 80).await {
                return;
            }
            emit_phase(&app, tier, "stretch");
            tokio::time::sleep(Duration::from_millis(640)).await;
            emit_phase(&app, tier, "done");
            return;
        }
        Tier::Medium => {
            let from_right = anchor_on_right(anchor);
            let start_pcx = if from_right { sx + sw + 60.0 } else { sx - 60.0 };
            // 先 emit phase（让前端关掉自适应 + 渲染 96px），再扩窗口到 320 摆到屏外
            emit_phase(&app, tier, "run");
            if !nap(&state, my_gen, 60).await {
                abort_cleanup(&app, tier);
                return;
            }
            let (wx, wy) = win_for_pet_center(start_pcx, home.1, EXPANDED_SIZE);
            crate::overlay_size::mark_expanded();
            set_win(&app, wx, wy, EXPANDED_SIZE);
            if reduced {
                settle(&app, anchor, tier).await;
                return;
            }
            if !travel(&app, &state, my_gen, (start_pcx, home.1), home, 440, ease_out_cubic).await {
                abort_cleanup(&app, tier);
                return;
            }
            emit_phase(&app, tier, "beat");
            if !nap(&state, my_gen, 560).await {
                abort_cleanup(&app, tier);
                return;
            }
            settle(&app, anchor, tier).await;
        }
        Tier::Loud => {
            let enter_left = anchor_on_right(anchor); // 家在右 → 从左边冲进来（横穿最长）
            let peek_start = (if enter_left { sx - 60.0 } else { sx + sw + 60.0 }, home.1);
            let peek_in = (if enter_left { sx + 70.0 } else { sx + sw - 70.0 }, home.1);
            let center = (sx + sw * 0.42, sy + sh * 0.46);
            let overshoot = (
                center.0 + if enter_left { sw * 0.05 } else { -sw * 0.05 },
                center.1,
            );

            // 先 emit phase（前端关自适应 + 渲染 96px），再扩窗口摆到屏外左/右
            emit_phase(&app, tier, "peek");
            if !nap(&state, my_gen, 60).await {
                abort_cleanup(&app, tier);
                return;
            }
            let (wx, wy) = win_for_pet_center(peek_start.0, peek_start.1, EXPANDED_SIZE);
            crate::overlay_size::mark_expanded();
            set_win(&app, wx, wy, EXPANDED_SIZE);

            if reduced {
                settle(&app, anchor, tier).await;
                return;
            }

            // 1) 探头：从屏外探进来一点 + 张望（前端 peek 摇头 + 抖耳）
            if !travel(&app, &state, my_gen, peek_start, peek_in, 240, ease_out_cubic).await {
                abort_cleanup(&app, tier);
                return;
            }
            if !nap(&state, my_gen, 380).await {
                abort_cleanup(&app, tier);
                return;
            }
            // 2) 横冲到中央
            emit_phase(&app, tier, "run");
            if !travel(&app, &state, my_gen, peek_in, center, 640, ease_out_cubic).await {
                abort_cleanup(&app, tier);
                return;
            }
            // 3) 急刹 + 过冲
            emit_phase(&app, tier, "skid");
            if !travel(&app, &state, my_gen, center, overshoot, 160, ease_out_cubic).await {
                abort_cleanup(&app, tier);
                return;
            }
            // 4) 张望 · 蹦跶 · 甩尾（原地）
            emit_phase(&app, tier, "beat");
            if !nap(&state, my_gen, 780).await {
                abort_cleanup(&app, tier);
                return;
            }
            // 5) 小跑回角落
            emit_phase(&app, tier, "run");
            if !travel(&app, &state, my_gen, overshoot, home, 600, ease_in_out_cubic).await {
                abort_cleanup(&app, tier);
                return;
            }
            settle(&app, anchor, tier).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg_with(onboarded: bool, seen: bool, anchor: PetAnchor) -> Config {
        Config {
            onboarded,
            seen_entrance: seen,
            pet_anchor: anchor,
            ..Config::default()
        }
    }

    #[test]
    fn not_onboarded_never_plays() {
        let c = cfg_with(false, false, PetAnchor::BottomRight);
        assert_eq!(decide_tier(&c, false), None);
        assert_eq!(decide_tier(&c, true), None);
    }

    #[test]
    fn hidden_and_follow_anchors_skip_entrance() {
        for a in [PetAnchor::Hidden, PetAnchor::Follow] {
            let c = cfg_with(true, false, a);
            assert_eq!(decide_tier(&c, false), None, "{a:?} should skip");
        }
    }

    #[test]
    fn first_run_is_loud_regardless_of_minimized() {
        let c = cfg_with(true, false, PetAnchor::BottomRight);
        assert_eq!(decide_tier(&c, false), Some(Tier::Loud));
        assert_eq!(decide_tier(&c, true), Some(Tier::Loud));
    }

    #[test]
    fn seen_then_minimized_is_subtle_else_medium() {
        let c = cfg_with(true, true, PetAnchor::TopLeft);
        assert_eq!(decide_tier(&c, true), Some(Tier::Subtle));
        assert_eq!(decide_tier(&c, false), Some(Tier::Medium));
    }

    #[test]
    fn home_center_matches_corner_anchor_math() {
        // 1920×1080，bottom-right：compact(80) 窗口左上角 = (1920-80-24, 1080-80-24)
        // = (1816, 976)；桌宠中心 = (+40, +40) = (1856, 1016)。
        let (cx, cy) = home_pet_center(PetAnchor::BottomRight, 0.0, 0.0, 1920.0, 1080.0);
        assert_eq!((cx, cy), (1856.0, 1016.0));
    }

    #[test]
    fn win_for_pet_center_round_trips_expanded() {
        // 桌宠中心 (1856,1016) 用 EXPANDED(320) 窗口反推左上角
        let (wx, wy) = win_for_pet_center(1856.0, 1016.0, EXPANDED_SIZE);
        assert_eq!(wx, 1856.0 - 160.0); // 居中
        assert_eq!(wy, 1016.0 - (320.0 - 40.0)); // 底部 offset
    }

    #[test]
    fn anchor_side_drives_entry_direction() {
        assert!(anchor_on_right(PetAnchor::BottomRight));
        assert!(anchor_on_right(PetAnchor::TopRight));
        assert!(!anchor_on_right(PetAnchor::BottomLeft));
        assert!(!anchor_on_right(PetAnchor::TopLeft));
    }

    #[test]
    fn ease_curves_hit_endpoints() {
        assert!((ease_out_cubic(0.0) - 0.0).abs() < 1e-9);
        assert!((ease_out_cubic(1.0) - 1.0).abs() < 1e-9);
        assert!((ease_in_out_cubic(0.0) - 0.0).abs() < 1e-9);
        assert!((ease_in_out_cubic(1.0) - 1.0).abs() < 1e-9);
        assert!((ease_in_out_cubic(0.5) - 0.5).abs() < 1e-9);
    }
}
