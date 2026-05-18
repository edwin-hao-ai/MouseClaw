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

pub mod anchor;
pub mod audio;
pub mod backend;
pub mod browser_bridge;
pub mod nudge;
pub mod presence;
pub mod claude_cli;
pub mod clipboard;
pub mod clipboard_crypto;
pub mod cursor_follow;
pub mod cursor_trail;
pub mod commands;
pub mod config;
pub mod events;
pub mod frontmost;
pub mod mode_b;
pub mod overlay;
pub mod permissions;
pub mod pipeline;
pub mod provider_env;
pub mod screenshot;
pub mod sessions;
pub mod skins;
pub mod tidy_up;
pub mod update_check;
pub mod voice_ime;
pub mod transcribe;
pub mod tray;

use std::str::FromStr;
use std::sync::atomic::{AtomicBool, AtomicU64};
use std::sync::{Arc, Mutex as StdMutex, RwLock as StdRwLock};
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
/// 「💬 继续追问」按钮 → 新开 Panel 窗口时携带的对话上下文
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PendingPanelContext {
    pub session_id: u64,
    pub transcript: String,
    pub reply: String,
}

pub struct AppState {
    pub sessions: Mutex<SessionStore>,
    pub last_screenshot: Mutex<Option<std::path::PathBuf>>,
    /// v0.1.20 · 上一次 AI 召唤的鼠标轨迹（已烘到 screenshot 上 + 送 prompt）
    pub last_trail: Mutex<Option<Vec<cursor_trail::TrailPoint>>>,
    pub recorder: StdMutex<Option<audio::Recorder>>,
    /// One-shot panel context —— open_panel_window 写入，前端 take_panel_context 取走 + 清空
    pub pending_panel_context: StdMutex<Option<PendingPanelContext>>,
    /// v0.1.18 · 打开 Hub 前记下当时的前台 app pid
    /// 用户点条目粘贴时，先 activate 这个 pid 让原 app 重新成 frontmost，再 ⌘V
    pub prev_frontmost_pid: StdMutex<Option<i32>>,
    /// 当前选用的 AI 后端 —— pipeline 每次调用前 .lock().await.clone() 读取。
    /// 启动时从 config 灌入；运行期不变（改后端要重新 onboard + 重启）。
    pub backend: Mutex<Backend>,
    /// Monotonic counter — bumped on any new shortcut press / pipeline run /
    /// pin / dismiss. Auto-hide timers capture the gen at scheduling time and
    /// no-op if it changed by the time they fire.
    pub gen: AtomicU64,
    /// 桌宠跟随鼠标开关 —— cursor_follow 后台任务 30fps 读它。
    /// emit_view 进 listening/idle 时 = true；进 thinking/reply/panel 时 = false。
    pub follow_cursor: AtomicBool,
    /// v0.1.27 P3 · Nudge 引擎状态（last-fired 时间戳 + nap-until）。
    /// presence::spawn 返回的 PresenceBuffer 也存到这里，commands 能读。
    pub nudge_state: Arc<StdRwLock<crate::nudge::NudgeState>>,
}

