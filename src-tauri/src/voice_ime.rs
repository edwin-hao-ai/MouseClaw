//! Voice IME · fn 长按触发的语音输入法（v0.1.11）
//!
//! 用户的 daily-use 高频功能：按住 fn 说话 → 松开 → 文字直接写到光标。
//! 不调 AI，纯 Whisper 转写（+ light_clean）→ mode_b paste。
//!
//! ## 为什么不能用 tauri-plugin-global-shortcut
//! fn 是 macOS 特殊键（NSEventModifierFlagFunction = 1 << 23），不是普通的
//! modifier。`global-shortcut` 插件只认 ⌘ ⌥ ⌃ ⇧ 这种「正经」 modifier。
//! 唯一办法：自己写 CGEventTap，订阅 kCGEventFlagsChanged。
//!
//! ## 长按 vs 误触
//! 短点 fn（< 300ms）= 误触，不触发（让用户偶尔单按 fn 切语言切到原生 macOS 行为）。
//! 按住 fn ≥ 300ms = "我要说话"，触发 listening。
//! 松开 = stop + transcribe + paste。
//!
//! ## 后台线程
//! CGEventTap 必须挂在某个 CFRunLoop 上。我们起一个独立 std::thread，跑一个
//! 永久 CFRunLoop。Tap 在那个 loop 里 dispatch 事件，回调里把状态发到 tokio
//! 的 mpsc，主 runtime 收到再 emit + 跑 pipeline。

#![cfg(target_os = "macos")]

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::time::{Duration, Instant};
use tauri::AppHandle;

use crate::AppState;

/// NSEventModifierFlagFunction —— fn 键的位
const FN_FLAG: u64 = 1 << 23;
/// 短按阈值 —— < 300ms 当误触不响应
const LONG_PRESS_MS: u128 = 300;

/// 触发 voice IME 的 fn 状态机
struct FnMonitor {
    pressed_at: AtomicI64, // micros since UNIX_EPOCH, 0 = not pressed
    triggered: AtomicBool, // 长按已触发 = 等松开做 stop
    enabled: AtomicBool,   // 用户在托盘可关
}

static FN_MONITOR: once_cell::sync::Lazy<FnMonitor> = once_cell::sync::Lazy::new(|| FnMonitor {
    pressed_at: AtomicI64::new(0),
    triggered: AtomicBool::new(false),
    enabled: AtomicBool::new(true),
});

pub fn set_enabled(on: bool) {
    FN_MONITOR.enabled.store(on, Ordering::Relaxed);
    println!("[mouseclaw] 🎙️ voice-ime enabled = {on}");
}

pub fn is_enabled() -> bool { FN_MONITOR.enabled.load(Ordering::Relaxed) }

/// 主入口 —— lib.rs setup 时调一次。起后台线程跑 CGEventTap。
pub fn spawn(app: AppHandle, state: Arc<AppState>) {
    // 从 config 读初始 enabled
    FN_MONITOR.enabled.store(
        crate::config::Config::load().voice_ime_enabled,
        Ordering::Relaxed,
    );

    std::thread::Builder::new()
        .name("mouseclaw-fn-tap".into())
        .spawn(move || run_event_tap(app, state))
        .expect("spawn fn tap thread");
}

// ────────────────── CGEventTap 部分 ──────────────────

#[repr(transparent)]
#[derive(Copy, Clone)]
struct CGEventRef(*mut std::ffi::c_void);
unsafe impl Send for CGEventRef {}

type CGEventTapCallBack = unsafe extern "C" fn(
    proxy: *mut std::ffi::c_void,
    event_type: u32,
    event: *mut std::ffi::c_void,
    user_info: *mut std::ffi::c_void,
) -> *mut std::ffi::c_void;

#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    fn CGEventTapCreate(
        tap: u32,             // kCGSessionEventTap = 1
        place: u32,           // kCGHeadInsertEventTap = 0
        options: u32,         // kCGEventTapOptionListenOnly = 1
        events_of_interest: u64,
        callback: CGEventTapCallBack,
        user_info: *mut std::ffi::c_void,
    ) -> *mut std::ffi::c_void; // CFMachPortRef

    fn CGEventGetFlags(event: *mut std::ffi::c_void) -> u64;
}

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    fn CFMachPortCreateRunLoopSource(
        allocator: *mut std::ffi::c_void,
        port: *mut std::ffi::c_void,
        order: isize,
    ) -> *mut std::ffi::c_void;

    fn CFRunLoopGetCurrent() -> *mut std::ffi::c_void;
    fn CFRunLoopAddSource(
        rl: *mut std::ffi::c_void,
        source: *mut std::ffi::c_void,
        mode: *const std::ffi::c_void,
    );
    fn CFRunLoopRun();

    static kCFRunLoopCommonModes: *const std::ffi::c_void;
}

