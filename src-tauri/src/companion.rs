//! v0.4+ · Companion animation tick —— 30 Hz 全局光标 + 键鼠 idle 推送。
//!
//! 设计原型：`docs/prototypes/companion-animations-20260519.html`
//!
//! 桌宠的"陪伴向"动画（眼球追鼠标 / 打字陪伴点头 / 贴近反应 / 闲置渐睡）
//! 需要桌宠 webview 知道**桌面上**鼠标在哪 + 用户上次敲键 / 移鼠多久前。
//! webview 自己只能拿 webview 范围内的 mousemove，没法看见全屏。
//!
//! ## 隐私边界
//! 与 [`presence`] 同一红线：**不读按键内容**，只读 `CGEventSourceSecondsSince…`
//! 时间戳和 `NSEvent.mouseLocation` 坐标。无需 Accessibility 权限。
//!
//! ## 节流策略
//! 30 FPS 推一次。坐标 4 字节 × 2 + 浮点 4 字节 × 2 ≈ 16B/tick → 480 B/s，
//! IPC 完全可忽略；CPU 主要花在 `NSEvent.mouseLocation`（μs 级）。
//!
//! ## 输出坐标系
//! 标准 Web 坐标（top-left 原点，y 朝下）—— 与 webview 的 `event.screenX/Y` 对齐，
//! 方便前端 `useCompanion` hook 算 webview-local 偏移。

use serde::Serialize;
use std::time::Duration;
use tauri::{AppHandle, Emitter};
use tokio::time::sleep;

pub const EV_COMPANION_TICK: &str = "companion-tick";

/// 30 FPS — 与人眼对动画追随的容忍上限一致；更高频率不带来视觉收益。
const TICK_INTERVAL_MS: u64 = 33;

#[derive(Debug, Clone, Copy, Serialize)]
pub struct CompanionTick {
    /// 桌面全局鼠标 X（CSS/Web 坐标，top-left 原点）
    pub x: f64,
    /// 桌面全局鼠标 Y（CSS/Web 坐标，top-left 原点）
    pub y: f64,
    /// 距上次按键秒数（CG combined session state）
    #[serde(rename = "sinceKey")]
    pub since_key: f64,
    /// 距上次鼠标移动秒数
    #[serde(rename = "sinceMouse")]
    pub since_mouse: f64,
}

/// 后台任务 —— lib.rs setup() 调一次。
pub fn spawn(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        loop {
            let tick = sample();
            // 只发给 mouse 窗口 —— picker / hub / panel 都不画桌宠主体
            if let Err(e) = app.emit_to("mouse", EV_COMPANION_TICK, &tick) {
                // 启动早期 mouse 窗口可能还没建好；只在 debug 噪音里出现
                #[cfg(debug_assertions)]
                eprintln!("[mouseclaw] 🐾 companion emit failed: {e}");
                let _ = e;
            }
            sleep(Duration::from_millis(TICK_INTERVAL_MS)).await;
        }
    });
    println!("[mouseclaw] 🐾 companion tick spawned ({TICK_INTERVAL_MS}ms · 30 FPS)");
}

/// 采一帧。non-macOS 平台返回零值（V1 macOS 优先，Windows 待 V2 各自实现）。
fn sample() -> CompanionTick {
    #[cfg(target_os = "macos")]
    {
        let (x, y) = mac::cursor_xy().unwrap_or((-1.0, -1.0));
        CompanionTick {
            x, y,
            since_key:   mac::secs_since_event(mac::EVENT_KEY_DOWN),
            since_mouse: mac::secs_since_event(mac::EVENT_MOUSE_MOVED),
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        CompanionTick { x: -1.0, y: -1.0, since_key: 999.0, since_mouse: 999.0 }
    }
}

#[cfg(target_os = "macos")]
mod mac {
    use cocoa::base::{id, nil};
    use cocoa::foundation::{NSPoint, NSRect, NSUInteger};
    use objc::{class, msg_send, sel, sel_impl};

    pub const EVENT_KEY_DOWN: u32 = 10;        // kCGEventKeyDown
    pub const EVENT_MOUSE_MOVED: u32 = 5;      // kCGEventMouseMoved
    const CG_STATE_COMBINED: u32 = 0;          // kCGEventSourceStateCombinedSessionState

    pub fn secs_since_event(event_type: u32) -> f64 {
        extern "C" {
            fn CGEventSourceSecondsSinceLastEventType(state: u32, event_type: u32) -> f64;
        }
        unsafe { CGEventSourceSecondsSinceLastEventType(CG_STATE_COMBINED, event_type) }
    }

    /// 全局鼠标坐标 → 翻成 Web 坐标（top-left 原点，y 朝下）。
    /// NSEvent.mouseLocation 是 Cocoa 坐标（bottom-left），y 朝上。
    /// 屏幕高度从主屏 frame.height 取，多屏场景以 mouse 当前所在屏的高度为准。
    pub fn cursor_xy() -> Option<(f64, f64)> {
        unsafe {
            // NSPoint NSEvent.mouseLocation —— class method, 无权限要求
            let pt: NSPoint = msg_send![class!(NSEvent), mouseLocation];
            // 找鼠标所在屏（多屏时按 frame.contains:pt）
            let screens: id = msg_send![class!(NSScreen), screens];
            if screens == nil { return Some((pt.x, pt.y)); }
            let count: NSUInteger = msg_send![screens, count];
            if count == 0 { return Some((pt.x, pt.y)); }

            let mut total_height: f64 = 0.0;
            // 多屏：用主屏（idx 0）frame 高度做坐标翻转参考 —— 与 Tauri webview 的
            // window.screenY 同坐标系（top-left of main screen, y 朝下）。
            let primary: id = msg_send![screens, objectAtIndex:0usize];
            if primary != nil {
                let frame: NSRect = msg_send![primary, frame];
                total_height = frame.size.height;
            }

            let web_y = total_height - pt.y;
            Some((pt.x, web_y))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tick_serializes_to_camelcase_keys() {
        let t = CompanionTick { x: 1.0, y: 2.0, since_key: 0.5, since_mouse: 0.7 };
        let j = serde_json::to_string(&t).unwrap();
        // 关键 key 名 —— 前端 hook 直接依赖
        assert!(j.contains("\"x\":1"));
        assert!(j.contains("\"y\":2"));
        assert!(j.contains("\"sinceKey\":0.5"), "got {j}");
        assert!(j.contains("\"sinceMouse\":0.7"), "got {j}");
    }

    #[test]
    fn tick_event_name_stable() {
        // 前端 listen("companion-tick") 是契约 —— 改名要同步前端
        assert_eq!(EV_COMPANION_TICK, "companion-tick");
    }
}
