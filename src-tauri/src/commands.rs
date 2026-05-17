//! 所有 `#[tauri::command]` —— 前端 `invoke(...)` 的入口。
//! 业务逻辑在 pipeline.rs / overlay.rs / permissions.rs，这里只做参数转发。

use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_global_shortcut::Shortcut;

use crate::backend::Backend;
use crate::events::EV_SKIN_CHANGED;
use crate::overlay::{bump_gen, hide_overlay};
use crate::pipeline::{on_shortcut_release, run_pipeline};
use crate::skins::SkinId;
use crate::{audio, config, permissions};
use crate::AppState;

/// 文本输入框 / 重新提问 → 跑完整 pipeline。
#[tauri::command]
pub async fn submit_query(
    text: String,
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
) -> Result<(), String> {
    let state = state.inner().clone();
    let app2 = app.clone();
    tauri::async_runtime::spawn(async move { run_pipeline(text, app2, state).await });
    Ok(())
}

/// Panel 里的 follow-up —— 同 submit_query。
#[tauri::command]
pub async fn follow_up(
    text: String,
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
) -> Result<(), String> {
    let state = state.inner().clone();
    let app2 = app.clone();
    tauri::async_runtime::spawn(async move { run_pipeline(text, app2, state).await });
    Ok(())
}

/// 显式新建 session（Panel 的 ＋ 按钮）。
#[tauri::command]
pub async fn new_session(state: State<'_, Arc<AppState>>) -> Result<u64, String> {
    let mut store = state.sessions.lock().await;
    Ok(store.touch(true, None))
}

/// Onboarding 完成 —— 保存快捷键 + 后端选择 + onboarded 标记。
/// 真正的快捷键注册在重启后的 setup() 里做（那时屏幕录制权限也活了）。
#[tauri::command]
pub fn save_shortcut(
    choice: String,
    backend: String,
    skin: Option<String>,
    _app: AppHandle,
) -> Result<(), String> {
    let new_str = config::choice_to_shortcut_str(&choice).to_string();
    Shortcut::from_str(&new_str)
        .map_err(|e| format!("解析快捷键 {new_str:?} 失败：{e}"))?;

    let backend = Backend::from_choice(&backend);
    let skin = SkinId::from_str(skin.as_deref().unwrap_or(""));
    // 保留用户之前选的 whisper 模型 + 语言（重走 onboarding 不要被重置成 default）
    let prev = config::Config::load();
    let cfg = config::Config {
        shortcut: new_str.clone(),
        backend,
        skin,
        whisper_model: prev.whisper_model,
        language: prev.language,
        tidy_up_enabled: prev.tidy_up_enabled,
        voice_ime_enabled: prev.voice_ime_enabled,
        voice_ime_trigger: prev.voice_ime_trigger,
        clipboard_paused: prev.clipboard_paused,
        workspace_path: prev.workspace_path,
        onboarded: true,
        version: config::CURRENT_CONFIG_VERSION,
    };
    cfg.save().map_err(|e| format!("保存配置失败：{e}"))?;

    println!(
        "[mouseclaw] config saved → shortcut={new_str}, backend={:?}, skin={:?}, onboarded ✓ — 等待重启",
        backend, skin
    );
    Ok(())
}

/// 运行期切换桌宠皮肤 —— 托盘子菜单调它。
/// 1) 持久化进 config.json
/// 2) emit `skin-changed` 事件，前端立即换皮肤（不重启）
#[tauri::command]
pub fn save_skin(skin: String, app: AppHandle) -> Result<(), String> {
    let parsed = SkinId::from_str(&skin);
    let mut cfg = config::Config::load();
    cfg.skin = parsed;
    cfg.save().map_err(|e| format!("保存皮肤失败：{e}"))?;

    // 广播给所有 webview 窗口（overlay / history / about / onboarding 都监听）
    let payload = parsed.as_str().to_string();
    for (_, w) in app.webview_windows() {
        let _ = w.emit(EV_SKIN_CHANGED, payload.clone());
    }
    println!("[mouseclaw] skin saved → {:?} (已广播 skin-changed)", parsed);
    Ok(())
}

