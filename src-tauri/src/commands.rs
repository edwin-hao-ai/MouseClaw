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
use crate::pipeline::{on_shortcut_press, on_shortcut_release, run_pipeline};
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

/// 显式新建 session（Panel 的 ＋ 按钮 / 菜单「🆕 新对话」/ 双击快捷键）。
/// 钉住任务时 touch(true) 会被忽略 —— 返回 `pinned=true` 让前端提示"先解钉"。
#[tauri::command]
pub async fn new_session(app: AppHandle, state: State<'_, Arc<AppState>>) -> Result<bool, String> {
    let snap = {
        let mut store = state.sessions.lock().await;
        if store.is_pinned() {
            // 钉住中：拒绝清空，保护长任务
            return Ok(true);
        }
        store.touch(true, None);
        store.state_snapshot()
    };
    let _ = app.emit(crate::events::EV_SESSION_STATE, snap);
    Ok(false)
}

/// 钉住 / 解除钉住当前会话（菜单「📌 钉住任务」）。返回钉住后的状态。
#[tauri::command]
pub async fn toggle_pin_session(app: AppHandle, state: State<'_, Arc<AppState>>) -> Result<bool, String> {
    let (now_pinned, snap) = {
        let mut store = state.sessions.lock().await;
        if store.is_pinned() {
            store.unpin();
        } else {
            // 用当前前台 app 名当任务标签（拿不到就 None）
            let label = crate::mode_b::frontmost_app_name();
            store.pin(label);
        }
        (store.is_pinned(), store.state_snapshot())
    };
    // 同步镜像给 tray（sync 上下文读）
    state.session_pinned.store(now_pinned, std::sync::atomic::Ordering::Relaxed);
    let _ = app.emit(crate::events::EV_SESSION_STATE, snap);
    crate::tray::rebuild_tray_menu(&app);
    println!("[mouseclaw] 📌 session pinned → {now_pinned}");
    Ok(now_pinned)
}

/// 前端启动 / 窗口挂载时读一次当前 session 状态（链条图标初始化）。
#[tauri::command]
pub async fn get_session_state(state: State<'_, Arc<AppState>>) -> Result<crate::events::SessionState, String> {
    let store = state.sessions.lock().await;
    Ok(store.state_snapshot())
}

/// Onboarding 完成 —— 保存快捷键 + 后端选择 + onboarded 标记。
/// 真正的快捷键注册在重启后的 setup() 里做（那时屏幕录制权限也活了）。
#[tauri::command]
pub fn save_shortcut(
    choice: String,
    backend: String,
    skin: Option<String>,
    voice_lang: Option<String>,
    app: AppHandle,
) -> Result<(), String> {
    let new_str = config::choice_to_shortcut_str(&choice).to_string();
    Shortcut::from_str(&new_str)
        .map_err(|e| format!("解析快捷键 {new_str:?} 失败：{e}"))?;

    let backend = Backend::from_choice(&backend);
    let skin = SkinId::from_str(skin.as_deref().unwrap_or(""));
    // v0.4.0 P0：双语模型锁死，老 onboarding 仍可能传 "zh" / "en"，统一存 "zh-en"。
    let _ = voice_lang; // 字段保留 API 兼容，值忽略
    let voice_lang = "zh-en".to_string();
    // 保留用户之前选的语言等设置（重走 onboarding 不要被重置成 default）
    let prev = config::Config::load();
    let cfg = config::Config {
        shortcut: new_str.clone(),
        backend,
        skin,
        language: prev.language,
        voice_lang: voice_lang.clone(),
        vocab_builtin_enabled: prev.vocab_builtin_enabled,
        firstrun_tour_done: prev.firstrun_tour_done,
        voice_ime_enabled: prev.voice_ime_enabled,
        voice_ime_trigger: prev.voice_ime_trigger,
        clipboard_paused: prev.clipboard_paused,
        workspace_path: prev.workspace_path,
        autostart: prev.autostart,
        pet_anchor: prev.pet_anchor,
        pet_custom_position: prev.pet_custom_position,
        tts_enabled: prev.tts_enabled,
        onboarded: true,
        version: config::CURRENT_CONFIG_VERSION,
    };
    cfg.save().map_err(|e| format!("保存配置失败：{e}"))?;

    println!(
        "[mouseclaw] config saved → shortcut={new_str}, backend={:?}, skin={:?}, voice_lang={voice_lang}, onboarded ✓",
        backend, skin
    );

    // v0.4.0 · Onboarding 完成立即触发模型下载（不等用户首次按快捷键）
    crate::transcribe_stream::kick_off_download_if_missing(app.clone());
    crate::punctuation::kick_off_download_if_missing(app.clone());

    // 若两个模型都还没下完 → 自动打开下载进度窗口，用户能看到动静
    let need_download = !crate::transcribe_stream::is_ready()
        || !crate::punctuation::is_ready();
    if need_download {
        if let Err(e) = open_downloader_window(app) {
            eprintln!("[mouseclaw] auto-open downloader failed: {e}");
        }
    }

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
    // v0.1.26 · 让托盘里「🎨 更换桌宠… (xxx)」label 也跟着换
    crate::tray::rebuild_tray_menu(&app);
    println!("[mouseclaw] skin saved → {:?} (已广播 skin-changed + 刷新托盘)", parsed);
    Ok(())
}

