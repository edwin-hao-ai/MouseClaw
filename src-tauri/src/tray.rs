//! Menubar tray icon — adds "召唤老鼠 / 查看历史 / 退出" menu in the macOS
//! status bar. Originally listed as V2 in CLAUDE.md; user explicitly asked
//! for it in v0.1.4 ("最好能有个托盘图标").
//!
//! Click the tray icon = same as pressing the global shortcut (summon).
//! Right-click = menu.

use tauri::{
    image::Image,
    menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Manager, WebviewUrl, WebviewWindowBuilder,
};

/// 16×16 monochrome mouse silhouette for the macOS menubar (template image).
/// Each u16 row encodes 16 horizontal pixels MSB-first: 1 = opaque black, 0 = transparent.
/// When `icon_as_template(true)` is set, macOS auto-tints this for dark/light menubars.
const MOUSE_TEMPLATE_BITMAP: [u16; 16] = [
    0b0001100001100000, // row 0: ear tops
    0b0011110011110000, // row 1: ears wide
    0b0011110011110000, // row 2
    0b0111111111111110, // row 3: head top
    0b0111111111111110, // row 4: head with eye slots
    0b0111101111011110, // row 5: eyes (gaps)
    0b0111111111111110, // row 6
    0b0011111111111110, // row 7
    0b0011111111111111, // row 8: belly + tail starts
    0b0011111111111111, // row 9
    0b0001111111111110, // row 10
    0b0000111111111100, // row 11
    0b0000110000110000, // row 12: paws gap
    0b0001100000011000, // row 13
    0b0000000000000000, // row 14
    0b0000000000000000, // row 15
];

/// Build the tray icon as raw RGBA. 16×16×4 = 1024 bytes.
fn build_template_icon() -> Image<'static> {
    let mut rgba = vec![0u8; 16 * 16 * 4];
    for y in 0..16 {
        let row = MOUSE_TEMPLATE_BITMAP[y];
        for x in 0..16 {
            let bit = (row >> (15 - x)) & 1;
            let i = (y * 16 + x) * 4;
            if bit == 1 {
                // RGB ignored when template image; alpha 255 = opaque
                rgba[i] = 0;
                rgba[i + 1] = 0;
                rgba[i + 2] = 0;
                rgba[i + 3] = 255;
            }
            // else: alpha 0 = fully transparent (default zeros)
        }
    }
    Image::new_owned(rgba, 16, 16)
}

pub fn setup(app: &AppHandle) -> tauri::Result<()> {
    let summon  = MenuItem::with_id(app, "summon",  "🦞 召唤老鼠",       true, None::<&str>)?;
    let history = MenuItem::with_id(app, "history", "📜 查看历史记录…",  true, None::<&str>)?;
    let about   = MenuItem::with_id(app, "about",   "ℹ️  关于 MouseClaw", true, None::<&str>)?;
    let sep1    = PredefinedMenuItem::separator(app)?;
    let quit    = MenuItem::with_id(app, "quit",    "退出 MouseClaw",     true, Some("CmdOrCtrl+Q"))?;

    let menu = Menu::with_items(app, &[&summon, &history, &sep1, &about, &quit])?;
    let icon = build_template_icon();

    TrayIconBuilder::with_id("main-tray")
        .icon(icon)
        .icon_as_template(true)
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
/// If user hasn't completed onboarding yet, divert to the onboarding window
/// instead — calling the pipeline without a registered shortcut isn't useful.
fn summon_via_tray(app: &AppHandle) {
    use std::sync::Arc;
    if !crate::config::Config::load().onboarded {
        println!("[mouseclaw] tray summon: not onboarded yet → opening Onboarding");
        open_onboarding_window(app);
        return;
    }
    let state: Arc<crate::AppState> = app.state::<Arc<crate::AppState>>().inner().clone();
    if let Some(w) = app.get_webview_window("mouse") {
        let _ = crate::show_mouse(&w);
    }
    let app_clone = app.clone();
    tauri::async_runtime::spawn(async move {
        crate::on_shortcut_pressed(app_clone, state).await;
    });
}

/// Force the .accessory-policy app to come forward so the new window has focus.
/// Without this, LSUIElement / setActivationPolicy(.accessory) causes new windows
/// to open behind whatever app is currently active.
#[cfg(target_os = "macos")]
fn activate_app() {
    use objc::{class, msg_send, sel, sel_impl};
    unsafe {
        let app: cocoa::base::id = msg_send![class!(NSApplication), sharedApplication];
        let _: () = msg_send![app, activateIgnoringOtherApps: true];
    }
}
#[cfg(not(target_os = "macos"))]
fn activate_app() {}

fn open_history_window(app: &AppHandle) {
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

fn open_about_dialog(app: &AppHandle) {
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
