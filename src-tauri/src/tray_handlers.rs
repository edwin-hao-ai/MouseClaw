//! 托盘菜单项的 **dispatch** —— `handle_menu_event` 按 menu id 路由：前缀型 id
//! （`skin:` / `lang:` / `anchor:` / `summon-shortcut:` / `vime-trigger:`）先匹配，
//! 其余走 `match id`，每个分支调到对应的执行动作并按需 `rebuild_tray_menu`。
//!
//! 从 tray.rs 抽出（tray.rs 已超 800 行硬上限 · 见 CLAUDE.md「单文件 ≤ 800 行」）。
//! 各 action 的实现在 [`crate::tray_actions`]，菜单组装在 [`crate::tray_menu`]，
//! 辅助窗口在 [`crate::tray_windows`]，快捷键热切换在 [`crate::shortcut_menu`]。

use tauri::{menu::MenuEvent, AppHandle, Manager};

use crate::tray_actions::{
    change_backend, change_language, change_skin, emit_vocab_reloaded, enable_browser_automation,
    recommend_backend_install, set_sfx_volume, set_workspace_via_picker, summon_via_tray,
    toggle_autostart, toggle_clipboard_pause, toggle_sfx, toggle_tts, toggle_voice_ime,
};
use crate::tray_menu::rebuild_tray_menu;
use crate::tray_windows::{open_about_dialog, open_history_window, open_status_window};

