//! 环境感知采样 (v0.1.27 P3)
//!
//! 每 30s 从 macOS 系统 API 采一个 PresenceSample，写进 30min 环形 buffer。
//! Nudge 引擎从 buffer 上读规则，决定要不要提醒。
//!
//! ### 隐私红线（写进 DESIGN / README 显著位置）
//!   - **只数键盘"打了多少下"**，不读"打了什么"
//!     —— 用 `CGEventSourceSecondsSinceLastEventType(.keyDown)`，
//!     这是个时间戳查询，**根本拿不到键值**
//!   - **只看前台 app bundle id**，不抓窗口内容
//!   - **30 分钟环形 buffer，过期自动覆盖**，不落盘、不联网
//!   - 日历 / 摄像头占用是可选授权（V1 不做）
//!
//! ### 为什么 30s 而不是 5s
//! 5s 采样省下来的精度对 nudge 规则没意义（最快的规则也要 5min 滑窗）。
//! 30s 是后台任务每分钟唤醒 2 次，CPU/电量基本可忽略。

use std::collections::VecDeque;
use std::sync::{Arc, RwLock};
use std::time::Duration;

use chrono::Local;
use tauri::AppHandle;
use tokio::time::sleep;

const SAMPLE_INTERVAL_SECS: u64 = 30;
/// 30min ÷ 30s = 60 samples
const BUFFER_CAPACITY: usize = 60;

/// 单次采样的所有原始信号。**没有任何用户内容**。
#[derive(Debug, Clone)]
pub struct PresenceSample {
    /// Unix 秒，本机墙钟。env::now 算 idle 用。
    pub ts_secs: i64,
    /// macOS 前台 app bundle id（如 `"com.microsoft.VSCode"`）。
    /// 拿不到时空字符串。
    pub frontmost_bundle: String,
    /// 距离上次任意 keyDown 的秒数 —— 大 = 没在敲键。
    pub secs_since_keyboard: f64,
    /// 距离上次鼠标移动的秒数 —— 大 = 鼠标静止。
    pub secs_since_mouse_move: f64,
    /// `[localtime] hour-of-day` 小时（0-23），nudge 用来决定时段。
    pub hour: u8,
}

impl PresenceSample {
    /// 当前是否"用户在键盘上活跃"（距离最后一次键击 < SAMPLE_INTERVAL）。
    pub fn keyboard_active(&self) -> bool {
        self.secs_since_keyboard < SAMPLE_INTERVAL_SECS as f64
    }
    /// 当前鼠标是否静止 > 给定阈值（秒）。
    pub fn mouse_idle_for(&self, threshold_secs: f64) -> bool {
        self.secs_since_mouse_move > threshold_secs
    }
}

/// 30-min 环形 buffer。nudge 引擎读它，不写。
#[derive(Debug, Default)]
pub struct PresenceBuffer {
    samples: VecDeque<PresenceSample>,
}

impl PresenceBuffer {
    pub fn new() -> Self { Self { samples: VecDeque::with_capacity(BUFFER_CAPACITY) } }

    pub fn push(&mut self, sample: PresenceSample) {
        if self.samples.len() >= BUFFER_CAPACITY {
            self.samples.pop_front();
        }
        self.samples.push_back(sample);
    }

    pub fn latest(&self) -> Option<&PresenceSample> { self.samples.back() }

    pub fn len(&self) -> usize { self.samples.len() }

    /// 用于规则：最近 `window_secs` 秒里，键盘活跃的 sample 数量。
    /// 例如 30min 窗口里 ≥ 50 个 active sample = "几乎一直在敲键 → 心流"。
    /// `now_ts` 由调用方传入 —— 让单测可注入合成时间戳。
    pub fn keyboard_active_count(&self, now_ts: i64, window_secs: u64) -> usize {
        self.samples.iter()
            .filter(|s| now_ts - s.ts_secs <= window_secs as i64 && s.keyboard_active())
            .count()
    }

    /// 用于规则：最近 `window_secs` 秒里，前台 app bundle 命中给定关键字
    /// 列表的 sample 数量。例如 IDE 列表里的 bundle 持续出现 → IDE 焦点中。
    pub fn focused_bundle_count_matching(
        &self, now_ts: i64, window_secs: u64, needles: &[&str],
    ) -> usize {
        self.samples.iter()
            .filter(|s| now_ts - s.ts_secs <= window_secs as i64)
            .filter(|s| {
                let b = s.frontmost_bundle.to_ascii_lowercase();
                needles.iter().any(|n| b.contains(*n))
            })
            .count()
    }
}

// ──────────────────────────────────────────────────────────────────
// macOS 系统调用 —— pure read-only 时间戳查询，no event tap, no hooks
// ──────────────────────────────────────────────────────────────────

#[cfg(target_os = "macos")]
fn sample_now() -> PresenceSample {
    let bundle = frontmost_bundle_id().unwrap_or_default();
    let secs_kb = secs_since_event(EVENT_KEY_DOWN);
    let secs_mv = secs_since_event(EVENT_MOUSE_MOVED);
    let now = Local::now();
    PresenceSample {
        ts_secs: now.timestamp(),
        frontmost_bundle: bundle,
        secs_since_keyboard: secs_kb,
        secs_since_mouse_move: secs_mv,
        hour: now.format("%H").to_string().parse().unwrap_or(0),
    }
}

