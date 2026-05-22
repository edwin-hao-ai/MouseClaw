//! MouseClaw 主入口。
//!
//! 模块拆分（每个文件 ≤800 行，见 CLAUDE.md 规则）：
//!   - `overlay`     —— 老鼠窗口显示/隐藏 + 视图事件广播 + 自动隐藏
//!   - `pipeline`    —— 截屏+转写+AI+输出模式 的核心流程；push-to-talk handler
//!   - `commands`    —— 所有 #[tauri::command]
//!   - `backend`     —— 多 AI 后端抽象（Claude / Codex / Gemini / Copilot / OpenCode /
//!                       Cline / Kimi / Kiro / Antigravity / Vibe / Pi / OpenClaw / Hermes）
//!   - `claude_cli`  —— Claude Code CLI 流式调用
//!   - `audio` / `transcribe_stream` —— cpal 录音 + sherpa-onnx 流式转写
//!   - `screenshot` / `mode_b` / `permissions` / `sessions` / `config` / `tray` / `events`
//!
//! 完整架构见 `/Users/edwinhao/MouseClaw/CLAUDE.md`。

pub mod ai_queue;
pub mod anchor;
pub mod audio;
pub mod backend;
pub mod backend_menu;
pub mod browser_bridge;
pub mod nudge;
pub mod presence;
pub mod punctuation;
#[cfg(target_os = "macos")]
pub mod tts;
pub mod transcribe_stream;
pub mod model_downloader;
pub mod claude_cli;
pub mod cli_install;
pub mod clipboard;
pub mod clipboard_action;
pub mod clipboard_crypto;
pub mod cursor_follow;
pub mod cursor_trail;
pub mod drag_detector;
pub mod companion;
pub mod entrance;
pub mod pet_passthrough;
pub mod overlay_size;
pub mod commands;
pub mod config;
pub mod memory;
pub mod events;
pub mod feed;
pub mod feed_flow;
pub mod frontmost;
pub mod mode_b;
pub mod overlay;
pub mod permissions;
pub mod pipeline;
pub mod provider_env;
pub mod reactive;
pub mod schedule;
pub mod scheduler;
pub mod screenshot;
pub mod selection;
pub mod sessions;
pub mod skins;
pub mod tidy_up;
pub mod update_check;
pub mod shortcut_menu;
pub mod voice_ime;
pub mod voice_correct;
pub mod vocab;
// transcribe (Whisper) deleted in v0.3 — superseded by transcribe_stream (sherpa-onnx)
pub mod tray;
pub mod tray_menu;
pub mod tray_handlers;
pub mod tray_actions;
pub mod tray_windows;

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
/// v0.3.11 · 加 `turns` 字段 —— 恢复历史 session 时把全部历史塞进来，
///   Panel 能渲染完整对话而不是只显示最后一对 user/assistant。
///   bubble 的「继续追问」入口不填 turns（None），Panel 退化到只渲染 transcript+reply。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PendingPanelContext {
    pub session_id: u64,
    pub transcript: String,
    pub reply: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turns: Option<Vec<crate::events::Turn>>,
}

