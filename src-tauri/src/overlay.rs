//! 老鼠 overlay 窗口的显示/隐藏 + 视图事件广播 + 自动隐藏调度。
//!
//! ⚠️ AppKit 线程安全：show_mouse / hide_overlay 里碰 NSEvent / NSScreen，
//! 必须 marshal 到主线程跑 —— 否则静默崩溃。

use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;
use tauri::{AppHandle, Emitter, LogicalPosition, Manager, WebviewWindow};

use crate::events::{ViewKind, EV_VIEW_CHANGED};
use crate::AppState;

/// 重新定位 overlay 到光标位置 —— cursor-follow 后台任务用。
/// 复用 show_mouse 的偏移算法（窗口下沿距离光标 32px）。
/// ⚠️ 必须在主线程 —— NSEvent.mouseLocation / NSScreen.mainScreen 都不是线程安全的。
pub fn reposition_to_cursor(app: &AppHandle) {
    let app2 = app.clone();
    let _ = app.run_on_main_thread(move || {
        let Some(window) = app2.get_webview_window("mouse") else { return };
        if let Some((x, y)) = current_mouse_pos_top_left(&window) {
            let (ww, wh) = match window.outer_size().ok() {
                Some(s) => (s.width as f64, s.height as f64),
                None => (320.0, 320.0),
            };
            let scale = window.scale_factor().unwrap_or(1.0);
            let pos_x = x - (ww / scale) / 2.0;
            let pos_y = y - (wh / scale) + 32.0;
            let _ = window.set_position(LogicalPosition::new(pos_x, pos_y));
        }
    });
}

/// Show the overlay window near the cursor.
///
/// ⚠️ **必须在主线程跑** —— 里面碰 NSEvent / NSScreen，AppKit 不是线程安全的。
/// 之前 show_mouse 从全局快捷键 handler 线程直接调，NSScreen.mainScreen 离开
/// 主线程访问会静默崩溃。现在统一 marshal 到主线程。
pub fn show_mouse(app: &AppHandle) {
    let app2 = app.clone();
    let _ = app.run_on_main_thread(move || {
        let Some(window) = app2.get_webview_window("mouse") else { return };
        // 现在在主线程 —— cocoa 调用安全
        if let Some((x, y)) = current_mouse_pos_top_left(&window) {
            let (ww, wh) = match window.outer_size().ok() {
                Some(s) => (s.width as f64, s.height as f64),
                None => (320.0, 320.0),
            };
            let scale = window.scale_factor().unwrap_or(1.0);
            let pos_x = x - (ww / scale) / 2.0;
            let pos_y = y - (wh / scale) + 32.0;
            let _ = window.set_position(LogicalPosition::new(pos_x, pos_y));
        }
        let _ = window.show();
        let _ = window.set_always_on_top(true);
    });
    // v0.1.8 召唤瞬间起就跟着鼠标走，直到出现气泡才停住
    if let Some(state) = app.try_state::<Arc<AppState>>() {
        crate::cursor_follow::enable(state.inner());
    }
}

/// v0.1.32 · 给 Nudge / 主动提醒用的 "show"。
///
/// 跟 `show_mouse` 的区别：
///   - **不**把窗口搬到鼠标位置（show_mouse 会，导致 bubble 出现位置乱跳 + 被屏幕边切半）
///   - **不**启用 cursor_follow（nudge 不该跟着鼠标走）
///   - 把 overlay 放到用户的 anchor 位置（4 角之一 / Hidden 时也放 BR 兜底）
pub fn show_mouse_at_anchor(app: &AppHandle) {
    let anchor = crate::config::Config::load().pet_anchor;
    // Hidden / Follow 也强行放右下兜底 —— 让 nudge bubble 有可见位置可挂
    let effective = if anchor.pin_visible_when_idle() {
        anchor
    } else {
        crate::config::PetAnchor::BottomRight
    };
    crate::anchor::apply_idle_anchor(app, effective);
    let app2 = app.clone();
    let _ = app.run_on_main_thread(move || {
        if let Some(w) = app2.get_webview_window("mouse") {
            let _ = w.show();
            let _ = w.set_always_on_top(true);
        }
    });
}

