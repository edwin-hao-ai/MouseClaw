//! macOS 权限检查与请求。
//!
//! 只检查可以安全查询的权限（不触发系统弹窗、不阻塞线程）：
//!   - Accessibility: AXIsProcessTrusted() — 纯只读，零副作用
//!   - Screen Recording: CGPreflightScreenCaptureAccess() — 纯只读，零副作用
//!   - Microphone: 读 TCC 数据库（SQLite），完全不涉及 CoreAudio/AVFoundation
//!
//! 不用 cpal / AVCaptureDevice：两者在 release 包里首次调用都可能触发
//! CoreAudio 初始化，在没有麦克风权限时会死锁。

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
        microphone:       check_microphone_tcc(),
    }
}

/// Accessibility: `AXIsProcessTrusted()` — 官方 API，只查询不弹窗，不阻塞。
#[cfg(target_os = "macos")]
pub fn check_accessibility() -> bool {
    #[link(name = "ApplicationServices", kind = "framework")]
    extern "C" {
        fn AXIsProcessTrusted() -> bool;
    }
    unsafe { AXIsProcessTrusted() }
}

/// Screen Recording: `CGPreflightScreenCaptureAccess()` (macOS 11+)
/// 只查询不弹窗，不阻塞。
#[cfg(target_os = "macos")]
pub fn check_screen_recording() -> bool {
    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        fn CGPreflightScreenCaptureAccess() -> bool;
    }
    unsafe { CGPreflightScreenCaptureAccess() }
}

/// Microphone: 读 TCC（Transparency, Consent, and Control）数据库。
///
/// TCC 数据库位于 ~/Library/Application Support/com.apple.TCC/TCC.db
/// 表 access 里 service='kTCCServiceMicrophone', client=bundle_id, auth_value=2 表示已授权。
///
/// 这是纯文件读取，完全不涉及 CoreAudio/AVFoundation，不会阻塞。
/// 如果读取失败（权限不足或数据库不存在），返回 false（保守策略）。
#[cfg(target_os = "macos")]
pub fn check_microphone_tcc() -> bool {
    let home = match std::env::var("HOME") {
        Ok(h) => h,
        Err(_) => return false,
    };
    let db_path = format!(
        "{}/Library/Application Support/com.apple.TCC/TCC.db",
        home
    );

    // 用 sqlite3 CLI 查询（macOS 自带，在 /usr/bin/sqlite3，release 包 PATH 里有）
    let bundle_id = "com.edwinhao.mouseclaw";
    let query = format!(
        "SELECT auth_value FROM access WHERE service='kTCCServiceMicrophone' AND client='{}' LIMIT 1;",
        bundle_id
    );
    let output = std::process::Command::new("/usr/bin/sqlite3")
        .arg(&db_path)
        .arg(&query)
        .output();

    match output {
        Ok(o) if o.status.success() => {
            let s = String::from_utf8_lossy(&o.stdout);
            let val = s.trim();
            // auth_value: 0=Denied, 2=Allowed (macOS 12+), 1=Allowed (older)
            val == "2" || val == "1"
        }
        _ => false,
    }
}

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
    let _ = std::process::Command::new("/usr/bin/open").arg(url).spawn();
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
pub fn check_microphone_tcc() -> bool { true }

#[cfg(not(target_os = "macos"))]
pub fn open_prefs_for(_name: &str) {}