#[cfg(not(target_os = "macos"))]
fn sample_now() -> PresenceSample {
    PresenceSample {
        ts_secs: Local::now().timestamp(),
        frontmost_bundle: String::new(),
        secs_since_keyboard: 0.0,
        secs_since_mouse_move: 0.0,
        hour: Local::now().format("%H").to_string().parse().unwrap_or(0),
    }
}

#[cfg(target_os = "macos")]
fn frontmost_bundle_id() -> Option<String> {
    use cocoa::base::{id, nil};
    use cocoa::foundation::NSString;
    use objc::{class, msg_send, sel, sel_impl};
    unsafe {
        let ws: id = msg_send![class!(NSWorkspace), sharedWorkspace];
        if ws == nil { return None; }
        let app: id = msg_send![ws, frontmostApplication];
        if app == nil { return None; }
        let bundle_id: id = msg_send![app, bundleIdentifier];
        if bundle_id == nil { return None; }
        let utf8: *const std::os::raw::c_char = msg_send![bundle_id, UTF8String];
        if utf8.is_null() { return None; }
        let cstr = std::ffi::CStr::from_ptr(utf8);
        cstr.to_str().ok().map(|s| s.to_string())
    }
}

#[cfg(target_os = "macos")]
const EVENT_KEY_DOWN: u32 = 10;        // kCGEventKeyDown
#[cfg(target_os = "macos")]
const EVENT_MOUSE_MOVED: u32 = 5;      // kCGEventMouseMoved
#[cfg(target_os = "macos")]
const CG_EVENT_SOURCE_STATE_COMBINED: u32 = 0;  // kCGEventSourceStateCombinedSessionState

#[cfg(target_os = "macos")]
fn secs_since_event(event_type: u32) -> f64 {
    extern "C" {
        // CFTimeInterval CGEventSourceSecondsSinceLastEventType(
        //     CGEventSourceStateID, CGEventType
        // );
        fn CGEventSourceSecondsSinceLastEventType(state: u32, event_type: u32) -> f64;
    }
    unsafe { CGEventSourceSecondsSinceLastEventType(CG_EVENT_SOURCE_STATE_COMBINED, event_type) }
}

// ──────────────────────────────────────────────────────────────────
// Public spawn
// ──────────────────────────────────────────────────────────────────

/// 后台任务 —— lib.rs setup() 调一次。返回的 Arc 同时给 nudge 引擎读取。
pub fn spawn(_app: AppHandle) -> Arc<RwLock<PresenceBuffer>> {
    let buf = Arc::new(RwLock::new(PresenceBuffer::new()));
    let buf_clone = buf.clone();
    tauri::async_runtime::spawn(async move {
        loop {
            // 头一次也立即采一次（不要等 30s 才有数据）
            let s = sample_now();
            if let Ok(mut w) = buf_clone.write() {
                w.push(s);
            }
            sleep(Duration::from_secs(SAMPLE_INTERVAL_SECS)).await;
        }
    });
    println!("[mouseclaw] 🦞 presence sampler spawned ({SAMPLE_INTERVAL_SECS}s · {BUFFER_CAPACITY}-cap buffer)");
    buf
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_sample(ts: i64, bundle: &str, kb_secs: f64, mv_secs: f64, hour: u8)
        -> PresenceSample
    {
        PresenceSample {
            ts_secs: ts,
            frontmost_bundle: bundle.into(),
            secs_since_keyboard: kb_secs,
            secs_since_mouse_move: mv_secs,
            hour,
        }
    }

    #[test]
    fn buffer_drops_oldest_when_full() {
        let mut buf = PresenceBuffer::new();
        for i in 0..(BUFFER_CAPACITY + 5) {
            buf.push(make_sample(i as i64, "x", 0.0, 0.0, 12));
        }
        assert_eq!(buf.len(), BUFFER_CAPACITY);
        assert_eq!(buf.latest().unwrap().ts_secs, (BUFFER_CAPACITY + 4) as i64);
    }

    #[test]
    fn keyboard_active_distinguishes_recent_keystroke() {
        let s_active = make_sample(0, "x", 5.0, 99.0, 12);
        let s_idle   = make_sample(0, "x", 90.0, 99.0, 12);
        assert!(s_active.keyboard_active());
        assert!(!s_idle.keyboard_active());
    }

    #[test]
    fn mouse_idle_for_threshold_works() {
        let s = make_sample(0, "x", 0.0, 120.0, 12);
        assert!(s.mouse_idle_for(60.0));
        assert!(!s.mouse_idle_for(180.0));
    }

    #[test]
    fn focused_bundle_count_matches_case_insensitive_substring() {
        let mut buf = PresenceBuffer::new();
        let now = 10_000_i64;
        buf.push(make_sample(now - 10, "com.microsoft.VSCode", 0.0, 0.0, 12));
        buf.push(make_sample(now - 5,  "com.microsoft.VSCode", 0.0, 0.0, 12));
        buf.push(make_sample(now - 2,  "com.google.Chrome",    0.0, 0.0, 12));
        let n = buf.focused_bundle_count_matching(now, 60, &["vscode", "xcode"]);
        assert_eq!(n, 2);
    }
}
