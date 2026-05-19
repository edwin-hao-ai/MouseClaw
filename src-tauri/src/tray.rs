//! Menubar tray icon — adds "召唤老鼠 / 查看历史 / 退出" menu in the macOS
//! status bar. Originally listed as V2 in CLAUDE.md; user explicitly asked
//! for it in v0.1.4 ("最好能有个托盘图标").
//!
//! Click the tray icon = same as pressing the global shortcut (summon).
//! Right-click = menu.

use tauri::{
    image::Image,
    menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Manager, WebviewUrl, WebviewWindowBuilder,
};

use crate::skins::SkinId;

/// 16×16 monochrome mouse silhouette for the macOS menubar (template image).
/// Each u16 row encodes 16 horizontal pixels MSB-first: 1 = opaque black, 0 = transparent.
/// When `icon_as_template(true)` is set, macOS auto-tints this for dark/light menubars.
const MOUSE_TEMPLATE_BITMAP: [u16; 16] = [
    0b0001100001100000, // row 0: ear tops
    0b0011110011110000, // row 1: ears wide
    0b0011110011110000, // row 2
    0b0111111111111110, // row 3: head top
    0b0111111111111110, // row 4: head with eye slots
    0b0111101111011110, // row 5: eyes (gaps)
    0b0111111111111110, // row 6
    0b0011111111111110, // row 7
    0b0011111111111111, // row 8: belly + tail starts
    0b0011111111111111, // row 9
    0b0001111111111110, // row 10
    0b0000111111111100, // row 11
    0b0000110000110000, // row 12: paws gap
    0b0001100000011000, // row 13
    0b0000000000000000, // row 14
    0b0000000000000000, // row 15
];

/// Build the tray icon as raw RGBA at 2× resolution for retina menubars.
/// The bitmap is 16×16 logical pixels, scaled to 32×32 physical pixels by
/// pixel-doubling. macOS will downscale to whatever its menubar height
/// requires (typically 22pt) while preserving sharpness on retina.
fn build_template_icon() -> Image<'static> {
    const SCALE: usize = 2;
    const SIZE: usize = 16 * SCALE; // 32
    let mut rgba = vec![0u8; SIZE * SIZE * 4];
    for y in 0..16 {
        let row = MOUSE_TEMPLATE_BITMAP[y];
        for x in 0..16 {
            let bit = (row >> (15 - x)) & 1;
            if bit == 1 {
                // Fill the corresponding 2×2 block in the upscaled grid
                for dy in 0..SCALE {
                    for dx in 0..SCALE {
                        let px = x * SCALE + dx;
                        let py = y * SCALE + dy;
                        let i = (py * SIZE + px) * 4;
                        rgba[i + 3] = 255; // template: alpha-only matters
                    }
                }
            }
        }
    }
    Image::new_owned(rgba, SIZE as u32, SIZE as u32)
}

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

    let summon  = MenuItem::with_id(app, "summon",  s_summon,  true, None::<&str>)?;
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

    // Voice IME trigger 子菜单 —— Fn/Option/Control/RightShift/RightCmd/RightOption
    let current_trigger = crate::voice_ime::ImeTrigger::from_str(
        &crate::config::Config::load().voice_ime_trigger
    );
    let mut trigger_items: Vec<CheckMenuItem<tauri::Wry>> = Vec::new();
    for t in crate::voice_ime::ImeTrigger::all() {
        let label = if en { t.display_en() } else { t.display_zh() };
        let item = CheckMenuItem::with_id(
            app, format!("vime-trigger:{}", t.as_str()),
            label, true, *t == current_trigger, None::<&str>,
        )?;
        trigger_items.push(item);
    }
    let trigger_refs: Vec<&dyn tauri::menu::IsMenuItem<tauri::Wry>> =
        trigger_items.iter().map(|i| i as &dyn tauri::menu::IsMenuItem<tauri::Wry>).collect();
    let trigger_submenu_label = if en { "    ↳ IME trigger key" } else { "    ↳ 触发键" };
    let trigger_submenu = Submenu::with_id_and_items(
        app, "vime-trigger-submenu", trigger_submenu_label, true, &trigger_refs,
    )?;

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
        &summon, &clipboard_item, &history,
        &skin_picker_item, &anchor_submenu, &lang_submenu,
        &sep1, &vime_item, &trigger_submenu, &vocab_submenu, &pause_item,
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

