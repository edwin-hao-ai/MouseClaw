//! 语音确认 (v0.4.0) + 首次使用引导 (tour) 相关 `#[tauri::command]` ——
//! 从 commands.rs 抽出（800 行硬规则）。

use std::sync::Arc;
use tauri::{AppHandle, State};

use crate::AppState;

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
    // v0.4.x fix · 用 _at_anchor（和能正常点击的 nudge 同一条路径），
    // 不用 show_mouse —— 后者会把窗口跳到光标 + 启用 cursor_follow，
    // 是 AI 召唤专用。tour 是静态教程气泡，应该稳停在锚点。
    crate::overlay::show_mouse_at_anchor(&app);
    crate::overlay::emit_view(&app, &crate::events::ViewKind::TourStep { step: 1 });
    Ok(())
}

/// 推进到下一步 —— 前端按用户操作（[好啊] / [已经打开了] / 完成对话）调用。
#[tauri::command]
pub fn tour_advance(step: u32, app: AppHandle) -> Result<(), String> {
    println!("[tour] advance → step={step} (click reached Rust ✓)");
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
    println!("[tour] skip (click reached Rust ✓)");
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