/// 启动时前端读当前皮肤 —— 避免每个窗口加载时闪一下默认皮再切换。
#[tauri::command]
pub fn get_skin() -> String {
    config::Config::load().skin.as_str().to_string()
}

/// v0.3.6 · 用户拖动桌宠到任意位置后调用 —— 保存窗口左上角坐标。
/// 下次启动 / apply_idle_anchor 会优先用这个坐标，跨重启持久。
/// 用户在托盘 anchor 子菜单点任一角落 → 自动清空回到 corner anchor。
#[tauri::command]
pub fn save_pet_custom_position(x: f64, y: f64) -> Result<(), String> {
    let mut cfg = config::Config::load();
    cfg.pet_custom_position = Some((x, y));
    cfg.save().map_err(|e| format!("保存自定义位置失败：{e}"))?;
    println!("[mouseclaw] pet dragged to custom position ({x}, {y})");
    Ok(())
}

/// v0.3.12 · 前端在 idle 状态下显示 React-only UI（下载提示 / petMenu / nudge）时调它。
/// 把 mouse overlay 窗口的 hit-box 从"右下角桌宠区"扩到"整个窗口"，避免气泡左半部分点不到。
/// 默认 idle 静默时 = false（右下 110×110 hit-box），其余区域穿透到底层 app。
#[tauri::command]
pub fn set_overlay_has_ui(has_ui: bool, app: AppHandle, state: State<'_, Arc<AppState>>) -> Result<(), String> {
    state.overlay_has_ui.store(has_ui, std::sync::atomic::Ordering::Relaxed);
    // v0.3.12 fix3 · React-only UI（下载提示 / petMenu / nudge / drag）也要触发窗口尺寸切换
    if has_ui {
        crate::overlay_size::expand_to_full(&app);
    } else {
        crate::overlay_size::shrink_to_compact(&app);
    }
    Ok(())
}

/// v0.4 · 内容驱动 overlay 尺寸 —— React 端 ResizeObserver 实测可见 UI 实际像素，传过来。
///
/// 解决"加一个新菜单项就被剪 / 又得改 EXPANDED_SIZE 常量"的循环。
/// 见 CLAUDE.md "Overlay 窗口尺寸：用内容测量，别拍数字"。
///
/// 行为：keep pet 视觉锚点（底部中央那点）不变 → 改窗口 size + position。
/// has_ui 默认按 true 处理（既然在测尺寸，说明肯定有 UI 要显示）。
#[tauri::command]
pub fn set_overlay_content_size(width: f64, height: f64, app: AppHandle, state: State<'_, Arc<AppState>>) -> Result<(), String> {
    state.overlay_has_ui.store(true, std::sync::atomic::Ordering::Relaxed);
    crate::overlay_size::set_to_explicit(&app, width, height);
    Ok(())
}

/// v0.3.6 · 一键打开 macOS 系统设置 → 隐私与安全性 → 辅助功能 面板。
/// 给 voice IME 失败气泡的"🔓 去授权"按钮用 —— 用户授权完退出 app 重启即可。
///
/// 用 `x-apple.systempreferences:` URL scheme，Sonoma 14+ 和老版 macOS 都支持。
#[tauri::command]
pub fn open_accessibility_settings() -> Result<(), String> {
    std::process::Command::new("open")
        .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility")
        .spawn()
        .map_err(|e| format!("open settings: {e}"))?;
    Ok(())
}

