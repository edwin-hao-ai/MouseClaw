//! MouseClaw 主入口。
//!
//! 模块拆分（每个文件 ≤800 行，见 CLAUDE.md 规则）：
//!   - `overlay`     —— 老鼠窗口显示/隐藏 + 视图事件广播 + 自动隐藏
//!   - `pipeline`    —— 截屏+转写+AI+输出模式 的核心流程；push-to-talk handler
//!   - `commands`    —— 所有 #[tauri::command]
//!   - `backend`     —— 多 AI 后端抽象（Claude / Codex / OpenClaw CLI）
//!   - `claude_cli`  —— Claude Code CLI 流式调用
//!   - `audio` / `transcribe` —— cpal 录音 + Whisper 转写
//!   - `screenshot` / `mode_b` / `permissions` / `sessions` / `config` / `tray` / `events`
//!
//! 完整架构见 `/Users/edwinhao/MouseClaw/CLAUDE.md`。

pub mod audio;
pub mod backend;
pub mod claude_cli;
pub mod commands;
pub mod config;
pub mod events;
pub mod mode_b;
pub mod overlay;
pub mod permissions;
pub mod pipeline;
pub mod screenshot;
pub mod sessions;
pub mod transcribe;
pub mod tray;

use std::str::FromStr;
use std::sync::atomic::AtomicU64;
use std::sync::{Arc, Mutex as StdMutex};
use tauri::Manager;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};
use tokio::sync::Mutex;

use crate::backend::Backend;
use crate::events::ViewKind;
use crate::overlay::{emit_view, show_mouse};
use crate::pipeline::{on_shortcut_press, on_shortcut_release};
use crate::sessions::SessionStore;

/// App-wide state. Mutex split: tokio::Mutex for async-accessed bits,
/// std::Mutex for the cpal Recorder (cpal::Stream is !Send so we never await
/// while holding the recorder lock).
pub struct AppState {
    pub sessions: Mutex<SessionStore>,
    pub last_screenshot: Mutex<Option<std::path::PathBuf>>,
    pub recorder: StdMutex<Option<audio::Recorder>>,
    /// 当前选用的 AI 后端 —— pipeline 每次调用前 .lock().await.clone() 读取。
    /// 启动时从 config 灌入；运行期不变（改后端要重新 onboard + 重启）。
    pub backend: Mutex<Backend>,
    /// Monotonic counter — bumped on any new shortcut press / pipeline run /
    /// pin / dismiss. Auto-hide timers capture the gen at scheduling time and
    /// no-op if it changed by the time they fire.
    pub gen: AtomicU64,
}

impl AppState {
    fn new(backend: Backend) -> anyhow::Result<Self> {
        Ok(Self {
            sessions: Mutex::new(SessionStore::new()?),
            last_screenshot: Mutex::new(None),
            recorder: StdMutex::new(None),
            backend: Mutex::new(backend),
            gen: AtomicU64::new(0),
        })
    }
}

