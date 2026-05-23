//! 定时任务 (v0.5) 相关 `#[tauri::command]` —— 从 commands.rs 抽出（800 行硬规则）。
//! 业务逻辑在 crate::schedule / crate::scheduler，这里只做参数转发 + 窗口打开。

use std::sync::Arc;
use tauri::{AppHandle, Manager, State};

use crate::AppState;

/// 打开「⏰ 定时任务」管理窗口（托盘 / 桌宠菜单 / 结果气泡"展开"都走它）。
/// `focus_task_id`：从完成气泡点开时带上刚跑完的任务 id —— 让窗口直接展开它的结果，
/// 不用用户再在列表里找（修"点开只看到列表、看不到最新内容"）。
#[tauri::command]
pub fn open_tasks_window(app: AppHandle, focus_task_id: Option<String>) -> Result<(), String> {
    use tauri::{Emitter, WebviewUrl, WebviewWindowBuilder};
    // 任务 id 是 URL-safe 的（字母数字+连字符），但保险起见只在匹配安全字符集时拼进 URL。
    let safe_focus = focus_task_id.as_deref().filter(|s| {
        !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    });
    if let Some(w) = app.get_webview_window("tasks") {
        let _ = w.show();
        let _ = w.set_focus();
        // 已开着的窗口：用事件告诉它聚焦哪条（新开窗口走下面的 URL）
        if let Some(id) = safe_focus {
            let _ = w.emit("tasks-focus", id.to_string());
        }
        return Ok(());
    }
    let url = match safe_focus {
        Some(id) => format!("index.html?view=tasks&focus={id}"),
        None => "index.html?view=tasks".to_string(),
    };
    let result = WebviewWindowBuilder::new(
        &app,
        "tasks",
        WebviewUrl::App(url.into()),
    )
    .title("MouseClaw — 定时任务")
    // v0.5.1 · 调大默认尺寸 + 可拖大（结果面板 44vh 会跟着放大，长结果有地方读）
    .inner_size(540.0, 700.0)
    .min_inner_size(440.0, 460.0)
    .resizable(true)
    .decorations(true)
    .focused(true)
    .build();

    match result {
        Ok(w) => {
            let _ = w.set_focus();
            Ok(())
        }
        Err(e) => Err(format!("打开定时任务窗口失败：{e:#}")),
    }
}

/// 列出所有定时任务（含计算出的 nextRun）。
#[tauri::command]
pub fn list_schedules() -> Result<Vec<crate::schedule::ScheduleView>, String> {
    let now = chrono::Local::now();
    let tasks = crate::schedule::load();
    Ok(tasks
        .into_iter()
        .map(|t| {
            let next_run = if t.enabled {
                t.next_after_last(now).map(|d| d.to_rfc3339())
            } else {
                None
            };
            crate::schedule::ScheduleView { task: t, next_run }
        })
        .collect())
}

/// 新建定时任务（确认卡 / 任务窗一句话新建都走它）。返回创建好的 task。
#[tauri::command]
pub fn create_schedule(
    input: crate::schedule::ScheduleInput,
) -> Result<crate::schedule::ScheduleTask, String> {
    let t = crate::schedule::create(input);
    println!("[mouseclaw] ⏰ schedule created: {} ({})", t.title, t.id);
    Ok(t)
}

#[tauri::command]
pub fn update_schedule(id: String, input: crate::schedule::ScheduleInput) -> Result<bool, String> {
    Ok(crate::schedule::update(&id, input))
}

#[tauri::command]
pub fn delete_schedule(id: String) -> Result<bool, String> {
    Ok(crate::schedule::delete(&id))
}

#[tauri::command]
pub fn toggle_schedule(id: String, enabled: bool) -> Result<bool, String> {
    Ok(crate::schedule::set_enabled(&id, enabled))
}

/// 立即手动跑一次某任务（任务窗"▶ 立刻跑"）。spawn 异步，不阻塞 UI。
#[tauri::command]
pub fn run_schedule_now(id: String, app: AppHandle) -> Result<(), String> {
    tauri::async_runtime::spawn(async move {
        crate::scheduler::run_now(&app, &id).await;
    });
    Ok(())
}

/// 读某任务最近 N 条执行记录（任务窗"展开看历史"）。
#[tauri::command]
pub fn get_schedule_runs(task_id: String) -> Result<Vec<crate::schedule::RunRecord>, String> {
    Ok(crate::schedule::recent_runs(&task_id, 12))
}

/// 任务窗"一句话新建" —— 把自然语言解析成结构化 ScheduleInput（不创建，只解析，前端确认后再 create）。
#[tauri::command]
pub async fn parse_schedule_phrase(
    phrase: String,
    state: State<'_, Arc<AppState>>,
) -> Result<crate::schedule::ScheduleInput, String> {
    let backend = state.backend.lock().await.clone();
    let prompt = format!(
        "把下面这句话解析成 MouseClaw 定时任务的 JSON。**只输出一个 JSON 对象**，不要任何别的文字 / 解释 / 代码栏。\n\
         格式：{{\"title\":\"简短任务名\",\"action\":\"到点要做的事（自包含、第二人称指令，不依赖截图）\",\"schedule\":{{...}}}}\n\
         schedule.kind 五选一（time 用 24 小时制 HH:MM）：\n\
         daily{{\"time\"}} / weekday{{\"time\"}} / weekly{{\"days\":[1-7],\"time\"}}(1=周一) / \
         interval{{\"everyMinutes\",\"activeStart\"?,\"activeEnd\"?}} / monthly{{\"day\",\"time\"}}\n\n\
         用户的话：{phrase}"
    );
    let raw = {
        let _ticket = crate::ai_queue::acquire().await;
        crate::backend::ask_text_only(backend, &prompt)
            .await
            .map_err(|e| crate::pipeline::friendly_backend_error(&format!("{e:#}"), backend))?
    };
    let json = extract_json_block(&raw).ok_or_else(|| "没能从回复里解析出定时任务".to_string())?;
    crate::schedule::parse_input(&json).map_err(|e| format!("解析失败：{e}"))
}

/// 从 backend 回复里抠出 JSON：优先 [SCHEDULE] 标记 → ``` 代码栏 → 第一个 {..} 块。
fn extract_json_block(raw: &str) -> Option<String> {
    if let Some(j) = crate::claude_cli::parse_schedule_directive(raw) {
        return Some(j);
    }
    let trimmed = raw.trim();
    let start = trimmed.find('{')?;
    let end = trimmed.rfind('}')?;
    if end > start {
        Some(trimmed[start..=end].to_string())
    } else {
        None
    }
}