impl AppState {
    fn new(backend: Backend) -> anyhow::Result<Self> {
        Ok(Self {
            sessions: Mutex::new(SessionStore::new()?),
            last_screenshot: Mutex::new(None),
            last_trail: Mutex::new(None),
            recorder: StdMutex::new(None),
            pending_panel_context: StdMutex::new(None),
            prev_frontmost_pid: StdMutex::new(None),
            backend: Mutex::new(backend),
            gen: AtomicU64::new(0),
            follow_cursor: AtomicBool::new(false),
            nudge_state: Arc::new(StdRwLock::new(crate::nudge::NudgeState::new())),
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
        // v0.1.26 · 开机自启动。--minimized 标志在 main.rs 检测，启动时不弹任何窗口
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec!["--minimized"]),
        ))
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(move |app, shortcut, event| {
                    let app_handle = app.clone();
                    let state: Arc<AppState> = app.state::<Arc<AppState>>().inner().clone();
                    let sk = format!("{shortcut:?}");
                    // v0.1.12 ⌘⇧V → 打开 Hub 剪贴板（v0.1.16 改成独立窗口）
                    if sk.contains("KeyV") || sk.contains("Char(\"v\")") || sk.contains("Code(V)") {
                        if event.state() == ShortcutState::Pressed {
                            println!("[mouseclaw] 📋 ⌘⇧V → 打开 Hub 窗口");
                            if let Err(e) = commands::open_hub_window(app.clone()) {
                                eprintln!("[mouseclaw] open_hub_window: {e}");
                            }
                        }
                        return;
                    }
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
            commands::start_recording,
            commands::pin_window,
            commands::dismiss,
            commands::read_history,
            commands::check_permissions,
            commands::request_permission,
            commands::restart_app,
            commands::save_skin,
            commands::get_skin,
            commands::enable_browser_automation,
            commands::capability_status,
            commands::save_whisper_model,
            commands::get_whisper_model,
            commands::save_language,
            commands::get_language,
            commands::list_clipboard,
            commands::delete_clipboard_item,
            commands::toggle_clipboard_pin,
            commands::clear_clipboard,
            commands::paste_clipboard_item,
            commands::open_panel_window,
            commands::take_panel_context,
            commands::open_hub_window,
            commands::save_tidy_up,
            commands::get_tidy_up,
            commands::save_voice_ime,
            commands::get_voice_ime,
            commands::save_voice_ime_trigger,
            commands::get_voice_ime_trigger,
            commands::save_clipboard_paused,
            commands::get_clipboard_paused,
            commands::save_workspace_path,
            commands::get_workspace_path,
            commands::set_autostart,
            commands::get_autostart,
            commands::save_pet_anchor,
            commands::get_pet_anchor,
            commands::set_nap_until,
            commands::dismiss_nudge,
            commands::check_backend_installed,
            commands::open_picker_window,
        ])
        .setup(move |app| {
            set_accessory_activation_policy();
            println!("[mouseclaw] activation policy = Accessory (no dock icon)");
            println!("[mouseclaw] 后端 = {}", cfg.backend.display_name());
            emit_view(&app.handle(), &ViewKind::Idle);

            // Tray always available (escape valve before/during onboarding)
            // v0.1.8 启动 cursor-follow 后台任务（30fps；由 AtomicBool 控制开 / 关）
            cursor_follow::spawn_follow_loop(app.handle().clone(), app_state.clone());

            // v0.1.27 P3 · 启动环境感知 + 主动提醒
            // presence 每 30s 采样到 30min ring buffer；nudge 每 60s 评估规则
            // 隐私红线：只数键击频率 / 鼠标移动时间戳 / 前台 bundle id —— 不读内容
            let presence_buf = presence::spawn(app.handle().clone());
            nudge::spawn(
                app.handle().clone(),
                presence_buf,
                app_state.nudge_state.clone(),
            );

            // v0.2 启动剪贴板历史捕获 —— 500ms 轮询 changeCount
            clipboard::spawn_capture_loop();
            // 用户上次会话设过暂停的话，恢复状态
            if cfg.clipboard_paused {
                clipboard::set_paused(true);
            }

            // v0.1.11 启动 fn 长按监听 —— CGEventTap on FlagsChanged
            #[cfg(target_os = "macos")]
            voice_ime::spawn(app.handle().clone(), app_state.clone());

            // v0.1.24 启动 60s 后做一次版本检查
            update_check::spawn(app.handle().clone());

            if let Err(e) = tray::setup(&app.handle()) {
                eprintln!("[mouseclaw] tray setup failed: {e:#}");
            } else {
                println!("[mouseclaw] tray icon registered");
            }

            // v0.1.26 · 双向同步 autostart 状态。tauri-plugin-autostart 的 plist
            // 是真实信源；config 里只是冗余镜像，方便 UI 决定 checkbox 状态。
            //   - 用户在系统设置→登录项 里手动关掉 → 这里检测到回写 config
            //   - config 说该开 / 该关但系统不一致 → 以 config 为准 enable()/disable()
            //   - 首启动（config 默认 true）→ 自动 enable() 一次，写入 LaunchAgent
            #[allow(unused_imports)]
            use tauri_plugin_autostart::ManagerExt;
            let autolaunch = app.autolaunch();
            let system_enabled = autolaunch.is_enabled().unwrap_or(false);
            let cfg_says = config::Config::load().autostart;
            if cfg_says != system_enabled {
                if cfg_says {
                    if let Err(e) = autolaunch.enable() {
                        eprintln!("[mouseclaw] autostart enable failed: {e}");
                    } else {
                        println!("[mouseclaw] ✅ autostart enabled (LaunchAgent installed)");
                    }
                } else {
                    let _ = autolaunch.disable();
                    println!("[mouseclaw] autostart disabled per config");
                }
                // 反向回写：以系统实际为准
                let actual = autolaunch.is_enabled().unwrap_or(false);
                if actual != cfg_says {
                    let mut c = config::Config::load();
                    c.autostart = actual;
                    let _ = c.save();
                    println!("[mouseclaw] autostart config ← system actual ({actual})");
                }
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

            // v0.1.27 · 已 onboarded 的用户：启动时把桌宠送到 anchor 位置打盹。
            // Follow 模式跳过 —— 由 cursor_follow 接管。
            if cfg.onboarded && cfg.pet_anchor.pin_visible_when_idle() {
                anchor::apply_idle_anchor(&app.handle(), cfg.pet_anchor);
                println!(
                    "[mouseclaw] 🦞 pet pinned to {} (idle anchor)",
                    cfg.pet_anchor.as_str()
                );
            }

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
                // v0.1.12 · 额外注册 ⌘⇧V → 打开 Hub 剪贴板（生态约定，Raycast/Paste 都用这个）
                if let Ok(hub_shortcut) = Shortcut::from_str("Super+Shift+KeyV") {
                    match app.global_shortcut().register(hub_shortcut) {
                        Ok(()) => println!("[mouseclaw] ✓ ⌘⇧V → Hub 剪贴板 注册成功"),
                        Err(e) => eprintln!("[mouseclaw] ⌘⇧V 注册失败（可能被其它 app 占）：{e}"),
                    }
                }
            } else {
                // v0.1.26 · --minimized 由 autostart plugin 在登录启动时传入。
                // 这种情况下用户没有主动启动，应保持完全静默：不弹 Onboarding，
                // 不显示任何窗口，只在菜单栏挂着等快捷键召唤。
                let minimized = std::env::args().any(|a| a == "--minimized");
                if minimized {
                    println!("[mouseclaw] launched via autostart (--minimized) · staying silent");
                } else {
                    println!("[mouseclaw] first launch → opening Onboarding window");
                    tray::open_onboarding_window(&app.handle());
                }
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