/// Tap 接收回调（C ABI）—— 仅做最少工作：解析 flags + 转 trigger 给主线程
unsafe extern "C" fn tap_callback(
    _proxy: *mut std::ffi::c_void,
    event_type: u32,
    event: *mut std::ffi::c_void,
    user_info: *mut std::ffi::c_void,
) -> *mut std::ffi::c_void {
    // kCGEventFlagsChanged = 12
    if event_type != 12 { return event; }
    if !FN_MONITOR.enabled.load(Ordering::Relaxed) { return event; }

    let flags = CGEventGetFlags(event);
    let fn_down = (flags & FN_FLAG) != 0;

    // 状态切换：把事件发到主线程处理
    let prev_pressed = FN_MONITOR.pressed_at.load(Ordering::Relaxed) != 0;
    if fn_down && !prev_pressed {
        // fn 刚被按下 —— 记录时间，启动 300ms 后的 long-press 检查
        let now_us = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_micros() as i64).unwrap_or(0);
        FN_MONITOR.pressed_at.store(now_us, Ordering::Relaxed);

        // 通过指针拿回 sender —— user_info 是 *const TapState
        let st = &*(user_info as *const TapState);
        let _ = st.tx.send(FnEvent::PressedDown);
    } else if !fn_down && prev_pressed {
        // fn 刚被松开 —— 检查是否曾经触发 long-press
        FN_MONITOR.pressed_at.store(0, Ordering::Relaxed);
        let st = &*(user_info as *const TapState);
        let was_triggered = FN_MONITOR.triggered.swap(false, Ordering::Relaxed);
        if was_triggered {
            let _ = st.tx.send(FnEvent::ReleasedAfterLong);
        } else {
            let _ = st.tx.send(FnEvent::ReleasedShortTap);
        }
    }
    event
}

#[derive(Debug)]
enum FnEvent {
    PressedDown,
    ReleasedAfterLong,
    #[allow(dead_code)]
    ReleasedShortTap,
}

struct TapState {
    tx: std::sync::mpsc::Sender<FnEvent>,
}

fn run_event_tap(app: AppHandle, state: Arc<AppState>) {
    let (tx, rx) = std::sync::mpsc::channel::<FnEvent>();

    // 起一个独立 thread 跑 main runtime 端处理（pressed → 等 300ms → trigger；released → stop+pipeline）
    let app2 = app.clone();
    let state2 = state.clone();
    std::thread::spawn(move || handle_fn_events(rx, app2, state2));

    let tap_state = Box::into_raw(Box::new(TapState { tx }));

    unsafe {
        // CGEventMaskBit(kCGEventFlagsChanged=12) = 1 << 12 = 4096
        let mask: u64 = 1 << 12;
        let port = CGEventTapCreate(
            1, // kCGSessionEventTap
            0, // kCGHeadInsertEventTap
            1, // kCGEventTapOptionListenOnly —— 只听不改
            mask,
            tap_callback,
            tap_state as *mut _,
        );
        if port.is_null() {
            eprintln!("[mouseclaw] ✘ CGEventTapCreate 失败 —— 辅助功能权限缺失？");
            return;
        }
        let source = CFMachPortCreateRunLoopSource(std::ptr::null_mut(), port, 0);
        if source.is_null() {
            eprintln!("[mouseclaw] ✘ CFMachPortCreateRunLoopSource failed");
            return;
        }
        let runloop = CFRunLoopGetCurrent();
        CFRunLoopAddSource(runloop, source, kCFRunLoopCommonModes);
        println!("[mouseclaw] 🎙️ voice-ime fn tap installed (CGEventTap on FlagsChanged)");
        CFRunLoopRun(); // 永久 block 本线程
    }
}

