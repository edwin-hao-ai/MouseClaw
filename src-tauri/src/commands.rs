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

// 按职责拆出的子模块（800 行硬规则）。`pub use` 重导出 → lib.rs `commands::xxx` 路径不变。
mod schedule;
mod windows;
mod interaction;
mod settings;
pub use schedule::*;
pub use windows::*;
pub use interaction::*;
pub use settings::*;

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
        pet_name: prev.pet_name,
        personality: prev.personality,
        personality_custom: prev.personality_custom,
        memory_enabled: prev.memory_enabled,
        memory_paused: prev.memory_paused,
        sfx_enabled: prev.sfx_enabled,
        sfx_volume: prev.sfx_volume,
        onboarded: true,
        seen_entrance: prev.seen_entrance,
        seen_schedule_hint: prev.seen_schedule_hint,
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
/// v0.4.x · 临时让桌宠 overlay 可获键盘焦点 —— 仅 voice-confirm 期间开，让用户
/// 能直接打字编辑识别出来的文本（overlay 平时是非激活面板，textarea 的 .focus()
/// 只是 DOM 级，按键其实进了后台 app，根本编辑不了）。确认结束（发送/取消）后关掉，
/// 恢复"不抢焦点"的默认气质。
#[tauri::command]
pub fn set_overlay_focusable(focusable: bool, app: AppHandle) -> Result<(), String> {
    if let Some(w) = app.get_webview_window("mouse") {
        let _ = w.set_focusable(focusable);
        if focusable {
            let _ = w.set_focus();
        }
    }
    Ok(())
}

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

/// v0.4.4 · picker 读当前桌宠身份(名字 + 性格)。
#[tauri::command]
pub fn get_pet_identity() -> serde_json::Value {
    let cfg = config::Config::load();
    serde_json::json!({
        "name": cfg.pet_name.unwrap_or_default(),
        "personality": cfg.personality.as_str(),
        "custom": cfg.personality_custom.unwrap_or_default(),
    })
}

/// v0.4.4 · picker 保存桌宠身份。空 name → None(回到通用自称「MouseClaw」)。
/// 影响:① 所有 backend 的 system_prompt 自我称呼+语气 ② 托盘「召唤 {name}」label。
#[tauri::command]
pub fn save_pet_identity(
    name: String,
    personality: String,
    custom: String,
    app: AppHandle,
) -> Result<(), String> {
    let mut cfg = config::Config::load();
    let trimmed = name.trim();
    cfg.pet_name = if trimmed.is_empty() { None } else { Some(trimmed.to_string()) };
    cfg.personality = config::Personality::from_str(&personality);
    let c = custom.trim();
    cfg.personality_custom = if c.is_empty() { None } else { Some(c.to_string()) };
    cfg.save().map_err(|e| format!("保存桌宠身份失败：{e}"))?;

    // 托盘「召唤 {name}」label 跟着换 + 广播给前端窗口
    crate::tray::rebuild_tray_menu(&app);
    let payload = cfg.pet_name.clone().unwrap_or_default();
    for (_, w) in app.webview_windows() {
        let _ = w.emit("pet-identity-changed", payload.clone());
    }
    println!("[mouseclaw] pet identity saved → name={:?}, personality={}", cfg.pet_name, cfg.personality.as_str());
    Ok(())
}

/// v0.4.4 · 起名可发现性 —— 还没起名的桌宠,启动 20s 后一次性提示去起名
/// (marker file 兜底,弹过不再弹)。CTA「起名」打开 picker。仿 cli_install::maybe_hint_upgrade。
pub fn maybe_show_name_hint(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_secs(20)).await;
        let cfg = config::Config::load();
        if !cfg.onboarded {
            return;
        }
        if cfg.pet_name.as_deref().map(|s| !s.trim().is_empty()).unwrap_or(false) {
            return; // 已经起过名
        }
        let marker = match std::env::var_os("HOME") {
            Some(h) => std::path::PathBuf::from(h).join(".mouseclaw").join("name_hint_shown"),
            None => return,
        };
        if marker.exists() {
            return;
        }
        let en = cfg.language != "zh";
        let (message, cta) = if en {
            ("🐭 I don't have a name yet — want to give me one?".to_string(), "Name me".to_string())
        } else {
            ("🐭 我还没有名字 —— 给我起一个?".to_string(), "起名".to_string())
        };
        let payload = crate::events::NudgePayload {
            kind: crate::events::NudgeKind::NameHint,
            message,
            cta_label: Some(cta),
            cta_action: Some("open-picker".into()),
        };
        if app.emit(crate::events::EV_NUDGE, payload).is_err() {
            return;
        }
        if let Some(dir) = marker.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(&marker, b"1");
        println!("[mouseclaw] name hint shown");
    });
}

/// v0.4.x · 桌宠音效配置（开关 + 音量）—— 前端 petAudio 启动读一次，
/// 之后托盘切换会 emit `sfx-changed` 让活着的 webview 实时更新。
#[derive(Clone, serde::Serialize)]
pub struct SfxConfig {
    pub enabled: bool,
    pub volume: f32,
}

