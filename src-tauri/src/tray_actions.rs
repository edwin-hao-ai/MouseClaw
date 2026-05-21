//! 托盘菜单项点击触发的**执行动作** —— 各种 toggle（voice IME / 剪贴板暂停 /
//! 自启动 / TTS）、change（皮肤 / 语言）、弹工作区 picker、启用浏览器自动化、
//! 召唤老鼠、术语表刷新反馈气泡。
//!
//! 从 tray.rs 抽出（tray.rs 已超 800 行硬上限 · 见 CLAUDE.md「单文件 ≤ 800 行」）。
//! 全部由 [`crate::tray_handlers::handle_menu_event`] 派发调用；切换后由 dispatch
//! 负责 `rebuild_tray_menu` 同步勾选。

use tauri::{AppHandle, Manager};

use crate::skins::SkinId;
use crate::tray_windows::open_onboarding_window;

/// 弹原生 NSOpenPanel 选目录 → 存进 config
/// 用 osascript 触发 —— Tauri 的 dialog plugin 也能做但要额外配权限
pub(crate) fn set_workspace_via_picker(app: &AppHandle) {
    use tauri::Emitter;
    let lang = crate::config::Config::load().language;
    let prompt = if lang == "en" {
        "Choose your project folder so AI can read & edit its files"
    } else {
        "选择项目目录，让 AI 能读写里面的文件"
    };
    // osascript: choose folder
    let script = format!(
        r#"set folderPath to POSIX path of (choose folder with prompt "{}")
        return folderPath"#,
        prompt.replace('"', "\\\"")
    );
    let out = std::process::Command::new("osascript")
        .args(["-e", &script])
        .output();
    let path = match out {
        Ok(o) if o.status.success() => {
            let s = String::from_utf8_lossy(&o.stdout).trim().to_string();
            if s.is_empty() { return; }
            // POSIX path 末尾常带 /，去掉
            s.trim_end_matches('/').to_string()
        }
        _ => return, // 用户取消 / 命令失败
    };

    match crate::commands::save_workspace_path(Some(path.clone())) {
        Ok(()) => {
            let msg = if lang == "en" {
                format!("📁 Workspace → {path}. AI will run from this folder.")
            } else {
                format!("📁 工作区 → {path}。AI 会在此目录里读写文件。")
            };
            for (_, w) in app.webview_windows() {
                let _ = w.emit(crate::events::EV_VIEW_CHANGED, serde_json::json!({
                    "kind": "reply", "transcript": "workspace set",
                    "reply": msg, "mode": "A", "streaming": false,
                }));
            }
        }
        Err(e) => eprintln!("[mouseclaw] 📁 save_workspace_path: {e}"),
    }
}

/// v0.4.0 P1 · 术语表刷新后弹个气泡告诉用户「N 词生效」。
///
/// fix(2026-05-19): 必须走完整 show_mouse_at_anchor + emit_view 路径，
/// 直接 emit JSON 不会 expand overlay 也不会 show window —— 用户
/// 反馈"气泡一直跳但没显示"就是这个原因（pet 在 80×80 compact 模式
/// 时 bubble 被切掉）。
pub(crate) fn emit_vocab_reloaded(app: &AppHandle, n: usize) {
    let lang = crate::config::Config::load().language;
    let msg = if lang == "en" {
        format!("📝 Vocabulary reloaded — {n} terms active")
    } else {
        format!("📝 术语表已刷新 — {n} 个词生效")
    };
    // 走 show_mouse_at_anchor 让窗口显示在 anchor 位置且 expand 到 320×320
    crate::overlay::show_mouse_at_anchor(app);
    crate::overlay::emit_view(app, &crate::events::ViewKind::Reply {
        transcript: "vocab reload".into(),
        reply: msg,
        mode: crate::events::ReplyMode::A,
        insert_text: None,
        streaming: false,
    });
    // 3 秒后自动收回 anchor
    if let Some(state) = app.try_state::<std::sync::Arc<crate::AppState>>() {
        crate::overlay::schedule_auto_hide(app, state.inner(), 3000);
    }
}

/// v0.4.0 · 切换 TTS（桌宠开口说话）
pub(crate) fn toggle_tts(app: &AppHandle) {
    let mut cfg = crate::config::Config::load();
    cfg.tts_enabled = !cfg.tts_enabled;
    let now_on = cfg.tts_enabled;
    if let Err(e) = cfg.save() {
        eprintln!("[mouseclaw] toggle_tts save: {e}");
        return;
    }
    #[cfg(target_os = "macos")]
    if !now_on {
        crate::tts::stop();
    }
    let _ = app;
    println!("[mouseclaw] 🔊 tts_enabled → {now_on}");
}

/// v0.4.x · 广播当前音效配置给所有 webview（托盘改完 → 桌宠实时生效，不重启）
fn emit_sfx_changed(app: &AppHandle, cfg: &crate::config::Config) {
    use tauri::Emitter; // Manager 已在模块顶层 use
    let payload = crate::commands::SfxConfig { enabled: cfg.sfx_enabled, volume: cfg.sfx_volume };
    for (_, w) in app.webview_windows() {
        let _ = w.emit("sfx-changed", payload.clone());
    }
}

