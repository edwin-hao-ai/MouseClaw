//! Clipboard history capture (v0.2 · P0)
//!
//! 一句话原理：
//!   - macOS 没有「剪贴板变化事件」 → 用 NSPasteboard.changeCount 轮询（500ms 一次）
//!   - changeCount 每次变了，读出当前 plain-text + 当前前台 app 信息 → append 到 jsonl
//!
//! 隐私边界（CLAUDE.md 硬要求）：
//!   1. **遵守 transient 标志**：org.nspasteboard.TransientType / ConcealedType 直接跳过
//!      （1Password / Bitwarden 等会打这个标，nspasteboard.org 的生态约定）
//!   2. **默认排除应用**：1Password / Bitwarden / Keychain / Terminal / iTerm / Warp
//!   3. **永远本地**：~/.mouseclaw/clipboard.jsonl，权限 0600，永远不联网
//!   4. 单条 1 MB 上限，超出截断
//!   5. 同内容连续复制只保留最新一条 timestamp
//!   6. 默认保留 50 条最近 + ⭐ 标星的永久
//!
//! 文件存 `~/.mouseclaw/clipboard.jsonl`（追加日志）+ 内存里维持 in-memory 索引。

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use std::time::Duration;
use serde::{Deserialize, Serialize};
use anyhow::{Context, Result};

/// 单条剪贴板记录。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClipItem {
    /// 唯一 id（用 nanos timestamp 当 id 够了 —— 跨 50 条不会撞）
    pub id: u64,
    /// 内容种类：现在 v0.2 只做 text；future 加 image / file / url 等
    #[serde(default = "default_kind")]
    pub kind: String,
    /// 完整内容（最多 1MB）。预览在前端按需截短显示
    pub text: String,
    /// 前台 app bundle id（如 com.microsoft.VSCode）
    #[serde(default)]
    pub app_bundle: String,
    /// 前台 app 显示名（如 "Visual Studio Code"）
    #[serde(default)]
    pub app_name: String,
    /// Unix epoch 秒
    pub ts: u64,
    /// ⭐ 用户标星 = 永久保留，不参与 50 条 LRU 清理
    #[serde(default)]
    pub pinned: bool,
}

fn default_kind() -> String { "text".into() }

/// 历史保留条数上限（标星的不算）。
pub const MAX_UNPINNED: usize = 50;
/// 单条体积上限：1 MB（超出截断，避免历史文件失控）
pub const MAX_TEXT_BYTES: usize = 1024 * 1024;
/// 轮询间隔（ms）。500ms 在 nspasteboard 生态是公认良好平衡（Maccy 等也用这个）
pub const POLL_INTERVAL_MS: u64 = 500;

/// 默认排除的 app bundle —— 不记录这些 app 复制的内容
pub const EXCLUDED_BUNDLES: &[&str] = &[
    "com.agilebits.onepassword7",
    "com.agilebits.onepassword-osx",
    "com.bitwarden.desktop",
    "com.dashlane.dashlanephonefinal",
    "com.apple.keychainaccess",
    "com.apple.Terminal",
    "com.googlecode.iterm2",
    "dev.warp.Warp-Stable",
    "co.zeit.hyper",
];

/// 全局 in-memory 历史（Arc<RwLock> —— 读多写少）
pub static HISTORY: once_cell::sync::Lazy<Arc<RwLock<VecDeque<ClipItem>>>> =
    once_cell::sync::Lazy::new(|| Arc::new(RwLock::new(VecDeque::with_capacity(64))));

/// 暂停开关 —— 用户托盘点「⏸️ 暂停记录」时置为 true，捕获 loop 看到就跳过新条目
/// 已经存的不影响；恢复后继续记录。
pub static PAUSED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub fn set_paused(p: bool) {
    PAUSED.store(p, std::sync::atomic::Ordering::Relaxed);
    println!("[mouseclaw] 📋 clipboard paused = {p}");
}
pub fn is_paused() -> bool {
    PAUSED.load(std::sync::atomic::Ordering::Relaxed)
}

fn jsonl_path() -> Result<PathBuf> {
    let home = std::env::var_os("HOME").context("HOME not set")?;
    Ok(PathBuf::from(home).join(".mouseclaw/clipboard.jsonl"))
}

/// 启动时从磁盘读历史 → 灌进 in-memory。容错：损坏的行跳过。
pub fn load_from_disk() -> Result<()> {
    let path = jsonl_path()?;
    if !path.exists() { return Ok(()); }
    let text = std::fs::read_to_string(&path)?;
    let mut hist = HISTORY.write().unwrap();
    hist.clear();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() { continue; }
        if let Ok(item) = serde_json::from_str::<ClipItem>(line) {
            hist.push_back(item);
        }
    }
    // 修剪掉超量的非标星历史
    trim_locked(&mut hist);
    println!("[mouseclaw] 📋 clipboard history: 加载 {} 条", hist.len());
    Ok(())
}

