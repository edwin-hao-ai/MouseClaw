//! Voice IME · fn 长按触发的语音输入法（v0.1.11）
//!
//! 用户的 daily-use 高频功能：按住 fn 说话 → 松开 → 文字直接写到光标。
//! 不调 AI，纯 sherpa-onnx 流式转写（+ light_clean）→ mode_b paste。
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

//! 跨平台说明（v0.5 跨平台移植）：本模块的**配置层**（`ImeTrigger` 枚举、`set_enabled`/
//! `set_trigger`、`MONITOR` 原子缓存）在所有平台编译，供托盘 / commands 调用。**事件监听
//! 实现**（CGEventTap）目前仅 macOS；Win/Linux 的全局热键触发待 §4 UX 决策后接入
//! （见 docs/design/cross-platform-port-20260522.md）。`spawn` 仅在 macOS 调用。

use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU64, Ordering};

#[cfg(target_os = "macos")]
use std::sync::Arc;
#[cfg(target_os = "macos")]
use std::time::{Duration, Instant};
#[cfg(target_os = "macos")]
use tauri::{AppHandle, Manager};
#[cfg(target_os = "macos")]
use crate::AppState;

/// 短按阈值 —— < 300ms 当误触不响应
#[cfg(target_os = "macos")]
const LONG_PRESS_MS: u128 = 300;
/// 录音上限 —— 防止误触后忘了松开
#[cfg(target_os = "macos")]
const MAX_RECORDING_MS: u64 = 60_000;

/// 触发键 —— 用户在 Onboarding / 托盘选。
///
/// 用 macOS keycode 识别（不是 CGEventFlags）—— FlagsChanged 事件里 keycode 字段
/// 直接告诉你哪个物理键变了，比纯 flag 位精确（能区分左/右 modifier）。
///
/// 标准 macOS modifier keycodes:
///   54 right-cmd · 55 left-cmd · 56 left-shift · 58 left-option ·
///   59 left-control · 60 right-shift · 61 right-option · 62 right-control · 63 fn
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImeTrigger {
    Fn,
    Option,       // 任意 option（左右都行）
    Control,      // 任意 control（左右都行）
    RightShift,
    RightCommand,
    RightOption,
}

impl ImeTrigger {
    pub fn from_str(s: &str) -> Self {
        match s {
            "option"        => ImeTrigger::Option,
            "control"       => ImeTrigger::Control,
            "right-shift"   => ImeTrigger::RightShift,
            "right-command" => ImeTrigger::RightCommand,
            "right-option"  => ImeTrigger::RightOption,
            _               => ImeTrigger::Fn,
        }
    }
    pub fn as_str(&self) -> &'static str {
        match self {
            ImeTrigger::Fn            => "fn",
            ImeTrigger::Option        => "option",
            ImeTrigger::Control       => "control",
            ImeTrigger::RightShift    => "right-shift",
            ImeTrigger::RightCommand  => "right-command",
            ImeTrigger::RightOption   => "right-option",
        }
    }
    /// 返回这个 trigger 关心哪些 keycode（按下任意一个就算激活）
    pub fn keycodes(&self) -> &'static [i64] {
        match self {
            ImeTrigger::Fn            => &[63],
            ImeTrigger::Option        => &[58, 61],
            ImeTrigger::Control       => &[59, 62],
            ImeTrigger::RightShift    => &[60],
            ImeTrigger::RightCommand  => &[54],
            ImeTrigger::RightOption   => &[61],
        }
    }
    /// 显示名（en）
    pub fn display_en(&self) -> &'static str {
        match self {
            ImeTrigger::Fn            => "Hold fn",
            ImeTrigger::Option        => "Hold ⌥ (option)",
            ImeTrigger::Control       => "Hold ⌃ (control)",
            ImeTrigger::RightShift    => "Hold right ⇧",
            ImeTrigger::RightCommand  => "Hold right ⌘",
            ImeTrigger::RightOption   => "Hold right ⌥",
        }
    }
    pub fn display_zh(&self) -> &'static str {
        match self {
            ImeTrigger::Fn            => "按住 fn",
            ImeTrigger::Option        => "按住 ⌥ (option)",
            ImeTrigger::Control       => "按住 ⌃ (control)",
            ImeTrigger::RightShift    => "按住 右 ⇧",
            ImeTrigger::RightCommand  => "按住 右 ⌘",
            ImeTrigger::RightOption   => "按住 右 ⌥",
        }
    }
    pub fn all() -> &'static [ImeTrigger] {
        &[ImeTrigger::Fn, ImeTrigger::Option, ImeTrigger::Control,
          ImeTrigger::RightShift, ImeTrigger::RightCommand, ImeTrigger::RightOption]
    }
}

