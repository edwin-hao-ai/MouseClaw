//! macOS 权限检查与请求。
//!
//! 设计原则：**只用官方状态查询 API，绝不读 TCC.db**。
//!
//! 上一版踩的坑：spawn sqlite3 读 `~/Library/Application Support/com.apple.TCC/TCC.db`
//! —— 这文件被 SIP 保护，普通 .app（没有「完全磁盘访问」）读不了，永远失败。
//! 而且要读它就得申请「完全磁盘访问」，比要查的权限本身还吓人，本末倒置。
//!
//! 正确做法 —— 每个权限都有官方的「查询」和「请求」API：
//!   - Accessibility: `AXIsProcessTrusted()` 查 / `AXIsProcessTrustedWithOptions(prompt)` 请求
//!   - Screen Recording: `CGPreflightScreenCaptureAccess()` 查 / `CGRequestScreenCaptureAccess()` 请求
//!   - Microphone: `AVCaptureDevice authorizationStatusForMediaType:` 查 /
//!                 `requestAccessForMediaType:completionHandler:` 请求
//!
//! ⚠️ macOS 设计约束：**屏幕录制授权后必须重启 app 才生效**。
//!    `CGPreflightScreenCaptureAccess()` 在重启前一直返回 false —— 这不是 bug，
//!    是系统行为。Onboarding 流程必须包含「重启」步骤。

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct PermissionStatus {
    pub accessibility: bool,
    pub screen_recording: bool,
    pub microphone: bool,
}

impl PermissionStatus {
    pub fn all_granted(&self) -> bool {
        self.accessibility && self.screen_recording && self.microphone
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// macOS 实现
// ─────────────────────────────────────────────────────────────────────────────

// 强制链接 AVFoundation framework —— 这样 `class!(AVCaptureDevice)` 在运行时
// 能解析到。空 extern block + #[link] 只是告诉 linker 加 `-framework AVFoundation`。
#[cfg(target_os = "macos")]
#[link(name = "AVFoundation", kind = "framework")]
extern "C" {}

#[cfg(target_os = "macos")]
pub fn check_all() -> PermissionStatus {
    let s = PermissionStatus {
        accessibility: check_accessibility(),
        screen_recording: check_screen_recording(),
        microphone: check_microphone(),
    };
    println!(
        "[mouseclaw] perm check → accessibility={} screen_recording={} microphone={}",
        s.accessibility, s.screen_recording, s.microphone
    );
    s
}

// ── Accessibility ────────────────────────────────────────────────────────────

/// `AXIsProcessTrusted()` — 纯只读查询，不弹窗、不阻塞。
#[cfg(target_os = "macos")]
pub fn check_accessibility() -> bool {
    extern "C" {
        fn AXIsProcessTrusted() -> bool;
    }
    unsafe { AXIsProcessTrusted() }
}

/// `AXIsProcessTrustedWithOptions({kAXTrustedCheckOptionPrompt: true})` —
/// 如果还没授权，会弹出系统对话框引导用户去「系统设置 → 辅助功能」。
/// 返回当前是否已授权（首次调用通常 false，弹窗后用户去授权）。
#[cfg(target_os = "macos")]
pub fn request_accessibility() -> bool {
    use core_foundation::base::TCFType;
    use core_foundation::boolean::CFBoolean;
    use core_foundation::dictionary::{CFDictionary, CFDictionaryRef};
    use core_foundation::string::{CFString, CFStringRef};

    extern "C" {
        static kAXTrustedCheckOptionPrompt: CFStringRef;
        fn AXIsProcessTrustedWithOptions(options: CFDictionaryRef) -> bool;
    }
    unsafe {
        let key = CFString::wrap_under_get_rule(kAXTrustedCheckOptionPrompt);
        let val = CFBoolean::true_value();
        let dict = CFDictionary::from_CFType_pairs(&[(key.as_CFType(), val.as_CFType())]);
        AXIsProcessTrustedWithOptions(dict.as_concrete_TypeRef())
    }
}

// ── Screen Recording ─────────────────────────────────────────────────────────

/// `CGPreflightScreenCaptureAccess()` — 只查询不弹窗。
/// macOS 11+。返回 true = 已授权且本进程可用。
///
/// ⚠️ 授权后必须重启 app —— 重启前这里一直 false，是系统行为不是 bug。
#[cfg(target_os = "macos")]
pub fn check_screen_recording() -> bool {
    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        fn CGPreflightScreenCaptureAccess() -> bool;
    }
    unsafe { CGPreflightScreenCaptureAccess() }
}

/// `CGRequestScreenCaptureAccess()` — 触发系统授权弹窗
/// （"MouseClaw 想要录制此电脑的屏幕"）。
/// 用户在弹窗里点「打开系统设置」→ 勾选 MouseClaw。
/// 返回值是请求时的状态（通常 false，要重启 app 才会变 true）。
#[cfg(target_os = "macos")]
pub fn request_screen_recording() -> bool {
    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        fn CGRequestScreenCaptureAccess() -> bool;
    }
    unsafe { CGRequestScreenCaptureAccess() }
}

