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

use crate::config::WhisperModel;
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

pub fn setup(app: &AppHandle) -> tauri::Result<()> {
    // 当前语言决定所有菜单文案 —— 一处 lookup，下面所有标签从 (s_*) 取
    let current_lang = crate::config::Config::load().language;
    let en = current_lang == "en";

    let (s_summon, s_history, s_skin, s_model, s_browser_on, s_browser_off,
         s_status, s_about, s_quit, s_lang_menu, s_tidy, s_vime) = if en {
        ("🦞 Summon", "📜 History…", "🎨 Change pet", "🎙️ Voice model",
         "🌐 Browser automation: enabled ✓", "🌐 Enable browser automation…",
         "📊 System status…", "ℹ️  About MouseClaw", "Quit MouseClaw", "🌐 Language",
         "✨ LLM polish voice (+3–8s, off by default)",
         "🎙️ Voice IME (hold fn → type at cursor)")
    } else {
        ("🦞 召唤老鼠", "📜 查看历史记录…", "🎨 换个桌宠", "🎙️ 语音模型",
         "🌐 浏览器自动化：已启用 ✓", "🌐 启用浏览器自动化…",
         "📊 系统状态…", "ℹ️  关于 MouseClaw", "退出 MouseClaw", "🌐 语言",
         "✨ LLM 精修语音（+3–8s，默认关）",
         "🎙️ 语音输入法（长按 fn → 写到光标）")
    };

    let summon  = MenuItem::with_id(app, "summon",  s_summon,  true, None::<&str>)?;
    let history = MenuItem::with_id(app, "history", s_history, true, None::<&str>)?;

    // 「🎨 换个桌宠 ▸」子菜单 —— 6 款 SkinId，当前选中的打勾
    let current_skin = crate::config::Config::load().skin;
    let mut skin_items: Vec<CheckMenuItem<tauri::Wry>> = Vec::new();
    for s in SkinId::all() {
        let item = CheckMenuItem::with_id(
            app,
            s.tray_menu_id(),
            s.display_name(),
            true,
            *s == current_skin,
            None::<&str>,
        )?;
        skin_items.push(item);
    }
    let skin_refs: Vec<&dyn tauri::menu::IsMenuItem<tauri::Wry>> =
        skin_items.iter().map(|i| i as &dyn tauri::menu::IsMenuItem<tauri::Wry>).collect();
    let skin_submenu = Submenu::with_id_and_items(
        app, "skin-submenu", s_skin, true, &skin_refs,
    )?;

    // 浏览器自动化标签 —— 根据当前 CDP 状态显示「启用 / 已启用 ✓」
    let cdp_alive = crate::browser_bridge::cdp_is_alive();
    let browser_label = if cdp_alive { s_browser_on } else { s_browser_off };
    let browser_item = MenuItem::with_id(app, "enable-browser", browser_label, true, None::<&str>)?;

    // 「🎙️ 语音模型 ▸」子菜单 —— 4 档 Whisper 模型，当前选中打勾
    let current_model = crate::config::Config::load().whisper_model;
    let mut model_items: Vec<CheckMenuItem<tauri::Wry>> = Vec::new();
    for m in WhisperModel::all() {
        let item = CheckMenuItem::with_id(
            app, m.tray_menu_id(), m.display_name(),
            true, *m == current_model, None::<&str>,
        )?;
        model_items.push(item);
    }
    let model_refs: Vec<&dyn tauri::menu::IsMenuItem<tauri::Wry>> =
        model_items.iter().map(|i| i as &dyn tauri::menu::IsMenuItem<tauri::Wry>).collect();
    let model_submenu = Submenu::with_id_and_items(
        app, "whisper-submenu", s_model, true, &model_refs,
    )?;

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

    // Tidy-up toggle —— Typeless 套路 LLM 清洗
    let current_tidy = crate::config::Config::load().tidy_up_enabled;
    let tidy_item = CheckMenuItem::with_id(app, "toggle-tidy", s_tidy,
        true, current_tidy, None::<&str>)?;
    // Voice IME toggle —— 长按 fn 写到光标
    let current_vime = crate::config::Config::load().voice_ime_enabled;
    let vime_item = CheckMenuItem::with_id(app, "toggle-voice-ime", s_vime,
        true, current_vime, None::<&str>)?;

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

    let status  = MenuItem::with_id(app, "status",  s_status,     true, None::<&str>)?;
    let about   = MenuItem::with_id(app, "about",   s_about, true, None::<&str>)?;
    let sep1    = PredefinedMenuItem::separator(app)?;
    let sep2    = PredefinedMenuItem::separator(app)?;
    let quit    = MenuItem::with_id(app, "quit",    s_quit,  true, Some("CmdOrCtrl+Q"))?;

    let menu = Menu::with_items(app, &[
        &summon, &history,
        &skin_submenu, &model_submenu, &lang_submenu,
        &sep1, &vime_item, &trigger_submenu, &tidy_item, &browser_item, &status,
        &sep2, &about, &quit,
    ])?;
    let icon = build_template_icon();

    TrayIconBuilder::with_id("main-tray")
        .icon(icon)
        .icon_as_template(true)
        .tooltip("MouseClaw 🦞 — 按 Cmd+Shift+Space 召唤")
        .menu(&menu)
        .on_menu_event(handle_menu_event)
        .on_tray_icon_event(|tray, event| {
            // Left-click on the icon body → summon (same as shortcut)
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                let app = tray.app_handle();
                summon_via_tray(app);
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
        return;
    }
    // Whisper 模型子菜单：id 形如 "whisper:small" / "whisper:turbo"
    if let Some(model_name) = id.strip_prefix("whisper:") {
        change_whisper_model(app, model_name);
        return;
    }
    // 语言子菜单：id 形如 "lang:zh" / "lang:en"
    if let Some(lang) = id.strip_prefix("lang:") {
        change_language(app, lang);
        return;
    }
    // voice IME 触发键子菜单：id 形如 "vime-trigger:option"
    if let Some(trigger) = id.strip_prefix("vime-trigger:") {
        change_ime_trigger(app, trigger);
        return;
    }
    match id {
        "summon"          => summon_via_tray(app),
        "history"         => open_history_window(app),
        "about"           => open_about_dialog(app),
        "enable-browser"  => enable_browser_automation(app),
        "status"          => open_status_window(app),
        "toggle-tidy"     => toggle_tidy_up(app),
        "toggle-voice-ime"=> toggle_voice_ime(app),
        "quit"            => app.exit(0),
        _ => {}
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

/// 切换 tidy-up（语音清洗）开关 —— 托盘 CheckMenuItem 调它
fn toggle_tidy_up(app: &AppHandle) {
    use tauri::Emitter;
    let mut cfg = crate::config::Config::load();
    cfg.tidy_up_enabled = !cfg.tidy_up_enabled;
    let now_on = cfg.tidy_up_enabled;
    if let Err(e) = cfg.save() {
        eprintln!("[mouseclaw] toggle_tidy_up save: {e}");
        return;
    }
    println!("[mouseclaw] ✨ tidy_up → {now_on}");
    let lang = cfg.language;
    let msg = if now_on {
        if lang == "en" {
            "✨ LLM polish: ON. Voice will be cleaned by AI before sending (adds 3–8s latency, off by default for speed). Regex cleanup still always-on."
        } else {
            "✨ LLM 精修：开。说完会先过一次 AI 整理（多 3–8 秒延迟，默认关是为了流畅）。基础 regex 清理一直都在跑。"
        }
    } else {
        if lang == "en" {
            "✋ LLM polish: OFF (default · faster). Regex light cleanup still runs (50ms) — filler words still removed."
        } else {
            "✋ LLM 精修：关（默认 · 更快）。Regex 轻量清理仍在跑（50ms），口头禅照样会被去掉。"
        }
    };
    for (_, w) in app.webview_windows() {
        let _ = w.emit(crate::events::EV_VIEW_CHANGED, serde_json::json!({
            "kind": "reply",
            "transcript": "tidy-up toggle",
            "reply": msg,
            "mode": "A",
            "streaming": false,
        }));
    }
}

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

/// 托盘点 Whisper 模型 → 持久化 + 触发后台下载（如缺）+ 通知用户。
fn change_whisper_model(app: &AppHandle, name: &str) {
    use tauri::Emitter;
    let parsed = WhisperModel::from_str(name);
    let mut cfg = crate::config::Config::load();
    if cfg.whisper_model == parsed {
        return;
    }
    cfg.whisper_model = parsed;
    if let Err(e) = cfg.save() {
        eprintln!("[mouseclaw] tray change_whisper_model save failed: {e}");
        return;
    }
    crate::transcribe::set_active_model(parsed);
    let msg = if crate::transcribe::is_available() {
        format!("✅ 已切到 {} — 立即生效", parsed.display_name())
    } else {
        format!(
            "📦 切到 {} — 后台下载 {}MB 中，下次提问就用新模型",
            parsed.display_name(), parsed.size_mb()
        )
    };
    println!("[mouseclaw] 🎙️ {msg}");
    for (_, w) in app.webview_windows() {
        let _ = w.emit(crate::events::EV_VIEW_CHANGED, serde_json::json!({
            "kind": "reply",
            "transcript": "切换语音模型",
            "reply": msg,
            "mode": "A",
            "streaming": false,
        }));
    }
}

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
