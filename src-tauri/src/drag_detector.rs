//! v0.4.x · Drag-detector full-screen invisible overlay (macOS only).
//!
//! 让桌宠在用户从屏幕**任意位置**开始拖文件时立刻"小狗式跑去迎接"。
//! 实现：新增一个**没有 webview** 的纯原生 NSWindow，全屏 transparent +
//! always-on-top + `setIgnoresMouseEvents:YES`。
//!
//! ## 为什么不简单扩大现有 mouse overlay？
//! v0.3.12 (f253762) 把 mouse overlay 从 320×320 缩到 80×80 的原因正是：
//! WebKit 在大的 transparent NSWindow 上会渲染半透明 backing layer，遮挡
//! Mission Control / Stage Manager / 桌面图标。新增窗口必须 **跳过 WebKit**
//! —— 纯 NSWindow + NSView 子类，无任何渲染负担。
//!
//! ## macOS NSDragging vs setIgnoresMouseEvents
//! macOS 上 click / drag 走两条不同的 event path。`setIgnoresMouseEvents:YES`
//! 只阻止 mouseDown/mouseUp/mouseMoved/mouseDragged，**不阻止** NSDragging
//! protocol 上的 draggingEntered/Updated/Exited/performDrag。所以这个窗口
//! 对用户来说"不存在"（点不到），但 file drag 一进入屏幕系统就路由到它。
//!
//! ## 单一信源
//! 同时存在 mouse overlay 的 Tauri DragDrop handler 和这个 detector：
//! 两者都走 `feed_flow::on_drag_enter`，里面 `feed_drag_active.swap(true)`
//! 提供幂等 dedup —— 第二次触发是 no-op。drop 由先 fire 的那个处理（通常是
//! 这个 detector，因为它先收到 cursor + file 的 enter 事件）。

#[cfg(target_os = "macos")]
mod imp {
    use std::sync::{Arc, Mutex};
    use std::path::PathBuf;
    use once_cell::sync::Lazy;
    use objc::{class, msg_send, sel, sel_impl};
    use objc::runtime::{Class, Object, Sel, BOOL, NO, YES};
    use objc::declare::ClassDecl;
    use cocoa::base::{id, nil};
    use cocoa::foundation::{NSRect, NSString, NSUInteger};
    use tauri::AppHandle;
    use crate::AppState;

    /// NSDragOperationCopy = 1 —— 给 drag source 反馈"会复制接收"
    const NS_DRAG_OPERATION_COPY: NSUInteger = 1;
    /// NSDragOperationNone = 0 —— 不接收（如 pasteboard 里没文件）
    const NS_DRAG_OPERATION_NONE: NSUInteger = 0;

    /// 全局 AppHandle + AppState —— callbacks 是 extern "C" fn，没法直接拿
    /// Rust 上下文，靠这个 Mutex<Option<...>> 在 install 时灌入。
    static APP_REF: Lazy<Mutex<Option<(AppHandle, Arc<AppState>)>>> = Lazy::new(|| Mutex::new(None));

    /// 创建过的 detector NSWindow —— 重启 / 重新装系统时 install 可能被调多次，
    /// 第二次起跳过（避免叠两个 detector 窗口）。
    static INSTALLED: std::sync::atomic::AtomicBool =
        std::sync::atomic::AtomicBool::new(false);

    pub fn install(app: AppHandle, state: Arc<AppState>) {
        *APP_REF.lock().unwrap() = Some((app.clone(), state));
        if INSTALLED.swap(true, std::sync::atomic::Ordering::SeqCst) {
            return;
        }
        // NSWindow 创建 + show 必须在主线程
        let _ = app.run_on_main_thread(|| {
            unsafe { setup_window(); }
        });
    }