/// 启动时前端读当前皮肤 —— 避免每个窗口加载时闪一下默认皮再切换。
#[tauri::command]
pub fn get_skin() -> String {
    config::Config::load().skin.as_str().to_string()
}

/// P0a · 一键启用浏览器自动化：注册 MCP + 启动带 CDP 的 Chrome。
/// 用户在托盘或 onboarding 里点这个。
#[tauri::command]
pub fn enable_browser_automation() -> Result<(), String> {
    crate::browser_bridge::enable().map_err(|e| format!("{e:#}"))
}

/// 查询当前各能力的就绪状态 —— 托盘 / health UI 用。
#[derive(serde::Serialize)]
pub struct CapabilityStatus {
    pub claude_cli: bool,
    pub agent_browser: bool,
    pub chrome_cdp: bool,
}

/// 切换 Whisper 模型 —— 托盘 / status window 调它。
/// 1. 持久化进 config.json
/// 2. transcribe::set_active_model 重新加载 + 缺失则后台下载
#[tauri::command]
pub fn save_whisper_model(model: String) -> Result<(), String> {
    let parsed = config::WhisperModel::from_str(&model);
    let mut cfg = config::Config::load();
    cfg.whisper_model = parsed;
    cfg.save().map_err(|e| format!("保存失败：{e}"))?;
    crate::transcribe::set_active_model(parsed);
    println!("[mouseclaw] whisper 模型 → {:?}", parsed);
    Ok(())
}

#[tauri::command]
pub fn get_whisper_model() -> String {
    config::Config::load().whisper_model.as_str().to_string()
}

/// 切换 UI 语言 —— 任意窗口 / 托盘调它。
/// 1. 持久化进 config.json
/// 2. 广播 EV_LANG_CHANGED；前端 i18n module 监听切换，所有 UI 立即重渲染
#[tauri::command]
pub fn save_language(lang: String, app: AppHandle) -> Result<(), String> {
    // 简单校验：只接受已知的 lang id（防止脏数据进 config）
    let allowed = ["zh", "en"];
    if !allowed.contains(&lang.as_str()) {
        return Err(format!("不支持的语言：{lang} (允许：{:?})", allowed));
    }
    let mut cfg = config::Config::load();
    cfg.language = lang.clone();
    cfg.save().map_err(|e| format!("保存失败：{e}"))?;
    for (_, w) in app.webview_windows() {
        let _ = w.emit(crate::events::EV_LANG_CHANGED, lang.clone());
    }
    println!("[mouseclaw] 🌐 language → {lang}");
    Ok(())
}

#[tauri::command]
pub fn get_language() -> String {
    config::Config::load().language
}

/// 切换语音整理（Typeless 套路） · 托盘菜单 / 状态窗调用
#[tauri::command]
pub fn save_tidy_up(enabled: bool) -> Result<(), String> {
    let mut cfg = config::Config::load();
    cfg.tidy_up_enabled = enabled;
    cfg.save().map_err(|e| format!("保存失败：{e}"))?;
    println!("[mouseclaw] tidy_up_enabled → {enabled}");
    Ok(())
}

#[tauri::command]
pub fn get_tidy_up() -> bool {
    config::Config::load().tidy_up_enabled
}

/// 切换 voice IME（fn 长按写到光标）
#[tauri::command]
pub fn save_voice_ime(enabled: bool) -> Result<(), String> {
    let mut cfg = config::Config::load();
    cfg.voice_ime_enabled = enabled;
    cfg.save().map_err(|e| format!("保存失败：{e}"))?;
    crate::voice_ime::set_enabled(enabled);
    Ok(())
}

#[tauri::command]
pub fn get_voice_ime() -> bool {
    config::Config::load().voice_ime_enabled
}

/// 设置 voice IME 触发键
#[tauri::command]
pub fn save_voice_ime_trigger(trigger: String) -> Result<(), String> {
    // 校验是已知值
    let allowed = ["fn", "option", "control", "right-shift", "right-command", "right-option"];
    if !allowed.contains(&trigger.as_str()) {
        return Err(format!("未知 trigger: {trigger}（允许 {:?}）", allowed));
    }
    let mut cfg = config::Config::load();
    cfg.voice_ime_trigger = trigger.clone();
    cfg.save().map_err(|e| format!("保存失败：{e}"))?;
    crate::voice_ime::set_trigger(crate::voice_ime::ImeTrigger::from_str(&trigger));
    Ok(())
}

