//! v0.4.x · 系统级 file-drag 探测（macOS only）。
//!
//! 让桌宠在用户从屏幕**任意位置**开始拖文件时立刻"小狗式跑去迎接"。
//!
//! ## 方案演进
//! v1（已废弃）：全屏隐形 NSWindow + NSView 子类 + NSDraggingDestination 协议。
//! 问题：用户硬约束「别影响到使用别的应用」意味着窗口必须
//! `setIgnoresMouseEvents:YES`，但这会**同时屏蔽 NSDragging 事件**（macOS 上
//! 所有 mouse-derived 事件都受此 flag 影响 —— Apple 文档没写但社区实测验证）。
//! → 协议方法永远不被调用，桌宠永远不知道有 drag。
//!
//! v2（当前）：后台轮询 NSPasteboard "Apple CFPasteboard drag" 的 changeCount +
//! NSEvent.pressedMouseButtons。完全 read-only，不创建任何窗口，零视觉占用，
//! 不影响其他 app 的鼠标/拖拽行为。
//!
//! ## 工作流
//! 100ms 一次轮询主线程上：
//!   1. 读 drag pasteboard changeCount —— 增加 = 新 drag session 开始
//!   2. 同时读鼠标左键状态 —— 按下 + drag pb 增 = 用户正在拖
//!   3. 如果 drag pb 包含 `public.file-url` → 调 feed_flow::on_drag_enter
//!      桌宠从 anchor 跑去光标
//!   4. 鼠标键松开 → 调 feed_flow::on_drag_leave 让桌宠 200ms 内回 anchor
//!      （如果 drop 在 pet 上，mouse window 的 Tauri DragDrop handler 会先 fire
//!      on_files_dropped，pet 把 leave 撤回）
//!
//! ## 失败模式 / 边界
//!   - 文本选区拖动：drag pb 也会更新但不含 file-url → 不触发，正确
//!   - Finder 内部拖动重排：同 app 的 drag pb 不一定走 system pasteboard，
//!     可能不触发（可接受 —— 用户在 Finder 内部排序时不想看到老鼠跑出来）
//!   - 用户拖到第三方 app（非 mouse overlay 窗口）：on_drag_leave 触发 pet 回家
//!   - 没装 Accessibility 权限：本模块不需要 AX，纯 pasteboard 读取，always works

#[cfg(target_os = "macos")]
mod imp {
    use std::sync::{Arc, Mutex};
    use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
    use std::time::Duration;
    use once_cell::sync::Lazy;
    use objc::{class, msg_send, sel, sel_impl};
    use cocoa::base::{id, nil};
    use cocoa::foundation::{NSAutoreleasePool, NSString, NSUInteger};
    use tauri::AppHandle;
    use crate::AppState;

    /// 拖拽 pasteboard 的官方名字 —— macOS NSPasteboardNameDrag 常量值。
    /// 直接用字符串，省去 link 一堆 cocoa-foundation 常量。
    const PASTEBOARD_NAME_DRAG: &str = "Apple CFPasteboard drag";
    /// 文件 URL UTI —— 文本 / 网址等其它 drag 不会触发我们
    const FILE_URL_UTI: &str = "public.file-url";

    static APP_REF: Lazy<Mutex<Option<(AppHandle, Arc<AppState>)>>> = Lazy::new(|| Mutex::new(None));
    static INSTALLED: AtomicBool = AtomicBool::new(false);

    /// 上次看到的 drag pasteboard changeCount —— 增加 = 新 drag session
    static LAST_CHANGE_COUNT: AtomicI64 = AtomicI64::new(-1);
    /// 我们已通知 feed_flow 进入 drag 态（dedup 防止 100ms 轮询期间重复触发）
    static DRAG_ENTER_SENT: AtomicBool = AtomicBool::new(false);
    /// v0.4 fix (2026-05-20)：当前 drag 里被识别为支持类型的文件路径。
    /// 鼠标松开时若光标在桌宠窗口内 → 直接喂这些文件（绕开 WKWebView 的 HTML 导航坑）。
    static DRAG_FILES: Lazy<Mutex<Vec<std::path::PathBuf>>> = Lazy::new(|| Mutex::new(Vec::new()));