// ── Microphone ───────────────────────────────────────────────────────────────

/// `[AVCaptureDevice authorizationStatusForMediaType:AVMediaTypeAudio]`
/// 纯状态查询 —— 不初始化 CoreAudio，不死锁。
/// AVAuthorizationStatus: 0=NotDetermined 1=Restricted 2=Denied 3=Authorized
///
/// ⚠️ 不链接 `AVMediaTypeAudio` extern 常量（上一版踩坑：链接失败导致传进去
///    的是垃圾指针，status 永远返回 0）。AVMediaTypeAudio 的实际值就是字符串
///    "soun" —— 直接构造 NSString 传进去，等价且无链接风险。
#[cfg(target_os = "macos")]
pub fn check_microphone() -> bool {
    use cocoa::base::nil;
    use cocoa::foundation::NSString;
    use objc::runtime::Class;
    use objc::{msg_send, sel, sel_impl};
    unsafe {
        // Class::get 而不是 class!() —— 后者类不存在会 panic
        let Some(cls) = Class::get("AVCaptureDevice") else {
            eprintln!("[mouseclaw] ✘ AVCaptureDevice class 未找到 — AVFoundation 没链上？");
            return false;
        };
        let media_type = NSString::alloc(nil).init_str("soun"); // == AVMediaTypeAudio
        let status: i64 = msg_send![cls, authorizationStatusForMediaType: media_type];
        status == 3 // Authorized
    }
}

/// `[AVCaptureDevice requestAccessForMediaType:AVMediaTypeAudio completionHandler:]`
/// 触发系统麦克风授权弹窗。completionHandler 是个空 block（靠前端轮询
/// check_permissions 拿结果，不依赖回调）。
#[cfg(target_os = "macos")]
pub fn request_microphone() {
    use block::ConcreteBlock;
    use cocoa::base::nil;
    use cocoa::foundation::NSString;
    use objc::runtime::Class;
    use objc::{msg_send, sel, sel_impl};
    unsafe {
        let Some(cls) = Class::get("AVCaptureDevice") else {
            eprintln!("[mouseclaw] ✘ AVCaptureDevice class 未找到，无法请求麦克风权限");
            return;
        };
        let media_type = NSString::alloc(nil).init_str("soun"); // == AVMediaTypeAudio
        let handler = ConcreteBlock::new(|_granted: bool| {});
        let handler = handler.copy();
        let _: () = msg_send![
            cls,
            requestAccessForMediaType: media_type
            completionHandler: handler
        ];
    }
}

// ── 打开系统设置面板（手动兜底用）────────────────────────────────────────────

/// 根据权限名称打开对应系统设置面板（手动兜底，主要靠 request_* 弹窗）。
#[cfg(target_os = "macos")]
pub fn open_prefs_for(name: &str) {
    let url = match name {
        "accessibility" =>
            "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility",
        "screen_recording" =>
            "x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture",
        "microphone" =>
            "x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone",
        _ => return,
    };
    let _ = std::process::Command::new("/usr/bin/open").arg(url).spawn();
}

/// 触发指定权限的系统授权弹窗 + 打开对应设置面板。
/// 这是 Onboarding「去开启」按钮真正该调的东西。
#[cfg(target_os = "macos")]
pub fn request_permission(name: &str) {
    match name {
        "accessibility" => { request_accessibility(); }
        "screen_recording" => { request_screen_recording(); }
        "microphone" => { request_microphone(); }
        _ => {}
    }
    // 同时打开设置面板 —— request_* 的弹窗有时会被忽略，面板让用户能手动勾
    open_prefs_for(name);
}

// ─────────────────────────────────────────────────────────────────────────────
// 非 macOS 平台（全部返回 true / no-op）
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(not(target_os = "macos"))]
pub fn check_all() -> PermissionStatus {
    PermissionStatus { accessibility: true, screen_recording: true, microphone: true }
}
#[cfg(not(target_os = "macos"))]
pub fn check_accessibility() -> bool { true }
#[cfg(not(target_os = "macos"))]
pub fn check_screen_recording() -> bool { true }
#[cfg(not(target_os = "macos"))]
pub fn check_microphone() -> bool { true }
#[cfg(not(target_os = "macos"))]
pub fn open_prefs_for(_name: &str) {}
#[cfg(not(target_os = "macos"))]
pub fn request_permission(_name: &str) {}
