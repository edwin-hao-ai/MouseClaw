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

/// 进入 compact 模式（100×100，只占桌宠像素）。
pub fn shrink_to_compact(app: &AppHandle) {
    set_mode(app, COMPACT_SIZE, 0);
}

/// 进入 expanded 模式（320×320，气泡有空间）。
pub fn expand_to_full(app: &AppHandle) {
    set_mode(app, EXPANDED_SIZE, 1);
}

fn set_mode(app: &AppHandle, new_size: f64, mode_tag: u32) {
    if CURRENT_MODE.swap(mode_tag, Ordering::SeqCst) == mode_tag {
        return; // 已是目标模式
    }
    let Some(window) = app.get_webview_window("mouse") else { return; };
    // 拿当前窗口位置和尺寸（逻辑像素）
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

    // 先 resize 再 reposition（顺序无所谓但要都做）
    let _ = window.set_size(LogicalSize::new(new_size, new_size));
    let _ = window.set_position(LogicalPosition::new(new_x, new_y));
    println!(
        "[overlay_size] mode={mode_tag} size={new_size:.0} anchor_xy=({pet_cx:.0},{pet_cy:.0}) win_xy=({new_x:.0},{new_y:.0})"
    );
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