/// v0.4.x · 切换桌宠音效（程序化 chiptune：睡觉鼾声 / 完成提示音等）
pub(crate) fn toggle_sfx(app: &AppHandle) {
    let mut cfg = crate::config::Config::load();
    cfg.sfx_enabled = !cfg.sfx_enabled;
    let now_on = cfg.sfx_enabled;
    if let Err(e) = cfg.save() {
        eprintln!("[mouseclaw] toggle_sfx save: {e}");
        return;
    }
    emit_sfx_changed(app, &cfg);
    println!("[mouseclaw] 🔉 sfx_enabled → {now_on}");
}

/// v0.4.x · 设音效音量（托盘 轻/中/响 三档）
pub(crate) fn set_sfx_volume(app: &AppHandle, volume: f32) {
    let mut cfg = crate::config::Config::load();
    cfg.sfx_volume = volume.clamp(0.0, 1.0);
    if let Err(e) = cfg.save() {
        eprintln!("[mouseclaw] set_sfx_volume save: {e}");
        return;
    }
    emit_sfx_changed(app, &cfg);
    println!("[mouseclaw] 🔉 sfx_volume → {}", cfg.sfx_volume);
}

/// 切换剪贴板暂停（隐私 ⑧）
pub(crate) fn toggle_clipboard_pause(app: &AppHandle) {
    use tauri::Emitter;
    let mut cfg = crate::config::Config::load();
    cfg.clipboard_paused = !cfg.clipboard_paused;
    let now_paused = cfg.clipboard_paused;
    if let Err(e) = cfg.save() {
        eprintln!("[mouseclaw] toggle_clipboard_pause save: {e}");
        return;
    }
    crate::clipboard::set_paused(now_paused);
    let lang = cfg.language;
    let msg = if now_paused {
        if lang == "en" { "⏸️ Clipboard recording paused. New copies won't be saved until you resume." }
        else { "⏸️ 剪贴板记录已暂停。新复制的内容不会被记录，直到你恢复。" }
    } else {
        if lang == "en" { "▶️ Clipboard recording resumed." }
        else { "▶️ 剪贴板记录已恢复。" }
    };
    for (_, w) in app.webview_windows() {
        let _ = w.emit(crate::events::EV_VIEW_CHANGED, serde_json::json!({
            "kind": "reply", "transcript": "clipboard pause",
            "reply": msg, "mode": "A", "streaming": false,
        }));
    }
}

/// v0.1.26 · 开机自启动切换 —— 真信源是 LaunchAgent，config 是镜像
pub(crate) fn toggle_autostart(app: &AppHandle) {
    use tauri::Emitter;
    use tauri_plugin_autostart::ManagerExt;
    let autolaunch = app.autolaunch();
    let want_on = !autolaunch.is_enabled().unwrap_or(false);
    let result = if want_on { autolaunch.enable() } else { autolaunch.disable() };
    if let Err(e) = result {
        eprintln!("[mouseclaw] toggle_autostart system call failed: {e}");
        return;
    }
    // 回写 config 镜像
    let mut cfg = crate::config::Config::load();
    cfg.autostart = autolaunch.is_enabled().unwrap_or(want_on);
    let _ = cfg.save();

    let msg = if cfg.language == "en" {
        if want_on { "🚀 Launch at login enabled. MouseClaw will start when you log in." }
        else { "🚪 Launch at login disabled." }
    } else {
        if want_on { "🚀 已开启开机自启动。下次登录 Mac 自动启动。" }
        else { "🚪 已关闭开机自启动。" }
    };
    for (_, w) in app.webview_windows() {
        let _ = w.emit(crate::events::EV_VIEW_CHANGED, serde_json::json!({
            "kind": "reply", "transcript": "autostart toggle",
            "reply": msg, "mode": "A", "streaming": false,
        }));
    }
}

/// 切换 voice IME（长按 fn → 写到光标）
pub(crate) fn toggle_voice_ime(app: &AppHandle) {
    use tauri::Emitter;
    let mut cfg = crate::config::Config::load();
    cfg.voice_ime_enabled = !cfg.voice_ime_enabled;
    let now_on = cfg.voice_ime_enabled;
    if let Err(e) = cfg.save() {
        eprintln!("[mouseclaw] toggle_voice_ime save: {e}");
        return;
    }
    #[cfg(target_os = "macos")]
    crate::voice_ime::set_enabled(now_on);
    let lang = cfg.language;
    let msg = if now_on {
        if lang == "en" {
            "🎙️ Voice IME: ON. Hold fn key for >300ms then speak; release → typed at cursor. Short tap on fn still works (macOS default)."
        } else {
            "🎙️ 语音输入法：开。按住 fn 键 >300ms 开始说话，松开 → 文字写到光标。短按 fn 仍走 macOS 原生行为。"
        }
    } else {
        if lang == "en" {
            "✋ Voice IME: OFF. fn key restored to macOS default behavior."
        } else {
            "✋ 语音输入法：关。fn 键恢复 macOS 原生行为。"
        }
    };
    for (_, w) in app.webview_windows() {
        let _ = w.emit(crate::events::EV_VIEW_CHANGED, serde_json::json!({
            "kind": "reply",
            "transcript": "voice IME toggle",
            "reply": msg,
            "mode": "A",
            "streaming": false,
        }));
    }
}

