//! v0.3.12 fix3 · 动态调整 mouse overlay 窗口物理尺寸，彻底消除"遮挡周围 app"问题。
//!
//! 之前用 set_ignore_cursor_events 做事件穿透，但用户反复反馈仍然看到 320×320
//! 的浅色矩形遮挡视觉。问题是：哪怕事件穿透 OK，WebKit 在 transparent window
//! 上仍可能渲染一个半透明 backing；而且 320×320 窗口物理上就那么大，就算
//! 完全透明也会因为 alwaysOnTop 在 macOS Mission Control / Stage Manager 上
//! 产生奇怪的可视化效果。
//!
//! 解：物理缩窗。
//!   - 静默（view=Idle 且无 React-only UI）→ 100×100，恰好桌宠像素 + 一点容错
//!   - 有 UI（任何气泡 / 菜单 / nudge / 倒数 / 下载提示）→ 320×320，气泡空间够
//!
//! 切换时让桌宠的**屏幕绝对位置**保持不变 —— 缩小时窗口 origin 向桌宠移，
//! 放大时反向。桌宠在窗口里始终是底部中央。

use std::sync::atomic::{AtomicI32, AtomicU32, AtomicU64, AtomicU8, Ordering};
use tauri::{AppHandle, Emitter, LogicalPosition, LogicalSize, Manager};

/// v0.4+ · 撞到的是哪面墙 —— 决定前端播哪个方向的挤压回弹动画。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BonkDir { Left, Right, Top, Bottom }
impl BonkDir {
    fn as_str(self) -> &'static str {
        match self { BonkDir::Left=>"left", BonkDir::Right=>"right", BonkDir::Top=>"top", BonkDir::Bottom=>"bottom" }
    }
}

