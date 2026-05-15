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
    let cfg = config::Config {
        shortcut: new_str.clone(),
        backend,
        skin,
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