    pub fn install(app: AppHandle, state: Arc<AppState>) {
        *APP_REF.lock().unwrap() = Some((app, state));
        if INSTALLED.swap(true, Ordering::SeqCst) {
            return;
        }
        // 后台 std::thread —— 不需要异步，NSPasteboard / NSEvent 类方法都是
        // 线程安全的（按 Apple 文档 + 实测，read-only 访问从任意线程都 OK）。
        std::thread::Builder::new()
            .name("mouseclaw-drag-poll".into())
            .spawn(poll_loop)
            .expect("spawn drag-poll thread");
        println!("[drag-detector] v2 NSPasteboard polling installed (100ms)");
    }

    fn poll_loop() {
        loop {
            std::thread::sleep(Duration::from_millis(100));
            unsafe { tick(); }
        }
    }

    unsafe fn tick() {
        // Autorelease pool 包一下 —— 每次 tick 产生的临时 NSString / NSArray 都释放掉
        let pool: id = msg_send![class!(NSAutoreleasePool), new];

        let pb_name = NSString::alloc(nil).init_str(PASTEBOARD_NAME_DRAG);
        let pb: id = msg_send![class!(NSPasteboard), pasteboardWithName: pb_name];
        if pb == nil {
            let _: () = msg_send![pool, drain];
            return;
        }

        let count: i64 = msg_send![pb, changeCount];
        let prev = LAST_CHANGE_COUNT.swap(count, Ordering::Relaxed);

        // 鼠标左键当前状态 —— NSEvent class method，跨线程读取安全
        let mouse_mask: NSUInteger = msg_send![class!(NSEvent), pressedMouseButtons];
        let left_down = (mouse_mask & 1) != 0;

        // v0.4 fix (2026-05-20)：只在拖的是**支持的文件类型**时才唤桌宠。
        // 用户反馈：拖文件是高频操作，多数时候并不想喂桌宠 —— 不该一拖就扑过来。
        // 读出真实文件路径，任一是 feed 支持类型（非 Reject）才触发。
        let supported_files = extract_supported_files(pb);
        let pb_has_supported = !supported_files.is_empty();

        if count != prev && left_down && pb_has_supported {
            // 新 drag session，是支持类型的文件 drag，鼠标还按着 → drag started
            if !DRAG_ENTER_SENT.swap(true, Ordering::SeqCst) {
                // 缓存这批支持文件 —— 松手若在桌宠上就喂它们
                *DRAG_FILES.lock().unwrap() = supported_files;
                println!(
                    "[drag-detector] 🐕 supported-file drag started (pb changeCount {prev} → {count}) → approach"
                );
                if let Some((app, state)) = APP_REF.lock().unwrap().clone() {
                    crate::feed_flow::on_drag_enter(&app, &state);
                }
            }
        }

        // drag 结束：鼠标松开。
        // v0.4 fix (2026-05-20)：drop 统一由 drag_detector 处理，**不再依赖 WKWebView
        // 的 DragDropEvent**（HTML 文件会被 webview 当成导航请求拦截，drop 喂不进去）。
        // 松手时若光标在桌宠窗口内 + 有缓存的支持文件 → 直接 on_files_dropped；
        // 否则当成 leave，桌宠回 anchor。lib.rs 的 Tauri DragDrop handler 已移除。
        if !left_down && DRAG_ENTER_SENT.swap(false, Ordering::SeqCst) {
            let files = std::mem::take(&mut *DRAG_FILES.lock().unwrap());
            if let Some((app, state)) = APP_REF.lock().unwrap().clone() {
                let over_pet = cursor_over_mouse_window(&app);
                if over_pet && !files.is_empty() {
                    println!("[drag-detector] 🐕 dropped on pet → feed {} file(s)", files.len());
                    crate::feed_flow::on_files_dropped(app, state, files);
                } else {
                    println!("[drag-detector] 🐕 released away from pet → return to anchor");
                    crate::feed_flow::on_drag_leave(&app, &state);
                }
            }
        }

        let _: () = msg_send![pool, drain];
    }

    /// 光标此刻是否在桌宠 mouse 窗口的物理范围内（top-left 逻辑像素比较）。
    /// 喂文件期间窗口是 320×320（FeedWaiting expand），命中区域足够宽松。
    unsafe fn cursor_over_mouse_window(app: &AppHandle) -> bool {
        use tauri::Manager;
        let Some((cx, cy)) = global_cursor_top_left() else { return false; };
        let Some(window) = app.get_webview_window("mouse") else { return false; };
        let Ok(pos) = window.outer_position() else { return false; };
        let Ok(size) = window.outer_size() else { return false; };
        let scale = window.scale_factor().unwrap_or(1.0).max(0.5);
        let wx = pos.x as f64 / scale;
        let wy = pos.y as f64 / scale;
        let ww = size.width as f64 / scale;
        let wh = size.height as f64 / scale;
        cx >= wx && cx <= wx + ww && cy >= wy && cy <= wy + wh
    }