/// v0.1.27 · 持久化桌宠悬停位置 + 立即把窗口送到新位置 + 刷新托盘 ✓ 标记。
/// Onboarding 完成 / 托盘子菜单切换 / Pet Picker 设置面板 都调它。
#[tauri::command]
pub fn save_pet_anchor(anchor: String, app: AppHandle) -> Result<(), String> {
    let parsed = config::PetAnchor::from_str(&anchor);
    let mut cfg = config::Config::load();
    cfg.pet_anchor = parsed;
    // v0.3.6 · 用户主动选角落 → 清掉拖动留下的 custom position，回归 corner anchor
    cfg.pet_custom_position = None;
    cfg.save().map_err(|e| format!("保存桌宠位置失败：{e}"))?;

    // 已 onboarded 的话立即应用 —— 让用户即时看到老鼠跑到新位置
    if cfg.onboarded {
        if parsed.pin_visible_when_idle() {
            crate::anchor::apply_idle_anchor(&app, parsed);
        } else {
            // Follow / Hidden → 让 overlay 立刻消失（旧角落不该残留）
            // Follow 后续靠 cursor_follow 接管；Hidden 就彻底等召唤
            if let Some(w) = app.webview_windows().get("mouse") {
                let _ = w.hide();
            }
        }
    }
    crate::tray::rebuild_tray_menu(&app);
    println!("[mouseclaw] pet_anchor saved → {:?}", parsed);
    Ok(())
}

#[tauri::command]
pub fn get_pet_anchor() -> String {
    config::Config::load().pet_anchor.as_str().to_string()
}

/// v0.1.28 · Onboarding 检测：给定后端 id，返回是否能在 PATH 里找到对应 CLI。
/// 同时把人类可读的安装命令 + 官网 URL 也带回去，前端没装时显示给用户。
#[derive(serde::Serialize)]
pub struct BackendInstallStatus {
    pub installed: bool,
    pub binary: String,
    #[serde(rename = "installCmd")]
    pub install_cmd: String,
    #[serde(rename = "installUrl")]
    pub install_url: String,
}

#[tauri::command]
pub fn check_backend_installed(backend: String) -> BackendInstallStatus {
    let b = Backend::from_choice(&backend);
    let installed = crate::claude_cli::find_binary(b.binary_name()).is_ok();
    BackendInstallStatus {
        installed,
        binary: b.binary_name().into(),
        install_cmd: b.install_cmd().into(),
        install_url: b.install_url().into(),
    }
}

/// v0.1.27 P3 · 让用户开启「休息一下」—— 在 minutes 分钟内所有 nudge 都不会发。
/// PetMenu 💤 项调它；nudge.rs 规则引擎读 `NudgeState::in_nap` 决定是否跳过。
#[tauri::command]
pub fn set_nap_until(
    minutes: u32,
    state: State<'_, Arc<AppState>>,
) -> Result<(), String> {
    let until = chrono::Local::now().timestamp() + (minutes as i64) * 60;
    let mut ns = state.nudge_state.write().map_err(|e| format!("nap lock: {e}"))?;
    ns.set_nap_until(until);
    println!("[mouseclaw] 💤 nap until {until} (in {minutes}min)");
    Ok(())
}

/// 前端 dismiss nudge bubble 时调 —— 当前实现把这种 nudge 的 cooldown 标到现在，
/// 阻止它在 30min 内再次触发。给"今天闭嘴"留出空间（后续可扩展到全天）。
#[tauri::command]
pub fn dismiss_nudge(
    kind: String,
    state: State<'_, Arc<AppState>>,
) -> Result<(), String> {
    use crate::events::NudgeKind;
    let parsed = match kind.as_str() {
        "stretch"      => NudgeKind::Stretch,
        "stuck"        => NudgeKind::Stuck,
        "late-night"   => NudgeKind::LateNight,
        "water"        => NudgeKind::Water,
        "good-morning" => NudgeKind::GoodMorning,
        "lunch"        => NudgeKind::Lunch,
        other => return Err(format!("unknown nudge kind: {other}")),
    };
    let now = chrono::Local::now().timestamp();
    let mut ns = state.nudge_state.write().map_err(|e| format!("nudge lock: {e}"))?;
    ns.mark_fired(parsed, now);
    Ok(())
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
    /// v0.4.x · OfficeCLI（读写 Word/Excel/PPT，自包含二进制无需 Office/账号）
    pub officecli: bool,
}

