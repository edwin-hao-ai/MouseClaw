//! 前台 app 救援（v0.1.18）
//!
//! 让用户从 Hub 选条目后，**字真的写到他之前在打字的那个输入框**。
//!
//! 问题：Hub 是独立 Tauri 窗口，open 时夺焦点；hide 时 macOS 「应该」把焦点
//! 还回去，但实际上经常还回到我们自己的 overlay/任意其它 app，光标不在原输入框。
//!
//! 解：open Hub 前，记下当时 frontmost 的 pid；用户点条目时：
//!   1. hide Hub
//!   2. 等 80ms
//!   3. 调 NSRunningApplication.activate() 把那个 pid 显式拉到前台
//!   4. 再等 80ms
//!   5. 合成 ⌘V

#![cfg(target_os = "macos")]

use cocoa::base::{id, nil};
use objc::{class, msg_send, sel, sel_impl};

/// 当前前台 app 的 pid（macOS）。失败返 None。
pub fn current_frontmost_pid() -> Option<i32> {
    unsafe {
        let pool: id = msg_send![class!(NSAutoreleasePool), new];
        let result = (|| -> Option<i32> {
            let ws: id = msg_send![class!(NSWorkspace), sharedWorkspace];
            if ws == nil { return None; }
            let app: id = msg_send![ws, frontmostApplication];
            if app == nil { return None; }
            let pid: i32 = msg_send![app, processIdentifier];
            if pid <= 0 { None } else { Some(pid) }
        })();
        if pool != nil { let _: () = msg_send![pool, drain]; }
        result
    }
}

/// 把指定 pid 的 app 强制拉到前台。
/// 用 `NSRunningApplication runningApplicationWithProcessIdentifier:` 拿 instance，
/// 再 `activateWithOptions:NSApplicationActivateIgnoringOtherApps (= 2)`。
/// 返 true = 成功调用（不保证 app 真的活了；有些 app 会拒绝）。
pub fn activate_pid(pid: i32) -> bool {
    unsafe {
        let pool: id = msg_send![class!(NSAutoreleasePool), new];
        let ok = (|| -> bool {
            let cls = class!(NSRunningApplication);
            let app: id = msg_send![cls, runningApplicationWithProcessIdentifier: pid];
            if app == nil { return false; }
            // NSApplicationActivateIgnoringOtherApps = 1 << 1 = 2
            let activated: bool = msg_send![app, activateWithOptions: 2u64];
            activated
        })();
        if pool != nil { let _: () = msg_send![pool, drain]; }
        ok
    }
}