#[tauri::command]
pub fn get_voice_ime_trigger() -> String {
    config::Config::load().voice_ime_trigger
}

#[tauri::command]
pub fn save_clipboard_paused(paused: bool) -> Result<(), String> {
    let mut cfg = config::Config::load();
    cfg.clipboard_paused = paused;
    cfg.save().map_err(|e| format!("保存失败：{e}"))?;
    crate::clipboard::set_paused(paused);
    Ok(())
}

#[tauri::command]
pub fn get_clipboard_paused() -> bool {
    crate::clipboard::is_paused()
}

/// v0.1.21 · 设置工作区路径
/// 传 None / 空字符串 = 清除（回到默认 cwd）
#[tauri::command]
pub fn save_workspace_path(path: Option<String>) -> Result<(), String> {
    let mut cfg = config::Config::load();
    let p = path.and_then(|s| if s.trim().is_empty() { None } else { Some(s) });
    // 校验路径存在 + 是目录
    if let Some(ref pp) = p {
        let pb = std::path::PathBuf::from(pp);
        if !pb.is_dir() {
            return Err(format!("路径不存在或不是目录：{pp}"));
        }
    }
    cfg.workspace_path = p.clone();
    cfg.save().map_err(|e| format!("保存失败：{e}"))?;
    println!("[mouseclaw] 📁 workspace → {p:?}");
    Ok(())
}

#[tauri::command]
pub fn get_workspace_path() -> Option<String> {
    config::Config::load().workspace_path
}

// ────────────────── Clipboard history (v0.2) ──────────────────

#[tauri::command]
pub fn list_clipboard() -> Vec<crate::clipboard::ClipItem> {
    crate::clipboard::list_items()
}

#[tauri::command]
pub fn delete_clipboard_item(id: u64) -> Result<(), String> {
    crate::clipboard::delete_item(id).map_err(|e| format!("{e}"))
}

#[tauri::command]
pub fn toggle_clipboard_pin(id: u64) -> Result<(), String> {
    crate::clipboard::toggle_pin(id).map_err(|e| format!("{e}"))
}

#[tauri::command]
pub fn clear_clipboard() -> Result<(), String> {
    crate::clipboard::clear_all().map_err(|e| format!("{e}"))
}

/// 把某条剪贴板粘贴到原 app 光标 —— v0.1.18 双段焦点切换 + 粘贴
/// 流程：
///   1. 拿 text
///   2. 把开 Hub 前记下的 pid 显式 activate → 那个 app 重新成 frontmost
///   3. 等 80ms 让焦点稳定
///   4. mode_b::write_at_cursor —— 自动识别 native / Electron 走 CGEvent 或 clipboard ⌘V
#[tauri::command]
pub async fn paste_clipboard_item(
    id: u64,
    state: State<'_, Arc<AppState>>,
) -> Result<(), String> {
    let text = crate::clipboard::get_text(id)
        .ok_or_else(|| "条目不存在".to_string())?;

    // 拿出并清空 prev pid（一次性）
    let prev_pid = state.prev_frontmost_pid.lock().unwrap().take();
    #[cfg(target_os = "macos")]
    if let Some(pid) = prev_pid {
        let ok = crate::frontmost::activate_pid(pid);
        println!("[mouseclaw] 📋 paste: activate pid {pid} → {ok}");
        // 给 macOS 一点时间完成焦点切换 + window ordering
        tokio::time::sleep(std::time::Duration::from_millis(120)).await;
    }
    crate::mode_b::write_at_cursor(&text).await.map_err(|e| format!("{e}"))?;
    Ok(())
}

// ────────────────── Panel window (v0.1.10) ──────────────────

