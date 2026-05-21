//! 托盘菜单**组装** —— `build_menu` 一次性按当前 config 构建整个 `Menu`，
//! `rebuild_tray_menu` 在单选项切换后重建并 `set_menu`，保证单选语义（只有 1 个 ✓）。
//!
//! 从 tray.rs 抽出（tray.rs 已超 800 行硬上限 · 见 CLAUDE.md「单文件 ≤ 800 行」）。
//! 子菜单的构建分散在各自模块：快捷键 → `shortcut_menu`，桌宠位置 → `anchor`。
//! 菜单项点击的 dispatch 在 `tray_handlers`，辅助窗口在 `tray_windows`。

use tauri::{
    menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu},
    AppHandle, Manager,
};

use crate::skins::SkinId;

/// 构建托盘菜单 —— 抽出来以便每次单选切换后重建（fix multi-check bug · v0.1.17）
/// 之前点不同 skin/model/lang 后旧勾不消、新勾叠加 → 看起来像多选。
/// 改成每次切换都用最新 config rebuild 整个 Menu + tray.set_menu()，单选语义保证。
pub fn build_menu(app: &AppHandle) -> tauri::Result<Menu<tauri::Wry>> {
    let current_lang = crate::config::Config::load().language;
    let en = current_lang == "en";

    let current_workspace = crate::config::Config::load().workspace_path;
    let workspace_short = current_workspace.as_ref()
        .map(|p| std::path::Path::new(p).file_name().and_then(|n| n.to_str())
                  .map(|s| s.to_string()).unwrap_or_else(|| p.clone()))
        .unwrap_or_else(|| if en { "(none)".into() } else { "(未设)".into() });

    let s_workspace_label = if en {
        format!("📁 Workspace: {workspace_short}")
    } else {
        format!("📁 工作区：{workspace_short}")
    };

    let (s_summon, s_history, s_clipboard, _s_skin, s_model, s_browser_on, s_browser_off,
         s_status, s_about, s_quit, s_lang_menu, s_tidy, s_vime, s_pause, s_autostart) = if en {
        ("🦞 Summon", "📜 History…", "📋 Clipboard… ⌘⇧V",
         "🎨 Change pet", "🎙️ Voice model",
         "🌐 Browser automation: enabled ✓", "🌐 Enable browser automation…",
         "📊 System status…", "ℹ️  About MouseClaw", "Quit MouseClaw", "🌐 Language",
         "✨ LLM polish voice (+3–8s, off by default)",
         "🎙️ Voice IME (hold fn → type at cursor)",
         "⏸️ Pause clipboard recording",
         "🚀 Launch at login")
    } else {
        ("🦞 召唤老鼠", "📜 查看历史记录…", "📋 剪贴板… ⌘⇧V",
         "🎨 换个桌宠", "🎙️ 语音模型",
         "🌐 浏览器自动化：已启用 ✓", "🌐 启用浏览器自动化…",
         "📊 系统状态…", "ℹ️  关于 MouseClaw", "退出 MouseClaw", "🌐 语言",
         "✨ LLM 精修语音（+3–8s，默认关）",
         "🎙️ 语音输入法（长按 fn → 写到光标）",
         "⏸️ 暂停剪贴板记录",
         "🚀 开机自启动")
    };

    // v0.4.4 · 有名字时托盘显示「召唤 {name}」,让身份在最常见入口可见。
    let summon_label = match crate::config::Config::load()
        .pet_name
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        Some(name) => if en { format!("🦞 Summon {name}") } else { format!("🦞 召唤{name}") },
        None => s_summon.to_string(),
    };
    let summon  = MenuItem::with_id(app, "summon",  &summon_label,  true, None::<&str>)?;
    let history = MenuItem::with_id(app, "history", s_history, true, None::<&str>)?;
    // v0.1.17 · 显式剪贴板入口（解决 ⌘⇧V 发现性问题）
    let clipboard_item = MenuItem::with_id(app, "open-clipboard", s_clipboard, true, None::<&str>)?;

    // 「🎨 换个桌宠 ▸」子菜单 —— 6 款 SkinId，当前选中的打勾
    let current_skin = crate::config::Config::load().skin;
    // v0.1.26 · 桌宠选择从 submenu 改为单条「🎨 更换桌宠…」打开 picker 窗口
    // 旧的 skin_items submenu 留着代码但不渲染 —— 9+ 款时 menu 又长又看不到形象。
    // current_skin 仍在 picker 里高亮当前选中。
    let _ = current_skin; // 暂留变量避免 unused 警告
    let skin_picker_label = if en {
        format!("🎨 Change pet… ({})", SkinId::from_str(&crate::config::Config::load().skin.as_str()).display_name())
    } else {
        format!("🎨 更换桌宠… ({})", crate::config::Config::load().skin.display_name())
    };
    let skin_picker_item = MenuItem::with_id(app, "open-picker", &skin_picker_label, true, None::<&str>)?;

    // v0.4.4 · 长期记忆查看器入口
    let s_memory = if en { "🧠 What it remembers…" } else { "🧠 它记得的事…" };
    let memory_item = MenuItem::with_id(app, "open-memory", s_memory, true, None::<&str>)?;

    // 浏览器自动化标签 —— 根据当前 CDP 状态显示「启用 / 已启用 ✓」
    let cdp_alive = crate::browser_bridge::cdp_is_alive();
    let browser_label = if cdp_alive { s_browser_on } else { s_browser_off };
    let browser_item = MenuItem::with_id(app, "enable-browser", browser_label, true, None::<&str>)?;

    // v0.3 · Whisper model picker submenu deleted —— sherpa-zh-en is the sole
    // bundled ASR. No user choice surface. s_model label no longer used.
    let _ = s_model;

    // 「🌐 语言」子菜单 —— v0.1.9 i18n
    let lang_zh = CheckMenuItem::with_id(app, "lang:zh", "🇨🇳 中文",
        true, current_lang == "zh", None::<&str>)?;
    let lang_en = CheckMenuItem::with_id(app, "lang:en", "🇬🇧 English",
        true, current_lang == "en", None::<&str>)?;
    let lang_submenu = Submenu::with_id_and_items(
        app, "lang-submenu", s_lang_menu, true,
        &[&lang_zh as &dyn tauri::menu::IsMenuItem<tauri::Wry>,
          &lang_en as &dyn tauri::menu::IsMenuItem<tauri::Wry>],
    )?;

    // v0.3.3 · tidy_up 默认 ON + 强制 Haiku 4.5（成本 ~$0.0003/次）—— 不再需要托盘 toggle
    // 用户想关可以编辑 ~/.mouseclaw/config.json 的 tidy_up_enabled。
    let _ = s_tidy;
    // Voice IME toggle —— 长按 fn 写到光标
    let current_vime = crate::config::Config::load().voice_ime_enabled;
    let vime_item = CheckMenuItem::with_id(app, "toggle-voice-ime", s_vime,
        true, current_vime, None::<&str>)?;
    // 剪贴板暂停开关
    let current_paused = crate::config::Config::load().clipboard_paused;
    let pause_item = CheckMenuItem::with_id(app, "toggle-clipboard-pause", s_pause,
        true, current_paused, None::<&str>)?;
    // v0.1.26 · 开机自启动
    let current_autostart = crate::config::Config::load().autostart;
    let autostart_item = CheckMenuItem::with_id(app, "toggle-autostart", s_autostart,
        true, current_autostart, None::<&str>)?;
    // v0.4.0 · 桌宠开口说话 —— AI 回复完后用 macOS `say` 朗读
    let current_tts = crate::config::Config::load().tts_enabled;
    let s_tts = if en { "🔊 Pet speaks AI replies" } else { "🔊 桌宠朗读 AI 回复" };
    let tts_item = CheckMenuItem::with_id(app, "toggle-tts", s_tts,
        true, current_tts, None::<&str>)?;

    // v0.4.2 · 快捷键设置（统一入口）—— 召唤 AI 快捷键 + 语音输入触发键两个子菜单
    // 相邻摆放，构建/热切换逻辑都在 shortcut_menu（tray.rs 已超 800 行硬上限）。
    let summon_submenu = crate::shortcut_menu::build_summon_submenu(app, en)?;
    let trigger_submenu = crate::shortcut_menu::build_trigger_submenu(app, en)?;

    // v0.4.0 P1 · 术语表子菜单（hotwords contextual biasing）
    //   编辑 user.txt / 刷新生效 / 内置程序员词表 toggle
    let s_vocab_root = if en { "📝 Vocabulary" } else { "📝 术语表" };
    let s_vocab_edit = if en { "    ↳ Edit user vocabulary…" } else { "    ↳ 编辑用户词表..." };
    let s_vocab_reload = if en { "    ↳ Reload vocabulary" } else { "    ↳ 刷新生效" };
    let s_vocab_builtin = if en { "    ↳ Built-in programmer terms" } else { "    ↳ 内置程序员词表" };
    let vocab_edit_item = MenuItem::with_id(app, "vocab-edit", s_vocab_edit, true, None::<&str>)?;
    let vocab_reload_item = MenuItem::with_id(app, "vocab-reload", s_vocab_reload, true, None::<&str>)?;
    let vocab_builtin_on = crate::config::Config::load().vocab_builtin_enabled;
    let vocab_builtin_item = CheckMenuItem::with_id(
        app, "toggle-vocab-builtin", s_vocab_builtin,
        true, vocab_builtin_on, None::<&str>,
    )?;
    let vocab_submenu = Submenu::with_id_and_items(
        app, "vocab-submenu", s_vocab_root, true,
        &[&vocab_edit_item as &dyn tauri::menu::IsMenuItem<tauri::Wry>,
          &vocab_reload_item as &dyn tauri::menu::IsMenuItem<tauri::Wry>,
          &vocab_builtin_item as &dyn tauri::menu::IsMenuItem<tauri::Wry>],
    )?;

    // v0.4.x · 会话控制：新对话 + 钉住任务
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

    // v0.1.21 · 工作区设置（指向同一窗口的设置项；用户点 → 弹 NSOpenPanel）
    let workspace_item = MenuItem::with_id(
        app, "set-workspace", &s_workspace_label, true, None::<&str>
    )?;
    // 「清除工作区」只有已设时才显示
    let clear_workspace_item = if current_workspace.is_some() {
        Some(MenuItem::with_id(
            app, "clear-workspace",
            if en { "    ↳ Clear workspace" } else { "    ↳ 清除工作区" },
            true, None::<&str>
        )?)
    } else { None };

    let status  = MenuItem::with_id(app, "status",  s_status,     true, None::<&str>)?;
    let about   = MenuItem::with_id(app, "about",   s_about, true, None::<&str>)?;
    let sep1    = PredefinedMenuItem::separator(app)?;
    let sep2    = PredefinedMenuItem::separator(app)?;
    let quit    = MenuItem::with_id(app, "quit",    s_quit,  true, Some("CmdOrCtrl+Q"))?;
    // v0.4.0 · 模型下载窗口入口
    let downloader_label = if en { "📥 Model download" } else { "📥 模型下载进度" };
    let downloader_item = MenuItem::with_id(app, "open-downloader", downloader_label, true, None::<&str>)?;

    // v0.1.27 · 📍 桌宠位置 ▸ 子菜单（4 角 + 跟随光标）
    let anchor_submenu = crate::anchor::build_tray_submenu(app, en)?;

    // List items (declaring early so the vec! below can reference them)
    let mut items: Vec<&dyn tauri::menu::IsMenuItem<tauri::Wry>> = vec![
        &summon, &new_session_item, &pin_item, &clipboard_item, &history,
        &skin_picker_item, &memory_item, &anchor_submenu, &lang_submenu,
        &sep1, &vime_item, &summon_submenu, &trigger_submenu, &vocab_submenu, &pause_item,
        &workspace_item,
    ];
    if let Some(ref clr) = clear_workspace_item {
        items.push(clr as &dyn tauri::menu::IsMenuItem<tauri::Wry>);
    }
    items.extend([
        &browser_item as &dyn tauri::menu::IsMenuItem<tauri::Wry>,
        &autostart_item as &dyn tauri::menu::IsMenuItem<tauri::Wry>,
        &tts_item as &dyn tauri::menu::IsMenuItem<tauri::Wry>,
        &downloader_item as &dyn tauri::menu::IsMenuItem<tauri::Wry>,
        &status as &dyn tauri::menu::IsMenuItem<tauri::Wry>,
        &sep2,
        &about,
        &quit,
    ]);
    let menu = Menu::with_items(app, &items)?;
    Ok(menu)
}

/// 单选项切换后调一次，重建整个菜单确保只有 1 个 ✓
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
