//! macOS 权限检查与请求。
//!
//! 三种权限：
//!   1. Accessibility（辅助功能）— 全局快捷键必须
//!   2. Screen Recording（屏幕录制）— screencapture 必须
//!   3. Microphone（麦克风）— cpal 录音必须
//!
//! 全部用 macOS 原生 C API，不依赖外部进程（swift/osascript 在 release 包
//! 的受限 PATH 下不可靠，且 osascript 本身需要 Accessibility 权限，会死循环）。

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

#[cfg(target_os = "macos")]
pub fn check_all() -> PermissionStatus {
    PermissionStatus {
        accessibility:    check_accessibility(),
        screen_recording: check_screen_recording(),
        microphone:       check_microphone(),
    }
}

/// Accessibility: `AXIsProcessTrusted()` — 官方 API，只查询不弹窗。
#[cfg(target_os = "macos")]
pub fn check_accessibility() -> bool {
    #[link(name = "ApplicationServices", kind = "framework")]
    extern "C" {
        fn AXIsProcessTrusted() -> bool;
    }
    unsafe { AXIsProcessTrusted() }
}

/// Screen Recording: `CGPreflightScreenCaptureAccess()` (macOS 11+)
/// 只查询不弹窗，返回 true = 已授权。
#[cfg(target_os = "macos")]
pub fn check_screen_recording() -> bool {
    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        fn CGPreflightScreenCaptureAccess() -> bool;
    }
    unsafe { CGPreflightScreenCaptureAccess() }
}

/// Microphone: `AVAuthorizationStatusAuthorized = 3`
/// 通过 Objective-C runtime 调用 AVCaptureDevice，不引入额外 crate。
#[cfg(target_os = "macos")]
pub fn check_microphone() -> bool {
    use objc::{class, msg_send, sel, sel_impl};
    use objc::runtime::Object;

    // AVMediaTypeAudio = "soun"
    let media_type = unsafe {
        let cls = class!(NSString);
        let s: *mut Object = msg_send![cls, stringWithUTF8String: b"soun\0".as_ptr()];
        s
    };

    // [AVCaptureDevice authorizationStatusForMediaType:] → i64
    // 0=NotDetermined, 1=Restricted, 2=Denied, 3=Authorized
    let status: i64 = unsafe {
        let cls = class!(AVCaptureDevice);
        msg_send![cls, authorizationStatusForMediaType: media_type]
    };

    status == 3 // Authorized
}

// ─────────────────────────────────────────────────────────────────────────────
// 打开系统偏好设置面板（引导用户手动授权）
// ─────────────────────────────────────────────────────────────────────────────

/// 根据权限名称打开对应系统设置面板。
/// name: "accessibility" | "screen_recording" | "microphone"
#[cfg(target_os = "macos")]
pub fn open_prefs_for(name: &str) {
    let url = match name {
        "accessibility"    => "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility",
        "screen_recording" => "x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture",
        "microphone"       => "x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone",
        _ => return,
    };
    let _ = std::process::Command::new("open").arg(url).spawn();
}

// ─────────────────────────────────────────────────────────────────────────────
// 非 macOS 平台（全部返回 true）
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