/// 「💬 继续追问」按钮 → 开一个独立的 720×560 Panel 窗口接管对话。
/// 1. 把当前对话上下文塞进 AppState.pending_panel_context
/// 2. 隐藏 overlay 窗口（mouse 那个透明小框）
/// 3. 开 / 聚焦 panel webview window
/// 前端 PanelView 挂载时 invoke `take_panel_context` 取出上下文 + 渲染
#[tauri::command]
pub fn open_panel_window(
    session_id: u64,
    transcript: String,
    reply: String,
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
) -> Result<(), String> {
    use tauri::WebviewWindowBuilder;
    use tauri::WebviewUrl;

    // 1. 存上下文
    *state.pending_panel_context.lock().unwrap() = Some(crate::PendingPanelContext {
        session_id, transcript, reply,
    });

    // 2. 隐藏 overlay —— 用户视觉焦点切到新窗口
    crate::overlay::hide_overlay(&app);

    // 3. 开 / 聚焦 panel 窗口
    if let Some(w) = app.get_webview_window("panel") {
        let _ = w.show();
        let _ = w.set_focus();
        // 已有窗口的话也要让它知道新上下文 —— emit 一下
        use tauri::Emitter;
        let _ = w.emit("panel-context-changed", ());
        return Ok(());
    }

    let result = WebviewWindowBuilder::new(
        &app, "panel",
        WebviewUrl::App("index.html?view=panel".into()),
    )
    .title("MouseClaw — 继续追问")
    .inner_size(480.0, 560.0)
    .min_inner_size(380.0, 380.0)
    .resizable(true).decorations(true).focused(true)
    .build();

    match result {
        Ok(w) => { let _ = w.set_focus(); Ok(()) }
        Err(e) => Err(format!("打开 panel 窗口失败：{e:#}")),
    }
}

/// PanelView 挂载时调用 —— 取走一次性上下文 + 清空 state
#[tauri::command]
pub fn take_panel_context(state: State<'_, Arc<AppState>>) -> Option<crate::PendingPanelContext> {
    state.pending_panel_context.lock().unwrap().take()
}

/// 内部入口 —— 非 #[tauri::command]，可从托盘 / 全局快捷键等地方调
/// v0.1.18：缩小窗口 420×480，开窗前记下用户当时的前台 app pid。
pub fn open_hub_window_inner(app: &AppHandle) -> Result<(), String> {
    use tauri::WebviewWindowBuilder;
    use tauri::WebviewUrl;

    // 1. 记下当前前台 app 的 pid（必须在我们抢焦点前做）
    #[cfg(target_os = "macos")] {
        let pid = crate::frontmost::current_frontmost_pid();
        if let Some(state) = app.try_state::<Arc<AppState>>() {
            *state.prev_frontmost_pid.lock().unwrap() = pid;
        }
        println!("[mouseclaw] 📋 open_hub: 记下 prev frontmost pid = {pid:?}");
    }

    crate::overlay::hide_overlay(app);

    if let Some(w) = app.get_webview_window("hub") {
        let _ = w.show();
        let _ = w.set_focus();
        return Ok(());
    }

    let result = WebviewWindowBuilder::new(
        app, "hub",
        WebviewUrl::App("index.html?view=hub".into()),
    )
    .title("MouseClaw — 剪贴板")
    .inner_size(420.0, 480.0)
    .min_inner_size(360.0, 360.0)
    .resizable(true)
    .decorations(true)
    .always_on_top(true)
    .focused(true)
    .build();

    match result {
        Ok(w) => { let _ = w.set_focus(); Ok(()) }
        Err(e) => Err(format!("打开 Hub 窗口失败：{e:#}")),
    }
}

/// Tauri 命令包装 —— 前端 invoke("open_hub_window") 用
#[tauri::command]
pub fn open_hub_window(app: AppHandle) -> Result<(), String> {
    open_hub_window_inner(&app)
}

#[tauri::command]
pub fn capability_status() -> CapabilityStatus {
    CapabilityStatus {
        claude_cli: crate::claude_cli::find_binary("claude").is_ok(),
        agent_browser: crate::claude_cli::find_binary("agent-browser").is_ok(),
        chrome_cdp: crate::browser_bridge::cdp_is_alive(),
    }
}

/// 取消当前 pipeline（cancel_pipeline）—— bump gen + 隐藏 overlay。
#[tauri::command]
pub fn cancel_pipeline(app: AppHandle, state: State<'_, Arc<AppState>>) -> Result<(), String> {
    bump_gen(&state.inner().clone());
    hide_overlay(&app);
    Ok(())
}