// v0.3.4 · toggle_tidy_up deleted alongside LLM polish.

/// 托盘点「启用浏览器自动化」—— 注册 chrome-devtools MCP + 启动带 CDP 的 Chrome。
/// 成功后弹一个 webview 通知窗口（about 复用风格）告诉用户「下次提问就能用了」。
pub(crate) fn enable_browser_automation(app: &AppHandle) {
    use tauri::Emitter;
    println!("[mouseclaw] 🌐 启用浏览器自动化（一键）…");
    match crate::browser_bridge::enable() {
        Ok(()) => {
            println!("[mouseclaw] ✓ 浏览器自动化已就绪 (CDP {} + chrome-devtools MCP)",
                     crate::browser_bridge::CDP_PORT);
            // 通过 view-changed 让 overlay 弹一个友好提示（4s 自动隐藏）
            for (_, w) in app.webview_windows() {
                let _ = w.emit(crate::events::EV_VIEW_CHANGED, serde_json::json!({
                    "kind": "reply",
                    "transcript": "启用浏览器自动化",
                    "reply": "✅ Chrome 已连上 —— 下次提问可以让我直接操作你的浏览器了。\n\
                              提示：debug profile 在首次使用前请登录一下要操作的网站。",
                    "mode": "A",
                    "streaming": false,
                }));
            }
        }
        Err(e) => {
            eprintln!("[mouseclaw] ✘ 启用浏览器自动化失败: {e:#}");
            for (_, w) in app.webview_windows() {
                let _ = w.emit(crate::events::EV_VIEW_CHANGED, serde_json::json!({
                    "kind": "blocked",
                    "reason": format!("启用失败：{e:#}"),
                }));
            }
        }
    }
}

/// 托盘点语言 → 持久化 + 广播 EV_LANG_CHANGED。前端 i18n 热切换。
/// 注意：托盘菜单本身的标签**不会**热更新（Tauri menu item 不支持 set_text），
/// 所以提示用户重启 / 下次启动看到的菜单是新语言。
pub(crate) fn change_language(app: &AppHandle, lang: &str) {
    use tauri::Emitter;
    if !(lang == "zh" || lang == "en") { return; }
    let mut cfg = crate::config::Config::load();
    if cfg.language == lang { return; }
    cfg.language = lang.to_string();
    if let Err(e) = cfg.save() {
        eprintln!("[mouseclaw] change_language save: {e}");
        return;
    }
    for (_, w) in app.webview_windows() {
        let _ = w.emit(crate::events::EV_LANG_CHANGED, lang.to_string());
    }
    println!("[mouseclaw] 🌐 tray: language → {lang} (托盘标签下次启动生效)");
    // 友好提示
    let tip = if lang == "en" {
        "🌐 Language switched. Restart MouseClaw to update the tray menu labels."
    } else {
        "🌐 已切换语言。重启 MouseClaw 以更新托盘菜单文字。"
    };
    for (_, w) in app.webview_windows() {
        let _ = w.emit(crate::events::EV_VIEW_CHANGED, serde_json::json!({
            "kind": "reply",
            "transcript": "language",
            "reply": tip,
            "mode": "A",
            "streaming": false,
        }));
    }
}

// v0.3 · change_whisper_model deleted — sherpa zh-en is sole bundled ASR.

/// 托盘子菜单点击 → 持久化 + 广播 skin-changed。
/// 复用 commands::save_skin 的实现，保证逻辑只有一处。
pub(crate) fn change_skin(app: &AppHandle, skin_name: &str) {
    use tauri::Emitter;
    let parsed = SkinId::from_str(skin_name);
    let mut cfg = crate::config::Config::load();
    cfg.skin = parsed;
    if let Err(e) = cfg.save() {
        eprintln!("[mouseclaw] tray change_skin save failed: {e}");
        return;
    }
    let payload = parsed.as_str().to_string();
    for (_, w) in app.webview_windows() {
        let _ = w.emit(crate::events::EV_SKIN_CHANGED, payload.clone());
    }
    println!("[mouseclaw] 🎨 tray: skin → {:?}", parsed);
}

/// Programmatically trigger the same flow as a global-shortcut press.
/// If user hasn't completed onboarding yet, divert to the onboarding window
/// instead — calling the pipeline without a registered shortcut isn't useful.
pub(crate) fn summon_via_tray(app: &AppHandle) {
    use std::sync::Arc;
    if !crate::config::Config::load().onboarded {
        println!("[mouseclaw] tray summon: not onboarded yet → opening Onboarding");
        open_onboarding_window(app);
        return;
    }
    let state: Arc<crate::AppState> = app.state::<Arc<crate::AppState>>().inner().clone();
    crate::overlay::show_mouse(app);
    let app_clone = app.clone();
    tauri::async_runtime::spawn(async move {
        crate::pipeline::on_shortcut_pressed(app_clone, state).await;
    });
}
