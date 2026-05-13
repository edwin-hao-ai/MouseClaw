use tauri::{Manager, WebviewWindow};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};

/// 在 macOS 上把进程变成纯后台 accessory app（无 dock 图标、不出现在 Cmd+Tab）。
/// 必须在窗口创建之后调用，否则首次显示窗口仍会拉起 dock 图标。
#[cfg(target_os = "macos")]
fn set_accessory_activation_policy() {
    use cocoa::appkit::{NSApp, NSApplication, NSApplicationActivationPolicy};
    unsafe {
        let app = NSApp();
        app.setActivationPolicy_(NSApplicationActivationPolicy::NSApplicationActivationPolicyAccessory);
    }
}

#[cfg(not(target_os = "macos"))]
fn set_accessory_activation_policy() {}

/// 老鼠窗口出现在屏幕右下角的「sleeping corner」(临时位置，Day 2 加跟鼠标)。
fn show_mouse_window(window: &WebviewWindow) -> tauri::Result<()> {
    // 暂时固定位置；Day 2 改成跟鼠标光标。
    window.show()?;
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, shortcut, event| {
                    if event.state() != ShortcutState::Pressed {
                        return;
                    }
                    println!("[mouseclaw] global shortcut fired: {:?}", shortcut);
                    if let Some(window) = app.get_webview_window("mouse") {
                        if let Err(e) = show_mouse_window(&window) {
                            eprintln!("[mouseclaw] failed to show window: {e}");
                        }
                    } else {
                        eprintln!("[mouseclaw] mouse window not found");
                    }
                })
                .build(),
        )
        .setup(|app| {
            // Day 1 测试快捷键：Cmd+Shift+Space。Day 3 改成用户在 Onboarding 选的。
            let shortcut = Shortcut::new(Some(Modifiers::SUPER | Modifiers::SHIFT), Code::Space);
            app.global_shortcut().register(shortcut)?;
            println!("[mouseclaw] registered global shortcut: Cmd+Shift+Space");

            // 把 dock 图标藏掉
            set_accessory_activation_policy();
            println!("[mouseclaw] activation policy set to accessory (no dock icon)");

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
