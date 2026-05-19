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

use std::sync::atomic::{AtomicI32, AtomicU32, Ordering};
use tauri::{AppHandle, LogicalPosition, LogicalSize, Manager};

/// 桌宠在窗口里的水平偏移（窗口宽 / 2，因为桌宠水平居中）
const PET_X_OFFSET_RATIO: f64 = 0.5;
/// 桌宠中心距窗口底部 = pet_size/2 + padding(8px).
/// idle 64 → 32+8=40；listening 96 → 48+8=56。
/// 取 40 跟 idle 一致（listening 时窗口已经扩到 320，offset 不影响视觉锚点）。
const PET_BOTTOM_OFFSET: f64 = 40.0;

/// 当前窗口模式。0 = compact(80), 1 = expanded(320)
static CURRENT_MODE: AtomicU32 = AtomicU32::new(0);
/// 上次桌宠中心的屏幕逻辑像素位置（用于切换时保持视觉锚点不变）。
static LAST_PET_CENTER_X: AtomicI32 = AtomicI32::new(0);
static LAST_PET_CENTER_Y: AtomicI32 = AtomicI32::new(0);

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
pub fn set_to_explicit(app: &AppHandle, want_w: f64, want_h: f64) {
    let w = want_w.clamp(COMPACT_SIZE, 1200.0);
    let h = want_h.clamp(COMPACT_SIZE, 1200.0);
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
        let pet_cx = cur_x + cur_w * PET_X_OFFSET_RATIO;
        let pet_cy = cur_y + cur_h - PET_BOTTOM_OFFSET;
        let raw_new_x = pet_cx - w * PET_X_OFFSET_RATIO;
        let raw_new_y = pet_cy - (h - PET_BOTTOM_OFFSET);
        // v0.4 fix · clamp 到 visibleFrame 防止累积飘移把窗口推出屏幕导致 NSWindow
        // 收到非法 frame crash。用 macOS visibleFrame（已扣 menubar + dock）。
        let (new_x, new_y) = clamp_to_screen(raw_new_x, raw_new_y, w, h);
        CURRENT_MODE.store(1, Ordering::Relaxed);
        LAST_PET_CENTER_X.store(pet_cx as i32, Ordering::Relaxed);
        LAST_PET_CENTER_Y.store(pet_cy as i32, Ordering::Relaxed);
        let _ = window.set_size(LogicalSize::new(w, h));
        let _ = window.set_position(LogicalPosition::new(new_x, new_y));
    });
}

/// 把 (x, y) clamp 到主屏 visibleFrame 内（top-left origin, logical pt），让宽 w / 高 h
/// 的窗口仍完整在屏幕内。返回 (safe_x, safe_y)。
/// 主屏拿不到 → 返回原值（兜底退化）。
fn clamp_to_screen(x: f64, y: f64, w: f64, h: f64) -> (f64, f64) {
    #[cfg(target_os = "macos")]
    {
        if let Some((sx, sy, sw, sh)) = visible_frame_top_left() {
            let max_x = sx + sw - w;
            let max_y = sy + sh - h;
            let safe_x = x.clamp(sx, max_x.max(sx));
            let safe_y = y.clamp(sy, max_y.max(sy));
            return (safe_x, safe_y);
        }
    }
    (x, y)
}

#[cfg(target_os = "macos")]
fn visible_frame_top_left() -> Option<(f64, f64, f64, f64)> {
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

fn set_mode(app: &AppHandle, new_size: f64, mode_tag: u32) {
    if CURRENT_MODE.swap(mode_tag, Ordering::SeqCst) == mode_tag {
        return; // 已是目标模式
    }
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

        // 当前桌宠的屏幕绝对位置（视觉锚点）
        let pet_cx = cur_x + cur_w * PET_X_OFFSET_RATIO;
        let pet_cy = cur_y + cur_h - PET_BOTTOM_OFFSET;

        // 新窗口需要放在哪里才能让桌宠中心保持原位
        let new_x = pet_cx - new_size * PET_X_OFFSET_RATIO;
        let new_y = pet_cy - new_size + PET_BOTTOM_OFFSET;

        LAST_PET_CENTER_X.store(pet_cx as i32, Ordering::Relaxed);
        LAST_PET_CENTER_Y.store(pet_cy as i32, Ordering::Relaxed);

        let _ = window.set_size(LogicalSize::new(new_size, new_size));
        let _ = window.set_position(LogicalPosition::new(new_x, new_y));
        println!(
            "[overlay_size] mode={mode_tag} size={new_size:.0} anchor_xy=({pet_cx:.0},{pet_cy:.0}) win_xy=({new_x:.0},{new_y:.0})"
        );
    });
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
