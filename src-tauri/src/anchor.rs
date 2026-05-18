//! 桌宠悬停位置管理 (v0.1.27)
//!
//! 闲置时 overlay 停在屏幕某个角落"打盹"；按快捷键召唤时跑到光标位置工作，
//! `schedule_auto_hide` 触发后 `apply_idle_anchor` 把它送回原角落。
//!
//! 4 corner = pinned visible. Follow = 隐藏，由 `cursor_follow` 接管。
//!
//! ⚠️ AppKit 线程安全：本模块所有读 NSScreen / 操作 NSWindow 的函数必须
//! 在主线程跑，调用方用 `app.run_on_main_thread(...)` marshal。

use tauri::menu::{CheckMenuItem, Submenu};
use tauri::{AppHandle, LogicalPosition, Manager};

use crate::config::PetAnchor;

/// 4-corner + Follow 的中英文人类可读 label。
/// 顺序 = `PetAnchor::all()` 的顺序。
pub fn anchor_label(a: PetAnchor, en: bool) -> &'static str {
    match (a, en) {
        (PetAnchor::TopLeft,     true)  => "↖︎ Top Left",
        (PetAnchor::TopLeft,     false) => "↖︎ 左上",
        (PetAnchor::TopRight,    true)  => "↗︎ Top Right",
        (PetAnchor::TopRight,    false) => "↗︎ 右上",
        (PetAnchor::BottomLeft,  true)  => "↙︎ Bottom Left",
        (PetAnchor::BottomLeft,  false) => "↙︎ 左下",
        (PetAnchor::BottomRight, true)  => "↘︎ Bottom Right",
        (PetAnchor::BottomRight, false) => "↘︎ 右下",
        (PetAnchor::Follow,      true)  => "✨ Follow cursor",
        (PetAnchor::Follow,      false) => "✨ 跟随光标",
    }
}

/// 构造 📍 桌宠位置 ▸ 子菜单 —— tray.rs 引用即可。
/// 当前选中项打勾；id 形如 "anchor:bottom-right"。
pub fn build_tray_submenu(app: &AppHandle, en: bool)
    -> tauri::Result<Submenu<tauri::Wry>>
{
    let current = crate::config::Config::load().pet_anchor;
    let mut items: Vec<CheckMenuItem<tauri::Wry>> = Vec::new();
    for a in PetAnchor::all() {
        items.push(CheckMenuItem::with_id(
            app, a.tray_menu_id(), anchor_label(*a, en),
            true, *a == current, None::<&str>,
        )?);
    }
    let refs: Vec<&dyn tauri::menu::IsMenuItem<tauri::Wry>> =
        items.iter().map(|i| i as &dyn tauri::menu::IsMenuItem<tauri::Wry>).collect();
    let title = if en { "📍 Pet location" } else { "📍 桌宠位置" };
    Submenu::with_id_and_items(app, "anchor-submenu", title, true, &refs)
}

/// 4 corner 距屏幕边缘的内缩 padding（pt，逻辑像素）。
/// 大概等于 DESIGN.md 第 5 节 spacing scale 的 --space-6 ≈ 24pt。
const ANCHOR_PADDING: f64 = 24.0;

/// 把 overlay 窗口送回当前 anchor 指定的位置。
/// `Follow` → no-op（位置由 cursor_follow 接管），返回 false。
/// 其它 4 个角 → 计算位置并 `set_position`，返回 true。
pub fn apply_idle_anchor(app: &AppHandle, anchor: PetAnchor) -> bool {
    if !anchor.pin_visible_when_idle() {
        return false;
    }
    let app2 = app.clone();
    let _ = app.run_on_main_thread(move || {
        let Some(window) = app2.get_webview_window("mouse") else { return };
        let Some((sx, sy, sw, sh)) = visible_screen_frame_top_left() else { return };
        let (ww, wh) = match window.outer_size().ok() {
            Some(s) => (s.width as f64, s.height as f64),
            None => (320.0, 320.0),
        };
        let scale = window.scale_factor().unwrap_or(1.0);
        let win_w = ww / scale;
        let win_h = wh / scale;

        let (x, y) = corner_position(anchor, sx, sy, sw, sh, win_w, win_h, ANCHOR_PADDING);
        let _ = window.set_position(LogicalPosition::new(x, y));
        let _ = window.show();
        let _ = window.set_always_on_top(true);
    });
    true
}