/// 触发 voice IME 的状态机（trigger 可配置）
///
/// ⚠️ v0.1.14 修：tap_callback 在 FFI hot path 上，**不能**调 Config::load()
///   （那会每次按键都读 + parse JSON 文件，跟其他线程写 config 也会撞）。
/// 所以这里把 trigger 的 keycodes 和 flag_bit 缓存进 atomic，set_trigger 时更新。
#[allow(dead_code)] // 非 macOS 上部分字段（keycode/flag 缓存）暂未被事件监听消费
struct ImeMonitor {
    pressed_at: AtomicI64,    // micros since UNIX_EPOCH, 0 = not pressed
    triggered: AtomicBool,    // 长按已触发 = 等松开做 stop
    enabled: AtomicBool,      // 用户在托盘可关
    // 缓存：当前 trigger 关心的两个 keycode（不需要的填 -1）+ flag bit
    trigger_kc1: AtomicI64,
    trigger_kc2: AtomicI64,
    trigger_flag: AtomicU64,
}

static MONITOR: once_cell::sync::Lazy<ImeMonitor> = once_cell::sync::Lazy::new(|| ImeMonitor {
    pressed_at: AtomicI64::new(0),
    triggered: AtomicBool::new(false),
    enabled: AtomicBool::new(true),
    trigger_kc1: AtomicI64::new(63), // fn 默认
    trigger_kc2: AtomicI64::new(-1),
    trigger_flag: AtomicU64::new(1 << 23), // fn flag
});

pub fn set_enabled(on: bool) {
    MONITOR.enabled.store(on, Ordering::Relaxed);
    println!("[mouseclaw] 🎙️ voice-ime enabled = {on}");
}

pub fn is_enabled() -> bool { MONITOR.enabled.load(Ordering::Relaxed) }

/// 切换触发键 —— 托盘 / Onboarding 调它，运行期热切换
/// 把 keycodes + flag 一次性缓存进 atomic，hot path 上就只读 atomic
pub fn set_trigger(t: ImeTrigger) {
    let kcs = t.keycodes();
    MONITOR.trigger_kc1.store(kcs.first().copied().unwrap_or(-1), Ordering::Relaxed);
    MONITOR.trigger_kc2.store(kcs.get(1).copied().unwrap_or(-1), Ordering::Relaxed);
    MONITOR.trigger_flag.store(trigger_to_flag_bit(t), Ordering::Relaxed);
    println!("[mouseclaw] 🎙️ voice-ime trigger → {:?}", t);
}

#[inline]
#[cfg(target_os = "macos")]
fn cached_matches(keycode: i64) -> bool {
    let kc1 = MONITOR.trigger_kc1.load(Ordering::Relaxed);
    let kc2 = MONITOR.trigger_kc2.load(Ordering::Relaxed);
    keycode == kc1 || (kc2 >= 0 && keycode == kc2)
}

/// 主入口 —— lib.rs setup 时调一次。起后台线程跑 CGEventTap。
/// 仅 macOS：调用点在 lib.rs 已 `#[cfg(target_os = "macos")]` 包裹。
#[cfg(target_os = "macos")]
pub fn spawn(app: AppHandle, state: Arc<AppState>) {
    let cfg = crate::config::Config::load();
    MONITOR.enabled.store(cfg.voice_ime_enabled, Ordering::Relaxed);
    let trigger = ImeTrigger::from_str(&cfg.voice_ime_trigger);
    // 把 keycodes + flag 缓存进 atomic（hot path 用）
    set_trigger(trigger);
    println!("[mouseclaw] 🎙️ voice-ime spawning · trigger={:?}", trigger);

    std::thread::Builder::new()
        .name("mouseclaw-ime-tap".into())
        .spawn(move || run_event_tap(app, state))
        .expect("spawn ime tap thread");
}

// ────────────────── CGEventTap 部分（仅 macOS）──────────────────

#[cfg(target_os = "macos")]
#[repr(transparent)]
#[derive(Copy, Clone)]
#[allow(dead_code)] // 保留给后续扩展（事件回调里手动构造 event ref）
struct CGEventRef(*mut std::ffi::c_void);
#[cfg(target_os = "macos")]
unsafe impl Send for CGEventRef {}