// v0.3 · save_whisper_model / get_whisper_model deleted alongside Whisper.
// sherpa zh-en is the sole ASR; no user-facing model picker needed.

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

// v0.3.4 · save_tidy_up / get_tidy_up deleted alongside LLM polish.

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
pub async fn open_panel_window(
    transcript: String,
    reply: String,
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
) -> Result<(), String> {
    use tauri::WebviewWindowBuilder;
    use tauri::WebviewUrl;

    // v0.3.11 · session_id 不再由前端传 —— 后端权威，直接读当前 session。
    // 之前 React 端 hardcode sessionId: 1，Panel chip 永远显示 #000001（cosmetic bug）。
    let session_id = state.sessions.lock().await.current_session();

    // 1. 存上下文（turns=None，Panel 退化到渲染 transcript+reply 一对）
    *state.pending_panel_context.lock().unwrap() = Some(crate::PendingPanelContext {
        session_id, transcript, reply, turns: None,
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

/// v0.4.0 · 打开模型下载进度窗口 —— 托盘菜单 / blocked 气泡的入口
#[tauri::command]
pub fn open_downloader_window(app: AppHandle) -> Result<(), String> {
    use tauri::{WebviewWindowBuilder, WebviewUrl};
    if let Some(w) = app.get_webview_window("downloader") {
        let _ = w.show();
        let _ = w.set_focus();
        return Ok(());
    }
    let res = WebviewWindowBuilder::new(
        &app, "downloader",
        WebviewUrl::App("index.html?view=downloader".into()),
    )
    .title("MouseClaw — 模型下载")
    .inner_size(520.0, 460.0)
    .min_inner_size(420.0, 360.0)
    .resizable(true).decorations(true).focused(true)
    .build();
    match res {
        Ok(w) => { let _ = w.set_focus(); Ok(()) }
        Err(e) => Err(format!("打开下载窗口失败：{e:#}")),
    }
}

/// v0.4.0 · 重试模型下载 —— 用户在 DownloaderView 点「🔁 重试下载」时调
#[tauri::command]
pub fn retry_model_downloads(app: AppHandle) -> Result<(), String> {
    crate::transcribe_stream::kick_off_download_if_missing(app.clone());
    crate::punctuation::kick_off_download_if_missing(app);
    Ok(())
}

/// v0.4.0 · DownloaderView 挂载时调一次拿当前所有模型的状态快照。
/// 没监听到 emit 也能正确显示「✅ 已就绪」或「等待开始」等状态。
#[tauri::command]
pub fn get_model_status() -> Vec<crate::model_downloader::ProgressEvent> {
    crate::model_downloader::snapshot_all()
}

// ────────────────── v0.4.0 · 语音确认 commands ──────────────────

/// 用户按 Enter / 点"发送" → 立即跳过倒数发送当前 text
#[tauri::command]
pub fn voice_confirm_send(state: State<'_, Arc<AppState>>) -> Result<(), String> {
    state.voice_confirm_action.store(crate::VC_SEND_NOW, std::sync::atomic::Ordering::SeqCst);
    Ok(())
}

/// 用户按 Esc / 点"取消" → 取消 pipeline
#[tauri::command]
pub fn voice_confirm_cancel(state: State<'_, Arc<AppState>>) -> Result<(), String> {
    state.voice_confirm_action.store(crate::VC_CANCEL, std::sync::atomic::Ordering::SeqCst);
    Ok(())
}

/// 用户编辑文本（点气泡进入输入框模式后 commit）→ 回写 + 立即发送
#[tauri::command]
pub async fn voice_confirm_edit(
    text: String,
    state: State<'_, Arc<AppState>>,
) -> Result<(), String> {
    *state.voice_confirm_text.lock().await = Some(text);
    state.voice_confirm_action.store(crate::VC_SEND_NOW, std::sync::atomic::Ordering::SeqCst);
    Ok(())
}

/// v0.3.11 · 用户开始打字 → 暂停倒数，回写当前 text（不发送）。
/// 等用户主动 Enter (voice_confirm_send / voice_confirm_edit) 或 Esc (voice_confirm_cancel)。
#[tauri::command]
pub async fn voice_confirm_hold(
    text: String,
    state: State<'_, Arc<AppState>>,
) -> Result<(), String> {
    *state.voice_confirm_text.lock().await = Some(text);
    state.voice_confirm_action.store(crate::VC_HOLD, std::sync::atomic::Ordering::SeqCst);
    Ok(())
}

// ────────────────── v0.4.0 · 首次使用引导 commands ──────────────────

/// 启动 / 重启首次引导 —— Step 1。
/// 模型下完后由 DownloaderView 自动调；PetMenu「📖 教我用」也调它强制重启。
#[tauri::command]
pub fn tour_start(app: AppHandle) -> Result<(), String> {
    crate::overlay::show_mouse(&app);
    crate::overlay::emit_view(&app, &crate::events::ViewKind::TourStep { step: 1 });
    Ok(())
}

/// 推进到下一步 —— 前端按用户操作（[好啊] / [已经打开了] / 完成对话）调用。
#[tauri::command]
pub fn tour_advance(step: u32, app: AppHandle) -> Result<(), String> {
    if step >= 5 {
        // 完成 —— 持久化 + 5 秒后自动收起
        let mut cfg = crate::config::Config::load();
        cfg.firstrun_tour_done = true;
        let _ = cfg.save();
        crate::overlay::emit_view(&app, &crate::events::ViewKind::TourStep { step: 5 });
        let app_clone = app.clone();
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            crate::overlay::hide_overlay(&app_clone);
        });
    } else {
        crate::overlay::emit_view(&app, &crate::events::ViewKind::TourStep { step });
    }
    Ok(())
}

/// 跳过引导 —— [下次再说] / [退出引导] / Esc 都走这条
#[tauri::command]
pub fn tour_skip(app: AppHandle) -> Result<(), String> {
    let mut cfg = crate::config::Config::load();
    cfg.firstrun_tour_done = true;
    let _ = cfg.save();
    crate::overlay::hide_overlay(&app);
    Ok(())
}

/// 查询引导是否已完成 —— DownloaderView「模型下完」时调，决定是否自动触发
#[tauri::command]
pub fn get_tour_done() -> bool {
    crate::config::Config::load().firstrun_tour_done
}

/// v0.3.11 · 从 HistoryView 「💬 继续这个话题」恢复一个旧 session 进 Panel 窗口。
///   1. 读 ~/.mouseclaw/sessions.jsonl，挑出指定 session_id 的全部 turn
///   2. SessionStore::resume —— in-memory state 切到这个 session（id + 最近 N 轮）
///   3. 把最后一对 user/assistant 塞进 pending_panel_context，渲染初始 Panel 内容
///   4. 隐藏 overlay + 开 / 聚焦 panel 窗口
///
/// 这之后用户的 follow-up 会按这个 session 续传给 Claude（context_preamble 拼上历史）。
#[tauri::command]
pub async fn resume_session(
    session_id: u64,
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
) -> Result<(), String> {
    use crate::events::{Turn, TurnRole};
    use crate::sessions::TurnRecord;
    use tauri::WebviewWindowBuilder;
    use tauri::WebviewUrl;

    // 1. 读 jsonl 拿这个 session 的 turn 列表
    let home = std::env::var_os("HOME").ok_or("HOME not set")?;
    let path = std::path::PathBuf::from(home).join(".mouseclaw/sessions.jsonl");
    if !path.exists() {
        return Err("还没有历史记录文件".to_string());
    }
    let text = std::fs::read_to_string(&path).map_err(|e| format!("read sessions.jsonl: {e}"))?;
    let mut turns: Vec<Turn> = Vec::new();
    let mut last_user: Option<String> = None;
    let mut last_assistant: Option<String> = None;
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() { continue; }
        let Ok(record) = serde_json::from_str::<TurnRecord>(line) else { continue; };
        if record.session_id != session_id { continue; }
        match record.role {
            TurnRole::User => { last_user = Some(record.text.clone()); }
            TurnRole::Assistant => { last_assistant = Some(record.text.clone()); }
        }
        turns.push(Turn { role: record.role, text: record.text, streaming: None });
    }
    if turns.is_empty() {
        return Err(format!("session #{session_id} 没有 turn"));
    }

    // 2. SessionStore in-memory state 切到这个 session（resume 只保留最近 N 轮做 prompt 上下文）
    let display_turns = turns.clone();
    {
        let mut store = state.sessions.lock().await;
        store.resume(session_id, turns);
    }

    // 3. 塞 Panel 初始上下文 —— 完整 turns 给 Panel 渲染整段对话历史，
    //    transcript/reply 作为兜底（如果前端 turns 字段没读出来还能显示最后一对）
    *state.pending_panel_context.lock().unwrap() = Some(crate::PendingPanelContext {
        session_id,
        transcript: last_user.unwrap_or_else(|| String::from("(无用户输入)")),
        reply: last_assistant.unwrap_or_else(|| String::from("(无回复)")),
        turns: Some(display_turns),
    });

    // 4. 隐藏 overlay + 开 / 聚焦 panel 窗口（和 open_panel_window 同样的窗口行为）
    crate::overlay::hide_overlay(&app);
    if let Some(w) = app.get_webview_window("panel") {
        let _ = w.show();
        let _ = w.set_focus();
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

/// v0.1.26 · 打开桌宠 picker 窗口（替代托盘里的 6-item 子菜单）
#[tauri::command]
pub fn open_picker_window(app: AppHandle) -> Result<(), String> {
    use tauri::WebviewWindowBuilder;
    use tauri::WebviewUrl;

    if let Some(w) = app.get_webview_window("picker") {
        let _ = w.show();
        let _ = w.set_focus();
        return Ok(());
    }

    let result = WebviewWindowBuilder::new(
        &app, "picker",
        WebviewUrl::App("index.html?view=picker".into()),
    )
    .title("MouseClaw — 选择桌宠")
    .inner_size(760.0, 600.0)
    .min_inner_size(680.0, 540.0)
    .resizable(true)
    .decorations(true)
    .focused(true)
    .build();

    match result {
        Ok(w) => { let _ = w.set_focus(); Ok(()) }
        Err(e) => Err(format!("打开 picker 窗口失败：{e:#}")),
    }
}

#[tauri::command]
pub fn capability_status() -> CapabilityStatus {
    CapabilityStatus {
        claude_cli: crate::claude_cli::find_binary("claude").is_ok(),
        agent_browser: crate::claude_cli::find_binary("agent-browser").is_ok(),
        chrome_cdp: crate::browser_bridge::cdp_is_alive(),
        officecli: crate::claude_cli::find_binary("officecli").is_ok(),
    }
}

/// v0.4.x · 一键装 agent-browser / officecli。
/// 立即返回 —— 进度通过 EV_INSTALL_PROGRESS 事件流式给前端。
#[tauri::command]
pub fn install_cli(app: AppHandle, target: String) -> Result<(), String> {
    let parsed = crate::cli_install::InstallTarget::parse(&target)
        .ok_or_else(|| format!("不支持的安装目标：{target}"))?;
    crate::cli_install::start_install(app, parsed);
    Ok(())
}

/// v0.4.x · 打开系统状态窗口（升级提示 nudge 的 CTA 用）。
#[tauri::command]
pub fn show_status_window(app: AppHandle) -> Result<(), String> {
    crate::tray::open_status_window(&app);
    Ok(())
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
/// v0.4 · 如果正在 feed 流程中（drag waiting / listening），也一并 cancel：
///   - 清掉 fed_docs（用户后悔了，AI 不要看那些文件）
///   - 停录音 / 销毁 sherpa session
///   - 桌宠滑回 anchor
#[tauri::command]
pub async fn dismiss(app: AppHandle, state: State<'_, Arc<AppState>>) -> Result<(), String> {
    let s = state.inner().clone();
    bump_gen(&s);
    let in_drag = s.feed_drag_active.load(std::sync::atomic::Ordering::SeqCst);
    let in_listening_with_docs = s
        .streaming_active
        .load(std::sync::atomic::Ordering::SeqCst)
        && s.fed_docs.lock().await.is_some();
    if in_drag || in_listening_with_docs {
        crate::feed_flow::cancel(app, s).await;
        return Ok(());
    }
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
                let _ = rec.stop_drain_remaining_16k();
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

/// v0.1.30 · PetMenu「🎤 召唤·说话」点击入口 —— 等价于"按下快捷键"。
/// 配合 toggle_recording（=松开），让 mouse-only 用户也能完成 push-to-talk:
///   1. 点 PetMenu 召唤 → start_recording → overlay 进 listening 状态
///   2. 用户对着麦克风说话
///   3. 点桌宠 / 点 Stop 按钮 / 按 Esc → toggle_recording → 转写+发送
/// 不打扰原 push-to-talk（按住快捷键）流程。
#[tauri::command]
pub async fn start_recording(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
) -> Result<(), String> {
    let state = state.inner().clone();
    // 触发 show_mouse + 进入 listening。和 lib.rs 全局快捷键 PRESS 分支等价。
    crate::overlay::show_mouse(&app);
    tauri::async_runtime::spawn(async move { on_shortcut_press(app, state).await });
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

/// v0.1.26 · 让 Onboarding step 5 / 「关于」面板能切开机自启动
#[tauri::command]
pub fn set_autostart(enable: bool, app: AppHandle) -> Result<bool, String> {
    use tauri_plugin_autostart::ManagerExt;
    let autolaunch = app.autolaunch();
    if enable {
        autolaunch.enable().map_err(|e| format!("enable autostart: {e}"))?;
    } else {
        autolaunch.disable().map_err(|e| format!("disable autostart: {e}"))?;
    }
    let now = autolaunch.is_enabled().unwrap_or(enable);
    let mut cfg = config::Config::load();
    cfg.autostart = now;
    cfg.save().map_err(|e| format!("save config: {e}"))?;
    Ok(now)
}

/// 启动时前端读当前自启动状态，反映勾选框
#[tauri::command]
pub fn get_autostart(app: AppHandle) -> bool {
    use tauri_plugin_autostart::ManagerExt;
    app.autolaunch().is_enabled().unwrap_or_else(|_| config::Config::load().autostart)
}

// ───────────────────── v0.4.0 P1 · 术语库 (vocab) commands ─────────────────────

/// 在系统默认编辑器里打开 `~/.mouseclaw/vocab/user.txt` —— 托盘
/// 「📝 编辑术语表...」走这条。文件不存在会先创建一份带说明的模板。
#[tauri::command]
pub fn vocab_open_user_file() -> Result<(), String> {
    crate::vocab::ensure_user_file().map_err(|e| format!("ensure user file: {e}"))?;
    let path = crate::vocab::user_file_path().map_err(|e| format!("path: {e}"))?;
    std::process::Command::new("open")
        .arg(&path)
        .spawn()
        .map_err(|e| format!("open {}: {e}", path.display()))?;
    Ok(())
}

/// 重读用户词表 → 重新生成 active.txt → 失效 sherpa recognizer 让下次
/// transcribe 重新 init 注入 hotwords。
/// 托盘「🔄 刷新术语表」 + 设置 builtin enabled 时都走这条。
#[tauri::command]
pub fn vocab_reload() -> Result<usize, String> {
    let cfg = config::Config::load();
    let n = crate::vocab::regenerate_active(cfg.vocab_builtin_enabled)
        .map_err(|e| format!("regen: {e}"))?;
    crate::transcribe_stream::invalidate_recognizer();
    println!("[mouseclaw] 📝 vocab reloaded: {n} entries");
    Ok(n)
}

#[tauri::command]
pub fn vocab_get_builtin_enabled() -> bool {
    config::Config::load().vocab_builtin_enabled
}

#[tauri::command]
pub fn vocab_set_builtin_enabled(enabled: bool) -> Result<usize, String> {
    let mut cfg = config::Config::load();
    cfg.vocab_builtin_enabled = enabled;
    cfg.save().map_err(|e| format!("save config: {e}"))?;
    let n = crate::vocab::regenerate_active(enabled).map_err(|e| format!("regen: {e}"))?;
    crate::transcribe_stream::invalidate_recognizer();
    println!("[mouseclaw] 📝 builtin vocab → {enabled}, {n} entries active");
    Ok(n)
}