/// 持久化到磁盘 —— 完整覆盖写（不是 append），因为 trim/dedupe 之后 indices 会变。
/// 量级 50 条 × 平均 1KB = 50KB，写一次 OK 不卡。
fn save_to_disk(hist: &VecDeque<ClipItem>) -> Result<()> {
    let path = jsonl_path()?;
    if let Some(parent) = path.parent() { let _ = std::fs::create_dir_all(parent); }
    let mut s = String::with_capacity(hist.len() * 256);
    for item in hist {
        let line = serde_json::to_string(item)?;
        s.push_str(&line);
        s.push('\n');
    }
    std::fs::write(&path, s)?;
    // 0600 权限 —— 仅当前用户能读
    #[cfg(unix)] {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

/// LRU 修剪：非标星条目最多 MAX_UNPINNED 条
fn trim_locked(hist: &mut VecDeque<ClipItem>) {
    let unpinned: usize = hist.iter().filter(|i| !i.pinned).count();
    if unpinned <= MAX_UNPINNED { return; }
    let mut to_remove = unpinned - MAX_UNPINNED;
    // 从最旧的非标星开始删
    let mut i = 0;
    while i < hist.len() && to_remove > 0 {
        if !hist[i].pinned {
            hist.remove(i);
            to_remove -= 1;
        } else {
            i += 1;
        }
    }
}

/// 启动后台轮询任务 —— 进程启动时调一次，永久跑。
/// 不依赖 AppHandle，纯独立任务。
pub fn spawn_capture_loop() {
    std::thread::spawn(|| {
        if let Err(e) = load_from_disk() {
            eprintln!("[mouseclaw] clipboard load_from_disk: {e}");
        }
        // 不需要 tokio runtime，cocoa 同步调用 OK
        let mut last_count: i64 = current_change_count();
        loop {
            std::thread::sleep(Duration::from_millis(POLL_INTERVAL_MS));
            let cur = current_change_count();
            if cur != last_count {
                last_count = cur;
                if let Err(e) = on_clipboard_changed() {
                    eprintln!("[mouseclaw] clipboard capture: {e}");
                }
            }
        }
    });
}

#[cfg(target_os = "macos")]
fn current_change_count() -> i64 {
    use cocoa::base::{id, nil};
    use objc::{class, msg_send, sel, sel_impl};
    unsafe {
        let pb: id = msg_send![class!(NSPasteboard), generalPasteboard];
        if pb == nil { return -1; }
        let c: i64 = msg_send![pb, changeCount];
        c
    }
}
#[cfg(not(target_os = "macos"))]
fn current_change_count() -> i64 { -1 }

/// 读当前 NSPasteboard 的 type 列表，判断是否含 transient/concealed 标志
#[cfg(target_os = "macos")]
fn is_transient() -> bool {
    use cocoa::base::{id, nil};
    use cocoa::foundation::{NSArray, NSString};
    use objc::{class, msg_send, sel, sel_impl};
    unsafe {
        let pb: id = msg_send![class!(NSPasteboard), generalPasteboard];
        if pb == nil { return false; }
        let types: id = msg_send![pb, types];
        if types == nil { return false; }
        let count: usize = msg_send![types, count];
        for i in 0..count {
            let t: id = msg_send![types, objectAtIndex: i];
            let cstr_ptr: *const std::os::raw::c_char = msg_send![t, UTF8String];
            if cstr_ptr.is_null() { continue; }
            let s = std::ffi::CStr::from_ptr(cstr_ptr).to_string_lossy();
            if s == "org.nspasteboard.TransientType"
               || s == "org.nspasteboard.ConcealedType"
               || s == "org.nspasteboard.AutoGeneratedType" {
                return true;
            }
        }
        // 防止 unused warning
        let _ = NSString::alloc(nil); let _ = NSArray::arrayWithObject(nil, nil);
        false
    }
}
#[cfg(not(target_os = "macos"))]
fn is_transient() -> bool { false }

/// 读当前 NSPasteboard 的 plain-text 内容
#[cfg(target_os = "macos")]
fn current_pasteboard_text() -> Option<String> {
    use cocoa::base::{id, nil};
    use cocoa::foundation::NSString;
    use objc::{class, msg_send, sel, sel_impl};
    unsafe {
        let pb: id = msg_send![class!(NSPasteboard), generalPasteboard];
        if pb == nil { return None; }
        let ns_type = NSString::alloc(nil).init_str("public.utf8-plain-text");
        let str_obj: id = msg_send![pb, stringForType: ns_type];
        if str_obj == nil { return None; }
        let ptr: *const std::os::raw::c_char = msg_send![str_obj, UTF8String];
        if ptr.is_null() { return None; }
        Some(std::ffi::CStr::from_ptr(ptr).to_string_lossy().into_owned())
    }
}
#[cfg(not(target_os = "macos"))]
fn current_pasteboard_text() -> Option<String> { None }

/// 拿前台 app 的 (bundle_id, display_name)
#[cfg(target_os = "macos")]
fn frontmost_app() -> (String, String) {
    use cocoa::base::{id, nil};
    use objc::{class, msg_send, sel, sel_impl};
    unsafe {
        let workspace: id = msg_send![class!(NSWorkspace), sharedWorkspace];
        if workspace == nil { return (String::new(), String::new()); }
        let app: id = msg_send![workspace, frontmostApplication];
        if app == nil { return (String::new(), String::new()); }
        let bundle: id = msg_send![app, bundleIdentifier];
        let name: id = msg_send![app, localizedName];
        let to_str = |o: id| -> String {
            if o == nil { return String::new(); }
            let p: *const std::os::raw::c_char = msg_send![o, UTF8String];
            if p.is_null() { return String::new(); }
            std::ffi::CStr::from_ptr(p).to_string_lossy().into_owned()
        };
        (to_str(bundle), to_str(name))
    }
}
#[cfg(not(target_os = "macos"))]
fn frontmost_app() -> (String, String) { (String::new(), String::new()) }

fn on_clipboard_changed() -> Result<()> {
    if is_paused() {
        return Ok(()); // 用户托盘暂停了
    }
    if is_transient() {
        println!("[mouseclaw] 📋 transient/concealed —— 跳过记录");
        return Ok(());
    }
    let Some(mut text) = current_pasteboard_text() else { return Ok(()); };
    if text.is_empty() { return Ok(()); }

    let (bundle, name) = frontmost_app();
    if EXCLUDED_BUNDLES.iter().any(|b| b.eq_ignore_ascii_case(&bundle)) {
        println!("[mouseclaw] 📋 排除应用 {bundle} —— 跳过记录");
        return Ok(());
    }

    // 截断到上限
    if text.len() > MAX_TEXT_BYTES {
        // 找一个安全的 char 边界
        let mut cut = MAX_TEXT_BYTES;
        while !text.is_char_boundary(cut) && cut > 0 { cut -= 1; }
        text.truncate(cut);
    }

    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs()).unwrap_or(0);
    let id = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64).unwrap_or(ts);

    let mut hist = HISTORY.write().unwrap();
    // Dedupe：如果最后一条文本跟新内容一样，只更新 ts，不新增
    if let Some(last) = hist.back_mut() {
        if last.text == text {
            last.ts = ts;
            let _ = save_to_disk(&hist);
            return Ok(());
        }
    }
    hist.push_back(ClipItem {
        id, kind: "text".into(), text, app_bundle: bundle, app_name: name,
        ts, pinned: false,
    });
    trim_locked(&mut hist);
    save_to_disk(&hist)?;
    Ok(())
}