#[cfg(target_os = "macos")]
type CGEventTapCallBack = unsafe extern "C" fn(
    proxy: *mut std::ffi::c_void,
    event_type: u32,
    event: *mut std::ffi::c_void,
    user_info: *mut std::ffi::c_void,
) -> *mut std::ffi::c_void;

#[cfg(target_os = "macos")]
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
    /// kCGKeyboardEventKeycode = 9
    fn CGEventGetIntegerValueField(event: *mut std::ffi::c_void, field: u32) -> i64;
}

#[cfg(target_os = "macos")]
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

/// Tap 接收回调（C ABI）—— 用 keycode 字段精确识别用户选的 trigger 键
#[cfg(target_os = "macos")]
unsafe extern "C" fn tap_callback(
    _proxy: *mut std::ffi::c_void,
    event_type: u32,
    event: *mut std::ffi::c_void,
    user_info: *mut std::ffi::c_void,
) -> *mut std::ffi::c_void {
    // kCGEventFlagsChanged = 12
    if event_type != 12 { return event; }
    if !MONITOR.enabled.load(Ordering::Relaxed) { return event; }

    // 拿出 keycode —— 哪个物理键变化了
    let keycode = CGEventGetIntegerValueField(event, 9); // kCGKeyboardEventKeycode = 9
    if !cached_matches(keycode) {
        return event;
    }

    // ⚠️ 全部走 atomic 缓存 —— 严禁在这个 FFI hot path 上做文件 I/O
    let flag = MONITOR.trigger_flag.load(Ordering::Relaxed);
    let flags = CGEventGetFlags(event);
    let is_down = (flags & flag) != 0;

    let prev_pressed = MONITOR.pressed_at.load(Ordering::Relaxed) != 0;
    if is_down && !prev_pressed {
        let now_us = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_micros() as i64).unwrap_or(0);
        MONITOR.pressed_at.store(now_us, Ordering::Relaxed);
        let st = &*(user_info as *const TapState);
        let _ = st.tx.send(ImeEvent::PressedDown);
    } else if !is_down && prev_pressed {
        MONITOR.pressed_at.store(0, Ordering::Relaxed);
        let st = &*(user_info as *const TapState);
        let was_triggered = MONITOR.triggered.swap(false, Ordering::Relaxed);
        if was_triggered {
            let _ = st.tx.send(ImeEvent::ReleasedAfterLong);
        } else {
            let _ = st.tx.send(ImeEvent::ReleasedShortTap);
        }
    }
    event
}

fn trigger_to_flag_bit(t: ImeTrigger) -> u64 {
    match t {
        ImeTrigger::Fn            => 1 << 23, // kCGEventFlagMaskSecondaryFn
        ImeTrigger::Option        => 1 << 19, // kCGEventFlagMaskAlternate
        ImeTrigger::Control       => 1 << 18, // kCGEventFlagMaskControl
        ImeTrigger::RightShift    => 1 << 17, // kCGEventFlagMaskShift
        ImeTrigger::RightCommand  => 1 << 20, // kCGEventFlagMaskCommand
        ImeTrigger::RightOption   => 1 << 19, // kCGEventFlagMaskAlternate
    }
}

#[cfg(target_os = "macos")]
#[derive(Debug)]
enum ImeEvent {
    PressedDown,
    ReleasedAfterLong,
    #[allow(dead_code)]
    ReleasedShortTap,
}

#[cfg(target_os = "macos")]
struct TapState {
    tx: std::sync::mpsc::Sender<ImeEvent>,
}

