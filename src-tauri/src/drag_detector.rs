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
        let pb_has_supported = pasteboard_has_supported_file(pb);

        if count != prev && left_down && pb_has_supported {
            // 新 drag session，是支持类型的文件 drag，鼠标还按着 → drag started
            if !DRAG_ENTER_SENT.swap(true, Ordering::SeqCst) {
                println!(
                    "[drag-detector] 🐕 supported-file drag started (pb changeCount {prev} → {count}) → approach"
                );
                if let Some((app, state)) = APP_REF.lock().unwrap().clone() {
                    crate::feed_flow::on_drag_enter(&app, &state);
                }
            }
        }

        // drag 结束：鼠标松开。LEAVE 不挑剔 pasteboard 类型 —— 一旦松开就退场。
        // 如果 drop 落在 pet 上，mouse window 的 Tauri DragDrop handler 会先收到
        // Drop 事件触发 on_files_dropped，feed_flow 的 200ms leave debounce 会把
        // 我们这次 leave 撤回。
        if !left_down && DRAG_ENTER_SENT.swap(false, Ordering::SeqCst) {
            println!("[drag-detector] 🐕 mouse released → notify leave");
            if let Some((app, state)) = APP_REF.lock().unwrap().clone() {
                crate::feed_flow::on_drag_leave(&app, &state);
            }
        }

        let _: () = msg_send![pool, drain];
    }

    /// drag pasteboard 是否含**支持类型**的文件（feed::classify != Reject）。
    /// 先快速判 file-url 类型，再读出真实路径逐个 classify。
    /// 任一支持即 true；全不支持 / 读不到路径 → false（不唤桌宠）。
    unsafe fn pasteboard_has_supported_file(pb: id) -> bool {
        // 1. 快筛：连 file-url 类型都没有，直接 false（文本 / 网址 drag）
        if !has_file_url(pb) { return false; }
        // 2. 读 NSURL 列表
        let nsurl_class: id = msg_send![class!(NSURL), class];
        let classes: id = msg_send![class!(NSArray), arrayWithObject: nsurl_class];
        let urls: id = msg_send![pb, readObjectsForClasses: classes options: nil];
        if urls == nil { return false; }
        let n: NSUInteger = msg_send![urls, count];
        for i in 0..n {
            let url: id = msg_send![urls, objectAtIndex: i];
            if url == nil { continue; }
            let path_id: id = msg_send![url, path];
            if path_id == nil { continue; }
            let cstr: *const std::os::raw::c_char = msg_send![path_id, UTF8String];
            if cstr.is_null() { continue; }
            let path_str = std::ffi::CStr::from_ptr(cstr).to_string_lossy().into_owned();
            let kind = crate::feed::classify(std::path::Path::new(&path_str));
            if kind != crate::feed::FeedKind::Reject {
                return true;
            }
        }
        false
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

#[cfg(not(target_os = "macos"))]
pub fn install(_app: tauri::AppHandle, _state: std::sync::Arc<crate::AppState>) {}
