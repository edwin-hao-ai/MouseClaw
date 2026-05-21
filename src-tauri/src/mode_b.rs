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

/// 已知 CGEventKeyboardSetUnicodeString 不太靠谱的 app —— 走 clipboard-paste fallback。
/// 这类 app 多半是 Electron / web-based 富文本编辑器，会吃掉合成的 unicode keystrokes：
///   - Slack / Discord / Lark / Notion / Linear / Obsidian（contenteditable）
///   - VS Code / Cursor（Monaco editor 自己接管 input）
///   - Chrome / Edge / Arc / Safari（页面里的 contenteditable / textarea 也常有问题）
const RICH_EDITOR_BUNDLES: &[&str] = &[
    "com.tinyspeck.slackmacgap",       // Slack
    "com.hnc.Discord",                  // Discord
    "com.electron.lark",                // Lark / 飞书
    "com.bytedance.macos.lark",         // Lark 另一个 bundle
    "notion.id",                        // Notion
    "com.linear",                       // Linear
    "md.obsidian",                      // Obsidian
    "com.microsoft.VSCode",             // VS Code
    "com.todesktop.230313mzl4w4u92",    // Cursor
    "com.google.Chrome",                // Chrome (contenteditable / textarea)
    "com.microsoft.edgemac",            // Edge
    "company.thebrowser.Browser",       // Arc
    "com.apple.Safari",                 // Safari
];

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

/// Decide which write strategy to use for the frontmost app.
/// 富文本/Electron/浏览器 → clipboard paste；其它 → direct CGEvent unicode keystrokes。
///
/// v0.4.2 · 改 pub —— 边说边写（voice_live_type）用它判断当前 app 能否走 direct
/// unicode：富文本 app 吃 keystroke，必须降级回松手后 clipboard-paste（Plan B）。
#[cfg(target_os = "macos")]
pub fn should_use_clipboard_paste() -> bool {
    let bundle = frontmost_app_bundle_id().unwrap_or_default();
    RICH_EDITOR_BUNDLES.iter().any(|b| bundle.eq_ignore_ascii_case(b))
}

/// Write text to the foreground app's cursor.
///
/// 两条路径，按前台 app 自动选：
///   1. **Direct CGEvent unicode**（默认）—— TextEdit / Pages / 终端外的普通 native app
///      用 `CGEventKeyboardSetUnicodeString` 合成 unicode 键盘事件，不污染剪贴板、绕过 IME
///   2. **Clipboard-paste fallback**（Electron / 浏览器 / Monaco / Notion / Slack 等）——
///      存原剪贴板 → 写 text → 合成 ⌘V → 200ms 后恢复原剪贴板
///      因为 Electron / contenteditable 经常吃掉合成的 unicode keystroke
///
/// Limit (path #1): macOS 每个事件最多 ~20 UTF-16 单位，分 15 char/chunk 发送。
#[cfg(target_os = "macos")]
pub async fn write_at_cursor(text: &str) -> Result<()> {
    assert_writable()?;
    let text = text.to_string();
    let use_paste = should_use_clipboard_paste();
    tokio::task::spawn_blocking(move || {
        if use_paste {
            paste_via_clipboard(&text)
        } else {
            type_unicode_string(&text)
        }
    })
    .await
    .context("spawn_blocking join")??;
    Ok(())
}

#[cfg(not(target_os = "macos"))]
pub async fn write_at_cursor(_text: &str) -> Result<()> {
    bail!("Mode B not implemented on non-macOS yet")
}

/// v0.4.x · 仅把 text 写进系统剪贴板（不 paste、不还原）—— Mode B 续写时
/// 光标丢了 / 原窗口失焦无法直接插入的兜底：把内容留在剪贴板让用户自己 ⌘V。
#[cfg(target_os = "macos")]
pub fn set_clipboard(text: &str) -> Result<()> {
    use cocoa::base::{id, nil};
    use cocoa::foundation::{NSString, NSArray};
    use objc::{class, msg_send, sel, sel_impl};
    unsafe {
        let pool: id = msg_send![class!(NSAutoreleasePool), new];
        let res: Result<()> = (|| {
            let pb: id = msg_send![class!(NSPasteboard), generalPasteboard];
            if pb == nil { bail!("generalPasteboard nil"); }
            let ns_type = NSString::alloc(nil).init_str("public.utf8-plain-text");
            let _: i64 = msg_send![pb, clearContents];
            let ns_text = NSString::alloc(nil).init_str(text);
            let types = NSArray::arrayWithObject(nil, ns_type);
            let _: bool = msg_send![pb, declareTypes: types owner: nil];
            let ok: bool = msg_send![pb, setString: ns_text forType: ns_type];
            let _: () = msg_send![ns_text, release];
            let _: () = msg_send![ns_type, release];
            if !ok { bail!("NSPasteboard setString failed"); }
            Ok(())
        })();
        if pool != nil { let _: () = msg_send![pool, drain]; }
        res
    }
}
#[cfg(not(target_os = "macos"))]
pub fn set_clipboard(_text: &str) -> Result<()> { bail!("set_clipboard only on macOS") }

/// v0.3.1 · 流式语音输入用 —— 同步快速 paste，不走 clipboard 路径。
/// 直接用 unicode keyboard event 注入，不污染剪贴板，跟 paste_via_clipboard 区分。
/// 给 voice_ime streaming poller 每 150ms 调用，必须返回快（无 await）。
#[cfg(target_os = "macos")]
pub fn type_unicode_sync(text: &str) -> Result<()> {
    type_unicode_string(text)
}
#[cfg(not(target_os = "macos"))]
pub fn type_unicode_sync(_text: &str) -> Result<()> {
    bail!("type_unicode_sync only on macOS")
}

