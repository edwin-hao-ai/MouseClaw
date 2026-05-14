//! macOS 权限检查与请求。
//!
//! 三种权限：
//!   1. Accessibility（辅助功能）— 全局快捷键必须
//!   2. Screen Recording（屏幕录制）— screencapture 必须
//!   3. Microphone（麦克风）— cpal 录音必须
//!
//! 重要：所有检查函数必须是非阻塞的——不能触发系统权限弹窗。
//! AVCaptureDevice authorizationStatusForMediaType: 在 NotDetermined 状态下
//! 会同步弹窗阻塞线程，所以麦克风检查改用 cpal 枚举设备的方式。

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

/// Microphone: 用 cpal 尝试枚举默认输入设备。
///
/// 为什么不用 AVCaptureDevice.authorizationStatus：
///   当状态是 NotDetermined(0) 时，该 API 会同步弹出权限对话框并阻塞调用线程，
///   在 Tauri 主线程上调用会导致整个 UI 卡死。
///
/// cpal 的 default_input_device() 在没有麦克风权限时返回 None，
/// 且不会触发系统弹窗，是安全的只读检查。
#[cfg(target_os = "macos")]
pub fn check_microphone() -> bool {
    use cpal::traits::HostTrait;
    let host = cpal::default_host();
    host.default_input_device().is_some()
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
