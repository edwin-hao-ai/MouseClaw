//! macOS 权限检查与请求。
//!
//! 三种权限：
//!   1. Accessibility（辅助功能）— 全局快捷键必须
//!   2. Screen Recording（屏幕录制）— screencapture 必须
//!   3. Microphone（麦克风）— cpal 录音必须
//!
//! 检测方式：
//!   - Accessibility: `AXIsProcessTrusted()` C API，最准确
//!   - Screen Recording: `CGPreflightScreenCaptureAccess()` (macOS 11+)
//!   - Microphone: `AVCaptureDevice.authorizationStatus` 通过 osascript 间接查询
//!     （避免引入 AVFoundation 绑定依赖）

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
        accessibility: check_accessibility(),
        screen_recording: check_screen_recording(),
        microphone: check_microphone(),
    }
}

/// Accessibility: 调用 `AXIsProcessTrusted()` — 这是 macOS 官方 API，
/// 不会弹出权限对话框，只是查询当前状态。
#[cfg(target_os = "macos")]
pub fn check_accessibility() -> bool {
    use std::process::Command;
    // AXIsProcessTrusted 通过 osascript 间接调用，避免 unsafe FFI
    // 更可靠的方式：直接用 C FFI
    extern "C" {
        fn AXIsProcessTrusted() -> bool;
    }
    unsafe { AXIsProcessTrusted() }
}

/// Screen Recording: `CGPreflightScreenCaptureAccess()` (macOS 11+)
/// 返回 true = 已授权，false = 未授权（不会弹对话框）。
#[cfg(target_os = "macos")]
pub fn check_screen_recording() -> bool {
    extern "C" {
        fn CGPreflightScreenCaptureAccess() -> bool;
    }
    unsafe { CGPreflightScreenCaptureAccess() }
}

/// Microphone: 通过 `AVCaptureDevice` 查询授权状态。
/// 0 = NotDetermined, 1 = Restricted, 2 = Denied, 3 = Authorized
#[cfg(target_os = "macos")]
pub fn check_microphone() -> bool {
    use std::process::Command;
    // 用 swift 单行脚本查询，不引入 AVFoundation 绑定
    let out = Command::new("swift")
        .args(["-e", "import AVFoundation; print(AVCaptureDevice.authorizationStatus(for: .audio).rawValue)"])
        .output();
    match out {
        Ok(o) if o.status.success() => {
            let s = String::from_utf8_lossy(&o.stdout);
            s.trim() == "3" // 3 = Authorized
        }
        _ => {
            // swift 不可用时 fallback：尝试打开默认输入设备
            // cpal 会触发系统权限请求，这里只是检查
            check_microphone_via_cpal()
        }
    }
}

/// Fallback：用 cpal 尝试枚举输入设备来判断麦克风权限。
/// 注意：在 macOS 上，如果权限未授予，`default_input_device()` 返回 None。
#[cfg(target_os = "macos")]
fn check_microphone_via_cpal() -> bool {
    use cpal::traits::HostTrait;
    let host = cpal::default_host();
    host.default_input_device().is_some()
}

// ─────────────────────────────────────────────────────────────────────────────
// 打开系统偏好设置面板（引导用户手动授权）
// ─────────────────────────────────────────────────────────────────────────────

/// 打开「系统设置 → 隐私与安全性 → 辅助功能」。
#[cfg(target_os = "macos")]
pub fn open_accessibility_prefs() {
    let _ = std::process::Command::new("open")
        .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility")
        .spawn();
}

/// 打开「系统设置 → 隐私与安全性 → 屏幕录制」。
#[cfg(target_os = "macos")]
pub fn open_screen_recording_prefs() {
    let _ = std::process::Command::new("open")
        .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture")
        .spawn();
}

/// 打开「系统设置 → 隐私与安全性 → 麦克风」。
#[cfg(target_os = "macos")]
pub fn open_microphone_prefs() {
    let _ = std::process::Command::new("open")
        .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone")
        .spawn();
}

/// 根据权限名称打开对应面板。name: "accessibility" | "screen_recording" | "microphone"
#[cfg(target_os = "macos")]
pub fn open_prefs_for(name: &str) {
    match name {
        "accessibility"    => open_accessibility_prefs(),
        "screen_recording" => open_screen_recording_prefs(),
        "microphone"       => open_microphone_prefs(),
        _ => {}
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// 非 macOS 平台（全部返回 true，不做任何事）
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
