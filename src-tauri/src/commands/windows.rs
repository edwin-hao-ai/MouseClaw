//! 窗口打开相关 `#[tauri::command]`（panel / downloader / hub / picker / memory）+
//! session 续聊 —— 从 commands.rs 抽出（800 行硬规则）。多为 WebviewWindowBuilder 样板。

use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::AppState;

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

/// v0.4.x · 点击桌宠头顶的「🔗 第 N 轮」链条 chip → 打开 Panel 看当前 session 完整对话。
/// 跟 open_panel_window 区别：带完整 turns（不是只一对），让用户回看整段聊了啥。
#[tauri::command]
pub async fn open_session_panel(app: AppHandle, state: State<'_, Arc<AppState>>) -> Result<(), String> {
    use tauri::{WebviewWindowBuilder, WebviewUrl};
    let (session_id, turns) = {
        let store = state.sessions.lock().await;
        (store.current_session(), store.snapshot_turns())
    };
    if turns.is_empty() {
        return Ok(()); // 没上下文，不开空 panel
    }
    // 末尾一对作 fallback 显示；turns 给完整历史让 Panel 渲染整段
    let transcript = turns.iter().rev()
        .find(|t| matches!(t.role, crate::events::TurnRole::User))
        .map(|t| t.text.clone()).unwrap_or_default();
    let reply = turns.iter().rev()
        .find(|t| matches!(t.role, crate::events::TurnRole::Assistant))
        .map(|t| t.text.clone()).unwrap_or_default();
    *state.pending_panel_context.lock().unwrap() = Some(crate::PendingPanelContext {
        session_id, transcript, reply, turns: Some(turns),
    });
    crate::overlay::hide_overlay(&app);
    if let Some(w) = app.get_webview_window("panel") {
        let _ = w.show();
        let _ = w.set_focus();
        let _ = w.emit("panel-context-changed", ());
        return Ok(());
    }
    WebviewWindowBuilder::new(&app, "panel", WebviewUrl::App("index.html?view=panel".into()))
        .title("MouseClaw — 对话").inner_size(480.0, 560.0).min_inner_size(380.0, 380.0)
        .resizable(true).decorations(true).focused(true)
        .build()
        .map(|w| { let _ = w.set_focus(); })
        .map_err(|e| format!("打开 panel 窗口失败：{e:#}"))
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

/// v0.4.x · Onboarding 一打开就后台预取语音模型(~260MB)。
/// 把下载提前到向导期间,等用户点完步骤基本下好,消灭"装完才开始下"的死等。
/// download() 自带并发锁(try_lock_download),与 onboarding 完成时 save_shortcut 的
/// kick_off 不会重复下载;权限重启打断后 .part 续传,已下字节不浪费。
/// (注:有意改变原"未 onboarded 不预下"的设计 —— 用户拍板"先下载可以"。)
#[tauri::command]
pub fn prefetch_models(app: AppHandle) -> Result<(), String> {
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

/// v0.4.4 · 打开「桌宠记得的事」窗口(画像 / 历史 / 暂停)。
#[tauri::command]
pub fn open_memory_window(app: AppHandle) -> Result<(), String> {
    use tauri::WebviewWindowBuilder;
    use tauri::WebviewUrl;

    if let Some(w) = app.get_webview_window("memory") {
        let _ = w.show();
        let _ = w.set_focus();
        return Ok(());
    }

    let result = WebviewWindowBuilder::new(
        &app, "memory",
        WebviewUrl::App("index.html?view=memory".into()),
    )
    .title("MouseClaw — 桌宠记得的事")
    .inner_size(560.0, 640.0)
    .min_inner_size(460.0, 480.0)
    .resizable(true)
    .decorations(true)
    .focused(true)
    .build();

    match result {
        Ok(w) => { let _ = w.set_focus(); Ok(()) }
        Err(e) => Err(format!("打开记忆窗口失败：{e:#}")),
    }
}
