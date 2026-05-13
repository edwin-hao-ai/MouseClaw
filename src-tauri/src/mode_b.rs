//! Mode B — write text to the foreground app's cursor.
//!
//! V1 strategy: save current clipboard → put new text → simulate Cmd+V → restore
//! original clipboard (with ~100ms pollution window). V2 will switch to direct
//! CGEvent keystrokes per CLAUDE.md.
//!
//! Safety red lines enforced here:
//!   • If frontmost app is a terminal (Terminal/iTerm/Warp/Alacritty), refuse.
//!   • If user is mid-interaction (we trust caller's countdown UI for this).

use anyhow::{bail, Context, Result};

const BLOCKED_BUNDLES: &[&str] = &[
    "com.apple.Terminal",
    "com.googlecode.iterm2",
    "dev.warp.Warp-Stable",
    "co.zeit.hyper",
    "io.alacritty",
    "org.alacritty",
    "net.kovidgoyal.kitty",
];

const BLOCKED_NAMES: &[&str] = &["Terminal", "iTerm", "iTerm2", "Warp", "Hyper", "Alacritty", "kitty"];

/// Detect whether write-back is allowed for the current foreground app.
/// Returns `Err` with a user-friendly Chinese reason when blocked.
#[cfg(target_os = "macos")]
pub fn assert_writable() -> Result<()> {
    let name = frontmost_app_name().unwrap_or_default();
    let bundle = frontmost_app_bundle_id().unwrap_or_default();
    if BLOCKED_BUNDLES.iter().any(|b| bundle.eq_ignore_ascii_case(b))
        || BLOCKED_NAMES.iter().any(|n| name.contains(n))
    {
        bail!("终端窗口禁止写入（{}）", if !name.is_empty() { name } else { bundle });
    }
    Ok(())
}

#[cfg(not(target_os = "macos"))]
pub fn assert_writable() -> Result<()> { Ok(()) }

/// Best-effort: get the bundle ID of the frontmost app via NSWorkspace.
#[cfg(target_os = "macos")]
pub fn frontmost_app_bundle_id() -> Option<String> {
    use cocoa::base::nil;
    use objc::{msg_send, sel, sel_impl, class};
    unsafe {
        let workspace: cocoa::base::id = msg_send![class!(NSWorkspace), sharedWorkspace];
        if workspace == nil { return None; }
        let app: cocoa::base::id = msg_send![workspace, frontmostApplication];
        if app == nil { return None; }
        let bid: cocoa::base::id = msg_send![app, bundleIdentifier];
        if bid == nil { return None; }
        let s: *const std::os::raw::c_char = msg_send![bid, UTF8String];
        if s.is_null() { return None; }
        Some(std::ffi::CStr::from_ptr(s).to_string_lossy().into_owned())
    }
}

#[cfg(target_os = "macos")]
pub fn frontmost_app_name() -> Option<String> {
    use cocoa::base::nil;
    use objc::{msg_send, sel, sel_impl, class};
    unsafe {
        let workspace: cocoa::base::id = msg_send![class!(NSWorkspace), sharedWorkspace];
        if workspace == nil { return None; }
        let app: cocoa::base::id = msg_send![workspace, frontmostApplication];
        if app == nil { return None; }
        let nm: cocoa::base::id = msg_send![app, localizedName];
        if nm == nil { return None; }
        let s: *const std::os::raw::c_char = msg_send![nm, UTF8String];
        if s.is_null() { return None; }
        Some(std::ffi::CStr::from_ptr(s).to_string_lossy().into_owned())
    }
}

#[cfg(not(target_os = "macos"))]
pub fn frontmost_app_bundle_id() -> Option<String> { None }
#[cfg(not(target_os = "macos"))]
pub fn frontmost_app_name() -> Option<String> { None }

/// Write text to the foreground app's cursor via clipboard + Cmd+V.
/// V1 implementation uses `pbcopy` + AppleScript for Cmd+V — no extra deps.
#[cfg(target_os = "macos")]
pub async fn write_at_cursor(text: &str) -> Result<()> {
    assert_writable()?;

    // 1. Save current clipboard
    let prior = read_clipboard_text().await.unwrap_or_default();

    // 2. Put new text on clipboard
    write_clipboard_text(text).await.context("set new clipboard")?;

    // 3. Simulate Cmd+V via AppleScript (osascript)
    let status = tokio::process::Command::new("osascript")
        .args([
            "-e",
            r#"tell application "System Events" to keystroke "v" using {command down}"#,
        ])
        .status()
        .await
        .context("osascript Cmd+V")?;
    if !status.success() {
        bail!("osascript Cmd+V exited {status}");
    }

    // 4. Wait a moment for the paste to take effect, then restore
    tokio::time::sleep(std::time::Duration::from_millis(120)).await;
    if !prior.is_empty() {
        let _ = write_clipboard_text(&prior).await;
    }
    Ok(())
}

#[cfg(not(target_os = "macos"))]
pub async fn write_at_cursor(_text: &str) -> Result<()> {
    bail!("Mode B not implemented on non-macOS yet")
}

#[cfg(target_os = "macos")]
async fn write_clipboard_text(text: &str) -> Result<()> {
    use tokio::io::AsyncWriteExt;
    let mut child = tokio::process::Command::new("pbcopy")
        .stdin(std::process::Stdio::piped())
        .spawn()?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(text.as_bytes()).await?;
    }
    let status = child.wait().await?;
    if !status.success() {
        bail!("pbcopy failed {status}");
    }
    Ok(())
}

#[cfg(target_os = "macos")]
async fn read_clipboard_text() -> Result<String> {
    let out = tokio::process::Command::new("pbpaste").output().await?;
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}
