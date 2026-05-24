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
            let win_w = ww / scale;
            let win_h = wh / scale;
            // 当前窗口左上角（逻辑像素）
            let (cur_x, cur_y) = match window.outer_position() {
                Ok(p) => (p.x as f64 / scale, p.y as f64 / scale),
                Err(_) => (f64::NAN, f64::NAN),
            };
            // v0.5.x · 冻结跟随 —— 鼠标移进 overlay 的**上半区**（气泡 / 「⌨️打字」按钮所在）
            //   就停止跟随，让用户能点到按钮。鼠标在下半区（桌宠附近，跟随常态位置）照常跟。
            //   解决「按钮跟着鼠标跑点不中」：你抬手去够按钮的那一刻，老鼠就定住了。
            if cur_x.is_finite()
                && x >= cur_x && x <= cur_x + win_w
                && y >= cur_y && y <= cur_y + win_h * 0.55
            {
                return;
            }
            let raw_x = x - win_w / 2.0;
            let raw_y = y - win_h + 32.0;
            // v0.4+ · clamp 让桌宠本体不跑出屏幕 + 撞边回弹（窗口透明边距可溢出，桌宠不被切）。
            let (pos_x, pos_y, bonk) =
                crate::overlay_size::clamp_follow_with_bonk(raw_x, raw_y, win_w, win_h);
            // v0.5.x · 缓动跟随（慢半拍）：每帧只挪向目标一部分，鼠标先到、老鼠过一会才追上。
            //   ALPHA 越小越懒；0.08 @ 30fps ≈ 1s 才追上。
            const ALPHA: f64 = 0.08;
            let (sx, sy) = if cur_x.is_finite() { (cur_x, cur_y) } else { (pos_x, pos_y) };
            let eased_x = sx + (pos_x - sx) * ALPHA;
            let eased_y = sy + (pos_y - sy) * ALPHA;
            let _ = window.set_position(LogicalPosition::new(eased_x, eased_y));
            // 接近目标边缘才回弹（缓动下很少真贴边，避免每帧虚假 bonk）。
            if bonk.is_some() && (pos_x - eased_x).abs() < 1.0 && (pos_y - eased_y).abs() < 1.0 {
                if let Some(dir) = bonk {
                    crate::overlay_size::emit_bonk_edge(&app2, dir);
                }
            }
        }
    });
}

