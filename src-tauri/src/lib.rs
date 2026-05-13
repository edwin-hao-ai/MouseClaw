//! MouseClaw 主入口。
//! 完整架构约束见 `/Users/edwinhao/MouseClaw/CLAUDE.md`。

pub mod claude_cli;
pub mod screenshot;

use tauri::{Manager, WebviewWindow};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};

/// 在 macOS 上把进程变成纯后台 accessory app（无 dock 图标、不出现在 Cmd+Tab）。
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

/// Day 1 显示老鼠窗口（先固定位置，Day 2 加跟鼠标）。
fn show_mouse_window(window: &WebviewWindow) -> tauri::Result<()> {
    window.show()?;
    Ok(())
}

/// Day 1 端到端 pipeline：截屏 → 调 Claude → 打印回复。
/// Day 2 之后这里会切换到流式 + Whisper transcript + 发事件给前端老鼠/气泡。
async fn run_day1_pipeline() {
    // Day 1 占位 transcript（Whisper 还没接）。这是装作"用户说"的内容。
    let fake_transcript = "看一眼当前屏幕，用一句中文告诉我你看到了什么。";

    println!("[mouseclaw] capturing screenshot...");
    let img_path = match screenshot::capture_main_screen().await {
        Ok(p) => {
            println!("[mouseclaw]   → saved {}", p.display());
            p
        }
        Err(e) => {
            eprintln!("[mouseclaw]   ✘ screenshot failed: {e:#}");
            return;
        }
    };

    println!("[mouseclaw] asking claude (this may take 10-30s)...");
    let started = std::time::Instant::now();
    match claude_cli::ask_claude(fake_transcript, &img_path, None).await {
        Ok(reply) => {
            let dt = started.elapsed();
            println!("[mouseclaw]   ← claude replied in {:.1}s:", dt.as_secs_f32());
            println!("─────────────────────────────────────────────");
            println!("{reply}");
            println!("─────────────────────────────────────────────");

            if let Some(insert_text) = claude_cli::parse_insert_directive(&reply) {
                println!("[mouseclaw]   ⌨  Mode B detected — would insert at cursor:");
                println!("       {insert_text:?}");
            } else {
                println!("[mouseclaw]   💬  Mode A (display in bubble only)");
            }
        }
        Err(e) => {
            eprintln!("[mouseclaw]   ✘ claude failed: {e:#}");
        }
    }
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
                    println!("\n[mouseclaw] 🦞 shortcut fired: {shortcut:?}");

                    // 显示老鼠窗口（Day 1 占位，真动画在 Day 2）。
                    if let Some(window) = app.get_webview_window("mouse") {
                        if let Err(e) = show_mouse_window(&window) {
                            eprintln!("[mouseclaw] failed to show window: {e}");
                        }
                    }

                    // 把 pipeline 扔到 tokio runtime 跑，handler 立刻返回避免阻塞 UI 线程。
                    tauri::async_runtime::spawn(async move {
                        run_day1_pipeline().await;
                    });
                })
                .build(),
        )
        .setup(|app| {
            let shortcut = Shortcut::new(Some(Modifiers::SUPER | Modifiers::SHIFT), Code::Space);
            app.global_shortcut().register(shortcut)?;
            println!("[mouseclaw] registered global shortcut: Cmd+Shift+Space");

            set_accessory_activation_policy();
            println!("[mouseclaw] activation policy set to accessory (no dock icon)");
            println!("[mouseclaw] 🦞 ready — press Cmd+Shift+Space to test pipeline");

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