    /// NSEvent.mouseLocation → top-left 逻辑像素（同 pet_passthrough 的算法）。
    unsafe fn global_cursor_top_left() -> Option<(f64, f64)> {
        use cocoa::foundation::NSPoint;
        let cls: id = msg_send![class!(NSEvent), class];
        let p: NSPoint = msg_send![cls, mouseLocation];
        let screen: id = msg_send![class!(NSScreen), mainScreen];
        if screen == nil { return None; }
        let frame: cocoa::foundation::NSRect = msg_send![screen, frame];
        Some((p.x, frame.size.height - p.y))
    }

    /// 读出 drag pasteboard 里所有**支持类型**的文件路径（feed::classify != Reject）。
    /// 先快速判 file-url 类型，再读真实 NSURL 路径逐个 classify。
    /// 全不支持 / 读不到 → 空 Vec（不唤桌宠）。
    unsafe fn extract_supported_files(pb: id) -> Vec<std::path::PathBuf> {
        let mut out = Vec::new();
        // 1. 快筛：连 file-url 类型都没有，直接空（文本 / 网址 drag）
        if !has_file_url(pb) { return out; }
        // 2. 读 NSURL 列表
        let nsurl_class: id = msg_send![class!(NSURL), class];
        let classes: id = msg_send![class!(NSArray), arrayWithObject: nsurl_class];
        let urls: id = msg_send![pb, readObjectsForClasses: classes options: nil];
        if urls == nil { return out; }
        let n: NSUInteger = msg_send![urls, count];
        for i in 0..n {
            let url: id = msg_send![urls, objectAtIndex: i];
            if url == nil { continue; }
            let path_id: id = msg_send![url, path];
            if path_id == nil { continue; }
            let cstr: *const std::os::raw::c_char = msg_send![path_id, UTF8String];
            if cstr.is_null() { continue; }
            let path_str = std::ffi::CStr::from_ptr(cstr).to_string_lossy().into_owned();
            let path = std::path::PathBuf::from(&path_str);
            if crate::feed::classify(&path) != crate::feed::FeedKind::Reject {
                out.push(path);
            }
        }
        out
    }

    /// drag pasteboard 是否包含 file URL UTI（快筛）
    unsafe fn has_file_url(pb: id) -> bool {
        let types: id = msg_send![pb, types];
        if types == nil { return false; }
        let count: NSUInteger = msg_send![types, count];
        let needle = NSString::alloc(nil).init_str(FILE_URL_UTI);
        for i in 0..count {
            let t: id = msg_send![types, objectAtIndex: i];
            if t == nil { continue; }
            let equal: bool = msg_send![t, isEqualToString: needle];
            if equal { return true; }
        }
        false
    }
}

#[cfg(target_os = "macos")]
pub fn install(app: tauri::AppHandle, state: std::sync::Arc<crate::AppState>) {
    imp::install(app, state);
}

/// 非 macOS：用 Tauri 的窗口级 DragDrop 事件（WebView2 / webkit2gtk 不像 WKWebView 那样
/// 吞文件）。限制：只在文件**拖到桌宠窗口上**时触发（非全屏检测，那需 OS 级 detector）。
/// ⚠️ 运行时未在真机验证。
#[cfg(not(target_os = "macos"))]
pub fn install(app: tauri::AppHandle, state: std::sync::Arc<crate::AppState>) {
    use tauri::{DragDropEvent, Manager, WindowEvent};
    let Some(win) = app.get_webview_window("mouse") else {
        eprintln!("[mouseclaw] drag_detector: mouse window 不存在，跳过");
        return;
    };
    let app2 = app.clone();
    let state2 = state.clone();
    win.on_window_event(move |ev| {
        if let WindowEvent::DragDrop(dd) = ev {
            match dd {
                DragDropEvent::Enter { .. } => crate::feed_flow::on_drag_enter(&app2, &state2),
                DragDropEvent::Leave => crate::feed_flow::on_drag_leave(&app2, &state2),
                DragDropEvent::Drop { paths, .. } => {
                    crate::feed_flow::on_files_dropped(app2.clone(), state2.clone(), paths.clone())
                }
                _ => {}
            }
        }
    });
    println!("[mouseclaw] drag_detector: Tauri DragDrop 已挂到 mouse 窗口");
}
