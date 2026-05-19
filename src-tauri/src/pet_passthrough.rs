//! v0.3.12 · 让 mouse overlay 窗口的透明区域真正穿透到底层 app。
//!
//! 问题：mouse 窗口是 320×320 透明 NSWindow + alwaysOnTop。macOS 在窗口层面
//! 先做 hit-test，即便 WebKit 内部 `pointer-events: none`，整个 320×320 仍然
//! 拦截鼠标事件 → 用户看不见但点不到下面的截图缩略、按钮、Finder 文件。
//!
//! 解：30fps 后台 task 拿全局光标位置 → 算它在 mouse 窗口里的局部坐标 →
//! 看光标是否在桌宠 / 气泡的实际像素区（hit-box）→ 用 `set_ignore_cursor_events`
//! 切换：
//!   - 在 hit-box 内 → false（窗口接收事件，桌宠/气泡可点）
//!   - 不在 → true（穿透到底层 app）
//!
//! hit-box 取决于 `AppState.overlay_has_ui`：
//!   - false（idle 静默）→ 右下 200×200，只有桌宠所在角落接收
//!   - true（有气泡/菜单/Panel）→ 整个 320×320（用户在跟 UI 互动，全窗口接收）

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tauri::{AppHandle, Manager};

use crate::AppState;

/// idle 状态下桌宠的 hit-box —— 桌宠用 .stage 的 `justify-content: flex-end + align-items: center`
/// 布局，永远在窗口**底部中央**。所以 hit-box 是底部中央的一条垂直矩形：
///   - 宽度 130px（盖住桌宠 idle 64 / listening 96 + padding，居中）
///   - 高度 140px（桌宠 + 上方少量气泡余地；更大气泡时由 set_overlay_has_ui(true) 扩到全窗口）
/// 剩下窗口的 ~85% 区域穿透到底层 app（4 角 + 上方 + 左右两侧）。
const HIT_BOX_W: f64 = 130.0;
const HIT_BOX_H: f64 = 140.0;

/// 当前是否处于"穿透"状态。避免每帧 spam set_ignore API（5/100ms 没必要重复设）
static CURRENT_IGNORE: AtomicBool = AtomicBool::new(false);

pub fn spawn_loop(app: AppHandle, state: Arc<AppState>) {
    tauri::async_runtime::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_millis(50)); // 20fps
        tick.tick().await; // skip first immediate fire
        loop {
            tick.tick().await;
            update(&app, &state);
        }
    });
}

fn update(app: &AppHandle, state: &Arc<AppState>) {
    let Some(window) = app.get_webview_window("mouse") else { return; };

    // mouse 窗口不可见时 → 走"穿透"分支：set_ignore=true（虽然此时也没 UI）
    let visible = window.is_visible().unwrap_or(false);
    if !visible {
        set_ignore(app, &window, true);
        return;
    }

    // 拿光标 (top-left coords) 和窗口位置/尺寸
    let Some((cx, cy)) = global_cursor_top_left() else { return; };
    let Ok(pos) = window.outer_position() else { return; };
    let Ok(size) = window.outer_size() else { return; };
    let Ok(scale) = window.scale_factor() else { return; };

    // outer_position/size 是物理像素，光标是逻辑像素 —— 都转成逻辑像素比对
    let wx = pos.x as f64 / scale;
    let wy = pos.y as f64 / scale;
    let ww = size.width as f64 / scale;
    let wh = size.height as f64 / scale;

    let local_x = cx - wx;
    let local_y = cy - wy;

    // 光标不在窗口内 → 一定穿透
    if local_x < 0.0 || local_y < 0.0 || local_x > ww || local_y > wh {
        set_ignore(app, &window, true);
        return;
    }

    // 在窗口内 → 看是否在 hit-box 内
    let has_ui = state.overlay_has_ui.load(Ordering::Relaxed);
    let in_hit_box = if has_ui {
        // 有气泡 / 菜单 / 引导 / nudge → 整个窗口接收（PetMenu 弹出区可能在桌宠四周任意方向）
        true
    } else {
        // idle 静默 → 只有底部中央矩形接收（桌宠在 .stage flex-end + center 位置）
        let cx = ww / 2.0;
        let bottom = wh;
        (local_x - cx).abs() <= HIT_BOX_W / 2.0
            && (bottom - local_y) <= HIT_BOX_H
            && (bottom - local_y) >= 0.0
    };

    set_ignore(app, &window, !in_hit_box);
}