    unsafe fn setup_window() {
        // 主屏 frame
        let screens: id = msg_send![class!(NSScreen), screens];
        let count: NSUInteger = msg_send![screens, count];
        if count == 0 { return; }
        let primary: id = msg_send![screens, objectAtIndex:0usize];
        let frame: NSRect = msg_send![primary, frame];

        let view_class = register_drag_view_class();

        // NSWindowStyleMaskBorderless = 0；NSBackingStoreBuffered = 2
        let window: id = msg_send![class!(NSWindow), alloc];
        let window: id = msg_send![window,
            initWithContentRect:frame
                      styleMask:0u64
                        backing:2u64
                          defer:NO];

        // 视觉：透明 / 无阴影 / clearColor
        let _: () = msg_send![window, setOpaque:NO];
        let clear: id = msg_send![class!(NSColor), clearColor];
        let _: () = msg_send![window, setBackgroundColor:clear];
        let _: () = msg_send![window, setHasShadow:NO];
        // 交互：穿透点击
        let _: () = msg_send![window, setIgnoresMouseEvents:YES];
        // 层级：NSPopUpMenuWindowLevel = 101，在 floating 之上，statusbar 之下
        let _: () = msg_send![window, setLevel:101isize];
        // collection behavior 位标志（NSWindowCollectionBehavior*）：
        //   CanJoinAllSpaces(1<<0) | Stationary(1<<4) |
        //   IgnoresCycle(1<<6) | FullScreenAuxiliary(1<<8)
        let behavior: NSUInteger = (1 << 0) | (1 << 4) | (1 << 6) | (1 << 8);
        let _: () = msg_send![window, setCollectionBehavior:behavior];
        let _: () = msg_send![window, setReleasedWhenClosed:NO];
        let _: () = msg_send![window, setHidesOnDeactivate:NO];

        // contentView = MCDragView 实例
        let view: id = msg_send![view_class, alloc];
        let view: id = msg_send![view, initWithFrame:frame];
        let _: () = msg_send![window, setContentView:view];

        // 只接收文件 URL —— "public.file-url" UTI，文本 drag 不会触发
        let file_url_uti = NSString::alloc(nil).init_str("public.file-url");
        let types: id = msg_send![class!(NSArray), arrayWithObject:file_url_uti];
        let _: () = msg_send![view, registerForDraggedTypes:types];

        // orderFront 不抢焦点（关键 —— LSUIElement app 不能 activate）
        let _: () = msg_send![window, orderFrontRegardless];

        println!(
            "[mouseclaw] 🐕 drag-detector installed: {}x{} at ({}, {})",
            frame.size.width as i32, frame.size.height as i32,
            frame.origin.x as i32, frame.origin.y as i32,
        );
    }

