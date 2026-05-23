//! v0.5 · 把桌宠 overlay（"mouse" 窗口）转成真正的 **NSPanel**，让它浮在全屏 app 之上。
//!
//! 为什么要这么做：普通 NSWindow 在 **release** 构建下，设 collectionBehavior / level 也浮不到
//! 全屏 app 之上（Tauri #5566 / #9556 —— dev 能用、build 不行）。NSPanel 是 macOS 上这类
//! "桌宠 / HUD 浮在全屏上"的标准方案（BongoCat 桌宠 / Cap 录屏 / Screenpipe 都用 tauri-nspanel）。
//!
//! macOS-only。失败只 log —— 退化为普通窗口（全屏看不到，但其余功能不受影响）。

use tauri::{AppHandle, Manager};
use tauri_nspanel::{tauri_panel, CollectionBehavior, PanelLevel, StyleMask, WebviewWindowExt};

tauri_panel! {
    panel!(MousePanel {
        config: {
            // 桌宠要能接收点击 / 键入（follow-up 输入、Esc）；但靠 nonactivating styleMask
            // 保证点它**不**把 MouseClaw 拉到前台（维持 accessory 后台特性）。
            can_become_key_window: true,
            can_become_main_window: false,
            is_floating_panel: true
        }
    })
}

/// 在 setup() 主线程调一次：把 "mouse" 窗口转成 NSPanel + 配置浮全屏行为。
pub fn convert_mouse_to_panel(app: &AppHandle) {
    let Some(window) = app.get_webview_window("mouse") else {
        eprintln!("[mouseclaw] nspanel: mouse window 不存在，跳过");
        return;
    };
    let panel = match window.to_panel::<MousePanel>() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("[mouseclaw] nspanel: to_panel 失败 ({e})，桌宠退化为普通窗口");
            return;
        }
    };
    // ScreenSaver(1000)：远高于菜单栏层级，浮在全屏 app 之上（录屏悬浮窗那一档）。
    panel.set_level(PanelLevel::ScreenSaver.value());
    // 无边框 + nonactivating：点桌宠不把 MouseClaw 拉到前台。
    panel.set_style_mask(StyleMask::empty().nonactivating_panel().into());
    // 出现在所有 Space（含当前全屏 Space）+ 与全屏窗口共存 + 切 Space 不被搬走。
    panel.set_collection_behavior(
        CollectionBehavior::new()
            .can_join_all_spaces()
            .full_screen_auxiliary()
            .stationary()
            .into(),
    );
    // 关键：app 失去焦点（用户点别的 app）时桌宠**不**自动隐藏 ——
    // NSPanel 默认 hidesOnDeactivate=YES，对常驻桌宠是灾难，必须关掉。
    panel.set_hides_on_deactivate(false);
    // 桌宠永不销毁（hide 不 close）。
    panel.set_released_when_closed(false);
    println!(
        "[mouseclaw] 🐭 mouse overlay → NSPanel (level=ScreenSaver, joinAllSpaces|fullScreenAux, hidesOnDeactivate=false)"
    );
}