fn set_ignore(app: &AppHandle, window: &tauri::WebviewWindow, ignore: bool) {
    // 跟上一次相同 → skip
    let prev = CURRENT_IGNORE.swap(ignore, Ordering::SeqCst);
    if prev == ignore { return; }

    // v0.4 crash fix (2026-05-20) · NSWindow API 必须在主线程调，否则会跟 AppKit
    // 自己的 -[NSWMWindowCoordinator performTransactionUsingBlock:] 抢窗口状态锁
    // → EXC_BREAKPOINT SIGTRAP。日志里 250+ 次 set_ignore_native 大部分正常通过，
    // 但只要碰上 useAdaptiveOverlay 触发 set_overlay_content_size resize 的同一帧
    // 就会撞窗口 transaction 锁炸进程。
    //
    // 修：整段 NSWindow 触碰路径走 run_on_main_thread。tokio worker 只下发任务。
    let window = window.clone();
    let _ = app.run_on_main_thread(move || {
        // 1. Tauri wrapper（保险）
        match window.set_ignore_cursor_events(ignore) {
            Ok(_) => println!("[passthrough] tauri set_ignore_cursor_events({ignore}) ok"),
            Err(e) => eprintln!("[passthrough] tauri set_ignore_cursor_events({ignore}) FAIL: {e}"),
        }
        // 2. native NSWindow setIgnoresMouseEvents:（绕过 wrapper 直接打 AppKit）
        #[cfg(target_os = "macos")]
        set_ignore_native(&window, ignore);
    });
}

/// 直接通过 ns_window() 调 NSWindow setIgnoresMouseEvents:。
/// 必须在主线程调（被 set_ignore 通过 run_on_main_thread marshal 进来）。
#[cfg(target_os = "macos")]
fn set_ignore_native(window: &tauri::WebviewWindow, ignore: bool) {
    use objc::{msg_send, sel, sel_impl};
    let Ok(ns_window_ptr) = window.ns_window() else {
        eprintln!("[passthrough] ns_window() failed");
        return;
    };
    unsafe {
        let ns_window: cocoa::base::id = ns_window_ptr as cocoa::base::id;
        if ns_window.is_null() {
            eprintln!("[passthrough] ns_window is null");
            return;
        }
        let val: cocoa::base::BOOL = if ignore { cocoa::base::YES } else { cocoa::base::NO };
        let _: () = msg_send![ns_window, setIgnoresMouseEvents: val];
        println!("[passthrough] native NSWindow setIgnoresMouseEvents:{ignore} sent");
    }
}

/// NSEvent.mouseLocation → 转为 top-left 逻辑像素坐标。
#[cfg(target_os = "macos")]
fn global_cursor_top_left() -> Option<(f64, f64)> {
    use cocoa::base::{id, nil};
    use cocoa::foundation::NSPoint;
    use objc::{class, msg_send, sel, sel_impl};
    unsafe {
        let pool: id = msg_send![class!(NSAutoreleasePool), new];
        let result = (|| {
            let cls: id = msg_send![class!(NSEvent), class];
            let p: NSPoint = msg_send![cls, mouseLocation];
            let screen: id = msg_send![class!(NSScreen), mainScreen];
            if screen == nil { return None; }
            let frame: cocoa::foundation::NSRect = msg_send![screen, frame];
            Some((p.x, frame.size.height - p.y))
        })();
        if pool != nil { let _: () = msg_send![pool, drain]; }
        result
    }
}

#[cfg(not(target_os = "macos"))]
fn global_cursor_top_left() -> Option<(f64, f64)> { None }