fn handle_fn_events(rx: std::sync::mpsc::Receiver<FnEvent>, app: AppHandle, state: Arc<AppState>) {
    while let Ok(ev) = rx.recv() {
        match ev {
            FnEvent::PressedDown => {
                // 起一个 300ms 的小定时器，到时如果 fn 还在按 → 真触发
                let app2 = app.clone();
                let state2 = state.clone();
                std::thread::spawn(move || {
                    let started = Instant::now();
                    std::thread::sleep(Duration::from_millis(LONG_PRESS_MS as u64));
                    // 仍在按吗？
                    let still_down = FN_MONITOR.pressed_at.load(Ordering::Relaxed) != 0;
                    if !still_down { return; } // 短按，被忽略
                    let pressed_us = FN_MONITOR.pressed_at.load(Ordering::Relaxed);
                    if pressed_us == 0 { return; }
                    let pressed_at = Instant::now() - started.elapsed(); // ~ press time
                    let _ = pressed_at;
                    // 防双触发
                    if FN_MONITOR.triggered.swap(true, Ordering::Relaxed) {
                        return;
                    }
                    println!("[mouseclaw] 🎙️ fn long-press → start voice IME recording");
                    start_recording_for_ime(app2, state2);
                });
            }
            FnEvent::ReleasedAfterLong => {
                println!("[mouseclaw] 🎙️ fn released → stop & transcribe & paste");
                stop_and_paste(app.clone(), state.clone());
            }
            FnEvent::ReleasedShortTap => {
                // 短按 —— 啥都不做，让用户偶尔单按 fn 是 macOS 原生行为（切语言/dictation 等）
            }
        }
    }
}

// ────────────────── 录音 → Whisper → paste 流程 ──────────────────

fn start_recording_for_ime(app: AppHandle, state: Arc<AppState>) {
    // 已经在录音（被 ⌘⇧Space 占着）→ 跳过
    if state.recorder.lock().unwrap().is_some() {
        println!("[mouseclaw] 🎙️ recorder busy (occupied by AI summon?), skip voice IME");
        FN_MONITOR.triggered.store(false, Ordering::Relaxed);
        return;
    }
    // 检查 Whisper 是否就绪
    if !crate::transcribe::is_available() {
        eprintln!("[mouseclaw] 🎙️ Whisper not ready, skip voice IME");
        FN_MONITOR.triggered.store(false, Ordering::Relaxed);
        return;
    }
    let recorder = match crate::audio::Recorder::start() {
        Ok(r) => r,
        Err(e) => {
            eprintln!("[mouseclaw] 🎙️ start recording: {e:#}");
            FN_MONITOR.triggered.store(false, Ordering::Relaxed);
            return;
        }
    };
    *state.recorder.lock().unwrap() = Some(recorder);
    // 召唤桌宠到光标 + listening 视图
    crate::overlay::show_mouse(&app);
    crate::overlay::emit_view(&app, &crate::events::ViewKind::Listening);
}

fn stop_and_paste(app: AppHandle, state: Arc<AppState>) {
    let recorder = state.recorder.lock().unwrap().take();
    let Some(recorder) = recorder else { return; };

    crate::overlay::emit_view(&app, &crate::events::ViewKind::Thinking {
        transcript: "(语音输入中…)".into(),
    });

    tauri::async_runtime::spawn_blocking(move || {
        let samples = match recorder.stop_and_take() {
            Ok(s) => s,
            Err(e) => {
                eprintln!("[mouseclaw] 🎙️ stop_and_take: {e}");
                crate::overlay::hide_overlay(&app);
                return;
            }
        };
        let raw = match crate::transcribe::transcribe(&samples) {
            Ok(t) => t,
            Err(e) => {
                eprintln!("[mouseclaw] 🎙️ Whisper 失败: {e}");
                crate::overlay::emit_view(&app, &crate::events::ViewKind::Blocked {
                    reason: format!("Whisper failed: {e}"),
                });
                return;
            }
        };
        let cfg = crate::config::Config::load();
        let cleaned = crate::tidy_up::light_clean(&raw, &cfg.language);
        if cleaned.trim().is_empty() {
            println!("[mouseclaw] 🎙️ 空输出，无声触发？hide");
            crate::overlay::hide_overlay(&app);
            return;
        }
        println!("[mouseclaw] 🎙️ paste → {cleaned:?}");

        // 切回 async 跑 write_at_cursor（它内部会 spawn_blocking）
        let app2 = app.clone();
        let cleaned_for_async = cleaned.clone();
        tauri::async_runtime::spawn(async move {
            match crate::mode_b::write_at_cursor(&cleaned_for_async).await {
                Ok(()) => {
                    crate::overlay::emit_view(&app2, &crate::events::ViewKind::Reply {
                        transcript: "voice IME".into(),
                        reply: format!("✍️ {cleaned}"),
                        mode: crate::events::ReplyMode::A,
                        insert_text: None,
                        streaming: false,
                    });
                    // 1.5s 后隐藏
                    tokio::time::sleep(Duration::from_millis(1500)).await;
                    crate::overlay::hide_overlay(&app2);
                }
                Err(e) => {
                    eprintln!("[mouseclaw] 🎙️ write_at_cursor: {e}");
                    crate::overlay::emit_view(&app2, &crate::events::ViewKind::Blocked {
                        reason: format!("写入失败：{e}"),
                    });
                }
            }
        });
    });
}