/// Show the overlay window near the cursor.
///
/// ⚠️ **必须在主线程跑** —— 里面碰 NSEvent / NSScreen，AppKit 不是线程安全的。
/// 之前 show_mouse 从全局快捷键 handler 线程直接调，NSScreen.mainScreen 离开
/// 主线程访问会静默崩溃。现在统一 marshal 到主线程。
pub fn show_mouse(app: &AppHandle) {
    use tauri::LogicalSize;
    // ⚠️ 必须在排队主线程闭包**之前**同步 mark_expanded（不能放进闭包里）：
    // voice IME 路径在 worker 线程跑 —— show_mouse 排完闭包后，调用方紧接着同步调
    // emit_view → expand_to_full → set_mode，此刻若 CURRENT_MODE 还是 compact(0)，
    // set_mode 不会 no-op，会再排一个"保持锚点"闭包；该闭包读窗口位置时（冷启动首帧
    // set_position 尚未提交）拿到旧 anchor 坐标 → 把桌宠拽回角落（用户报：第一次按 fn
    // 桌宠不移到光标，第二次才移）。提前同步置 expanded，让 expand_to_full 真 no-op。
    crate::overlay_size::mark_expanded();
    // v0.5 · 用户主动召唤 = 打断进行中的开场入场动画（硬规则：任意阶段可打断）。
    if let Some(state) = app.try_state::<Arc<AppState>>() {
        crate::entrance::abort(state.inner());
    }
    let app2 = app.clone();
    let _ = app.run_on_main_thread(move || {
        let Some(window) = app2.get_webview_window("mouse") else { return };
        // 先 expand 到 320，再用 320 尺寸算光标偏移，最后 set_position。
        let _ = window.set_size(LogicalSize::new(
            crate::overlay_size::EXPANDED_SIZE,
            crate::overlay_size::EXPANDED_SIZE,
        ));
        if let Some((x, y)) = current_mouse_pos_top_left(&window) {
            let sz = crate::overlay_size::EXPANDED_SIZE;
            let half = sz / 2.0;
            let raw_x = x - half;
            // 桌宠在窗口底部中心，下沿距光标 32px
            let raw_y = y - sz + 32.0;
            // v0.5.x · bug1 修：**整窗**钳制在屏幕可见区内（不是只保桌宠 —— 那会让窗口顶部
            //   的气泡溢出屏幕外被切）。光标偏上时 raw_y 会变负 → 窗口顶出屏幕 → 头顶
            //   "正在说话"气泡看不见（"第一次能看到、再召唤位置偏上就看不到"的真因）。
            let (pos_x, pos_y) = match crate::overlay_size::visible_frame_top_left() {
                Some((sx, sy, sw, sh)) => (
                    raw_x.clamp(sx, (sx + sw - sz).max(sx)),
                    raw_y.clamp(sy, (sy + sh - sz).max(sy)),
                ),
                None => (raw_x, raw_y),
            };
            let _ = window.set_position(LogicalPosition::new(pos_x, pos_y));
        }
        let _ = window.show();
        // macOS：NSPanel 自带 always-on-top（level=ScreenSaver），**不**调 set_always_on_top
        // （会把 level 压回 floating 又浮不上全屏）。非 macOS 在此 helper 里 set_always_on_top。
        apply_overlay_window_behavior(&window);
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
    // v0.5 · nudge 弹气泡也算"有事发生" → 打断进行中的入场动画。
    if let Some(state) = app.try_state::<Arc<AppState>>() {
        crate::entrance::abort(state.inner());
    }
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
            apply_overlay_window_behavior(&w);
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
    // Win/X11 经平台层；Wayland 返回 None → 跟随静默 no-op（§4：Wayland 不跟随）。
    crate::platform::global_cursor()
}

/// 召唤 / 显示桌宠后保持它在最上层。
///
/// macOS：overlay 已在 setup 里转成 NSPanel（见 `mouse_panel`），level=ScreenSaver +
/// collectionBehavior 让它常驻并浮在全屏之上，**且 sticky**。这里**故意不**调
/// `set_always_on_top(true)` —— 那会把 NSPanel 的 level 压回 floating(4)，又浮不上全屏
/// （这正是 v0.4.x 几次没修好的根因）。所以 macOS 上是 no-op。
///
/// 非 macOS（Windows V2 占位）：还没有 NSPanel 等价物，退回 set_always_on_top。
#[cfg(target_os = "macos")]
pub fn apply_overlay_window_behavior(_window: &WebviewWindow) {}

#[cfg(not(target_os = "macos"))]
pub fn apply_overlay_window_behavior(window: &WebviewWindow) {
    let _ = window.set_always_on_top(true);
}

/// v0.5.x · 让桌宠 overlay 成为 / 退出 **key window**（接收物理键盘）。
///
/// ⚠️ **必须**走 nspanel 的 `make_key_window` / `resign_key_window`，**不能**用 tao 的
/// `window.set_focusable()` —— overlay 是自定义 NSPanel 子类（`MousePanel`，见 mouse_panel.rs），
/// `set_focusable()` 通过 KVO 找 `focusable` ivar，该类没有 → objc 抛异常 → Rust 无法 catch
/// foreign exception → **整个 app abort**（2026-05-24 真机 crash 根因）。
///
/// `MousePanel` 是 **nonactivating** panel：成为 key window 只"借"键盘焦点，**不改变系统
/// frontmost app**（`NSWorkspace.frontmostApplication` 仍是用户原来的 app）。所以让 listening
/// 获焦接收"敲键即切文字"的按键，**不污染** Mode B 的 `prev_frontmost_pid`、不破坏续写光标。
///
/// marshal 到主线程 —— `makeKeyWindow` 碰 AppKit，非主线程会崩。
#[cfg(target_os = "macos")]
pub fn set_overlay_key_window(app: &AppHandle, key: bool) {
    use tauri_nspanel::ManagerExt;
    let app2 = app.clone();
    let _ = app.run_on_main_thread(move || {
        match app2.get_webview_panel("mouse") {
            Ok(panel) => {
                if key { panel.make_key_window(); } else { panel.resign_key_window(); }
            }
            Err(e) => eprintln!("[mouseclaw] set_overlay_key_window: get_webview_panel 失败 ({e:?})"),
        }
    });
}
#[cfg(not(target_os = "macos"))]
pub fn set_overlay_key_window(_app: &AppHandle, _key: bool) {}

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
        ViewKind::TextInput { initial } => {
            format!("text-input({} chars)", initial.chars().count())
        }
        ViewKind::FeedWaiting => "feed-waiting".to_string(),
        ViewKind::FeedListening { files, partial } => {
            format!("feed-listening({} files, {}…)",
                files.len(),
                partial.chars().take(20).collect::<String>())
        }
        ViewKind::Thinking { transcript, .. } => {
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
        ViewKind::ScheduleConfirm { title, .. } => format!("schedule-confirm({title})"),
        ViewKind::Blocked { reason } => format!("blocked({reason})"),
    };
    // v0.1.8 cursor-follow gating —— 只在 AI 召唤 listening 时跟随鼠标。
    // v0.4.0 · 语音输入法 (VoiceImeListening) 不再跟随 —— 用户反馈：长按 fn 说话时，
    //   光标在目标输入框附近移动选词，桌宠跟着乱飞挡视线，体验差。
    //   AI 召唤本身鼠标是"画圈圈定"动作，跟随有意义；语音输入鼠标是"选输入框位置"，跟随无意义。
    if let Some(state) = app.try_state::<Arc<AppState>>() {
        // v0.5.x · 只有 summon_follow=true 的 listening 才跟随（hold/托盘）。PetMenu 召唤
        //   summon_follow=false → 原地不跟随，气泡稳定（头顶文字看得见、「⌨️打字」点得中）。
        let should_follow = matches!(view, ViewKind::Listening { .. })
            && state.summon_follow.load(Ordering::Relaxed);
        if should_follow {
            crate::cursor_follow::enable(state.inner());
        } else {
            crate::cursor_follow::disable(state.inner());
        }
        // v0.3.12 · 同步 overlay_has_ui —— idle 表示静默（仅桌宠睡觉，无气泡）。
        // 其它 view kind 都有可见 UI（气泡 / 菜单 / 倒数 / 引导）→ 整个窗口接收点击
        let has_ui = !matches!(view, ViewKind::Idle);
        state.overlay_has_ui.store(has_ui, std::sync::atomic::Ordering::Relaxed);
        // v0.3.12 fix3 · 同步窗口物理尺寸 —— idle 静默时缩到 100×100 不再占整个 320×320
        // 给周围 app 让位；有 UI 时扩到 320×320 让气泡有空间。桌宠视觉位置保持不变。
        if has_ui {
            crate::overlay_size::expand_to_full(app);
        } else {
            crate::overlay_size::shrink_to_compact(app);
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
    // v0.5.x · 关掉 toggle 召唤可能开过的键盘焦点 —— 回 idle 不需要（没开过 = no-op）。
    set_overlay_key_window(app, false);
    let anchor = crate::config::Config::load().pet_anchor;
    // v0.4 fix (2026-05-20): 顺序很关键 ——
    // 1. 先 emit_view(Idle) 触发 shrink_to_compact (320→80)，窗口尺寸先正确
    // 2. 再 apply_idle_anchor 读 80 尺寸计算右下角位置，准确停到角落
    // 之前顺序反了：apply_idle_anchor 用 320 算出来的"右下角"实际离边 320+24px，
    // 再 shrink 保持 pet 视觉位置 → pet 偏离真正右下角 ~120px。用户反馈
    // 「输入完之后桌宠不会再回去右下角」就是这个 bug。
    emit_view(app, &ViewKind::Idle);
    if anchor.pin_visible_when_idle() {
        crate::anchor::apply_idle_anchor(app, anchor);
    } else {
        // Follow / Hidden 模式：彻底隐藏（旧行为）
        let app2 = app.clone();
        let _ = app.run_on_main_thread(move || {
            if let Some(w) = app2.get_webview_window("mouse") {
                let _ = w.hide();
            }
        });
    }
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