/// Release builds run as a `.app` bundle where stdout/stderr go nowhere.
/// Redirect both to `~/.mouseclaw/mouseclaw.log` so users (and us) can debug.
/// Dev builds (`bun tauri dev`) keep console output — no redirect.
#[cfg(all(target_os = "macos", not(debug_assertions)))]
fn init_file_logging() {
    use std::os::unix::io::IntoRawFd;
    let home = std::env::var("HOME").unwrap_or_default();
    let dir = std::path::PathBuf::from(&home).join(".mouseclaw");
    let _ = std::fs::create_dir_all(&dir);
    let log_path = dir.join("mouseclaw.log");
    if let Ok(file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
    {
        let fd = file.into_raw_fd();
        unsafe {
            libc::dup2(fd, libc::STDOUT_FILENO);
            libc::dup2(fd, libc::STDERR_FILENO);
            libc::close(fd);
        }
        println!(
            "\n========== MouseClaw 启动 {} ==========",
            chrono::Local::now().format("%Y-%m-%d %H:%M:%S")
        );
    }
}

#[cfg(not(all(target_os = "macos", not(debug_assertions))))]
fn init_file_logging() {}

#[cfg(target_os = "macos")]
fn set_accessory_activation_policy() {
    use cocoa::appkit::{NSApp, NSApplication, NSApplicationActivationPolicy};
    unsafe {
        let app = NSApp();
        app.setActivationPolicy_(
            NSApplicationActivationPolicy::NSApplicationActivationPolicyAccessory,
        );
    }
}
#[cfg(not(target_os = "macos"))]
fn set_accessory_activation_policy() {}

/// 装一个 panic hook —— 任何线程 panic 都把完整信息 + backtrace 写进
/// ~/.mouseclaw/mouseclaw.log（stderr 已被 init_file_logging 重定向过去）。
fn install_panic_hook() {
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let thread = std::thread::current();
        let name = thread.name().unwrap_or("<unnamed>");
        eprintln!("\n[mouseclaw] ╔══════════ PANIC ══════════");
        eprintln!("[mouseclaw] ║ thread: {name}");
        eprintln!("[mouseclaw] ║ {info}");
        eprintln!(
            "[mouseclaw] ║ backtrace:\n{}",
            std::backtrace::Backtrace::force_capture()
        );
        eprintln!("[mouseclaw] ╚═══════════════════════════\n");
        default(info);
    }));
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    init_file_logging();
    install_panic_hook();

    let cfg = config::Config::load();
    let app_state = Arc::new(AppState::new(cfg.backend).expect("init AppState"));

    tauri::Builder::default()
        .manage(app_state.clone())
        .plugin(tauri_plugin_opener::init())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(move |app, shortcut, event| {
                    let app_handle = app.clone();
                    let state: Arc<AppState> = app.state::<Arc<AppState>>().inner().clone();
                    match event.state() {
                        ShortcutState::Pressed => {
                            println!("[mouseclaw] 🦞 shortcut PRESS: {shortcut:?}");
                            show_mouse(app);
                            tauri::async_runtime::spawn(async move {
                                on_shortcut_press(app_handle, state).await;
                            });
                        }
                        ShortcutState::Released => {
                            println!("[mouseclaw] 🦞 shortcut RELEASE: {shortcut:?}");
                            tauri::async_runtime::spawn(async move {
                                on_shortcut_release(app_handle, state).await;
                            });
                        }
                    }
                })
                .build(),
        )
        .invoke_handler(tauri::generate_handler![
            commands::submit_query,
            commands::follow_up,
            commands::new_session,
            commands::save_shortcut,
            commands::cancel_pipeline,
            commands::toggle_recording,
            commands::pin_window,
            commands::dismiss,
            commands::read_history,
            commands::check_permissions,
            commands::request_permission,
            commands::restart_app,
        ])
        .setup(move |app| {
            set_accessory_activation_policy();
            println!("[mouseclaw] activation policy = Accessory (no dock icon)");
            println!("[mouseclaw] 后端 = {}", cfg.backend.display_name());
            emit_view(&app.handle(), &ViewKind::Idle);

            // Tray always available (escape valve before/during onboarding)
            if let Err(e) = tray::setup(&app.handle()) {
                eprintln!("[mouseclaw] tray setup failed: {e:#}");
            } else {
                println!("[mouseclaw] tray icon registered");
            }

            // Diagnostic: PATH + claude location
            println!(
                "[mouseclaw] inherited PATH = {}",
                std::env::var("PATH").unwrap_or_default()
            );
            match claude_cli::find_binary(cfg.backend.binary_name()) {
                Ok(p) => println!("[mouseclaw] backend binary: {}", p.display()),
                Err(_) => eprintln!(
                    "[mouseclaw] ⚠️  后端 `{}` 二进制未找到 —— 调用时会用拓宽 PATH 重试",
                    cfg.backend.binary_name()
                ),
            }

            // Background Whisper model download if missing
            transcribe::kick_off_download_if_missing();

            if cfg.onboarded {
                let perm = permissions::check_all();
                if !perm.accessibility {
                    eprintln!("[mouseclaw] ⚠️  辅助功能权限缺失 — 全局快捷键不会工作");
                }
                if !perm.screen_recording {
                    eprintln!("[mouseclaw] ⚠️  屏幕录制权限缺失 — 截图会失败");
                }
                if !perm.microphone {
                    eprintln!("[mouseclaw] ⚠️  麦克风权限缺失 — 录音会失败");
                }

                let shortcut = match Shortcut::from_str(&cfg.shortcut) {
                    Ok(s) => s,
                    Err(e) => {
                        eprintln!(
                            "[mouseclaw] ✘ 快捷键 {:?} 解析失败：{e} — 重弹 Onboarding",
                            cfg.shortcut
                        );
                        tray::open_onboarding_window(&app.handle());
                        return Ok(());
                    }
                };
                match app.global_shortcut().register(shortcut) {
                    Ok(()) => {
                        println!(
                            "[mouseclaw] ✓ 全局快捷键注册成功: {} (按住录音/松开发送)",
                            cfg.shortcut
                        );
                        println!("[mouseclaw] 🦞 ready — 按住 {} 开始说话", cfg.shortcut);
                    }
                    Err(e) => {
                        eprintln!("[mouseclaw] ✘ 快捷键 {} 注册失败：{e}", cfg.shortcut);
                        eprintln!("[mouseclaw]    → 可能被系统或其他 app 占用，重弹 Onboarding");
                        tray::open_onboarding_window(&app.handle());
                    }
                }
            } else {
                println!("[mouseclaw] first launch → opening Onboarding window");
                tray::open_onboarding_window(&app.handle());
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