/// 给定屏幕 visible frame（top-left origin, pt）和窗口逻辑尺寸，计算
/// 4 个角对应的窗口左上角位置。Follow 走另一条路径，不进这里。
///
/// 用 free function 是为了好测 —— 单元测试可以脱离 AppHandle / NSScreen
/// 直接验数学。
pub fn corner_position(
    anchor: PetAnchor,
    screen_x: f64, screen_y: f64,
    screen_w: f64, screen_h: f64,
    win_w: f64, win_h: f64,
    pad: f64,
) -> (f64, f64) {
    match anchor {
        PetAnchor::TopLeft => (
            screen_x + pad,
            screen_y + pad,
        ),
        PetAnchor::TopRight => (
            screen_x + screen_w - win_w - pad,
            screen_y + pad,
        ),
        PetAnchor::BottomLeft => (
            screen_x + pad,
            screen_y + screen_h - win_h - pad,
        ),
        PetAnchor::BottomRight => (
            screen_x + screen_w - win_w - pad,
            screen_y + screen_h - win_h - pad,
        ),
        // Follow 不会进来 —— pin_visible_when_idle 已挡。兜底回 BR。
        PetAnchor::Follow => (
            screen_x + screen_w - win_w - pad,
            screen_y + screen_h - win_h - pad,
        ),
    }
}

/// 当前 main screen 的 visibleFrame，转成 top-left origin 的 pt 坐标。
/// 返回 (x, y, w, h) —— x/y 是 visible 区域左上角在虚拟屏幕坐标系里的位置。
///
/// macOS NSScreen 用 bottom-left origin；Tauri set_position 用 top-left origin。
/// 这里统一转好，调用方不用再翻 Y 轴。
///
/// ⚠️ 必须主线程调用。
#[cfg(target_os = "macos")]
fn visible_screen_frame_top_left() -> Option<(f64, f64, f64, f64)> {
    use cocoa::base::id;
    use cocoa::foundation::NSRect;
    use objc::{class, msg_send, sel, sel_impl};
    unsafe {
        let screen: id = msg_send![class!(NSScreen), mainScreen];
        if screen as usize == 0 { return None; }
        // 整屏（含 menubar / dock 区域）—— 用来翻 Y
        let full: NSRect = msg_send![screen, frame];
        // 实际可放窗口的区域（去掉 menubar / dock）
        let visible: NSRect = msg_send![screen, visibleFrame];

        let screen_h = full.size.height;
        // NSScreen.visibleFrame.origin.y 是从屏幕底部算的；翻成 top-left。
        let top_y = screen_h - (visible.origin.y + visible.size.height);
        Some((visible.origin.x, top_y, visible.size.width, visible.size.height))
    }
}

#[cfg(not(target_os = "macos"))]
fn visible_screen_frame_top_left() -> Option<(f64, f64, f64, f64)> {
    // 非 macOS 兜底：1920x1080 主屏。
    Some((0.0, 0.0, 1920.0, 1080.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    // 用 1920x1080 主屏 + 320x320 overlay 跑数学验证。
    const SX: f64 = 0.0;
    const SY: f64 = 0.0;
    const SW: f64 = 1920.0;
    const SH: f64 = 1080.0;
    const WW: f64 = 320.0;
    const WH: f64 = 320.0;
    const PAD: f64 = 24.0;

    #[test]
    fn top_left_sits_at_padding_offset() {
        let (x, y) = corner_position(PetAnchor::TopLeft, SX, SY, SW, SH, WW, WH, PAD);
        assert_eq!((x, y), (24.0, 24.0));
    }

    #[test]
    fn top_right_keeps_window_inside_screen() {
        let (x, y) = corner_position(PetAnchor::TopRight, SX, SY, SW, SH, WW, WH, PAD);
        assert_eq!((x, y), (1920.0 - 320.0 - 24.0, 24.0));
    }

    #[test]
    fn bottom_left_offsets_from_bottom() {
        let (x, y) = corner_position(PetAnchor::BottomLeft, SX, SY, SW, SH, WW, WH, PAD);
        assert_eq!((x, y), (24.0, 1080.0 - 320.0 - 24.0));
    }

    #[test]
    fn bottom_right_default_position() {
        let (x, y) = corner_position(PetAnchor::BottomRight, SX, SY, SW, SH, WW, WH, PAD);
        assert_eq!((x, y), (1920.0 - 320.0 - 24.0, 1080.0 - 320.0 - 24.0));
    }

    #[test]
    fn follow_falls_back_to_bottom_right_in_math() {
        // 物理上 Follow 不该走 corner_position，但若意外调用应给个安全位置
        let (x, y) = corner_position(PetAnchor::Follow, SX, SY, SW, SH, WW, WH, PAD);
        assert_eq!((x, y), (1576.0, 736.0));
    }

    #[test]
    fn non_zero_screen_origin_offsets_correctly() {
        // 模拟"外接显示器作为主屏"：screen 在虚拟桌面 (1920, 0) 处
        let (x, y) = corner_position(PetAnchor::BottomRight, 1920.0, 0.0,
                                     SW, SH, WW, WH, PAD);
        assert_eq!((x, y), (1920.0 + 1920.0 - 320.0 - 24.0, 1080.0 - 320.0 - 24.0));
    }
}