pub struct AppState {
    pub sessions: Mutex<SessionStore>,
    pub last_screenshot: Mutex<Option<std::path::PathBuf>>,
    /// v0.1.20 · 上一次 AI 召唤的鼠标轨迹（已烘到 screenshot 上 + 送 prompt）
    pub last_trail: Mutex<Option<Vec<cursor_trail::TrailPoint>>>,
    pub recorder: StdMutex<Option<audio::Recorder>>,
    /// v0.2 · 当前流式转录 session —— on_press 创建，on_release 取出 finalize。
    /// Sync mutex 因为 sherpa OnlineStream 是 Send+Sync 但不是 async-friendly。
    pub stream_session: StdMutex<Option<transcribe_stream::StreamSession>>,
    /// v0.2 · streaming polling task 通过这个 flag 知道何时退出
    pub streaming_active: AtomicBool,
    /// v0.3.1 · voice IME 流式 type-as-you-speak —— poller 实时更新已 paste 的字符串。
    /// stop_and_paste 拿它跟 final cleaned 求 LCP，仅删/补 delta，避免删除整段+重写的闪烁。
    pub ime_typed: StdMutex<String>,
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
    /// v0.4 · 用户刚拖给桌宠的文件（drop → ingest → 放这里 → run_pipeline 取走 + 清空）。
    /// 一次 feed 周期独占；新一次 drop 会替换。
    pub fed_docs: Mutex<Option<feed::FeedBundle>>,
    /// v0.4 · 用来防抖 drag-enter（macOS 在拖动期间会反复 enter/leave）。
    pub feed_drag_active: AtomicBool,
    /// v0.4 · drag-leave 防抖：记最后一次"打算 leave"的时刻，
    /// 200ms 内 re-enter 就 cancel 这次 leave（避免动画闪烁）。
    pub feed_last_leave: StdMutex<Option<std::time::Instant>>,
    /// v0.3.12 · overlay 是否处于"有 UI 显示"状态 —— pet_passthrough 用它决定 hit-box：
    ///   false (idle 静默) → 只有右下桌宠区接收点击（其余区域穿透到底层 app）
    ///   true (任何气泡/菜单/下载提示) → 整个窗口接收点击
    /// emit_view 时同步更新；下载中显示迷你气泡时由 commands 单独设 true。
    pub overlay_has_ui: AtomicBool,
    /// v0.4.0 · 语音确认状态 —— 转写完进 3 秒倒数，用户可 Esc 取消 / Enter 立即发 /
    /// 编辑后发。`None` = 没在等用户操作；`Some(action)` = 用户已经做了选择，
    /// 倒数 task 读到立即 break。简化版用 AtomicU8 表示三种状态。
    pub voice_confirm_action: std::sync::atomic::AtomicU8,
    /// v0.4.0 · 语音确认当前文本 —— commands::voice_confirm_edit 改写它
    pub voice_confirm_text: Mutex<Option<String>>,
    /// v0.4.x · session 是否钉住的同步镜像 —— tray（sync 上下文）读它显示
    /// 「钉住 / 解除钉住」标签，避免去 lock tokio::Mutex 的 SessionStore。
    /// 真值在 SessionStore.pinned；toggle_pin_session 同步更新这个镜像。
    pub session_pinned: AtomicBool,
    /// v0.5 · 开场入场动画的取消令牌（generation）。entrance::play 认领时 bump 并
    /// 每帧自检；用户按召唤快捷键 / 拖文件 → entrance::abort bump 它 → 进行中的入场
    /// 下一帧自检失败立刻停 + 归位让位。见 `entrance.rs`。
    pub entrance_gen: AtomicU64,
    /// v0.5 · 前端启动上报的 prefers-reduced-motion 偏好。入场动画据此决定是否
    /// 跳过横穿/蹦跶（直接让桌宠出现在 anchor）。默认 false（全动画）。
    pub reduced_motion: AtomicBool,
}

/// v0.4.0 · 语音确认动作枚举
pub const VC_PENDING: u8 = 0;
pub const VC_SEND_NOW: u8 = 1;
pub const VC_CANCEL: u8 = 2;
/// v0.3.11 · 用户开始编辑文本 → 暂停倒数，等用户主动 Enter/Esc 才走
pub const VC_HOLD: u8 = 3;

impl AppState {
    fn new(backend: Backend) -> anyhow::Result<Self> {
        Ok(Self {
            sessions: Mutex::new(SessionStore::new()?),
            last_screenshot: Mutex::new(None),
            last_trail: Mutex::new(None),
            recorder: StdMutex::new(None),
            stream_session: StdMutex::new(None),
            streaming_active: AtomicBool::new(false),
            ime_typed: StdMutex::new(String::new()),
            pending_panel_context: StdMutex::new(None),
            prev_frontmost_pid: StdMutex::new(None),
            backend: Mutex::new(backend),
            gen: AtomicU64::new(0),
            follow_cursor: AtomicBool::new(false),
            nudge_state: Arc::new(StdRwLock::new(crate::nudge::NudgeState::new())),
            fed_docs: Mutex::new(None),
            feed_drag_active: AtomicBool::new(false),
            overlay_has_ui: AtomicBool::new(false),
            feed_last_leave: StdMutex::new(None),
            voice_confirm_action: std::sync::atomic::AtomicU8::new(VC_PENDING),
            voice_confirm_text: Mutex::new(None),
            session_pinned: AtomicBool::new(false),
            entrance_gen: AtomicU64::new(0),
            reduced_motion: AtomicBool::new(false),
        })
    }
}