#[tauri::command]
pub fn get_sfx_config() -> SfxConfig {
    let c = config::Config::load();
    SfxConfig { enabled: c.sfx_enabled, volume: c.sfx_volume }
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
pub fn set_overlay_has_ui(has_ui: bool, _app: AppHandle, state: State<'_, Arc<AppState>>) -> Result<(), String> {
    // 这个命令**只**切 passthrough hit-box 布尔（whole-window vs 右下角小框）。
    //
    // v0.4+ 漂移根治（2026-05-21）：之前这里还会 expand_to_full/shrink_to_compact 改窗口
    // 尺寸。但 idle 视图下尺寸已经由 useAdaptiveOverlay → set_overlay_content_size 全权管理，
    // 两条路同时 reposition → nudge 出现/「稍后」消失时桌宠"跳两下"漂移（用户多次报）。
    // 现在 idle 尺寸唯一来源 = 自适应 hook；本命令不再碰尺寸，只管 hit-box。
    // 见 CLAUDE.md「前后端不要同时管同一个窗口尺寸」。
    state.overlay_has_ui.store(has_ui, std::sync::atomic::Ordering::Relaxed);
    Ok(())
}

/// v0.5 · 前端启动上报 prefers-reduced-motion 偏好 —— 开场入场动画据此决定是否
/// 跳过横穿/蹦跶（reduced 时桌宠直接出现在角落）。前端在 App.tsx mount 时调，
/// 并监听 media query change 变化再次上报。
#[tauri::command]
pub fn report_reduced_motion(reduced: bool, state: State<'_, Arc<AppState>>) {
    state.reduced_motion.store(reduced, std::sync::atomic::Ordering::Relaxed);
    println!("[mouseclaw] reduced-motion reported = {reduced}");
}

/// v0.4 · 内容驱动 overlay 尺寸 —— React 端 ResizeObserver 实测可见 UI 实际像素，传过来。
///
/// 解决"加一个新菜单项就被剪 / 又得改 EXPANDED_SIZE 常量"的循环。
/// 见 CLAUDE.md "Overlay 窗口尺寸：用内容测量，别拍数字"。
///
/// 行为：keep pet 视觉锚点（底部中央那点）不变 → 改窗口 size + position。
///
/// v0.4+ 漂移根治（2026-05-21）：**不再**在这里 store(has_ui=true)。has_ui（passthrough
/// hit-box 布尔）唯一来源 = 前端 set_overlay_has_ui（它才知道有没有可交互 UI vs 只有桌宠）。
/// 之前这里强制 true → 「稍后」消失时本 hook（测桌宠 88px）跑在前端 set_overlay_has_ui(false)
/// 之后，又把布尔设回 true → 整窗一直接收点击、盖住底层 app + 尺寸/hit-box 两路打架。
/// 现在职责分离：本命令只管尺寸，前端 effect 只管 hit-box。
#[tauri::command]
pub fn set_overlay_content_size(
    width: f64,
    height: f64,
    pet_ratio_x: f64,
    pet_from_bottom: f64,
    pet_anchored: bool,
    app: AppHandle,
    _state: State<'_, Arc<AppState>>,
) -> Result<(), String> {
    crate::overlay_size::set_to_explicit(&app, width, height, pet_ratio_x, pet_from_bottom, pet_anchored);
    Ok(())
}

/// v0.4.3 · PetMenu 打开时查询该往哪边展开（贴屏幕边时翻向内侧，避免被切）。
/// 主线程拿 visibleFrame + 桌宠中心算 (h, v)，前端据此设 data-h / data-v。
#[tauri::command]
pub async fn get_pet_menu_orientation(app: AppHandle) -> Result<serde_json::Value, String> {
    let (tx, rx) = std::sync::mpsc::channel();
    app.run_on_main_thread(move || {
        let frame = crate::overlay_size::visible_frame_top_left();
        let (h, v) = crate::overlay_size::compute_menu_orientation(frame);
        let _ = tx.send((h.to_string(), v.to_string()));
    })
    .map_err(|e| e.to_string())?;
    let (h, v) = rx.recv().map_err(|e| e.to_string())?;
    Ok(serde_json::json!({ "h": h, "v": v }))
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
        "memory-glance" => NudgeKind::MemoryGlance,
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

/// v0.5.x · 召唤(listening)后用户敲了字符键 → 不想语音说话，原地切成文字输入框。
/// 停录音并**丢弃**当前音频：drop recorder + sherpa session 而**不** finalize ——
/// 不产生 transcript、不进 voice-confirm 倒数（区别于 toggle_recording = 松开转写）。
/// 然后 emit text-input 视图：emit_view 会自动关 cursor_follow + 撑大窗口，
/// 前端拿到 `initial`（触发切换的那个字符）塞进输入框。提交走 submit_query → 主 pipeline，
/// 截图复用召唤瞬间那张（last_screenshot），和语音转写完全同一条路。
#[tauri::command]
pub fn switch_to_text_input(
    initial: String,
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
) -> Result<(), String> {
    use std::sync::atomic::Ordering;
    let state = state.inner().clone();
    bump_gen(&state);
    // 停 streaming poller —— 它每 150ms tick 检查这个 flag，看到 false 就退出。
    state.streaming_active.store(false, Ordering::SeqCst);
    // 丢弃录音器 + sherpa stream session（take + drop = 停录音，不 finalize → 无 transcript）。
    let _ = state.recorder.lock().unwrap().take();
    let _ = state.stream_session.lock().unwrap().take();
    // 丢弃光标轨迹采样（文字态没有"按住快捷键画圈"语义，不该把它烘进截图）。
    #[cfg(target_os = "macos")]
    {
        let _ = crate::cursor_trail::stop_and_take();
    }
    crate::overlay::emit_view(&app, &crate::events::ViewKind::TextInput { initial });
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

