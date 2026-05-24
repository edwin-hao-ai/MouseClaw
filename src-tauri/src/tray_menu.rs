//! 托盘菜单**组装** —— `build_menu` 一次性构建整个 `Menu`，`rebuild_tray_menu` 在状态变化后重建。
//!
//! 从 tray.rs 抽出（tray.rs 已超 800 行硬上限 · 见 CLAUDE.md「单文件 ≤ 800 行」）。
//!
//! v0.5.x · 托盘瘦身：所有**配置开关**（语言 / 自启 / TTS / 音效 / 语音输入法开关 / 剪贴板暂停 /
//! 桌宠位置 / AI 后端 / 工作区 / 浏览器自动化 / 内置词表）已收进**设置窗**（`open_settings_window`，
//! 见 SettingsView）。托盘只留**动作**：召唤 / 会话控制 / 打开各窗口 / 召唤快捷键（设置窗里只读展示）/
//! 术语表编辑（设置窗没有这俩文件动作）/ 设置… / 退出。诊断（原「系统状态」）= 设置窗的诊断分页。

use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem, Submenu},
    AppHandle, Manager,
};

use crate::skins::SkinId;

/// 构建托盘菜单（瘦身版 · v0.5.x）。
pub fn build_menu(app: &AppHandle) -> tauri::Result<Menu<tauri::Wry>> {
    let en = crate::config::Config::load().language == "en";

    let (s_summon, s_history, s_clipboard, s_about, s_quit) = if en {
        ("🦞 Summon", "📜 History…", "📋 Clipboard… ⌘⇧V", "ℹ️  About MouseClaw", "Quit MouseClaw")
    } else {
        ("🦞 召唤老鼠", "📜 查看历史记录…", "📋 剪贴板… ⌘⇧V", "ℹ️  关于 MouseClaw", "退出 MouseClaw")
    };

    // 召唤标签带名字（身份在最常见入口可见）
    let summon_label = match crate::config::Config::load()
        .pet_name.as_deref().map(str::trim).filter(|s| !s.is_empty())
    {
        Some(name) => if en { format!("🦞 Summon {name}") } else { format!("🦞 召唤{name}") },
        None => s_summon.to_string(),
    };
    let summon = MenuItem::with_id(app, "summon", &summon_label, true, None::<&str>)?;
    let history = MenuItem::with_id(app, "history", s_history, true, None::<&str>)?;
    let s_tasks = if en { "⏰ Scheduled tasks…" } else { "⏰ 定时任务…" };
    let tasks_item = MenuItem::with_id(app, "open-tasks", s_tasks, true, None::<&str>)?;
    let clipboard_item = MenuItem::with_id(app, "open-clipboard", s_clipboard, true, None::<&str>)?;

    // 换桌宠（快捷动作，打开 picker）
    let skin_picker_label = if en {
        format!("🎨 Change pet… ({})", SkinId::from_str(&crate::config::Config::load().skin.as_str()).display_name())
    } else {
        format!("🎨 更换桌宠… ({})", crate::config::Config::load().skin.display_name())
    };
    let skin_picker_item = MenuItem::with_id(app, "open-picker", &skin_picker_label, true, None::<&str>)?;

    // 它记得的事（记忆窗）
    let s_memory = if en { "🧠 What it remembers…" } else { "🧠 它记得的事…" };
    let memory_item = MenuItem::with_id(app, "open-memory", s_memory, true, None::<&str>)?;

    // 召唤快捷键子菜单 —— 设置窗里 shortcut 是只读展示，改键的唯一入口在这。
    let summon_submenu = crate::shortcut_menu::build_summon_submenu(app, en)?;

    // 术语表子菜单 —— 「编辑 user.txt / 刷新生效」是文件动作，设置窗没有；内置词表开关在设置窗。
    let s_vocab_root = if en { "📝 Vocabulary" } else { "📝 术语表" };
    let s_vocab_edit = if en { "    ↳ Edit user vocabulary…" } else { "    ↳ 编辑用户词表..." };
    let s_vocab_reload = if en { "    ↳ Reload vocabulary" } else { "    ↳ 刷新生效" };
    let vocab_edit_item = MenuItem::with_id(app, "vocab-edit", s_vocab_edit, true, None::<&str>)?;
    let vocab_reload_item = MenuItem::with_id(app, "vocab-reload", s_vocab_reload, true, None::<&str>)?;
    let vocab_submenu = Submenu::with_id_and_items(
        app, "vocab-submenu", s_vocab_root, true,
        &[&vocab_edit_item as &dyn tauri::menu::IsMenuItem<tauri::Wry>,
          &vocab_reload_item as &dyn tauri::menu::IsMenuItem<tauri::Wry>],
    )?;

    // 会话控制：新对话 + 钉住任务
    let pinned_now = app.try_state::<std::sync::Arc<crate::AppState>>()
        .map(|s| s.session_pinned.load(std::sync::atomic::Ordering::Relaxed))
        .unwrap_or(false);
    let s_new_session = if en { "🆕 New chat" } else { "🆕 新对话" };
    let s_pin = if pinned_now {
        if en { "📌 Unpin task (keep continuous)" } else { "📌 解除钉住任务" }
    } else {
        if en { "📌 Pin task (multi-round, never reset)" } else { "📌 钉住任务（多轮不重置）" }
    };
    let new_session_item = MenuItem::with_id(app, "new-session", s_new_session, true, None::<&str>)?;
    let pin_item = MenuItem::with_id(app, "toggle-pin-session", s_pin, true, None::<&str>)?;

    // 模型下载进度
    let downloader_label = if en { "📥 Model download" } else { "📥 模型下载进度" };
    let downloader_item = MenuItem::with_id(app, "open-downloader", downloader_label, true, None::<&str>)?;

    // 统一「设置…」入口（含诊断分页）+ 关于 + 退出
    let s_settings = if en { "⚙️  Settings…" } else { "⚙️  设置…" };
    let settings_item = MenuItem::with_id(app, "open-settings", s_settings, true, None::<&str>)?;
    let about = MenuItem::with_id(app, "about", s_about, true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", s_quit, true, Some("CmdOrCtrl+Q"))?;
    let sep1 = PredefinedMenuItem::separator(app)?;
    let sep2 = PredefinedMenuItem::separator(app)?;

    let items: Vec<&dyn tauri::menu::IsMenuItem<tauri::Wry>> = vec![
        &summon, &new_session_item, &pin_item,
        &sep1,
        &clipboard_item, &history, &tasks_item, &memory_item, &skin_picker_item,
        &summon_submenu, &vocab_submenu,
        &sep2,
        &settings_item, &downloader_item, &about, &quit,
    ];
    Menu::with_items(app, &items)
}

/// 状态变化后调一次，重建整个菜单（召唤名 / 钉住态 / 皮肤名 等变了要刷新 label）。
pub fn rebuild_tray_menu(app: &AppHandle) {
    let Some(tray) = app.tray_by_id("main-tray") else {
        eprintln!("[mouseclaw] rebuild_tray_menu: tray not found");
        return;
    };
    match build_menu(app) {
        Ok(menu) => {
            if let Err(e) = tray.set_menu(Some(menu)) {
                eprintln!("[mouseclaw] tray.set_menu: {e}");
            }
        }
        Err(e) => eprintln!("[mouseclaw] build_menu: {e}"),
    }
}