// ────────────────── Public API for commands.rs ──────────────────

/// 返回当前历史快照（按时间倒序，最新在前）
pub fn list_items() -> Vec<ClipItem> {
    let hist = HISTORY.read().unwrap();
    hist.iter().rev().cloned().collect()
}

/// 按 id 删除一条
pub fn delete_item(id: u64) -> Result<()> {
    let mut hist = HISTORY.write().unwrap();
    if let Some(pos) = hist.iter().position(|i| i.id == id) {
        hist.remove(pos);
        save_to_disk(&hist)?;
    }
    Ok(())
}

/// 标星 / 取消标星
pub fn toggle_pin(id: u64) -> Result<()> {
    let mut hist = HISTORY.write().unwrap();
    if let Some(item) = hist.iter_mut().find(|i| i.id == id) {
        item.pinned = !item.pinned;
        save_to_disk(&hist)?;
    }
    Ok(())
}

/// 清空（包括标星 —— 用户明确要求清）
pub fn clear_all() -> Result<()> {
    let mut hist = HISTORY.write().unwrap();
    hist.clear();
    save_to_disk(&hist)?;
    Ok(())
}

/// 按 id 拿完整 text —— 给「粘贴」用
pub fn get_text(id: u64) -> Option<String> {
    let hist = HISTORY.read().unwrap();
    hist.iter().find(|i| i.id == id).map(|i| i.text.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trim_keeps_pinned() {
        let mut h: VecDeque<ClipItem> = VecDeque::new();
        for i in 0..60u64 {
            h.push_back(ClipItem {
                id: i, kind: "text".into(), text: format!("x{i}"),
                app_bundle: "".into(), app_name: "".into(), ts: i,
                pinned: i < 5, // 头 5 条标星
            });
        }
        trim_locked(&mut h);
        // 非标星上限 50；标星 5 条保留 → 总数应 = 50 + 5
        let pinned = h.iter().filter(|i| i.pinned).count();
        let unpinned = h.iter().filter(|i| !i.pinned).count();
        assert_eq!(pinned, 5);
        assert_eq!(unpinned, 50);
    }

    #[test]
    fn excluded_bundles_includes_password_managers() {
        assert!(EXCLUDED_BUNDLES.contains(&"com.agilebits.onepassword7"));
        assert!(EXCLUDED_BUNDLES.contains(&"com.bitwarden.desktop"));
        assert!(EXCLUDED_BUNDLES.contains(&"com.apple.Terminal"));
    }

    #[test]
    fn constants_within_sensible_bounds() {
        assert!(MAX_UNPINNED >= 20 && MAX_UNPINNED <= 200);
        assert!(POLL_INTERVAL_MS >= 200 && POLL_INTERVAL_MS <= 2000);
        assert!(MAX_TEXT_BYTES >= 64 * 1024);
    }
}
