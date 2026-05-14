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

/// Write text to the foreground app's cursor.
///
/// V1.5 (current): Direct CGEvent keyboard events with unicode string injection.
/// Bypasses IME (no composition collision) and doesn't pollute the clipboard.
/// `CGEventKeyboardSetUnicodeString` sends raw text to whichever app is frontmost.
///
/// Limit: macOS caps each event at ~20 UTF-16 code units, so we chunk longer
/// text and add a tiny gap between chunks so the app's input loop catches up.
#[cfg(target_os = "macos")]
pub async fn write_at_cursor(text: &str) -> Result<()> {
    assert_writable()?;
    let text = text.to_string();
    // CGEvent calls aren't Send-friendly when held across awaits, so run on a
    // blocking thread. The post operation itself is fast (microseconds per event).
    tokio::task::spawn_blocking(move || type_unicode_string(&text))
        .await
        .context("spawn_blocking join")??;
    Ok(())
}

#[cfg(not(target_os = "macos"))]
pub async fn write_at_cursor(_text: &str) -> Result<()> {
    bail!("Mode B not implemented on non-macOS yet")
}

#[cfg(target_os = "macos")]
fn type_unicode_string(text: &str) -> Result<()> {
    use core_graphics::event::{CGEvent, CGEventTapLocation};
    use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};

    let source = CGEventSource::new(CGEventSourceStateID::HIDSystemState)
        .map_err(|_| anyhow::anyhow!("CGEventSource::new failed"))?;

    // Chunk into ≤ 15-character pieces (16-bit UTF-16 units each).
    // CGEventKeyboardSetUnicodeString accepts up to ~20 UCS-2 codepoints in practice;
    // we stay below the limit to handle surrogate pairs / emojis safely.
    const CHUNK: usize = 15;
    let chars: Vec<char> = text.chars().collect();
    for chunk in chars.chunks(CHUNK) {
        let s: String = chunk.iter().collect();
        // Fire a key-down event with the unicode string. The app sees this as raw
        // text input bypassing whatever IME state the user has active.
        let down = CGEvent::new_keyboard_event(source.clone(), 0, true)
            .map_err(|_| anyhow::anyhow!("CGEvent::new_keyboard_event(down) failed"))?;
        down.set_string(&s);
        down.post(CGEventTapLocation::HID);

        // Matching key-up for cleanliness (some apps require paired events).
        let up = CGEvent::new_keyboard_event(source.clone(), 0, false)
            .map_err(|_| anyhow::anyhow!("CGEvent::new_keyboard_event(up) failed"))?;
        up.set_string(&s);
        up.post(CGEventTapLocation::HID);

        // 5ms between chunks gives focused apps time to consume the event.
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    Ok(())
}