    /// 注册 NSView 子类 MCDragView —— 用 once_cell 保证只注册一次。
    fn register_drag_view_class() -> &'static Class {
        static REGISTERED: std::sync::Once = std::sync::Once::new();
        static mut CLASS: *const Class = std::ptr::null();
        unsafe {
            REGISTERED.call_once(|| {
                let superclass = class!(NSView);
                let mut decl = ClassDecl::new("MCDragView", superclass)
                    .expect("MCDragView ClassDecl failed (class name collision?)");
                decl.add_method(
                    sel!(draggingEntered:),
                    drag_entered as extern "C" fn(&mut Object, Sel, id) -> NSUInteger,
                );
                decl.add_method(
                    sel!(draggingUpdated:),
                    drag_updated as extern "C" fn(&mut Object, Sel, id) -> NSUInteger,
                );
                decl.add_method(
                    sel!(draggingExited:),
                    drag_exited as extern "C" fn(&mut Object, Sel, id),
                );
                decl.add_method(
                    sel!(prepareForDragOperation:),
                    prepare_for_drag as extern "C" fn(&mut Object, Sel, id) -> BOOL,
                );
                decl.add_method(
                    sel!(performDragOperation:),
                    perform_drag as extern "C" fn(&mut Object, Sel, id) -> BOOL,
                );
                CLASS = decl.register();
            });
            &*CLASS
        }
    }

    // ─────────────── NSDraggingDestination callbacks ───────────────

    extern "C" fn drag_entered(_this: &mut Object, _sel: Sel, sender: id) -> NSUInteger {
        unsafe {
            if !pasteboard_has_files(sender) {
                return NS_DRAG_OPERATION_NONE;
            }
        }
        if let Some((app, state)) = APP_REF.lock().unwrap().clone() {
            println!("[mouseclaw] 🐕 drag entered screen → run to greet");
            crate::feed_flow::on_drag_enter(&app, &state);
        }
        NS_DRAG_OPERATION_COPY
    }

    extern "C" fn drag_updated(_this: &mut Object, _sel: Sel, sender: id) -> NSUInteger {
        unsafe {
            if pasteboard_has_files(sender) {
                NS_DRAG_OPERATION_COPY
            } else {
                NS_DRAG_OPERATION_NONE
            }
        }
    }

    extern "C" fn drag_exited(_this: &mut Object, _sel: Sel, _sender: id) {
        if let Some((app, state)) = APP_REF.lock().unwrap().clone() {
            println!("[mouseclaw] 🐕 drag exited screen → return to anchor");
            crate::feed_flow::on_drag_leave(&app, &state);
        }
    }

    extern "C" fn prepare_for_drag(_this: &mut Object, _sel: Sel, _sender: id) -> BOOL {
        YES
    }

    extern "C" fn perform_drag(_this: &mut Object, _sel: Sel, sender: id) -> BOOL {
        let files = unsafe { extract_file_paths(sender) };
        if files.is_empty() {
            return NO;
        }
        if let Some((app, state)) = APP_REF.lock().unwrap().clone() {
            println!("[mouseclaw] 🐕 dropped {} file(s)", files.len());
            crate::feed_flow::on_files_dropped(app, state, files);
        }
        YES
    }

    // ────────────────── helpers ──────────────────

    unsafe fn pasteboard_has_files(sender: id) -> bool {
        let pb: id = msg_send![sender, draggingPasteboard];
        if pb == nil { return false; }
        let file_url_uti = NSString::alloc(nil).init_str("public.file-url");
        let types: id = msg_send![class!(NSArray), arrayWithObject:file_url_uti];
        let can: BOOL = msg_send![pb, canReadItemWithDataConformingToTypes:types];
        can != NO
    }

    unsafe fn extract_file_paths(sender: id) -> Vec<PathBuf> {
        let pb: id = msg_send![sender, draggingPasteboard];
        if pb == nil { return Vec::new(); }

        // readObjectsForClasses:[NSURL] options:nil → NSArray<NSURL>
        let nsurl_class: id = msg_send![class!(NSURL), class];
        let classes: id = msg_send![class!(NSArray), arrayWithObject:nsurl_class];
        let urls: id = msg_send![pb, readObjectsForClasses:classes options:nil];
        if urls == nil { return Vec::new(); }

        let count: NSUInteger = msg_send![urls, count];
        let mut out = Vec::with_capacity(count as usize);
        for i in 0..count {
            let url: id = msg_send![urls, objectAtIndex:i];
            if url == nil { continue; }
            let path_id: id = msg_send![url, path];
            if path_id == nil { continue; }
            let cstr: *const std::os::raw::c_char = msg_send![path_id, UTF8String];
            if cstr.is_null() { continue; }
            let s = std::ffi::CStr::from_ptr(cstr).to_string_lossy().into_owned();
            out.push(PathBuf::from(s));
        }
        out
    }
}

#[cfg(target_os = "macos")]
pub fn install(app: tauri::AppHandle, state: std::sync::Arc<crate::AppState>) {
    imp::install(app, state);
}

#[cfg(not(target_os = "macos"))]
pub fn install(_app: tauri::AppHandle, _state: std::sync::Arc<crate::AppState>) {}