#[derive(serde::Serialize, Clone)]
struct BonkPayload { dir: &'static str }

/// 上次发撞边事件的时间 + 方向 —— 防止贴着墙拖动时每帧狂发。
static LAST_BONK_MS: AtomicU64 = AtomicU64::new(0);
static LAST_BONK_DIR: AtomicU8 = AtomicU8::new(255);

fn now_ms() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

/// 发"撞墙"事件给前端（带方向）。同方向 400ms 内去重；换方向立即发。
/// 既给 follow 路径（reposition_to_cursor）用，也给 resize 路径（set_to_explicit）用。
pub fn emit_bonk_edge(app: &AppHandle, dir: BonkDir) {
    let now = now_ms();
    let code = dir as u8;
    if code == LAST_BONK_DIR.load(Ordering::Relaxed)
        && now.saturating_sub(LAST_BONK_MS.load(Ordering::Relaxed)) < 400 {
        return;
    }
    LAST_BONK_MS.store(now, Ordering::Relaxed);
    LAST_BONK_DIR.store(code, Ordering::Relaxed);
    let _ = app.emit(crate::events::EV_EDGE_BONK, BonkPayload { dir: dir.as_str() });
}

/// 桌宠在窗口里的水平偏移（窗口宽 / 2，因为桌宠水平居中）
const PET_X_OFFSET_RATIO: f64 = 0.5;
/// 桌宠中心距窗口底部 = pet_size/2 + padding(8px).
/// idle 64 → 32+8=40；listening 96 → 48+8=56。
/// 取 40 跟 idle 一致（listening 时窗口已经扩到 320，offset 不影响视觉锚点）。
const PET_BOTTOM_OFFSET: f64 = 40.0;

/// 当前窗口模式。0 = compact(80), 1 = expanded(320)
static CURRENT_MODE: AtomicU32 = AtomicU32::new(0);
/// 桌宠中心的"权威屏幕位置"（逻辑像素，top-left origin）。
/// v0.4+ 漂移根治：这是 set_to_explicit 的真锚点 —— 当窗口被屏幕边 clamp 顶住时，
/// 不信任"被顶过的当前位置"，而用这里存的"未被 clamp 的真值"反算，让内容缩回后
/// 桌宠回到原位（不再累积漂移）。窗口不贴边时，当前位置即真值，存回这里。
/// 哨兵 i32::MIN = 尚未初始化（首帧用当前位置兜底）。
static LAST_PET_CENTER_X: AtomicI32 = AtomicI32::new(i32::MIN);
static LAST_PET_CENTER_Y: AtomicI32 = AtomicI32::new(i32::MIN);

pub const COMPACT_SIZE: f64 = 80.0;
pub const EXPANDED_SIZE: f64 = 320.0;

/// v0.4 · 给 show_mouse 用：把 CURRENT_MODE 强制设为 expanded，
/// 这样后续 emit_view 调 expand_to_full 时是 no-op（已是 expanded 模式），
/// 不会再触发一次 set_size + anchor-preserve 资源浪费 / 视觉抖动。
pub fn mark_expanded() {
    CURRENT_MODE.store(1, Ordering::SeqCst);
}

/// 进入 compact 模式（100×100，只占桌宠像素）。
pub fn shrink_to_compact(app: &AppHandle) {
    set_mode(app, COMPACT_SIZE, 0);
}

/// 进入 expanded 模式（320×320，气泡有空间）。
pub fn expand_to_full(app: &AppHandle) {
    set_mode(app, EXPANDED_SIZE, 1);
}

/// v0.4 · 内容驱动尺寸 —— React 端 ResizeObserver 把"需要多大"传过来。
/// 跟 set_mode 同样保持桌宠视觉锚点不变（底部居中那点不动）。
/// 调用者：commands::set_overlay_content_size（由前端 useAdaptiveOverlay 触发）。
///
/// 设计取舍：
///   - 跳过 CURRENT_MODE 检查 —— 每次 React 渲染都可能不一样大小，需要直接生效
///   - clamp 上限 1200×1200，防止 React 异常算出疯狂数字撑爆屏幕
///   - clamp 下限 COMPACT_SIZE，防止比桌宠还小（hit-box 失效）
/// v0.4.3 · 自适应窗口尺寸 + 桌宠定位。
///
/// `pet_ratio_x` / `pet_from_bottom` —— 桌宠中心在内容窗口里的相对位置（由前端
/// useAdaptiveOverlay 实测：菜单居中时 ratio≈0.5、距底≈40；PetMenu 贴边往侧边展开时
/// 桌宠不在窗口中心，ratio 会偏向一侧）。后端据此摆窗口，保证桌宠中心始终落在屏幕锚点。
///
/// `pet_anchored` —— true 时用「桌宠本体保持」clamp（PetMenu 方向感知路径：菜单已往
/// 屏幕内侧展开不会越界，只需让桌宠那侧的透明边距/阴影允许溢出 → 桌宠纹丝不动）；
/// false 时用整窗 clamp（bubble / ribbon 等居中对称内容的老行为，保证完整在屏，不动）。
pub fn set_to_explicit(
    app: &AppHandle,
    want_w: f64,
    want_h: f64,
    pet_ratio_x: f64,
    pet_from_bottom: f64,
    pet_anchored: bool,
) {
    let w = want_w.clamp(COMPACT_SIZE, 1200.0);
    let h = want_h.clamp(COMPACT_SIZE, 1200.0);
    let rx = pet_ratio_x.clamp(0.0, 1.0);
    let fb = pet_from_bottom.clamp(0.0, h);
    let app2 = app.clone();
    let _ = app.run_on_main_thread(move || {
        let Some(window) = app2.get_webview_window("mouse") else { return; };
        let Ok(pos) = window.outer_position() else { return; };
        let Ok(size) = window.outer_size() else { return; };
        let Ok(scale) = window.scale_factor() else { return; };
        let cur_x = pos.x as f64 / scale;
        let cur_y = pos.y as f64 / scale;
        let cur_w = size.width as f64 / scale;
        let cur_h = size.height as f64 / scale;
        // v0.4 fix (2026-05-20)：把容差从 0.5px 提到 3px。useAdaptiveOverlay
        // 用 Math.ceil + 亚像素 getBoundingClientRect → 经常飘 1-2px，老阈值挡不住
        // → 触发 resize → 锚点保持反算又造成 1-2px 偏移 → 累积 → 桌宠"飘"+ crash。
        if (cur_w - w).abs() < 3.0 && (cur_h - h).abs() < 3.0 {
            return;
        }
        let frame = visible_frame_top_left();
        let (anchor_x, anchor_y) = resolve_anchor(cur_x, cur_y, cur_w, cur_h, frame);
        // 桌宠中心落在屏幕锚点 → 反算窗口左上角（用前端实测的桌宠相对位置，不再写死居中）
        let raw_new_x = anchor_x - w * rx;
        let raw_new_y = anchor_y - (h - fb);
        let (new_x, new_y, bonk) = if pet_anchored {
            clamp_keep_pet(raw_new_x, raw_new_y, w, h, rx, fb, frame)
        } else {
            clamp_origin_with_bonk(raw_new_x, raw_new_y, w, h, frame)
        };
        CURRENT_MODE.store(1, Ordering::Relaxed);
        let _ = window.set_size(LogicalSize::new(w, h));
        let _ = window.set_position(LogicalPosition::new(new_x, new_y));
        // 摆完后把真锚点（桌宠中心屏幕坐标）存回 —— 偏侧布局下 resolve_anchor 用固定
        // RATIO 反算当前桌宠位置会偏，显式存真值消除累积误差。
        LAST_PET_CENTER_X.store(anchor_x.round() as i32, Ordering::Relaxed);
        LAST_PET_CENTER_Y.store(anchor_y.round() as i32, Ordering::Relaxed);
        if let Some(dir) = bonk { emit_bonk_edge(&app2, dir); }
    });
}

/// v0.4+ 漂移根治 · flush-aware 权威锚点解析（set_mode + set_to_explicit 共用）。
///
/// 窗口当前正贴着屏幕边 = 上次被 clamp 顶住了 → 当前位置不可信，改用上次存的
/// "未被 clamp 真锚点"。不贴边 → 当前位置即真值，存回去。
/// 哨兵 i32::MIN 表示从未存过 → 一律用当前位置兜底。
///
/// 返回桌宠中心的屏幕绝对坐标（逻辑像素，top-left origin）。
///
/// ⚠️ 为什么 set_mode 也必须走这个：之前 set_mode（shrink/expand）无条件用当前位置
/// 反算并存锚点。当 nudge / petMenu 把窗口扩到 320 并被屏幕边 clamp 后，用户点"稍后"
/// 触发 shrink_to_compact → set_mode 读到的是"被顶住的 320 窗口"位置 → 把 clamp 偏移
/// 烤进 LAST_PET_CENTER → 桌宠收缩后回不到角落，向屏幕中心漂移（2026-05-21 用户报）。
fn resolve_anchor(
    cur_x: f64, cur_y: f64, cur_w: f64, cur_h: f64,
    frame: Option<(f64, f64, f64, f64)>,
) -> (f64, f64) {
    let cur_pet_cx = cur_x + cur_w * PET_X_OFFSET_RATIO;
    let cur_pet_cy = cur_y + cur_h - PET_BOTTOM_OFFSET;
    let stored_x = LAST_PET_CENTER_X.load(Ordering::Relaxed);
    let stored_y = LAST_PET_CENTER_Y.load(Ordering::Relaxed);
    let have_stored = stored_x != i32::MIN && stored_y != i32::MIN;
    let flush = is_flush_against_edge(cur_x, cur_y, cur_w, cur_h, frame);
    if flush && have_stored {
        (stored_x as f64, stored_y as f64)
    } else {
        // ⚠️ 用 .round() 不是 `as i32`（向零截断）：截断对正 y（向下为正）= 每次都往
        // 屏幕上方截 <1px，多次 resize 累积成"往上漂"。round 取最近整数，无单向偏置。
        LAST_PET_CENTER_X.store(cur_pet_cx.round() as i32, Ordering::Relaxed);
        LAST_PET_CENTER_Y.store(cur_pet_cy.round() as i32, Ordering::Relaxed);
        (cur_pet_cx, cur_pet_cy)
    }
}

/// 窗口是否正贴着屏幕 visible frame 的任一边（说明被 clamp 顶住了）。
/// 拿不到屏幕 → false（兜底当作没贴边，用当前位置）。
fn is_flush_against_edge(x: f64, y: f64, w: f64, h: f64, frame: Option<(f64, f64, f64, f64)>) -> bool {
    let Some((sx, sy, sw, sh)) = frame else { return false; };
    const EPS: f64 = 2.0;
    x <= sx + EPS || y <= sy + EPS || x + w >= sx + sw - EPS || y + h >= sy + sh - EPS
}

/// 把窗口左上角 (x,y) clamp 进 visible frame，并报出被顶向了哪面墙（=撞了那面墙）。
/// 拿不到屏幕 → 原值 + None。纯函数，可单测。
fn clamp_origin_with_bonk(x: f64, y: f64, w: f64, h: f64, frame: Option<(f64, f64, f64, f64)>)
    -> (f64, f64, Option<BonkDir>)
{
    let Some((sx, sy, sw, sh)) = frame else { return (x, y, None); };
    let max_x = (sx + sw - w).max(sx);
    let max_y = (sy + sh - h).max(sy);
    let cx = x.clamp(sx, max_x);
    let cy = y.clamp(sy, max_y);
    let mut dir = None;
    // 被往右推 = 撞左墙；被往左推 = 撞右墙（水平优先）
    if cx > x + 0.5 { dir = Some(BonkDir::Left); }
    else if cx < x - 0.5 { dir = Some(BonkDir::Right); }
    else if cy > y + 0.5 { dir = Some(BonkDir::Top); }
    else if cy < y - 0.5 { dir = Some(BonkDir::Bottom); }
    (cx, cy, dir)
}

/// v0.4.3 · 「桌宠本体保持」clamp —— 只保证桌宠本体留在屏内，允许窗口（桌宠那侧的
/// 透明边距 / 阴影）溢出屏幕。PetMenu 方向感知路径用：菜单已往屏幕内侧展开不会越界，
/// 桌宠在角落时本体本就在屏 → 不被 clamp → 桌宠纹丝不动（不再被整窗 clamp 拽走）。
/// 纯函数，桌宠相对位置由 (pet_ratio_x, pet_from_bottom) 给出。
fn clamp_keep_pet(
    raw_x: f64, raw_y: f64, w: f64, h: f64,
    pet_ratio_x: f64, pet_from_bottom: f64,
    frame: Option<(f64, f64, f64, f64)>,
) -> (f64, f64, Option<BonkDir>) {
    let Some((sx, sy, sw, sh)) = frame else { return (raw_x, raw_y, None); };
    const PET_HALF: f64 = 48.0; // 桌宠本体约 96px 宽，半宽
    const PET_TOP: f64 = 48.0;
    const PET_BOT: f64 = 8.0;
    let cx = raw_x + w * pet_ratio_x;        // 桌宠中心 x
    let cy = raw_y + (h - pet_from_bottom);  // 桌宠中心 y
    let lo_x = sx + PET_HALF;
    let hi_x = (sx + sw - PET_HALF).max(lo_x);
    let lo_y = sy + PET_TOP;
    let hi_y = (sy + sh - PET_BOT).max(lo_y);
    let ccx = cx.clamp(lo_x, hi_x);
    let ccy = cy.clamp(lo_y, hi_y);
    let nx = ccx - w * pet_ratio_x;
    let ny = ccy - (h - pet_from_bottom);
    let mut dir = None;
    if ccx > cx + 0.5 { dir = Some(BonkDir::Left); }
    else if ccx < cx - 0.5 { dir = Some(BonkDir::Right); }
    else if ccy > cy + 0.5 { dir = Some(BonkDir::Top); }
    else if ccy < cy - 0.5 { dir = Some(BonkDir::Bottom); }
    (nx, ny, dir)
}

/// PetMenu 估算尺寸（跟 PetMenu.css width:220 + 8 项高度一致）—— 算展开方向用。
pub const PET_MENU_W: f64 = 220.0;
pub const PET_MENU_H: f64 = 300.0;

/// v0.4.3 · 算 PetMenu 该往哪边展开（贴边时翻向屏幕内侧），逻辑同
/// docs/prototypes/petmenu-edge-aware-20260521.html。
/// 返回 (h, v)：h ∈ {"left","right","center"}（菜单锚向），v ∈ {"up","down"}。
/// 用上次存的桌宠中心屏幕坐标 (LAST_PET_CENTER) + 屏幕 visibleFrame 算。
/// 拿不到屏幕 / 没有桌宠位置 → 退回 ("center","up")（= 老行为）。
pub fn compute_menu_orientation(frame: Option<(f64, f64, f64, f64)>) -> (&'static str, &'static str) {
    let Some((sx, sy, sw, sh)) = frame else { return ("center", "up"); };
    let pcx = LAST_PET_CENTER_X.load(Ordering::Relaxed);
    let pcy = LAST_PET_CENTER_Y.load(Ordering::Relaxed);
    if pcx == i32::MIN || pcy == i32::MIN { return ("center", "up"); }
    let (petcx, petcy) = (pcx as f64, pcy as f64);
    let space_right = (sx + sw) - petcx;
    let space_left = petcx - sx;
    let h = if space_right < PET_MENU_W / 2.0 + 8.0 { "right" }       // 贴右 → 往左展开
            else if space_left < PET_MENU_W / 2.0 + 8.0 { "left" }    // 贴左 → 往右展开
            else { "center" };
    let space_up = petcy - sy;
    let v = if space_up < PET_MENU_H + 20.0 { "down" } else { "up" };
    (h, v)
}

/// follow 路径专用 clamp：桌宠跟随光标时，要留在屏内的是**桌宠本体**（约 96px，
/// 居中靠底），而不是整个 320 透明窗口。允许透明边距溢出屏幕，但桌宠不被切。
/// 返回 clamp 后的窗口左上角 + 撞了哪面墙。
pub fn clamp_follow_with_bonk(raw_x: f64, raw_y: f64, w: f64, h: f64)
    -> (f64, f64, Option<BonkDir>)
{
    let Some((sx, sy, sw, sh)) = visible_frame_top_left() else { return (raw_x, raw_y, None); };
    const PET_HALF: f64 = 48.0;  // 桌宠 listening 时约 96px 宽，半宽
    const PET_FULL: f64 = 96.0;
    const BOTTOM_PAD: f64 = 8.0; // 桌宠底距窗口底
    // 水平：桌宠中心 = 窗口中心
    let center_x = raw_x + w / 2.0;
    let lo_x = sx + PET_HALF;
    let hi_x = (sx + sw - PET_HALF).max(lo_x);
    let cc = center_x.clamp(lo_x, hi_x);
    // 垂直：桌宠底部留在屏内
    let pet_bottom = raw_y + h - BOTTOM_PAD;
    let lo_b = sy + PET_FULL;
    let hi_b = (sy + sh).max(lo_b);
    let cb = pet_bottom.clamp(lo_b, hi_b);
    let nx = cc - w / 2.0;
    let ny = cb - h + BOTTOM_PAD;
    let mut dir = None;
    if cc > center_x + 0.5 { dir = Some(BonkDir::Left); }
    else if cc < center_x - 0.5 { dir = Some(BonkDir::Right); }
    else if cb > pet_bottom + 0.5 { dir = Some(BonkDir::Top); }
    else if cb < pet_bottom - 0.5 { dir = Some(BonkDir::Bottom); }
    (nx, ny, dir)
}

/// 主屏 visibleFrame（top-left origin, logical pt）—— (x, y, w, h)。已扣 menubar + dock。
/// 拿不到 → None。clamp 逻辑 + reposition_to_cursor 共用。⚠️ 必须主线程调用。
#[cfg(target_os = "macos")]
pub(crate) fn visible_frame_top_left() -> Option<(f64, f64, f64, f64)> {
    use cocoa::base::id;
    use cocoa::foundation::NSRect;
    use objc::{class, msg_send, sel, sel_impl};
    unsafe {
        let screen: id = msg_send![class!(NSScreen), mainScreen];
        if screen as usize == 0 { return None; }
        let full: NSRect = msg_send![screen, frame];
        let visible: NSRect = msg_send![screen, visibleFrame];
        let screen_h = full.size.height;
        let top_y = screen_h - (visible.origin.y + visible.size.height);
        Some((visible.origin.x, top_y, visible.size.width, visible.size.height))
    }
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn visible_frame_top_left() -> Option<(f64, f64, f64, f64)> { None }

fn set_mode(app: &AppHandle, new_size: f64, mode_tag: u32) {
    // v0.5.x bug1 根因修复：早返回**不再**只看 CURRENT_MODE 标签 —— 改为在主线程闭包里
    // 看**窗口实际尺寸**。
    //   为什么：set_to_explicit（自适应测量）和 set_mode（expand/shrink）共用 CURRENT_MODE，
    //   但 set_to_explicit 会把窗口设成任意尺寸（如菜单收起瞬间测到桌宠 ~120px）同时
    //   store(CURRENT_MODE=1)。此后 expand_to_full→set_mode(320,1) 看到 swap(1)==1 直接
    //   no-op → 窗口卡在 120，气泡被剪（"点开始对话第二次看不到气泡"真因，日志实锤）。
    //   只信"模式标签"会被 set_to_explicit 的 size 跟 mode 解耦坑到。改信实际 size 后，
    //   只要当前尺寸 ≠ 目标尺寸就真 resize，常态(已 320)仍 no-op、无额外漂移。
    CURRENT_MODE.store(mode_tag, Ordering::SeqCst);
    // v0.4.0 fix · 必须 marshal 到主线程 ——
    // emit_view 可能来自 CGEventTap 工作线程（fn 按键 → start_recording_for_ime →
    // show_mouse 排队主线程闭包 → emit_view 同步直接调 set_mode）。
    // 如果这里同步在工作线程跑 set_size + set_position，会跟主线程上 show_mouse 闭包的
    // set_position 形成竞态，结果是窗口最终位置 = 锚点位置（老 anchor），桌宠没跟到光标。
    // 全部走 run_on_main_thread 后所有窗口操作按 FIFO 序列在主线程上跑，竞态消失。
    let app2 = app.clone();
    let _ = app.run_on_main_thread(move || {
        let Some(window) = app2.get_webview_window("mouse") else { return; };
        let Ok(pos) = window.outer_position() else { return; };
        let Ok(size) = window.outer_size() else { return; };
        let Ok(scale) = window.scale_factor() else { return; };
        let cur_x = pos.x as f64 / scale;
        let cur_y = pos.y as f64 / scale;
        let cur_w = size.width as f64 / scale;
        let cur_h = size.height as f64 / scale;

        // 已是目标尺寸 → no-op（按实际 size 判，不按 mode 标签 —— 见函数头注释）。
        // 容差 3px 跟 set_to_explicit 一致，挡住亚像素抖动。
        if (cur_w - new_size).abs() < 3.0 && (cur_h - new_size).abs() < 3.0 {
            return;
        }

        // 当前桌宠的屏幕绝对位置（视觉锚点）—— flush-aware：窗口被屏幕边顶住时
        // 用存的真锚点而非"被顶过的当前位置"，否则收缩会把 clamp 偏移烤进锚点 → 漂移。
        let frame = visible_frame_top_left();
        let (pet_cx, pet_cy) = resolve_anchor(cur_x, cur_y, cur_w, cur_h, frame);

        // 新窗口需要放在哪里才能让桌宠中心保持原位
        let raw_new_x = pet_cx - new_size * PET_X_OFFSET_RATIO;
        let raw_new_y = pet_cy - new_size + PET_BOTTOM_OFFSET;
        // v0.4+ 漂移根治（2026-05-21）：必须跟 set_to_explicit 一样 clamp 进 visible frame。
        // 之前 set_mode 不 clamp → 在角落把 320 窗口摆到屏幕外（如右下溢出）→ macOS
        // NSWindow.constrainFrameRect 会把溢出窗口悄悄往屏内（左上）挪 → 我们下次读
        // outer_position() 读到的是被挪过的位置 → 跨 resize 周期锚点不一致 → 桌宠"往斜上方
        // 不停漂移"。clamp 后窗口永远在屏内，macOS 没机会偷偷挪它。
        let (new_x, new_y, bonk) = clamp_origin_with_bonk(raw_new_x, raw_new_y, new_size, new_size, frame);

        let _ = window.set_size(LogicalSize::new(new_size, new_size));
        let _ = window.set_position(LogicalPosition::new(new_x, new_y));
        if let Some(dir) = bonk { emit_bonk_edge(&app2, dir); }
        println!(
            "[overlay_size] mode={mode_tag} size={new_size:.0} anchor_xy=({pet_cx:.0},{pet_cy:.0}) win_xy=({new_x:.0},{new_y:.0})"
        );
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    // 1920×1080 主屏，visible frame 从 (0,0) 起（忽略 menubar 简化）。
    const F: Option<(f64, f64, f64, f64)> = Some((0.0, 0.0, 1920.0, 1080.0));

    #[test]
    fn clamp_no_edge_no_bonk() {
        // 窗口完全在屏内 → 不动 + 无撞墙
        let (x, y, b) = clamp_origin_with_bonk(800.0, 400.0, 320.0, 320.0, F);
        assert_eq!((x, y), (800.0, 400.0));
        assert!(b.is_none());
    }

    #[test]
    fn clamp_left_edge_reports_left_bonk() {
        // 想去 x=-50（左溢出）→ 推回 0 → 撞左墙
        let (x, _, b) = clamp_origin_with_bonk(-50.0, 400.0, 320.0, 320.0, F);
        assert_eq!(x, 0.0);
        assert_eq!(b, Some(BonkDir::Left));
    }

    #[test]
    fn clamp_right_edge_reports_right_bonk() {
        let (x, _, b) = clamp_origin_with_bonk(1700.0, 400.0, 320.0, 320.0, F);
        assert_eq!(x, 1920.0 - 320.0);
        assert_eq!(b, Some(BonkDir::Right));
    }

    #[test]
    fn clamp_top_edge_reports_top_bonk() {
        let (_, y, b) = clamp_origin_with_bonk(800.0, -30.0, 320.0, 320.0, F);
        assert_eq!(y, 0.0);
        assert_eq!(b, Some(BonkDir::Top));
    }

    #[test]
    fn clamp_bottom_edge_reports_bottom_bonk() {
        let (_, y, b) = clamp_origin_with_bonk(800.0, 900.0, 320.0, 320.0, F);
        assert_eq!(y, 1080.0 - 320.0);
        assert_eq!(b, Some(BonkDir::Bottom));
    }

    #[test]
    fn no_frame_passes_through() {
        let (x, y, b) = clamp_origin_with_bonk(-999.0, -999.0, 320.0, 320.0, None);
        assert_eq!((x, y), (-999.0, -999.0));
        assert!(b.is_none());
    }

    /// 漂移根治回归。两个断言放同一 test 内顺序跑 —— LAST_PET_CENTER 是进程级
    /// 全局静态，拆成两个 #[test] 会因 cargo 并行执行互相污染（flaky）。
    #[test]
    fn resolve_anchor_flush_aware() {
        // (1) 贴边窗口收缩时必须用存的真锚点，不被 clamp 偏移污染。
        // 先存一个"角落真锚点"（桌宠中心在 sw-64 = 1856, sh-64 = 1016）
        LAST_PET_CENTER_X.store(1856, Ordering::Relaxed);
        LAST_PET_CENTER_Y.store(1016, Ordering::Relaxed);
        // 模拟被右下角 clamp 顶住的 320 窗口：x=1600 → x+w=1920 贴右边
        let (ax, ay) = resolve_anchor(1600.0, 760.0, 320.0, 320.0, F);
        // 必须返回存的真锚点，不是被顶过的当前位置反算值
        assert_eq!((ax, ay), (1856.0, 1016.0));

        // (2) 不贴边时用当前位置并存回（自愈 stale anchor）。
        LAST_PET_CENTER_X.store(9999, Ordering::Relaxed); // 故意放一个 stale 值
        LAST_PET_CENTER_Y.store(9999, Ordering::Relaxed);
        // 屏幕中间的 80 compact 窗口，不贴边
        let (ax, ay) = resolve_anchor(800.0, 400.0, 80.0, 80.0, F);
        // 当前位置反算：cx = 800+40 = 840, cy = 400+80-40 = 440
        assert_eq!((ax, ay), (840.0, 440.0));
        // 且存回了当前真值
        assert_eq!(LAST_PET_CENTER_X.load(Ordering::Relaxed), 840);
        assert_eq!(LAST_PET_CENTER_Y.load(Ordering::Relaxed), 440);
    }

    #[test]
    fn flush_detection() {
        // 贴左边
        assert!(is_flush_against_edge(0.0, 400.0, 320.0, 320.0, F));
        // 贴右边
        assert!(is_flush_against_edge(1600.0, 400.0, 320.0, 320.0, F));
        // 屏幕中间不贴边
        assert!(!is_flush_against_edge(800.0, 400.0, 320.0, 320.0, F));
        // 拿不到屏幕 → 不算贴边
        assert!(!is_flush_against_edge(0.0, 0.0, 320.0, 320.0, None));
    }
}

/// 启动时初始化 —— 把窗口缩到 compact 模式，让默认 idle 静默状态就只占桌宠区域。
pub fn init(app: &AppHandle) {
    // tauri.conf.json 已经设 100×100，这里再 force 一次防止 anchor.rs 等模块改过尺寸。
    // 不调 set_mode（要走"保持视觉位置"逻辑，启动时窗口可能还没渲染）—— 直接 set_size。
    if let Some(window) = app.get_webview_window("mouse") {
        let _ = window.set_size(LogicalSize::new(COMPACT_SIZE, COMPACT_SIZE));
        CURRENT_MODE.store(0, Ordering::Relaxed);
        println!("[overlay_size] init: compact 100×100");
    }
}