/// Release builds run as a bundled GUI app where stdout/stderr go nowhere.
/// Redirect both to `~/.mouseclaw/mouseclaw.log` so users (and us) can debug.
/// Dev builds (`bun tauri dev`) keep console output — no redirect.
///
/// 跨平台：macOS + Linux 都走 unix `dup2`（libc 是 unix 依赖）。Windows GUI 子系统
/// 无 console，且无 `dup2`；release 版日志暂不落盘（已知降级，见 cross-platform-port doc）。
#[cfg(all(unix, not(debug_assertions)))]
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

#[cfg(not(all(unix, not(debug_assertions))))]
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
        // v0.4 fix (2026-05-20)：喂文件的 enter/leave/drop **统一走 drag_detector**
        // （NSPasteboard 轮询 + 鼠标键状态）。之前这里用 Tauri 的 WindowEvent::DragDrop，
        // 但 WKWebView 会把拖进来的 HTML 文件当导航请求拦截，drop 喂不进去（图片正常）。
        // drag_detector 在 OS 层判定 drop，不分文件类型，所有类型一视同仁。
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
                    // 语音确认倒数期间临时注册的全局 Esc → 取消。
                    // overlay 是非激活 NSPanel，物理 Esc 到不了 webview，必须全局捕获。
                    // （注册/注销在 pipeline::voice_confirm_countdown 里，只在倒数那几秒生效）
                    if sk.contains("Escape") {
                        if event.state() == ShortcutState::Pressed {
                            state.voice_confirm_action
                                .store(crate::VC_CANCEL, std::sync::atomic::Ordering::SeqCst);
                            println!("[mouseclaw] ⎋ voice-confirm Esc → 取消");
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
            commands::toggle_pin_session,
            commands::get_session_state,
            commands::open_session_panel,
            commands::set_overlay_focusable,
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
            commands::get_pet_identity,
            commands::save_pet_identity,
            commands::open_memory_window,
            memory::memory_list_turns,
            memory::memory_get_profile,
            memory::memory_get_graph,
            memory::memory_delete_turn,
            memory::memory_delete_profile_item,
            memory::memory_clear_all,
            memory::memory_get_settings,
            memory::memory_set_paused,
            memory::memory_set_enabled,
            commands::get_sfx_config,
            commands::enable_browser_automation,
            commands::capability_status,
            commands::install_cli,
            commands::show_status_window,
            commands::save_language,
            commands::get_language,
            commands::list_clipboard,
            commands::delete_clipboard_item,
            commands::toggle_clipboard_pin,
            commands::clear_clipboard,
            commands::paste_clipboard_item,
            commands::open_panel_window,
            commands::take_panel_context,
            commands::resume_session,
            commands::open_downloader_window,
            commands::retry_model_downloads,
            commands::get_model_status,
            commands::voice_confirm_send,
            commands::voice_confirm_cancel,
            commands::voice_confirm_edit,
            commands::voice_confirm_hold,
            commands::tour_start,
            commands::tour_advance,
            commands::tour_skip,
            commands::get_tour_done,
            commands::open_hub_window,
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
            commands::open_accessibility_settings,
            commands::set_overlay_has_ui,
            commands::set_overlay_content_size,
            commands::report_reduced_motion,
            commands::get_pet_menu_orientation,
            commands::save_pet_custom_position,
            commands::set_nap_until,
            commands::dismiss_nudge,
            commands::check_backend_installed,
            commands::open_picker_window,
            commands::vocab_open_user_file,
            commands::vocab_reload,
            commands::vocab_get_builtin_enabled,
            commands::vocab_set_builtin_enabled,
            commands::open_tasks_window,
            commands::list_schedules,
            commands::create_schedule,
            commands::update_schedule,
            commands::delete_schedule,
            commands::toggle_schedule,
            commands::run_schedule_now,
            commands::get_schedule_runs,
            commands::parse_schedule_phrase,
            clipboard_action::process_reactive_action,
        ])
        .setup(move |app| {
            set_accessory_activation_policy();
            println!("[mouseclaw] activation policy = Accessory (no dock icon)");
            println!("[mouseclaw] 后端 = {}", cfg.backend.display_name());
            emit_view(&app.handle(), &ViewKind::Idle);

            // v0.4.4 · 长期记忆建库(失败只 log,不拖垮启动)。
            if let Err(e) = memory::init() {
                eprintln!("[mouseclaw] memory init failed (记忆功能本次禁用): {e:#}");
            }
            // v0.4.4 · reflection 定时巩固:每 30 min,有未消化且空闲时蒸馏画像/图谱。
            // (pipeline 攒够 6 条也会即时触发;这个定时器兜底处理零散积压。)
            tauri::async_runtime::spawn(async {
                loop {
                    tokio::time::sleep(std::time::Duration::from_secs(1800)).await;
                    if memory::unprocessed_count() > 0 && !ai_queue::is_busy() {
                        let _ = memory::run_reflection().await;
                    }
                    memory::prune(); // 遗忘/剪枝,防长期膨胀
                }
            });

            // Tray always available (escape valve before/during onboarding)
            // v0.1.8 启动 cursor-follow 后台任务（30fps；由 AtomicBool 控制开 / 关）
            cursor_follow::spawn_follow_loop(app.handle().clone(), app_state.clone());

            // v0.3.12 · 启动 overlay 穿透 hit-test —— 让透明区域真的不挡底层 app 点击。
            // 20fps 后台 task：光标在桌宠 hit-box 内 = 接收事件；在透明区 = 穿透。
            pet_passthrough::spawn_loop(app.handle().clone(), app_state.clone());

            // v0.4.x · 全屏隐形 drag-detector —— 用户从屏幕任意位置拖文件，
            //   detector 收 NSDragging enter → feed_flow::on_drag_enter →
            //   桌宠跑过去迎接（puppy greets drag）。
            //   纯 NSWindow + NSView 子类，无 WebKit backing，零视觉遮挡。
            drag_detector::install(app.handle().clone(), app_state.clone());

            // v0.4+ · 陪伴向动画 tick —— 30 FPS 推 mouse 窗口全局光标 + idle 秒数
            // 隐私同 presence：只读 CGEventSourceSecondsSince… 和 NSEvent.mouseLocation
            companion::spawn(app.handle().clone());

            // v0.1.27 P3 · 启动环境感知 + 主动提醒
            // presence 每 30s 采样到 30min ring buffer；nudge 每 60s 评估规则
            // 隐私红线：只数键击频率 / 鼠标移动时间戳 / 前台 bundle id —— 不读内容
            let presence_buf = presence::spawn(app.handle().clone());
            nudge::spawn(
                app.handle().clone(),
                presence_buf,
                app_state.nudge_state.clone(),
            );

            // v0.5 · 定时任务调度循环 —— 每 60s 扫 schedules.json 跑到点的任务。
            // tick 内部自查 onboarded + backend，未配置时静默 no-op。
            scheduler::spawn(app.handle().clone());

            // v0.2 启动剪贴板历史捕获 —— 500ms 轮询 changeCount
            // v0.4 · reactive 模块挂 AppHandle，clipboard 新条目时按 tier 发 event
            reactive::init(app.handle().clone());
            clipboard::spawn_capture_loop();
            // v0.4 · 选区轮询 —— Accessibility API 监听 AXSelectedText
            // 没 AX 权限时线程自己 skip，等用户授权后自动生效
            selection::spawn_capture_loop();
            // 用户上次会话设过暂停的话，恢复状态
            if cfg.clipboard_paused {
                clipboard::set_paused(true);
                selection::set_paused(true);
            }

            // v0.1.11 启动 fn 长按监听 —— CGEventTap on FlagsChanged
            #[cfg(target_os = "macos")]
            voice_ime::spawn(app.handle().clone(), app_state.clone());

            // v0.1.24 启动 60s 后做一次版本检查
            update_check::spawn(app.handle().clone());

            // v0.4.x · 老用户升级发现性 —— 启动 8s 后，若已 onboarded 但缺
            // browser/office CLI 且没弹过，弹一次性 nudge 引导去状态页装。
            cli_install::maybe_hint_upgrade(app.handle().clone());

            // v0.4.4 · 记忆默认开 → 首次一次性透明告知(本地存、可看可删)。
            memory::maybe_show_memory_intro(app.handle().clone());
            // v0.4.4 · 还没起名的桌宠 → 一次性提示去起名(20s 后,错峰于记忆告知)。
            commands::maybe_show_name_hint(app.handle().clone());

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

            // v0.4.0 · sherpa 模型按需下载（DMG 不再打包，~260MB → 35MB）。
            //   只在已 onboarded 用户启动时跑 —— 否则首次安装会下了 199MB
            //   zh-en 才发现用户在 onboarding 选了 English，前面下的全废。
            //   未 onboarded 由 commands::save_shortcut 在 onboarding 完成时调起。
            if cfg.onboarded {
                transcribe_stream::kick_off_download_if_missing(app.handle().clone());
                punctuation::kick_off_download_if_missing(app.handle().clone());
            }

            // v0.4.0 P1 · 启动时确保术语表用户文件存在 + 重新生成 active.txt。
            // 失败不阻塞启动，sherpa 在缺 hotwords_file 时退回 greedy decoding。
            if let Err(e) = vocab::ensure_user_file() {
                eprintln!("[mouseclaw] vocab init failed: {e}");
            }
            match vocab::regenerate_active(cfg.vocab_builtin_enabled) {
                Ok(n) => println!("[mouseclaw] 📝 vocab active.txt regenerated: {n} entries"),
                Err(e) => eprintln!("[mouseclaw] vocab regen failed: {e}"),
            }

            // v0.1.26 · --minimized 由 autostart plugin 在登录启动时传入。
            // 提到这里统一算一次：决定 idle 落点 / 入场档位 / Onboarding 是否弹都要它。
            let minimized = std::env::args().any(|a| a == "--minimized");
            // v0.5 · 本次启动是否要播开场入场动画 —— 要播的话**不**先把桌宠摆到角落
            //   （那样会"角落闪一下 → 又被拽去屏外 → 再冲进来"），让 entrance 自己摆窗口，
            //   窗口在入场起跑前保持隐藏（tauri.conf visible:false）。
            let will_play_entrance = entrance::decide_tier(&cfg, minimized).is_some();

            // v0.1.27 · 已 onboarded 的用户：启动时把桌宠送到 anchor 位置打盹。
            // Follow 模式跳过 —— 由 cursor_follow 接管。入场要播的话也跳过（entrance 接管）。
            if cfg.onboarded && cfg.pet_anchor.pin_visible_when_idle() && !will_play_entrance {
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
                // v0.5 · 开场调皮入场动画 —— 按场景分档（首次炸 / 冷启中 / 自启轻）。
                //   内部判定档位 + 延迟 ~700ms 起跑（等前端挂载 + 上报 reduced-motion）。
                //   anchor=Follow/Hidden 或没 onboarded → 自动不播。见 entrance.rs。
                entrance::maybe_play_on_launch(app.handle().clone(), &cfg, minimized);
            } else {
                // v0.1.26 · --minimized 由 autostart plugin 在登录启动时传入。
                // 这种情况下用户没有主动启动，应保持完全静默：不弹 Onboarding，
                // 不显示任何窗口，只在菜单栏挂着等快捷键召唤。
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