pub(crate) fn handle_menu_event(app: &AppHandle, event: MenuEvent) {
    let id = event.id.as_ref();
    // 皮肤子菜单：id 形如 "skin:lab" / "skin:cyber"
    if let Some(skin_name) = id.strip_prefix("skin:") {
        change_skin(app, skin_name);
        rebuild_tray_menu(app);
        return;
    }
    // v0.3 · Whisper submenu deleted
    // 语言子菜单：id 形如 "lang:zh" / "lang:en"
    if let Some(lang) = id.strip_prefix("lang:") {
        change_language(app, lang);
        rebuild_tray_menu(app);
        return;
    }
    // voice IME 触发键子菜单：id 形如 "vime-trigger:option"
    if let Some(trigger) = id.strip_prefix("vime-trigger:") {
        crate::shortcut_menu::change_ime_trigger(app, trigger);
        rebuild_tray_menu(app);
        return;
    }
    // 召唤 AI 快捷键子菜单：id 形如 "summon-shortcut:Super+Shift+Space"
    if let Some(sc) = id.strip_prefix("summon-shortcut:") {
        crate::shortcut_menu::change_summon_shortcut(app, sc);
        rebuild_tray_menu(app);
        return;
    }
    // v0.4.4 · AI 后端切换子菜单：id 形如 "backend:gemini" / "backend:kiro-cli"
    if let Some(slug) = id.strip_prefix("backend:") {
        change_backend(app, slug);
        rebuild_tray_menu(app);
        return;
    }
    // v0.1.27 · 桌宠位置子菜单：id 形如 "anchor:bottom-right"
    if let Some(anchor) = id.strip_prefix("anchor:") {
        if let Err(e) = crate::commands::save_pet_anchor(anchor.into(), app.clone()) {
            eprintln!("[mouseclaw] save_pet_anchor: {e}");
        }
        return;
    }
    match id {
        "summon"          => summon_via_tray(app),
        "new-session"     => {
            let app2 = app.clone();
            tauri::async_runtime::spawn(async move {
                if let Some(state) = app2.try_state::<std::sync::Arc<crate::AppState>>() {
                    let pinned = {
                        let mut store = state.sessions.lock().await;
                        if store.is_pinned() { true } else { store.touch(true, None); false }
                    };
                    use tauri::Emitter;
                    if pinned {
                        // 钉住中拒绝重置 —— 提示用户先解钉
                        let lang = crate::config::Config::load().language;
                        let msg = if lang == "en" { "📌 Task is pinned — unpin first to start a new chat" }
                                  else { "📌 任务已钉住 —— 先解除钉住才能开新对话" };
                        for (_, w) in app2.webview_windows() {
                            let _ = w.emit(crate::events::EV_VIEW_CHANGED, serde_json::json!({
                                "kind":"reply","transcript":"new chat","reply":msg,"mode":"A","streaming":false,
                            }));
                        }
                    } else {
                        let snap = { state.sessions.lock().await.state_snapshot() };
                        let _ = app2.emit(crate::events::EV_SESSION_STATE, snap);
                    }
                }
            });
        }
        "toggle-pin-session" => {
            let app2 = app.clone();
            tauri::async_runtime::spawn(async move {
                use tauri::Emitter;
                if let Some(state) = app2.try_state::<std::sync::Arc<crate::AppState>>() {
                    let (now_pinned, snap) = {
                        let mut store = state.sessions.lock().await;
                        if store.is_pinned() { store.unpin(); }
                        else { store.pin(crate::mode_b::frontmost_app_name()); }
                        (store.is_pinned(), store.state_snapshot())
                    };
                    state.session_pinned.store(now_pinned, std::sync::atomic::Ordering::Relaxed);
                    let _ = app2.emit(crate::events::EV_SESSION_STATE, snap);
                    rebuild_tray_menu(&app2);
                }
            });
        }
        "open-clipboard"  => {
            if let Err(e) = crate::commands::open_hub_window(app.clone()) {
                eprintln!("[mouseclaw] open-clipboard: {e}");
            }
        }
        "history"         => open_history_window(app),
        "open-tasks"      => {
            if let Err(e) = crate::commands::open_tasks_window(app.clone()) {
                eprintln!("[mouseclaw] open-tasks: {e}");
            }
        }
        "open-downloader" => {
            if let Err(e) = crate::commands::open_downloader_window(app.clone()) {
                eprintln!("[mouseclaw] open-downloader: {e}");
            }
        }
        "about"           => open_about_dialog(app),
        "enable-browser"  => enable_browser_automation(app),
        "open-picker"     => {
            if let Err(e) = crate::commands::open_picker_window(app.clone()) {
                eprintln!("[mouseclaw] open-picker: {e}");
            }
        }
        "open-memory"     => {
            if let Err(e) = crate::commands::open_memory_window(app.clone()) {
                eprintln!("[mouseclaw] open-memory: {e}");
            }
        }
        "backend-install" => recommend_backend_install(app),
        "status"          => open_status_window(app),
        // toggle-tidy removed in v0.3.3 — tidy_up is default-on via Haiku
        "toggle-voice-ime"=> { toggle_voice_ime(app); rebuild_tray_menu(app); }
        "toggle-clipboard-pause" => { toggle_clipboard_pause(app); rebuild_tray_menu(app); }
        "toggle-autostart" => { toggle_autostart(app); rebuild_tray_menu(app); }
        "toggle-tts" => { toggle_tts(app); rebuild_tray_menu(app); }
        "toggle-sfx" => { toggle_sfx(app); rebuild_tray_menu(app); }
        "sfx-vol:low"  => { set_sfx_volume(app, 0.25); rebuild_tray_menu(app); }
        "sfx-vol:mid"  => { set_sfx_volume(app, 0.45); rebuild_tray_menu(app); }
        "sfx-vol:high" => { set_sfx_volume(app, 0.70); rebuild_tray_menu(app); }
        // v0.4.0 P1 · 术语表
        "vocab-edit" => {
            if let Err(e) = crate::commands::vocab_open_user_file() {
                eprintln!("[mouseclaw] vocab-edit: {e}");
            }
        }
        "vocab-reload" => {
            match crate::commands::vocab_reload() {
                Ok(n) => emit_vocab_reloaded(app, n),
                Err(e) => eprintln!("[mouseclaw] vocab-reload: {e}"),
            }
        }
        "toggle-vocab-builtin" => {
            let cur = crate::config::Config::load().vocab_builtin_enabled;
            match crate::commands::vocab_set_builtin_enabled(!cur) {
                Ok(n) => emit_vocab_reloaded(app, n),
                Err(e) => eprintln!("[mouseclaw] toggle-vocab-builtin: {e}"),
            }
            rebuild_tray_menu(app);
        }
        "set-workspace"   => { set_workspace_via_picker(app); rebuild_tray_menu(app); }
        "clear-workspace" => {
            let _ = crate::commands::save_workspace_path(None);
            rebuild_tray_menu(app);
            use tauri::Emitter;
            let lang = crate::config::Config::load().language;
            let msg = if lang == "en" {
                "📁 Workspace cleared. AI will run from default cwd."
            } else {
                "📁 工作区已清除。AI 将走默认目录。"
            };
            for (_, w) in app.webview_windows() {
                let _ = w.emit(crate::events::EV_VIEW_CHANGED, serde_json::json!({
                    "kind": "reply", "transcript": "workspace clear",
                    "reply": msg, "mode": "A", "streaming": false,
                }));
            }
        }
        "quit"            => app.exit(0),
        _ => {}
    }
}
