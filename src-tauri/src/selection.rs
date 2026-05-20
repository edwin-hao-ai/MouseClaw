//! 选区轮询 · macOS Accessibility（v0.4+）
//!
//! 500ms 后台 task → 拿系统级 focused element → 读 AXSelectedText →
//! 内容变化且 ≥ 阈值 → 走 `reactive::on_new_text(Source::Selection, ...)`。
//!
//! 重要约束（参考 CLAUDE.md "动手前先深度思考 UX"）：
//!   1. 没有 Accessibility 权限 → 立即静默退出（不污染日志），权限给了才工作
//!   2. 跟剪贴板共用 reactive::classify —— 静音规则一致，避免一边响一边静
//!   3. 阈值 ≥ 30 字符（比剪贴板 20 字宽松一点 —— 用户经常双击选个词，不该弹）
//!   4. 同段文字 cooldown 10s —— 用户调整选区时不要反复弹
//!   5. clipboard 的 PAUSED 同时静音选区 —— 用户托盘里"暂停剪贴板"应该一并暂停选区
//!
//! 实现要点：
//!   - 用 ApplicationServices framework 的 AXUIElementRef API
//!   - core-foundation 拿 CFStringRef → String
//!   - 所有 CFRelease 在 autoreleasepool 里跑，跟现有 clipboard.rs 风格保持一致

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

#[cfg(target_os = "macos")]
use core_foundation::base::{CFRelease, CFTypeRef, TCFType};
#[cfg(target_os = "macos")]
use core_foundation::string::{CFString, CFStringRef};

/// 选区轮询间隔。500ms 跟剪贴板对齐，CPU 开销可忽略（AX 调用 ~50us）。
pub const POLL_INTERVAL_MS: u64 = 500;

/// 选区最少 30 个字符才触发 —— 排除用户双击选一个词
pub const MIN_SELECTION_CHARS: usize = 30;

/// 同段内容 cooldown：选了之后 10s 内再次选同样的不触发
pub const COOLDOWN_SECS: u64 = 10;

/// 暂停开关。复用 clipboard::PAUSED 还是有自己一个？
/// 决定：自己一个，但默认跟随 clipboard。后续可以分别控制。
pub static PAUSED: AtomicBool = AtomicBool::new(false);

pub fn set_paused(p: bool) { PAUSED.store(p, Ordering::Relaxed); }
pub fn is_paused() -> bool { PAUSED.load(Ordering::Relaxed) }

/// 启动选区轮询线程。lib.rs::setup 里调一次。
pub fn spawn_capture_loop() {
    #[cfg(target_os = "macos")]
    std::thread::Builder::new()
        .name("mouseclaw-selection".into())
        .spawn(|| run_loop())
        .expect("spawn selection thread");
    // 非 macOS 直接 no-op
}

#[cfg(target_os = "macos")]
fn run_loop() {
    let mut last_text: Option<String> = None;
    let mut last_emit_at: Option<Instant> = None;

    println!("[mouseclaw] 🔤 selection capture loop started (poll {POLL_INTERVAL_MS}ms, min {MIN_SELECTION_CHARS} chars)");
    // 节流诊断：每 ~5s 最多打一次失败原因，避免刷屏
    let mut last_diag: Option<Instant> = None;
    let mut diag = |msg: String| {
        let now = last_diag.map_or(true, |t: Instant| t.elapsed() > Duration::from_secs(5));
        if now {
            println!("[mouseclaw] 🔤 {msg}");
            last_diag = Some(Instant::now());
        }
    };

    loop {
        std::thread::sleep(Duration::from_millis(POLL_INTERVAL_MS));

        if is_paused() || crate::clipboard::is_paused() {
            continue;
        }
        if !crate::permissions::check_accessibility() {
            diag("需要「辅助功能」权限 —— 系统设置 → 隐私与安全性 → 辅助功能 放行 MouseClaw".into());
            continue;
        }

        let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(current_selection_diag));
        let text = match r {
            Err(_) => { eprintln!("[mouseclaw] 🔤 selection panic — skip"); continue; }
            Ok(SelResult::NoFocused(code)) => {
                diag(format!("AXFocusedUIElement 失败 (err={code}) — 前台 app 可能不支持 AX 或没聚焦文本框"));
                last_text = None; continue;
            }
            Ok(SelResult::NoSelectedAttr(code)) => {
                diag(format!("AXSelectedText 不可用 (err={code}) — 该 app 不暴露选区（Chrome/Electron/VSCode 类 web app 常见，属已知盲区）"));
                last_text = None; continue;
            }
            Ok(SelResult::EmptySelection) => { last_text = None; continue; }
            Ok(SelResult::Text(t)) => t,
        };

        let n = text.chars().count();
        if n < MIN_SELECTION_CHARS {
            diag(format!("选区 {n} 字 < {MIN_SELECTION_CHARS} 阈值，跳过"));
            continue;
        }
        println!("[mouseclaw] 🔤 selection detected: {n} chars");

        // 同内容 + cooldown 内 → skip
        if last_text.as_deref() == Some(text.as_str()) {
            if let Some(when) = last_emit_at {
                if when.elapsed() < Duration::from_secs(COOLDOWN_SECS) {
                    continue;
                }
            }
        }

        last_text = Some(text.clone());
        last_emit_at = Some(Instant::now());

        let (bundle, _name) = crate::clipboard::frontmost_app_pub();
        crate::reactive::on_new_text(
            crate::reactive::Source::Selection,
            &text,
            &bundle,
        );
    }
}

