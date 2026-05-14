//! 截屏：调 macOS 自带的 `screencapture` CLI 抓**光标所在屏幕的完整画面**。
//!
//! 用户原话拒绝了 300×300 局部："如果是针对一个网站呢，300*300啥也看不明白，
//! 反正都是上传一个图片了"。所以这里抓的是整张屏幕。
//!
//! 多显示器：检测光标在哪块屏幕上，用 `screencapture -D <displayID>` 抓那张。
//! 否则用户在副屏操作时，AI 拿到的会是空荡荡的主屏。
//!
//! `-x` 静音（不出快门声音）
//! `-o` 不包含窗口阴影
//! `-D <id>` 指定 CGDirectDisplayID
//! 不加 `-i` / `-W`：那些是交互式的，会卡住 daemon。

use std::path::PathBuf;
use anyhow::{bail, Context, Result};

pub async fn capture_main_screen() -> Result<PathBuf> {
    let ts = chrono::Local::now().format("%Y%m%d-%H%M%S-%3f").to_string();
    let path = std::env::temp_dir().join(format!("mouseclaw-frame-{ts}.png"));

    let display_id = cursor_display_id();

    let mut cmd = tokio::process::Command::new("screencapture");
    cmd.args(["-x", "-o"]);
    if let Some(id) = display_id {
        cmd.args(["-D", &id.to_string()]);
    }
    let status = cmd
        .arg(&path)
        .status()
        .await
        .context("failed to spawn `screencapture`")?;

    if !status.success() {
        bail!("`screencapture` exited with status {status}");
    }
    if !path.exists() {
        bail!("screencapture reported success but no file at {}", path.display());
    }
    if let Ok(meta) = std::fs::metadata(&path) {
        println!(
            "[mouseclaw] screenshot: {} ({:.1} KB, display={})",
            path.display(),
            meta.len() as f64 / 1024.0,
            display_id.map(|id| id.to_string()).unwrap_or_else(|| "main".into())
        );
    }
    Ok(path)
}

/// Return the CGDirectDisplayID of the screen the mouse cursor is currently on.
/// Falls back to None (→ main display) if NSEvent / NSScreen can't be queried.
///
/// NSScreen.frame is in screen coordinates with bottom-left origin. We iterate
/// all NSScreens and find the one whose frame contains the cursor position.
#[cfg(target_os = "macos")]
fn cursor_display_id() -> Option<u32> {
    use cocoa::base::nil;
    use cocoa::foundation::{NSArray, NSPoint, NSRect, NSString};
    use objc::{class, msg_send, sel, sel_impl};
    unsafe {
        // 1. Get cursor location (bottom-left origin)
        let event_class: cocoa::base::id = msg_send![class!(NSEvent), class];
        let cursor: NSPoint = msg_send![event_class, mouseLocation];

        // 2. Iterate NSScreen.screens to find the one containing the cursor
        let screens: cocoa::base::id = msg_send![class!(NSScreen), screens];
        if screens == nil { return None; }
        let count: usize = msg_send![screens, count];
        let key_str = NSString::alloc(nil).init_str("NSScreenNumber");
        for i in 0..count {
            let screen: cocoa::base::id = screens.objectAtIndex(i as u64);
            if screen == nil { continue; }
            let frame: NSRect = msg_send![screen, frame];
            let in_x = cursor.x >= frame.origin.x
                && cursor.x < frame.origin.x + frame.size.width;
            let in_y = cursor.y >= frame.origin.y
                && cursor.y < frame.origin.y + frame.size.height;
            if in_x && in_y {
                // Pull CGDirectDisplayID from deviceDescription["NSScreenNumber"]
                let device_desc: cocoa::base::id = msg_send![screen, deviceDescription];
                if device_desc == nil { continue; }
                let num: cocoa::base::id = msg_send![device_desc, objectForKey: key_str];
                if num == nil { continue; }
                let id: u32 = msg_send![num, unsignedIntValue];
                return Some(id);
            }
        }
        None
    }
}

#[cfg(not(target_os = "macos"))]
fn cursor_display_id() -> Option<u32> { None }
