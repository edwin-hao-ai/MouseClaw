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

/// 一次截图的结果。
pub struct CaptureResult {
    pub path: PathBuf,
    /// 光标在截图坐标系（top-left origin, screen-local logical points）里的位置。
    /// None 表示找不到光标所在屏幕（单屏 fallback 或多屏切换瞬间）。
    pub cursor: Option<(i32, i32)>,
    /// 截图覆盖的屏幕尺寸（logical points）。
    pub screen_size: Option<(i32, i32)>,
}

#[cfg(target_os = "macos")]
pub async fn capture_main_screen() -> Result<CaptureResult> {
    let ts = chrono::Local::now().format("%Y%m%d-%H%M%S-%3f").to_string();
    let path = std::env::temp_dir().join(format!("mouseclaw-frame-{ts}.png"));

    let (display_id, cursor_local, screen_size) = cursor_display_info();

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
            "[mouseclaw] screenshot: {} ({:.1} KB, display={}, cursor={:?}, screen={:?})",
            path.display(),
            meta.len() as f64 / 1024.0,
            display_id.map(|id| id.to_string()).unwrap_or_else(|| "main".into()),
            cursor_local,
            screen_size,
        );
    }
    Ok(CaptureResult { path, cursor: cursor_local, screen_size })
}

/// 返回 (CGDirectDisplayID, 光标局部坐标 top-left, 屏幕尺寸 logical points)
/// 三个值的 Option 三元组——None 表示找不到 / 单屏 fallback。
#[cfg(target_os = "macos")]
fn cursor_display_info() -> (Option<u32>, Option<(i32, i32)>, Option<(i32, i32)>) {
    use cocoa::base::nil;
    use cocoa::foundation::{NSArray, NSPoint, NSRect, NSString};
    use objc::{class, msg_send, sel, sel_impl};
    unsafe {
        let event_class: cocoa::base::id = msg_send![class!(NSEvent), class];
        let cursor: NSPoint = msg_send![event_class, mouseLocation];
        let screens: cocoa::base::id = msg_send![class!(NSScreen), screens];
        if screens == nil { return (None, None, None); }
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
                let device_desc: cocoa::base::id = msg_send![screen, deviceDescription];
                if device_desc == nil { continue; }
                let num: cocoa::base::id = msg_send![device_desc, objectForKey: key_str];
                if num == nil { continue; }
                let id: u32 = msg_send![num, unsignedIntValue];
                // Convert global cursor → screen-local top-left coords (logical points)
                let local_x = (cursor.x - frame.origin.x) as i32;
                // Flip Y axis (NSScreen is bottom-left origin)
                let local_y = (frame.size.height - (cursor.y - frame.origin.y)) as i32;
                let size = (frame.size.width as i32, frame.size.height as i32);
                return (Some(id), Some((local_x, local_y)), Some(size));
            }
        }
        (None, None, None)
    }
}

// ───────────────────── 非 macOS：xcap 截图 ─────────────────────
// 光标所在屏（拿得到光标时）整图；Wayland 拿不到光标 → 主屏。xcap 在 Wayland 走
// portal/wlr-screencopy，首次可能弹授权框（onboarding 已说明，见跨平台移植 doc）。
#[cfg(not(target_os = "macos"))]
pub async fn capture_main_screen() -> Result<CaptureResult> {
    let ts = chrono::Local::now().format("%Y%m%d-%H%M%S-%3f").to_string();
    let path = std::env::temp_dir().join(format!("mouseclaw-frame-{ts}.png"));
    let cursor = crate::platform::global_cursor();
    let path2 = path.clone();
    // xcap capture 是阻塞调用 —— 丢到 blocking 池，别堵 async runtime。
    tokio::task::spawn_blocking(move || capture_via_xcap(&path2, cursor))
        .await
        .context("screenshot task join")?
}

#[cfg(not(target_os = "macos"))]
fn capture_via_xcap(path: &std::path::Path, cursor: Option<(f64, f64)>) -> Result<CaptureResult> {
    use xcap::Monitor;
    let monitor = match cursor {
        Some((cx, cy)) => Monitor::from_point(cx as i32, cy as i32)
            .or_else(|_| primary_monitor())
            .map_err(|e| anyhow::anyhow!("xcap monitor from point: {e}"))?,
        None => primary_monitor().map_err(|e| anyhow::anyhow!("xcap primary monitor: {e}"))?,
    };
    let mx = monitor.x().unwrap_or(0);
    let my = monitor.y().unwrap_or(0);
    let mw = monitor.width().unwrap_or(0) as i32;
    let mh = monitor.height().unwrap_or(0) as i32;
    let img = monitor
        .capture_image()
        .map_err(|e| anyhow::anyhow!("xcap capture: {e}"))?;
    img.save(path).context("save screenshot png")?;
    let cursor_local = cursor.map(|(cx, cy)| (cx as i32 - mx, cy as i32 - my));
    if let Ok(meta) = std::fs::metadata(path) {
        println!(
            "[mouseclaw] screenshot(xcap): {} ({:.1} KB, cursor={:?}, screen={:?})",
            path.display(),
            meta.len() as f64 / 1024.0,
            cursor_local,
            (mw, mh),
        );
    }
    Ok(CaptureResult {
        path: path.to_path_buf(),
        cursor: cursor_local,
        screen_size: Some((mw, mh)),
    })
}

/// 主屏（is_primary）；找不到就取第一块。
#[cfg(not(target_os = "macos"))]
fn primary_monitor() -> xcap::XCapResult<xcap::Monitor> {
    let all = xcap::Monitor::all()?;
    let mut chosen: Option<xcap::Monitor> = None;
    for m in all {
        if m.is_primary().unwrap_or(false) {
            return Ok(m);
        }
        if chosen.is_none() {
            chosen = Some(m);
        }
    }
    chosen.ok_or_else(|| xcap::XCapError::new("no monitor found"))
}

#[cfg(not(target_os = "macos"))]
#[allow(dead_code)]
fn cursor_display_info() -> (Option<u32>, Option<(i32, i32)>, Option<(i32, i32)>) {
    (None, None, None)
}