/// v0.4 · Pub wrapper for callers outside this module (feed_flow::run-to-cursor).
/// Marshals to main thread; returns None if call site can't await main thread reply.
/// Use only from main-thread closures or via `app.run_on_main_thread`.
#[cfg(target_os = "macos")]
pub fn cursor_screen_pos_unchecked(window: &WebviewWindow) -> Option<(f64, f64)> {
    current_mouse_pos_top_left(window)
}
#[cfg(not(target_os = "macos"))]
pub fn cursor_screen_pos_unchecked(_w: &WebviewWindow) -> Option<(f64, f64)> { None }

/// Get cursor position in top-left-origin screen coordinates (global).
/// ⚠️ 只能在主线程调用（碰 NSScreen）。
///
/// v0.3.8 · 多显示器修复：原来用 `NSScreen.mainScreen`（key window 所在屏，不一定是
/// 带菜单栏的主屏），副屏激活时拿错 height → y 翻转偏 100-200px。
/// 现在固定用 `NSScreen.screens[0]`（永远是主屏 / 带菜单栏 / 全局坐标原点所在屏），
/// 跟 macOS 全局坐标系一致。
#[cfg(target_os = "macos")]
fn current_mouse_pos_top_left(window: &WebviewWindow) -> Option<(f64, f64)> {
    use cocoa::base::id;
    use cocoa::foundation::NSPoint;
    use objc::{class, msg_send, sel, sel_impl};
    unsafe {
        let event_class: id = msg_send![class!(NSEvent), class];
        let point: NSPoint = msg_send![event_class, mouseLocation];
        let screen_h = primary_screen_height_pts().unwrap_or(1080.0);
        let _scale = window.scale_factor().unwrap_or(1.0);
        Some((point.x, screen_h - point.y))
    }
}

/// 主屏（screens[0]）高度 —— macOS 全局坐标系的 y 翻转基准。
/// ⚠️ 只能在主线程调用（NSScreen.screens 不是线程安全的）。
#[cfg(target_os = "macos")]
fn primary_screen_height_pts() -> Option<f64> {
    use cocoa::base::id;
    use cocoa::foundation::{NSArray, NSRect, NSSize};
    use objc::{class, msg_send, sel, sel_impl};
    unsafe {
        // NSScreen.screens 返回所有屏数组；[0] = 主屏（菜单栏所在）
        let screens: id = msg_send![class!(NSScreen), screens];
        if screens as usize == 0 {
            return None;
        }
        let count: usize = msg_send![screens, count];
        if count == 0 {
            return None;
        }
        let primary: id = NSArray::objectAtIndex(screens, 0);
        if primary as usize == 0 {
            return None;
        }
        let frame: NSRect = msg_send![primary, frame];
        let size: NSSize = frame.size;
        Some(size.height)
    }
}

#[cfg(not(target_os = "macos"))]
fn current_mouse_pos_top_left(_w: &WebviewWindow) -> Option<(f64, f64)> {
    None
}

