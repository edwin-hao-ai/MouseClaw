//! 托盘触发的**辅助窗口**打开逻辑 —— 系统状态 / 历史记录 / Onboarding / 关于。
//! 每个都是「已存在则 show+focus，否则 WebviewWindowBuilder 新建」的同一套模式，
//! 配合 `activate_app()` 把 .accessory-policy 的 app 拉到前台（否则新窗口开在后面）。
//!
//! 从 tray.rs 抽出（tray.rs 已超 800 行硬上限 · 见 CLAUDE.md「单文件 ≤ 800 行」）。
//! 注意：剪贴板 / Hub / 下载器 / picker 等窗口走 `commands::*`，不在这里。

use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

/// 打开「📊 系统状态」窗口 —— 一眼看到 claude / agent-browser / Chrome CDP / 权限的就绪状态。
/// 每行都有"去解决"按钮（装 / 启用 / 开权限）。
/// pub(crate) —— commands::show_status_window 也复用（升级提示 nudge 的 CTA）。
pub(crate) fn open_status_window(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("status") {
        let _ = w.show();
        activate_app();
        let _ = w.set_focus();
        return;
    }
    let result = WebviewWindowBuilder::new(
        app,
        "status",
        WebviewUrl::App("index.html?view=status".into()),
    )
    .title("MouseClaw — 系统状态")
    .inner_size(520.0, 560.0)
    .min_inner_size(420.0, 420.0)
    .resizable(true)
    .decorations(true)
    .focused(true)
    .build();
    match result {
        Ok(w) => {
            activate_app();
            let _ = w.set_focus();
        }
        Err(e) => eprintln!("[mouseclaw] open status window failed: {e:#}"),
    }
}

/// Force the .accessory-policy app to come forward so the new window has focus.
/// Without this, LSUIElement / setActivationPolicy(.accessory) causes new windows
/// to open behind whatever app is currently active.
#[cfg(target_os = "macos")]
pub(crate) fn activate_app() {
    use objc::{class, msg_send, sel, sel_impl};
    unsafe {
        let app: cocoa::base::id = msg_send![class!(NSApplication), sharedApplication];
        let _: () = msg_send![app, activateIgnoringOtherApps: true];
    }
}
#[cfg(not(target_os = "macos"))]
pub(crate) fn activate_app() {}

pub(crate) fn open_history_window(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("history") {
        let _ = w.show();
        activate_app();
        let _ = w.set_focus();
        return;
    }
    let result = WebviewWindowBuilder::new(
        app,
        "history",
        WebviewUrl::App("index.html?view=history".into()),
    )
    .title("MouseClaw — 历史记录")
    .inner_size(720.0, 560.0)
    .min_inner_size(480.0, 360.0)
    .resizable(true)
    .decorations(true)
    .focused(true)
    .build();
    match result {
        Ok(w) => {
            activate_app();
            let _ = w.set_focus();
        }
        Err(e) => eprintln!("[mouseclaw] failed to open history window: {e:#}"),
    }
}

pub fn open_onboarding_window(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("onboarding") {
        let _ = w.show();
        activate_app();
        let _ = w.set_focus();
        return;
    }
    let result = WebviewWindowBuilder::new(
        app,
        "onboarding",
        WebviewUrl::App("index.html?view=onboarding".into()),
    )
    .title("欢迎使用 MouseClaw 🦞")
    .inner_size(700.0, 760.0)
    .min_inner_size(560.0, 680.0)
    .resizable(false)
    .decorations(true)
    .focused(true)
    .build();
    match result {
        Ok(w) => {
            activate_app();
            let _ = w.set_focus();
            println!("[mouseclaw] onboarding window opened");
        }
        Err(e) => eprintln!("[mouseclaw] failed to open onboarding window: {e:#}"),
    }
}

pub(crate) fn open_about_dialog(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("about") {
        let _ = w.show();
        activate_app();
        let _ = w.set_focus();
        return;
    }
    let result = WebviewWindowBuilder::new(
        app,
        "about",
        WebviewUrl::App("index.html?view=about".into()),
    )
    .title("关于 MouseClaw")
    .inner_size(420.0, 340.0)
    .resizable(false)
    .decorations(true)
    .build();
    if let Ok(w) = result {
        activate_app();
        let _ = w.set_focus();
    }
}