/// v0.3.1 · 流式语音输入用 —— 删除光标前 `n` 个 char。
/// 给 sherpa partial 自我纠错时回退用：发 n 次 backspace key event (keycode 51)。
/// 注意：`n` 是 char 数（中文 1 char = 1 backspace）—— 已和 type_unicode_string 一致。
#[cfg(target_os = "macos")]
pub fn delete_chars(n: usize) -> Result<()> {
    if n == 0 { return Ok(()); }
    use core_graphics::event::{CGEvent, CGEventTapLocation};
    use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
    let source = CGEventSource::new(CGEventSourceStateID::HIDSystemState)
        .map_err(|_| anyhow::anyhow!("CGEventSource::new failed"))?;
    // keycode 51 = Delete (Backspace) on macOS
    const BACKSPACE_KEYCODE: u16 = 51;
    for _ in 0..n {
        let down = CGEvent::new_keyboard_event(source.clone(), BACKSPACE_KEYCODE, true)
            .map_err(|_| anyhow::anyhow!("CGEvent backspace down failed"))?;
        down.post(CGEventTapLocation::HID);
        let up = CGEvent::new_keyboard_event(source.clone(), BACKSPACE_KEYCODE, false)
            .map_err(|_| anyhow::anyhow!("CGEvent backspace up failed"))?;
        up.post(CGEventTapLocation::HID);
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    Ok(())
}
#[cfg(not(target_os = "macos"))]
pub fn delete_chars(_n: usize) -> Result<()> {
    bail!("delete_chars only on macOS")
}

/// Clipboard-paste fallback for rich/Electron editors.
///
/// 流程：
///   1. snapshot NSPasteboard 的 changeCount + plain-text 内容
///   2. 写 text 进 NSPasteboard (NSPasteboardTypeString)
///   3. 合成 Cmd+V 键盘事件
///   4. 等 250ms 让目标 app 完成 paste（>200ms 是 GenClipboard 等实测的安全值）
///   5. 恢复原 plain-text 内容（如果还是我们写的那次 changeCount）
///
/// 已知 trade-off：只恢复 plain-text flavor。RTF/图片/文件等不还原 ——
/// 大多数 daily-use 场景剪贴板里就是文字，这个权衡可接受。
#[cfg(target_os = "macos")]
fn paste_via_clipboard(text: &str) -> Result<()> {
    use cocoa::base::{id, nil};
    use cocoa::foundation::{NSString, NSArray};
    use objc::{class, msg_send, sel, sel_impl};
    use core_graphics::event::{CGEvent, CGEventFlags, CGEventTapLocation};
    use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};

    // v0.1.15 autorelease pool —— mode_b 在 tokio blocking 线程跑，仍无 ObjC pool
    // 用 inner closure 跑业务，外层 unsafe 块保证 drain 一定执行。
    unsafe {
        let pool: id = msg_send![class!(NSAutoreleasePool), new];
        let res: Result<()> = (|| {
            let pb: id = msg_send![class!(NSPasteboard), generalPasteboard];
            if pb == nil { bail!("NSPasteboard generalPasteboard returned nil"); }

            // alloc/init = retained，需要手动 release
            let ns_type_string = NSString::alloc(nil).init_str("public.utf8-plain-text");
            let old_str_obj: id = msg_send![pb, stringForType: ns_type_string];
            let old_str: Option<String> = if old_str_obj == nil { None } else {
                let ptr: *const std::os::raw::c_char = msg_send![old_str_obj, UTF8String];
                if ptr.is_null() { None }
                else { Some(std::ffi::CStr::from_ptr(ptr).to_string_lossy().into_owned()) }
            };

            let _: i64 = msg_send![pb, clearContents];
            let ns_text = NSString::alloc(nil).init_str(text);
            let types = NSArray::arrayWithObject(nil, ns_type_string);
            let _: bool = msg_send![pb, declareTypes: types owner: nil];
            let ok: bool = msg_send![pb, setString: ns_text forType: ns_type_string];

            // 合成 Cmd+V 之前先 release ns_text
            let _: () = msg_send![ns_text, release];

            if !ok {
                let _: () = msg_send![ns_type_string, release];
                bail!("NSPasteboard setString 失败");
            }

            let source = CGEventSource::new(CGEventSourceStateID::HIDSystemState)
                .map_err(|_| anyhow::anyhow!("CGEventSource::new failed"))?;
            const V_KEYCODE: u16 = 9;
            let down = CGEvent::new_keyboard_event(source.clone(), V_KEYCODE, true)
                .map_err(|_| anyhow::anyhow!("CGEvent v-down failed"))?;
            down.set_flags(CGEventFlags::CGEventFlagCommand);
            down.post(CGEventTapLocation::HID);
            let up = CGEvent::new_keyboard_event(source.clone(), V_KEYCODE, false)
                .map_err(|_| anyhow::anyhow!("CGEvent v-up failed"))?;
            up.set_flags(CGEventFlags::CGEventFlagCommand);
            up.post(CGEventTapLocation::HID);

            std::thread::sleep(std::time::Duration::from_millis(250));

            if let Some(prev) = old_str {
                let _: i64 = msg_send![pb, clearContents];
                let ns_prev = NSString::alloc(nil).init_str(prev.as_str());
                let _: bool = msg_send![pb, declareTypes: types owner: nil];
                let _: bool = msg_send![pb, setString: ns_prev forType: ns_type_string];
                let _: () = msg_send![ns_prev, release];
            }
            let _: () = msg_send![ns_type_string, release];
            Ok(())
        })();
        if pool != nil { let _: () = msg_send![pool, drain]; }
        res
    }
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