// ───────────────────────── Accessibility FFI ─────────────────────────

#[cfg(target_os = "macos")]
mod ax {
    use core_foundation::base::CFTypeRef;
    use core_foundation::string::CFStringRef;

    pub type AXUIElementRef = *const std::ffi::c_void;
    pub type AXError = i32;
    pub const K_AX_ERROR_SUCCESS: AXError = 0;

    #[link(name = "ApplicationServices", kind = "framework")]
    extern "C" {
        pub fn AXUIElementCreateSystemWide() -> AXUIElementRef;
        pub fn AXUIElementCopyAttributeValue(
            element: AXUIElementRef,
            attribute: CFStringRef,
            value: *mut CFTypeRef,
        ) -> AXError;
    }
}

/// 选区抓取结果 —— 带诊断原因，loop 节流打印帮定位"选词不触发"卡在哪步。
#[cfg(target_os = "macos")]
enum SelResult {
    /// 成功拿到非空选区
    Text(String),
    /// AXFocusedUIElement 失败（err 码）—— 通常是 app 不支持 AX 或没真正聚焦元素
    NoFocused(i32),
    /// 有 focused 但 AXSelectedText 失败（err 码）—— 该 app 不暴露选区属性
    /// （Chrome / Electron / VSCode 等 web 系常见 = -25205 kAXErrorAttributeUnsupported）
    NoSelectedAttr(i32),
    /// 拿到属性但选区为空 / 没选东西
    EmptySelection,
}

/// 拉当前 focused 元素的选区文字（带诊断）。
#[cfg(target_os = "macos")]
fn current_selection_diag() -> SelResult {
    use cocoa::base::{id, nil};
    use objc::{class, msg_send, sel, sel_impl};

    unsafe {
        let pool: id = msg_send![class!(NSAutoreleasePool), new];
        let result = (|| -> SelResult {
            let systemwide = ax::AXUIElementCreateSystemWide();
            if systemwide.is_null() { return SelResult::NoFocused(-1); }

            let focused_attr = CFString::new("AXFocusedUIElement");
            let mut focused: CFTypeRef = std::ptr::null();
            let err = ax::AXUIElementCopyAttributeValue(
                systemwide,
                focused_attr.as_concrete_TypeRef(),
                &mut focused,
            );
            if err != ax::K_AX_ERROR_SUCCESS || focused.is_null() {
                return SelResult::NoFocused(err);
            }

            let sel_attr = CFString::new("AXSelectedText");
            let mut sel_value: CFTypeRef = std::ptr::null();
            let err2 = ax::AXUIElementCopyAttributeValue(
                focused as ax::AXUIElementRef,
                sel_attr.as_concrete_TypeRef(),
                &mut sel_value,
            );
            CFRelease(focused);

            if err2 != ax::K_AX_ERROR_SUCCESS || sel_value.is_null() {
                return SelResult::NoSelectedAttr(err2);
            }
            let s = cfstring_to_string(sel_value as CFStringRef);
            CFRelease(sel_value);
            match s.filter(|t| !t.is_empty()) {
                Some(t) => SelResult::Text(t),
                None => SelResult::EmptySelection,
            }
        })();
        let _: () = msg_send![pool, drain];
        result
    }
}


#[cfg(target_os = "macos")]
fn cfstring_to_string(cf: CFStringRef) -> Option<String> {
    if cf.is_null() { return None; }
    // 注意：CFString::wrap_under_get_rule retain，但这里我们正在退出前 release，
    // 所以用 wrap_under_get_rule —— wrap_under_create_rule 会 over-release。
    let cfs = unsafe { CFString::wrap_under_get_rule(cf) };
    Some(cfs.to_string())
}

#[cfg(not(target_os = "macos"))]
#[allow(dead_code)]
fn current_selection_text() -> Option<String> { None }

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paused_toggle() {
        set_paused(true);
        assert!(is_paused());
        set_paused(false);
        assert!(!is_paused());
    }

    #[test]
    fn min_chars_consistent() {
        assert!(MIN_SELECTION_CHARS >= 20, "应至少 20 字符避免双击单词触发");
    }

    #[test]
    fn cooldown_reasonable() {
        assert!(COOLDOWN_SECS >= 5);
        assert!(COOLDOWN_SECS <= 60);
    }
}