#[cfg(target_os = "macos")]
fn run_event_tap(app: AppHandle, state: Arc<AppState>) {
    let (tx, rx) = std::sync::mpsc::channel::<ImeEvent>();

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

#[cfg(target_os = "macos")]
fn handle_fn_events(rx: std::sync::mpsc::Receiver<ImeEvent>, app: AppHandle, state: Arc<AppState>) {
    while let Ok(ev) = rx.recv() {
        match ev {
            ImeEvent::PressedDown => {
                let app2 = app.clone();
                let state2 = state.clone();
                std::thread::spawn(move || {
                    std::thread::sleep(Duration::from_millis(LONG_PRESS_MS as u64));
                    let still_down = MONITOR.pressed_at.load(Ordering::Relaxed) != 0;
                    if !still_down { return; }
                    // 安全检查 #1：密码 / secure input 字段 → 整段拒绝触发
                    if is_secure_input_active() {
                        println!("[mouseclaw] 🎙️ secure input active, refuse voice IME");
                        MONITOR.pressed_at.store(0, Ordering::Relaxed);
                        return;
                    }
                    if MONITOR.triggered.swap(true, Ordering::Relaxed) {
                        return;
                    }
                    let app3 = app2.clone();
                    println!("[mouseclaw] 🎙️ trigger long-press → start voice IME recording");
                    start_recording_for_ime(app2, state2);

                    // 安全检查 #2：60s 强制截止 —— 防止误触后忘了松开
                    std::thread::spawn(move || {
                        let started = Instant::now();
                        while MONITOR.triggered.load(Ordering::Relaxed) {
                            if started.elapsed() >= Duration::from_millis(MAX_RECORDING_MS) {
                                eprintln!("[mouseclaw] 🎙️ 60s 强制截止 voice IME");
                                MONITOR.pressed_at.store(0, Ordering::Relaxed);
                                MONITOR.triggered.store(false, Ordering::Relaxed);
                                // 模拟松开（提交录音）
                                let app_state = match app3.try_state::<std::sync::Arc<AppState>>() {
                                    Some(s) => s.inner().clone(),
                                    None => return,
                                };
                                stop_and_paste(app3.clone(), app_state);
                                return;
                            }
                            std::thread::sleep(Duration::from_millis(500));
                        }
                    });
                });
            }
            ImeEvent::ReleasedAfterLong => {
                println!("[mouseclaw] 🎙️ trigger released → stop & transcribe & paste");
                stop_and_paste(app.clone(), state.clone());
            }
            ImeEvent::ReleasedShortTap => {
                // 短按 —— 不响应（让 macOS 原生 modifier 行为正常）
            }
        }
    }
}

/// 检测前台 app 是否有 SecureKeyboardEntry（密码输入框/终端 sudo 等）
/// 避免在密码框走 voice IME 把密码暴露给 ASR / 屏幕。
#[cfg(target_os = "macos")]
fn is_secure_input_active() -> bool {
    extern "C" {
        fn IsSecureEventInputEnabled() -> bool;
    }
    unsafe { IsSecureEventInputEnabled() }
}
#[cfg(not(target_os = "macos"))]
fn is_secure_input_active() -> bool { false }

// ────────────────── 录音 → sherpa ASR → paste 流程 ──────────────────

#[cfg(target_os = "macos")]
fn start_recording_for_ime(app: AppHandle, state: Arc<AppState>) {
    // 已经在录音（被 ⌘⇧Space 占着）→ 跳过
    if state.recorder.lock().unwrap().is_some() {
        println!("[mouseclaw] 🎙️ recorder busy (occupied by AI summon?), skip voice IME");
        MONITOR.triggered.store(false, Ordering::Relaxed);
        return;
    }
    if !crate::transcribe_stream::is_ready() {
        eprintln!("[mouseclaw] 🎙️ sherpa not ready (model still downloading?), skip voice IME");
        // v0.4.0 · 给用户气泡反馈 —— 之前只 eprintln 用户看不到，按 fn 按了半天以为坏了。
        // 复用 pipeline 的 blocked 消息组装（带实时下载进度）。
        crate::overlay::show_mouse(&app);
        crate::overlay::emit_view(&app, &crate::events::ViewKind::Blocked {
            reason: crate::pipeline::build_model_blocked_msg(),
        });
        crate::overlay::schedule_auto_hide(&app, &state, 6000);
        MONITOR.triggered.store(false, Ordering::Relaxed);
        return;
    }
    // v0.3.6 · Accessibility 检查 —— 没权限的话 CGEvent.post 会静默失败，
    // 字根本进不去输入框。提前 fail loud：弹气泡告诉用户去授权（带按钮）。
    if !crate::permissions::check_accessibility() {
        eprintln!("[mouseclaw] 🎙️ Accessibility 权限缺失，voice IME 字打不进输入框");
        crate::overlay::show_mouse(&app);
        // [OPEN_AX_SETTINGS] 前缀 = 前端 Bubble 识别后渲染"🔓 去授权"按钮
        crate::overlay::emit_view(&app, &crate::events::ViewKind::Blocked {
            reason: "[OPEN_AX_SETTINGS]🔒 macOS 辅助功能权限未授权 —— \
                     语音转写出来的字没法送进输入框。\
                     点下面按钮去系统设置授权（然后退出 app 重启）".into(),
        });
        crate::overlay::schedule_auto_hide(&app, &state, 15_000);
        MONITOR.triggered.store(false, Ordering::Relaxed);
        return;
    }
    // v0.3.6 · ⚠️ 关键：在录音开始前 snapshot 当前前台 app pid。
    // macOS 的 fn 键自带系统语义（拼音切换 / Spotlight / Dock 等），按住瞬间会
    // 把当前输入框失焦 → 后面 CGEvent 字会被丢到错的 app。
    // 解：把 pid 存到 state，paste 之前先 NSRunningApplication.activate 把它拉回前台。
    #[cfg(target_os = "macos")]
    {
        if let Some(pid) = crate::frontmost::current_frontmost_pid() {
            *state.prev_frontmost_pid.lock().unwrap() = Some(pid);
            println!("[mouseclaw] 🎙️ snapshot pre-fn frontmost pid={pid}");
        }
    }
    let recorder = match crate::audio::Recorder::start() {
        Ok(r) => r,
        Err(e) => {
            eprintln!("[mouseclaw] 🎙️ start recording: {e:#}");
            MONITOR.triggered.store(false, Ordering::Relaxed);
            return;
        }
    };
    *state.recorder.lock().unwrap() = Some(recorder);
    // v0.3.1 · streaming: create sherpa session + start poller for live typing
    match crate::transcribe_stream::StreamSession::new() {
        Ok(s) => *state.stream_session.lock().unwrap() = Some(s),
        Err(e) => eprintln!("[mouseclaw] 🎙️ StreamSession::new for IME failed: {e:#}"),
    }
    state.streaming_active.store(true, Ordering::SeqCst);
    // v0.4.2 · bug2 修复 —— 新会话开始即 bump_gen，让上一次 stop_and_paste 排的
    // auto-hide timer 失效（否则它会在新会话进行中 hide 掉桌宠 → 跳）。
    crate::overlay::bump_gen(&state);
    crate::overlay::show_mouse(&app);
    crate::overlay::emit_view(&app, &crate::events::ViewKind::VoiceImeListening { partial: String::new() });

    // v0.3.1 · 流式 type-as-you-speak —— 150ms 一次轮询，partial 增量直接 paste 到光标
    // 走 sherpa partial 与上一次已 type 的文本求 longest common prefix，
    // 删掉发散部分 + paste 新部分。模型自我纠错时回退 N char，再写新。
    // 已 type 的字符串同步进 state.ime_typed，stop_and_paste 拿它做 final delta。
    {
        let mut g = state.ime_typed.lock().unwrap();
        g.clear();
    }
    // v0.3.8 · Plan B —— streaming poller 只更新桌宠气泡 partial，**不**写入光标。
    // 真正的 paste 延后到 fn 松开后（stop_and_paste_for_ime），那时候 macOS fn
    // 系统行为已经结束，焦点回到原 app，CGEvent 字才进得了输入框。
    // 这跟 Whisper 时代的 timing 一致 —— 用户看到流式视觉但写入是 batch 的。
    let state_stream = state.clone();
    let app_stream = app.clone();
    // v0.4 fix (2026-05-20) · 用专用 std::thread 而非 async_runtime::spawn 跑流式解码。
    //   原因：sherpa s.accept/partial + add_punctuation 都是 CPU 密集**同步**调用，放在
    //   tokio async 任务里会占住一个 worker 线程。当 AI 子进程（翻译 / 召唤）同时在跑、
    //   抢 tokio runtime 时，worker 被饿死 → 听写 partial 卡住（用户实测"打开语音输入法
    //   就卡住"）。挪到独立 OS 线程后，操作系统抢占式调度保证它永远能跑，不受 tokio 状态影响。
    std::thread::Builder::new()
        .name("mouseclaw-ime-poller".into())
        .spawn(move || {
        let mut last_partial = String::new();
        loop {
            std::thread::sleep(Duration::from_millis(150));
            if !state_stream.streaming_active.load(Ordering::SeqCst) {
                break;
            }
            let samples = {
                let g = state_stream.recorder.lock().unwrap();
                match g.as_ref() {
                    Some(r) => r.drain_resampled_16k(),
                    None => break,
                }
            };
            if samples.is_empty() { continue; }
            let partial = {
                let mut g = state_stream.stream_session.lock().unwrap();
                if let Some(s) = g.as_mut() {
                    s.accept(&samples);
                    s.partial()
                } else {
                    continue;
                }
            };
            if partial == last_partial { continue; }
            last_partial = partial.clone();
            // v0.4.1 · 先把模型的全大写英文还原成自然大小写（OPENAI→OpenAI），
            // partial 也做，让"边说边出"的英文一开始就正常，不是最后一刻才变。
            let recased = crate::vocab::recase_english(&partial);
            // v0.4.0 · 流式 partial 也加标点 —— 沿用 add_punctuation（~10ms, soft-fail）
            // 短片段（< 4 字符）模型加标点效果差，跳过让 raw 出。
            let display = if recased.chars().count() >= 4 {
                crate::punctuation::add_punctuation(&recased)
            } else {
                recased
            };
            // 桌宠头顶气泡实时显示新 partial —— 用户看到"边说边出"的视觉反馈
            crate::overlay::emit_view(
                &app_stream,
                &crate::events::ViewKind::VoiceImeListening { partial: display },
            );
        }
        println!("[mouseclaw] 🎙️ IME streaming poller exited (final paste in stop_and_paste)");
    }).expect("spawn ime poller thread");

    // v0.1.13 安全 #3：记录当前前台 app 的 bundle id —— 录音中切走就取消
    let start_bundle = std::panic::catch_unwind(frontmost_bundle).unwrap_or_default();
    let app2 = app.clone();
    let state2 = state.clone();
    std::thread::spawn(move || {
        let started = Instant::now();
        loop {
            std::thread::sleep(Duration::from_millis(400));
            // 已经被 stop_and_paste 收掉了 → 退出
            if !MONITOR.triggered.load(Ordering::Relaxed) { return; }
            // 时间到上限 → 让 60s cutoff 处理，不重复触发
            if started.elapsed() >= Duration::from_millis(MAX_RECORDING_MS) { return; }
            let cur = std::panic::catch_unwind(frontmost_bundle).unwrap_or_default();
            if !cur.is_empty() && !start_bundle.is_empty() && cur != start_bundle {
                println!("[mouseclaw] 🎙️ 前台 {start_bundle} → {cur}，取消录音（不写入）");
                MONITOR.pressed_at.store(0, Ordering::Relaxed);
                MONITOR.triggered.store(false, Ordering::Relaxed);
                // 丢弃录音数据
                if let Some(rec) = state2.recorder.lock().unwrap().take() {
                    let _ = rec.stop_drain_remaining_16k();
                }
                crate::overlay::emit_view(&app2, &crate::events::ViewKind::Blocked {
                    reason: if crate::config::Config::load().language == "en" {
                        "Voice input cancelled — frontmost app changed.".into()
                    } else {
                        "前台 app 切走 — 已取消语音输入，未写入。".into()
                    },
                });
                tokio::runtime::Handle::try_current().map(|h| h.spawn(async move {
                    tokio::time::sleep(Duration::from_millis(2000)).await;
                    crate::overlay::hide_overlay(&app2);
                })).ok();
                return;
            }
        }
    });
}

/// 拿当前前台 app 的 bundle id
/// v0.1.15 修：包 NSAutoreleasePool —— frontmostApplication / bundleIdentifier 都返 autoreleased
#[cfg(target_os = "macos")]
fn frontmost_bundle() -> String {
    use cocoa::base::{id, nil};
    use objc::{class, msg_send, sel, sel_impl};
    unsafe {
        let pool: id = msg_send![class!(NSAutoreleasePool), new];
        let result = (|| -> String {
            let ws: id = msg_send![class!(NSWorkspace), sharedWorkspace];
            if ws == nil { return String::new(); }
            let app: id = msg_send![ws, frontmostApplication];
            if app == nil { return String::new(); }
            let bid: id = msg_send![app, bundleIdentifier];
            if bid == nil { return String::new(); }
            let p: *const std::os::raw::c_char = msg_send![bid, UTF8String];
            if p.is_null() { return String::new(); }
            std::ffi::CStr::from_ptr(p).to_string_lossy().into_owned()
        })();
        if pool != nil { let _: () = msg_send![pool, drain]; }
        result
    }
}
#[cfg(not(target_os = "macos"))]
fn frontmost_bundle() -> String { String::new() }

#[cfg(target_os = "macos")]
fn stop_and_paste(app: AppHandle, state: Arc<AppState>) {
    // v0.3.1 · 停止 streaming poller —— 让它最后一帧完成后退出
    state.streaming_active.store(false, Ordering::SeqCst);
    let recorder = state.recorder.lock().unwrap().take();
    let Some(recorder) = recorder else { return; };

    crate::overlay::emit_view(&app, &crate::events::ViewKind::Thinking {
        transcript: "(语音输入中…)".into(),
        status: None,
    });

    tauri::async_runtime::spawn_blocking(move || {
        // v0.3.1 · streaming poller 已 paste 增量了，这里只需 stop + finalize
        // sherpa 拿到最终 transcript → 跟 poller 已 paste 的对比 → 仅补足 delta
        let remaining = match recorder.stop_drain_remaining_16k() {
            Ok(s) => s,
            Err(e) => {
                eprintln!("[mouseclaw] 🎙️ stop_drain_remaining_16k: {e}");
                crate::overlay::hide_overlay(&app);
                return;
            }
        };
        let session = state.stream_session.lock().unwrap().take();
        let raw = if let Some(mut sess) = session {
            if !remaining.is_empty() { sess.accept(&remaining); }
            match sess.finalize() {
                Ok(t) => t,
                Err(e) => {
                    eprintln!("[mouseclaw] 🎙️ sherpa finalize 失败: {e}");
                    crate::overlay::emit_view(&app, &crate::events::ViewKind::Blocked {
                        reason: format!("transcribe failed: {e}"),
                    });
                    return;
                }
            }
        } else {
            String::new()
        };
        let cfg = crate::config::Config::load();
        let cleaned = crate::tidy_up::light_clean(&raw, &cfg.language);
        if cleaned.trim().is_empty() {
            println!("[mouseclaw] 🎙️ 空输出，无声触发？hide");
            crate::overlay::hide_overlay(&app);
            return;
        }
        println!("[mouseclaw] 🎙️ light cleaned → {cleaned:?}");

        // v0.4.1 · 英文大小写还原 —— 模型英文输出全大写，这里用词表把术语还原成
        // 正确大小写（OPENAI→OpenAI / OPEN CLAW→OpenClaw / USEEFFECT→useEffect），
        // 其余未知英文转小写。中文/数字/标点不动。在加标点之前做。
        let recased = crate::vocab::recase_english(&cleaned);
        if recased != cleaned {
            println!("[mouseclaw] 🔤 recased → {recased:?}");
        }

        // v0.3.6 · 本地标点 —— sherpa CT-Transformer，~10ms 加 。，？
        // 失败兜底返回原文，永远不阻断主流程
        let punctuated = crate::punctuation::add_punctuation(&recased);
        if punctuated != recased {
            println!("[mouseclaw] 🎯 punctuated → {punctuated:?}");
        }

        // v0.3.8 · Plan B —— streaming poller 不 type，只在 fn 松开后一次性写。
        // 跟 Whisper batch timing 一致：fn 松开 → 等焦点回到原 app → activate + paste。
        // 不再有 LCP delta（streaming 没 type 任何东西，typed 永远是空）。
        let app2 = app.clone();
        let state2 = state.clone();
        tauri::async_runtime::spawn(async move {
            let final_text = punctuated.clone();
            println!("[mouseclaw] 🎙️ ready to paste final → {:?}", final_text);

            // 关键 timing：fn 松开后 macOS fn 系统行为结束，但焦点回到原 app
            // 需要一点时间。这里先睡 150ms 让 macOS 自己稳定，再 activate_pid
            // 强制把原 app 拉回前台，再等 60ms 让 activation 真生效，最后 paste。
            tokio::time::sleep(Duration::from_millis(150)).await;

            #[cfg(target_os = "macos")]
            {
                let pid_opt = *state2.prev_frontmost_pid.lock().unwrap();
                if let Some(pid) = pid_opt {
                    crate::frontmost::activate_pid(pid);
                    tokio::time::sleep(Duration::from_millis(60)).await;
                    println!("[mouseclaw] 🎙️ re-activated pre-fn frontmost pid={pid}");
                }
            }

            // v0.4.0 P2 · 3 秒规则纠错 —— 在写之前检查 transcript 是不是「把 X 改成 Y / 重说」等指令
            let current_bundle = frontmost_bundle();
            let correction = crate::voice_correct::try_parse(&final_text, &current_bundle);
            if let Some(action) = correction {
                handle_correction(&app2, action, &current_bundle).await;
                return;
            }

            match crate::mode_b::write_at_cursor(&final_text).await {
                Ok(()) => {
                    // v0.4.0 P2 · 记录这次写入 —— 下一次 3 秒内的语音可能要纠错它
                    crate::voice_correct::record_write(&final_text, &current_bundle);
                    crate::overlay::emit_view(&app2, &crate::events::ViewKind::Reply {
                        transcript: "voice IME".into(),
                        reply: format!("✍️ {final_text}"),
                        mode: crate::events::ReplyMode::A,
                        insert_text: None,
                        streaming: false,
                    });
                    // v0.4.2 · bug2 修复 —— 用受 gen 保护的 schedule_auto_hide 替代裸
                    // sleep+hide。之前裸 sleep 1500ms 后无条件 hide：用户在这期间马上再
                    // 按触发键召唤新会话，旧 timer 醒来仍 hide，把刚召唤出来的桌宠拽回
                    // 角落 → 视觉"跳"。schedule_auto_hide 记录当前 gen，新会话 start 时
                    // bump_gen 让这个旧 timer no-op（与 AI 召唤路径一致）。
                    crate::overlay::schedule_auto_hide(&app2, &state2, 1500);
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

/// v0.4.0 P2 · 执行纠错动作 —— Replace 走 backspace + write，UndoAll 走 backspace，
/// NotFound 仅气泡反馈不写入。所有路径走完发个气泡 + 自动 hide。
#[cfg(target_os = "macos")]
async fn handle_correction(
    app: &AppHandle,
    action: crate::voice_correct::CorrectionAction,
    current_bundle: &str,
) {
    use crate::voice_correct::CorrectionAction;
    match action {
        CorrectionAction::Replace { old, new, full_replacement, prev_char_count } => {
            if let Err(e) = crate::mode_b::delete_chars(prev_char_count) {
                eprintln!("[mouseclaw] 🎙️ correction backspace failed: {e}");
                crate::overlay::emit_view(app, &crate::events::ViewKind::Blocked {
                    reason: format!("纠错失败：{e}"),
                });
                return;
            }
            // 给前台 app 一点点时间消化 backspace
            tokio::time::sleep(Duration::from_millis(40)).await;
            match crate::mode_b::write_at_cursor(&full_replacement).await {
                Ok(()) => {
                    crate::voice_correct::record_write(&full_replacement, current_bundle);
                    println!("[mouseclaw] 🔄 corrected: '{old}' → '{new}'");
                    crate::overlay::emit_view(app, &crate::events::ViewKind::Reply {
                        transcript: "voice correction".into(),
                        reply: format!("✓ 已替换「{old} → {new}」"),
                        mode: crate::events::ReplyMode::A,
                        insert_text: None,
                        streaming: false,
                    });
                    tokio::time::sleep(Duration::from_millis(1500)).await;
                    crate::overlay::hide_overlay(app);
                }
                Err(e) => {
                    eprintln!("[mouseclaw] 🎙️ correction rewrite failed: {e}");
                    crate::overlay::emit_view(app, &crate::events::ViewKind::Blocked {
                        reason: format!("重写失败：{e}"),
                    });
                }
            }
        }
        CorrectionAction::UndoAll { prev_char_count } => {
            if let Err(e) = crate::mode_b::delete_chars(prev_char_count) {
                eprintln!("[mouseclaw] 🎙️ correction undo failed: {e}");
                crate::overlay::emit_view(app, &crate::events::ViewKind::Blocked {
                    reason: format!("撤销失败：{e}"),
                });
                return;
            }
            crate::voice_correct::clear();
            println!("[mouseclaw] 🔄 undo-all: deleted {prev_char_count} chars");
            crate::overlay::emit_view(app, &crate::events::ViewKind::Reply {
                transcript: "voice correction".into(),
                reply: "↶ 已撤销".into(),
                mode: crate::events::ReplyMode::A,
                insert_text: None,
                streaming: false,
            });
            tokio::time::sleep(Duration::from_millis(1500)).await;
            crate::overlay::hide_overlay(app);
        }
        CorrectionAction::NotFound { needle } => {
            println!("[mouseclaw] 🔄 correction target not found in last write: '{needle}'");
            // 不动 last_written —— 用户可能想再试一次
            crate::overlay::emit_view(app, &crate::events::ViewKind::Reply {
                transcript: "voice correction".into(),
                reply: format!("⚠️ 上一段没找到「{needle}」"),
                mode: crate::events::ReplyMode::A,
                insert_text: None,
                streaming: false,
            });
            tokio::time::sleep(Duration::from_millis(2000)).await;
            crate::overlay::hide_overlay(app);
        }
    }
}