/// 把一个 ViewKind 广播给所有 webview 窗口（前端的状态机靠它驱动）。
/// 全链路日志 —— 每个 emit 都打出 kind，配合 panic hook 能定位"气泡不显示"问题。
pub fn emit_view(app: &AppHandle, view: &ViewKind) {
    let kind = match view {
        ViewKind::Idle => "idle".to_string(),
        ViewKind::Onboarding => "onboarding".to_string(),
        ViewKind::Listening { partial } => {
            if partial.is_empty() {
                "listening".to_string()
            } else {
                format!("listening({}…)", partial.chars().take(20).collect::<String>())
            }
        }
        ViewKind::VoiceImeListening { partial } => {
            if partial.is_empty() {
                "voice-ime-listening".to_string()
            } else {
                format!("voice-ime-listening({}…)", partial.chars().take(20).collect::<String>())
            }
        }
        ViewKind::FeedWaiting => "feed-waiting".to_string(),
        ViewKind::FeedListening { files, partial } => {
            format!("feed-listening({} files, {}…)",
                files.len(),
                partial.chars().take(20).collect::<String>())
        }
        ViewKind::Thinking { transcript } => {
            format!("thinking({})", transcript.chars().take(20).collect::<String>())
        }
        ViewKind::Reply { reply, streaming, .. } => {
            format!("reply(streaming={streaming}, {} chars)", reply.chars().count())
        }
        ViewKind::Panel { .. } => "panel".to_string(),
        ViewKind::ModeBCountdown { remaining, .. } => format!("mode-b-countdown({remaining})"),
        ViewKind::ModeBInserting { .. } => "mode-b-inserting".to_string(),
        ViewKind::VoiceConfirm { remaining, transcript } =>
            format!("voice-confirm({remaining}, {} chars)", transcript.chars().count()),
        ViewKind::TourStep { step } => format!("tour-step({step})"),
        ViewKind::Blocked { reason } => format!("blocked({reason})"),
    };
    // v0.1.8 cursor-follow gating —— 只在 AI 召唤 listening 时跟随鼠标。
    // v0.4.0 · 语音输入法 (VoiceImeListening) 不再跟随 —— 用户反馈：长按 fn 说话时，
    //   光标在目标输入框附近移动选词，桌宠跟着乱飞挡视线，体验差。
    //   AI 召唤本身鼠标是"画圈圈定"动作，跟随有意义；语音输入鼠标是"选输入框位置"，跟随无意义。
    if let Some(state) = app.try_state::<Arc<AppState>>() {
        let should_follow = matches!(view, ViewKind::Listening { .. });
        if should_follow {
            crate::cursor_follow::enable(state.inner());
        } else {
            crate::cursor_follow::disable(state.inner());
        }
    }
    match app.emit(EV_VIEW_CHANGED, view) {
        Ok(()) => println!("[mouseclaw] emit_view → {kind}"),
        Err(e) => eprintln!("[mouseclaw] ✘ emit_view 失败 ({kind}): {e}"),
    }
}

/// Hide the overlay window, emit Idle. Idempotent.
///
/// v0.1.27 · 如果 config.pet_anchor 是 4 个角之一，**不真隐藏**，
/// 而是把窗口送回那个角落打盹（pinned visible）。
/// 只有 Follow 模式才彻底 hide —— Follow 没有"家"，闲置就该消失。
///
/// ⚠️ window.hide() / set_position marshal 到主线程 —— 同 show_mouse，
/// 避免 AppKit 跨线程崩溃。
pub fn hide_overlay(app: &AppHandle) {
    let anchor = crate::config::Config::load().pet_anchor;
    if anchor.pin_visible_when_idle() {
        // 送回 anchor 打盹 —— overlay 保持可见但落到角落
        crate::anchor::apply_idle_anchor(app, anchor);
    } else {
        // Follow 模式：彻底隐藏（旧行为）
        let app2 = app.clone();
        let _ = app.run_on_main_thread(move || {
            if let Some(w) = app2.get_webview_window("mouse") {
                let _ = w.hide();
            }
        });
    }
    emit_view(app, &ViewKind::Idle); // emit 是发事件，跨线程安全
}

/// Schedule `hide_overlay` after `after_ms` ms — but only fire if the app's
/// generation hasn't changed (i.e., no new pipeline/shortcut activity).
pub fn schedule_auto_hide(app: &AppHandle, state: &Arc<AppState>, after_ms: u64) {
    let my_gen = state.gen.load(Ordering::SeqCst);
    let app_clone = app.clone();
    let state_clone = state.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_millis(after_ms)).await;
        if state_clone.gen.load(Ordering::SeqCst) == my_gen {
            println!("[mouseclaw] auto-hide (gen {my_gen} still current after {after_ms}ms)");
            hide_overlay(&app_clone);
        } else {
            println!("[mouseclaw] auto-hide skipped (gen advanced, user did something)");
        }
    });
}

/// Bump the generation — invalidates any pending auto-hide timer.
pub fn bump_gen(state: &Arc<AppState>) -> u64 {
    state.gen.fetch_add(1, Ordering::SeqCst) + 1
}
