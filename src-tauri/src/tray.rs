//! Menubar tray icon — adds "召唤老鼠 / 查看历史 / 退出" menu in the macOS
//! status bar. Originally listed as V2 in CLAUDE.md; user explicitly asked
//! for it in v0.1.4 ("最好能有个托盘图标").
//!
//! Click the tray icon = same as pressing the global shortcut (summon).
//! Right-click = menu.

use tauri::{
    menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Manager, WebviewUrl, WebviewWindowBuilder,
};

pub fn setup(app: &AppHandle) -> tauri::Result<()> {
    let summon  = MenuItem::with_id(app, "summon",  "🦞 召唤老鼠",       true, None::<&str>)?;
    let history = MenuItem::with_id(app, "history", "📜 查看历史记录…",  true, None::<&str>)?;
    let about   = MenuItem::with_id(app, "about",   "ℹ️  关于 MouseClaw", true, None::<&str>)?;
    let sep1    = PredefinedMenuItem::separator(app)?;
    let quit    = MenuItem::with_id(app, "quit",    "退出 MouseClaw",     true, Some("CmdOrCtrl+Q"))?;

    let menu = Menu::with_items(app, &[&summon, &history, &sep1, &about, &quit])?;
    // Use the bundle's pre-loaded app icon. The colorful lobster isn't a
    // perfect template image (which should be monochrome) so we leave
    // icon_as_template = false to keep it recognizable.
    let icon = app
        .default_window_icon()
        .ok_or_else(|| tauri::Error::AssetNotFound("default_window_icon".into()))?
        .clone();

    TrayIconBuilder::with_id("main-tray")
        .icon(icon)
        .icon_as_template(false)
        .tooltip("MouseClaw 🦞 — 按 Cmd+Shift+Space 召唤")
        .menu(&menu)
        .on_menu_event(handle_menu_event)
        .on_tray_icon_event(|tray, event| {
            // Left-click on the icon body → summon (same as shortcut)
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                let app = tray.app_handle();
                summon_via_tray(app);
            }
        })
        .build(app)?;

    Ok(())
}

fn handle_menu_event(app: &AppHandle, event: MenuEvent) {
    match event.id.as_ref() {
        "summon"  => summon_via_tray(app),
        "history" => open_history_window(app),
        "about"   => open_about_dialog(app),
        "quit"    => app.exit(0),
        _ => {}
    }
}

/// Programmatically trigger the same flow as a global-shortcut press.
fn summon_via_tray(app: &AppHandle) {
    use std::sync::Arc;
    let state: Arc<crate::AppState> = app.state::<Arc<crate::AppState>>().inner().clone();
    if let Some(w) = app.get_webview_window("mouse") {
        let _ = crate::show_mouse(&w);
    }
    let app_clone = app.clone();
    tauri::async_runtime::spawn(async move {
        crate::on_shortcut_pressed(app_clone, state).await;
    });
}

fn open_history_window(app: &AppHandle) {
    // Re-focus existing window if open
    if let Some(w) = app.get_webview_window("history") {
        let _ = w.show();
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
    if let Err(e) = result {
        eprintln!("[mouseclaw] failed to open history window: {e:#}");
    }
}

fn open_about_dialog(app: &AppHandle) {
    // Reuse history window mechanism with a different URL flag
    if let Some(w) = app.get_webview_window("about") {
        let _ = w.show();
        let _ = w.set_focus();
        return;
    }
    let _ = WebviewWindowBuilder::new(
        app,
        "about",
        WebviewUrl::App("index.html?view=about".into()),
    )
    .title("关于 MouseClaw")
    .inner_size(420.0, 340.0)
    .resizable(false)
    .decorations(true)
    .build();
}