pub fn setup(app: &AppHandle) -> tauri::Result<()> {
    let menu = build_menu(app)?;
    let icon = build_template_icon();

    TrayIconBuilder::with_id("main-tray")
        .icon(icon)
        .icon_as_template(true)
        .tooltip("MouseClaw 🦞 — 按 Cmd+Shift+Space 召唤  ·  ⌘⇧V 看剪贴板")
        .menu(&menu)
        .on_menu_event(handle_menu_event)
        .on_tray_icon_event(|tray, event| {
            let app = tray.app_handle();
            match event {
                // v0.1.13 · 双击托盘 → 打开 Hub（v0.1.16 改独立窗口）
                TrayIconEvent::DoubleClick { button: MouseButton::Left, .. } => {
                    println!("[mouseclaw] 📋 tray double-click → open Hub window");
                    if let Err(e) = crate::commands::open_hub_window(app.clone()) {
                        eprintln!("[mouseclaw] tray double-click open hub: {e}");
                    }
                }
                // 单击托盘 → 召唤
                TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                } => {
                    summon_via_tray(app);
                }
                _ => {}
            }
        })
        .build(app)?;

    Ok(())
}

fn handle_menu_event(app: &AppHandle, event: MenuEvent) {
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
        change_ime_trigger(app, trigger);
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
        "open-clipboard"  => {
            if let Err(e) = crate::commands::open_hub_window(app.clone()) {
                eprintln!("[mouseclaw] open-clipboard: {e}");
            }
        }
        "history"         => open_history_window(app),
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
        "status"          => open_status_window(app),
        // toggle-tidy removed in v0.3.3 — tidy_up is default-on via Haiku
        "toggle-voice-ime"=> { toggle_voice_ime(app); rebuild_tray_menu(app); }
        "toggle-clipboard-pause" => { toggle_clipboard_pause(app); rebuild_tray_menu(app); }
        "toggle-autostart" => { toggle_autostart(app); rebuild_tray_menu(app); }
        "toggle-tts" => { toggle_tts(app); rebuild_tray_menu(app); }
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

/// 弹原生 NSOpenPanel 选目录 → 存进 config
/// 用 osascript 触发 —— Tauri 的 dialog plugin 也能做但要额外配权限
fn set_workspace_via_picker(app: &AppHandle) {
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

/// v0.4.0 · 切换 TTS（桌宠开口说话）
/// v0.4.0 P1 · 术语表刷新后弹个气泡告诉用户「N 词生效」。
fn emit_vocab_reloaded(app: &AppHandle, n: usize) {
    use tauri::Emitter;
    let lang = crate::config::Config::load().language;
    let msg = if lang == "en" {
        format!("📝 Vocabulary reloaded — {n} terms active")
    } else {
        format!("📝 术语表已刷新 — {n} 个词生效")
    };
    for (_, w) in app.webview_windows() {
        let _ = w.emit(crate::events::EV_VIEW_CHANGED, serde_json::json!({
            "kind": "reply", "transcript": "vocab reload",
            "reply": msg, "mode": "A", "streaming": false,
        }));
    }
}

fn toggle_tts(app: &AppHandle) {
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

/// 切换剪贴板暂停（隐私 ⑧）
fn toggle_clipboard_pause(app: &AppHandle) {
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
fn toggle_autostart(app: &AppHandle) {
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

/// 换 voice IME 触发键 —— 托盘子菜单调
fn change_ime_trigger(app: &AppHandle, trigger_str: &str) {
    use tauri::Emitter;
    let trigger = crate::voice_ime::ImeTrigger::from_str(trigger_str);
    let mut cfg = crate::config::Config::load();
    if cfg.voice_ime_trigger == trigger_str { return; }
    cfg.voice_ime_trigger = trigger.as_str().to_string();
    if let Err(e) = cfg.save() {
        eprintln!("[mouseclaw] change_ime_trigger save: {e}");
        return;
    }
    crate::voice_ime::set_trigger(trigger);
    let lang = cfg.language;
    let msg = if lang == "en" {
        format!("🎙️ IME trigger → {}. Voice IME re-bound. (Restart tray to update menu labels.)", trigger.display_en())
    } else {
        format!("🎙️ 语音输入触发键 → {}。已重新绑定。（重启托盘后菜单标签同步。）", trigger.display_zh())
    };
    for (_, w) in app.webview_windows() {
        let _ = w.emit(crate::events::EV_VIEW_CHANGED, serde_json::json!({
            "kind": "reply",
            "transcript": "IME trigger change",
            "reply": msg,
            "mode": "A",
            "streaming": false,
        }));
    }
}

/// 切换 voice IME（长按 fn → 写到光标）
fn toggle_voice_ime(app: &AppHandle) {
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

/// 打开「📊 系统状态」窗口 —— 一眼看到 claude / agent-browser / Chrome CDP / 权限的就绪状态。
/// 每行都有"去解决"按钮（装 / 启用 / 开权限）。
fn open_status_window(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("status") {
        let _ = w.show();
        activate_app();
        let _ = w.set_focus();
        return;
    }
    let result = WebviewWindowBuilder::new(
        app,
        "status",
        WebviewUrl::App("index.html?view=status".into()),
    )
    .title("MouseClaw — 系统状态")
    .inner_size(520.0, 560.0)
    .min_inner_size(420.0, 420.0)
    .resizable(true)
    .decorations(true)
    .focused(true)
    .build();
    match result {
        Ok(w) => {
            activate_app();
            let _ = w.set_focus();
        }
        Err(e) => eprintln!("[mouseclaw] open status window failed: {e:#}"),
    }
}

/// 托盘点「启用浏览器自动化」—— 注册 chrome-devtools MCP + 启动带 CDP 的 Chrome。
/// 成功后弹一个 webview 通知窗口（about 复用风格）告诉用户「下次提问就能用了」。
fn enable_browser_automation(app: &AppHandle) {
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

/// 托盘子菜单点击 → 持久化 + 广播 skin-changed。
/// 托盘点语言 → 持久化 + 广播 EV_LANG_CHANGED。前端 i18n 热切换。
/// 注意：托盘菜单本身的标签**不会**热更新（Tauri menu item 不支持 set_text），
/// 所以提示用户重启 / 下次启动看到的菜单是新语言。
fn change_language(app: &AppHandle, lang: &str) {
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

/// 复用 commands::save_skin 的实现，保证逻辑只有一处。
fn change_skin(app: &AppHandle, skin_name: &str) {
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
fn summon_via_tray(app: &AppHandle) {
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

/// Force the .accessory-policy app to come forward so the new window has focus.
/// Without this, LSUIElement / setActivationPolicy(.accessory) causes new windows
/// to open behind whatever app is currently active.
#[cfg(target_os = "macos")]
fn activate_app() {
    use objc::{class, msg_send, sel, sel_impl};
    unsafe {
        let app: cocoa::base::id = msg_send![class!(NSApplication), sharedApplication];
        let _: () = msg_send![app, activateIgnoringOtherApps: true];
    }
}
#[cfg(not(target_os = "macos"))]
fn activate_app() {}

fn open_history_window(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("history") {
        let _ = w.show();
        activate_app();
        let _ = w.set_focus();
        return;
    }
    let result = WebviewWindowBuilder::new(
        app,
        "history",
        WebviewUrl::App("index.html?view=history".into()),
    )
    .title("MouseClaw — 历史记录")
    .inner_size(720.0, 560.0)
    .min_inner_size(480.0, 360.0)
    .resizable(true)
    .decorations(true)
    .focused(true)
    .build();
    match result {
        Ok(w) => {
            activate_app();
            let _ = w.set_focus();
        }
        Err(e) => eprintln!("[mouseclaw] failed to open history window: {e:#}"),
    }
}

pub fn open_onboarding_window(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("onboarding") {
        let _ = w.show();
        activate_app();
        let _ = w.set_focus();
        return;
    }
    let result = WebviewWindowBuilder::new(
        app,
        "onboarding",
        WebviewUrl::App("index.html?view=onboarding".into()),
    )
    .title("欢迎使用 MouseClaw 🦞")
    .inner_size(700.0, 760.0)
    .min_inner_size(560.0, 680.0)
    .resizable(false)
    .decorations(true)
    .focused(true)
    .build();
    match result {
        Ok(w) => {
            activate_app();
            let _ = w.set_focus();
            println!("[mouseclaw] onboarding window opened");
        }
        Err(e) => eprintln!("[mouseclaw] failed to open onboarding window: {e:#}"),
    }
}

fn open_about_dialog(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("about") {
        let _ = w.show();
        activate_app();
        let _ = w.set_focus();
        return;
    }
    let result = WebviewWindowBuilder::new(
        app,
        "about",
        WebviewUrl::App("index.html?view=about".into()),
    )
    .title("关于 MouseClaw")
    .inner_size(420.0, 340.0)
    .resizable(false)
    .decorations(true)
    .build();
    if let Ok(w) = result {
        activate_app();
        let _ = w.set_focus();
    }
}
