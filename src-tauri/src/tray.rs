//! Menubar tray icon — adds "召唤老鼠 / 查看历史 / 退出" menu in the macOS
//! status bar. Originally listed as V2 in CLAUDE.md; user explicitly asked
//! for it in v0.1.4 ("最好能有个托盘图标").
//!
//! Click the tray icon = same as pressing the global shortcut (summon).
//! Right-click = menu.
//!
//! 本文件只保留**托盘图标 bitmap + TrayIconBuilder 初始化**。其余职责拆出：
//! - 菜单组装 → [`crate::tray_menu`]（`build_menu` / `rebuild_tray_menu`）
//! - 菜单 dispatch + 各 action handler → [`crate::tray_handlers`]
//! - 辅助窗口打开（status / history / onboarding / about）→ [`crate::tray_windows`]
//!
//! 为保持外部调用路径稳定（`crate::tray::build_menu` 等），下面 re-export 这些符号。

use tauri::{
    image::Image,
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle,
};

// 保持历史调用路径 `crate::tray::*` 不变（commands.rs / lib.rs 仍引用这些）。
pub use crate::tray_menu::{build_menu, rebuild_tray_menu};
pub use crate::tray_windows::open_onboarding_window;
// open_status_window 是 pub(crate)，只在 crate 内复用（commands::show_status_window）。
pub(crate) use crate::tray_windows::open_status_window;

use crate::tray_actions::summon_via_tray;
use crate::tray_handlers::handle_menu_event;

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

/// Build the tray icon as raw RGBA at 2× resolution for retina menubars.
/// The bitmap is 16×16 logical pixels, scaled to 32×32 physical pixels by
/// pixel-doubling. macOS will downscale to whatever its menubar height
/// requires (typically 22pt) while preserving sharpness on retina.
fn build_template_icon() -> Image<'static> {
    const SCALE: usize = 2;
    const SIZE: usize = 16 * SCALE; // 32
    let mut rgba = vec![0u8; SIZE * SIZE * 4];
    for y in 0..16 {
        let row = MOUSE_TEMPLATE_BITMAP[y];
        for x in 0..16 {
            let bit = (row >> (15 - x)) & 1;
            if bit == 1 {
                // Fill the corresponding 2×2 block in the upscaled grid
                for dy in 0..SCALE {
                    for dx in 0..SCALE {
                        let px = x * SCALE + dx;
                        let py = y * SCALE + dy;
                        let i = (py * SIZE + px) * 4;
                        rgba[i + 3] = 255; // template: alpha-only matters
                    }
                }
            }
        }
    }
    Image::new_owned(rgba, SIZE as u32, SIZE as u32)
}

/// 托盘 tooltip 文案 —— 跟随当前召唤快捷键（换键后调 update_tray_tooltip 刷新）。
pub fn tray_tooltip() -> String {
    let cfg = crate::config::Config::load();
    let sc = crate::shortcut_menu::pretty_shortcut(&cfg.shortcut);
    if cfg.language == "en" {
        format!("MouseClaw 🦞 — {sc} to summon  ·  ⌘⇧V clipboard")
    } else {
        format!("MouseClaw 🦞 — 按 {sc} 召唤  ·  ⌘⇧V 看剪贴板")
    }
}

/// 换召唤快捷键 / 切语言后刷新托盘 tooltip（change_summon_shortcut / save_language 调）。
pub fn update_tray_tooltip(app: &AppHandle) {
    if let Some(tray) = app.tray_by_id("main-tray") {
        let _ = tray.set_tooltip(Some(tray_tooltip()));
    }
}

pub fn setup(app: &AppHandle) -> tauri::Result<()> {
    let menu = build_menu(app)?;
    let icon = build_template_icon();

    TrayIconBuilder::with_id("main-tray")
        .icon(icon)
        .icon_as_template(true)
        .tooltip(tray_tooltip())
        .menu(&menu)
        .on_menu_event(handle_menu_event)
        .on_tray_icon_event(|tray, event| {
            let app = tray.app_handle();
            match event {
                // v0.1.13 · 双击托盘 → 打开 Hub（v0.1.16 改独立窗口）
                TrayIconEvent::DoubleClick { button: MouseButton::Left, .. } => {
                    println!("[mouseclaw] 📋 tray double-click → open Hub window");
                    if let Err(e) = crate::commands::open_hub_window(app.clone()) {
                        eprintln!("[mouseclaw] tray double-click open hub: {e}");
                    }
                }
                // 单击托盘 → 召唤
                TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                } => {
                    summon_via_tray(app);
                }
                _ => {}
            }
        })
        .build(app)?;

    Ok(())
}