/// React 进入 Panel / sticky 状态时调，取消挂起的 3s 自动隐藏。
#[tauri::command]
pub fn pin_window(state: State<'_, Arc<AppState>>) -> Result<(), String> {
    let g = bump_gen(&state.inner().clone());
    println!("[mouseclaw] window pinned (gen → {g})");
    Ok(())
}

/// 用户按 Esc / 点窗口外 → 立即隐藏。
#[tauri::command]
pub fn dismiss(app: AppHandle, state: State<'_, Arc<AppState>>) -> Result<(), String> {
    bump_gen(&state.inner().clone());
    hide_overlay(&app);
    Ok(())
}

/// 前端查询当前权限状态（Onboarding 用）。三个 check 都是官方状态查询 API，
/// 纯只读、不弹窗、不阻塞 —— 直接同步调用。
#[tauri::command]
pub fn check_permissions() -> permissions::PermissionStatus {
    permissions::check_all()
}

/// 前端「去开启」按钮 —— 触发系统授权弹窗 + 打开设置面板。
///
/// 麦克风特殊：`AVCaptureDevice requestAccessForMediaType:` + block 回调不可靠，
/// 改为直接用 cpal 开一下输入流 —— macOS 见到 app 访问麦克风会立刻弹授权框。
#[tauri::command]
pub fn request_permission(name: String) {
    if name == "microphone" {
        std::thread::spawn(|| match audio::Recorder::start() {
            Ok(rec) => {
                std::thread::sleep(Duration::from_millis(400));
                let _ = rec.stop_and_take();
                println!("[mouseclaw] microphone prompt triggered via cpal input stream");
            }
            Err(e) => eprintln!("[mouseclaw] mic prompt trigger via cpal failed: {e:#}"),
        });
        permissions::open_prefs_for("microphone");
    } else {
        permissions::request_permission(&name);
    }
}

/// 重启 MouseClaw 自身。屏幕录制权限授权后必须重启才生效（macOS 设计）。
#[tauri::command]
pub fn restart_app(app: AppHandle) {
    println!("[mouseclaw] 重启 app（让屏幕录制权限生效）");
    app.restart();
}

/// ◼ Stop 按钮 / 松开快捷键的等价 —— 停止录音并跑 pipeline。
#[tauri::command]
pub async fn toggle_recording(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
) -> Result<(), String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn(async move { on_shortcut_release(app, state).await });
    Ok(())
}

// ────────────────── History ──────────────────

/// 读 ~/.mouseclaw/sessions.jsonl，按 session_id 分组，倒序返回给历史窗口。
#[tauri::command]
pub fn read_history() -> Result<Vec<HistorySession>, String> {
    use crate::sessions::TurnRecord;
    use std::collections::BTreeMap;

    let home = std::env::var_os("HOME").ok_or("HOME not set")?;
    let path = std::path::PathBuf::from(home).join(".mouseclaw/sessions.jsonl");
    if !path.exists() {
        return Ok(Vec::new());
    }
    let text = std::fs::read_to_string(&path).map_err(|e| format!("read: {e}"))?;
    let mut sessions: BTreeMap<u64, HistorySession> = BTreeMap::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Ok(record) = serde_json::from_str::<TurnRecord>(line) else {
            continue;
        };
        let entry = sessions
            .entry(record.session_id)
            .or_insert_with(|| HistorySession {
                session_id: record.session_id,
                started_at: record.timestamp,
                ended_at: record.timestamp,
                turns: Vec::new(),
            });
        entry.ended_at = record.timestamp;
        entry.turns.push(HistoryTurn {
            role: format!("{:?}", record.role).to_lowercase(),
            text: record.text,
            timestamp: record.timestamp,
            screenshot: record.screenshot,
        });
    }
    let mut list: Vec<HistorySession> = sessions.into_values().collect();
    list.sort_by(|a, b| b.started_at.cmp(&a.started_at));
    Ok(list)
}

#[derive(serde::Serialize)]
pub struct HistorySession {
    pub session_id: u64,
    pub started_at: chrono::DateTime<chrono::Utc>,
    pub ended_at: chrono::DateTime<chrono::Utc>,
    pub turns: Vec<HistoryTurn>,
}

#[derive(serde::Serialize)]
pub struct HistoryTurn {
    pub role: String,
    pub text: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub screenshot: Option<String>,
}
