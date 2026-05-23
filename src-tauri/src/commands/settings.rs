//! 设置类 `#[tauri::command]`（语言 / voice IME / 剪贴板暂停 / 工作区 / 自启动 / 术语库）——
//! 从 commands.rs 抽出（800 行硬规则）。多为读写 config.json + 旁路生效。

use std::str::FromStr;
use tauri::{AppHandle, Emitter, Manager};

use crate::config;

/// 切换 UI 语言 —— 任意窗口 / 托盘调它。
/// 1. 持久化进 config.json
/// 2. 广播 EV_LANG_CHANGED；前端 i18n module 监听切换，所有 UI 立即重渲染
#[tauri::command]
pub fn save_language(lang: String, app: AppHandle) -> Result<(), String> {
    // 简单校验：只接受已知的 lang id（防止脏数据进 config）
    let allowed = ["zh", "en"];
    if !allowed.contains(&lang.as_str()) {
        return Err(format!("不支持的语言：{lang} (允许：{:?})", allowed));
    }
    let mut cfg = config::Config::load();
    cfg.language = lang.clone();
    cfg.save().map_err(|e| format!("保存失败：{e}"))?;
    for (_, w) in app.webview_windows() {
        let _ = w.emit(crate::events::EV_LANG_CHANGED, lang.clone());
    }
    println!("[mouseclaw] 🌐 language → {lang}");
    Ok(())
}

#[tauri::command]
pub fn get_language() -> String {
    config::Config::load().language
}

/// 切换 voice IME（fn 长按写到光标）
#[tauri::command]
pub fn save_voice_ime(enabled: bool) -> Result<(), String> {
    let mut cfg = config::Config::load();
    cfg.voice_ime_enabled = enabled;
    cfg.save().map_err(|e| format!("保存失败：{e}"))?;
    crate::voice_ime::set_enabled(enabled);
    Ok(())
}

#[tauri::command]
pub fn get_voice_ime() -> bool {
    config::Config::load().voice_ime_enabled
}

/// 设置 voice IME 触发键
#[tauri::command]
pub fn save_voice_ime_trigger(trigger: String) -> Result<(), String> {
    // 校验是已知值
    let allowed = ["fn", "option", "control", "right-shift", "right-command", "right-option"];
    if !allowed.contains(&trigger.as_str()) {
        return Err(format!("未知 trigger: {trigger}（允许 {:?}）", allowed));
    }
    let mut cfg = config::Config::load();
    cfg.voice_ime_trigger = trigger.clone();
    cfg.save().map_err(|e| format!("保存失败：{e}"))?;
    crate::voice_ime::set_trigger(crate::voice_ime::ImeTrigger::from_str(&trigger));
    Ok(())
}

#[tauri::command]
pub fn get_voice_ime_trigger() -> String {
    config::Config::load().voice_ime_trigger
}

#[tauri::command]
pub fn save_clipboard_paused(paused: bool) -> Result<(), String> {
    let mut cfg = config::Config::load();
    cfg.clipboard_paused = paused;
    cfg.save().map_err(|e| format!("保存失败：{e}"))?;
    crate::clipboard::set_paused(paused);
    Ok(())
}

#[tauri::command]
pub fn get_clipboard_paused() -> bool {
    crate::clipboard::is_paused()
}

/// v0.1.21 · 设置工作区路径
/// 传 None / 空字符串 = 清除（回到默认 cwd）
#[tauri::command]
pub fn save_workspace_path(path: Option<String>) -> Result<(), String> {
    let mut cfg = config::Config::load();
    let p = path.and_then(|s| if s.trim().is_empty() { None } else { Some(s) });
    // 校验路径存在 + 是目录
    if let Some(ref pp) = p {
        let pb = std::path::PathBuf::from(pp);
        if !pb.is_dir() {
            return Err(format!("路径不存在或不是目录：{pp}"));
        }
    }
    cfg.workspace_path = p.clone();
    cfg.save().map_err(|e| format!("保存失败：{e}"))?;
    println!("[mouseclaw] 📁 workspace → {p:?}");
    Ok(())
}

#[tauri::command]
pub fn get_workspace_path() -> Option<String> {
    config::Config::load().workspace_path
}

/// v0.1.26 · 让 Onboarding step 5 / 「关于」面板能切开机自启动
#[tauri::command]
pub fn set_autostart(enable: bool, app: AppHandle) -> Result<bool, String> {
    use tauri_plugin_autostart::ManagerExt;
    let autolaunch = app.autolaunch();
    if enable {
        autolaunch.enable().map_err(|e| format!("enable autostart: {e}"))?;
    } else {
        autolaunch.disable().map_err(|e| format!("disable autostart: {e}"))?;
    }
    let now = autolaunch.is_enabled().unwrap_or(enable);
    let mut cfg = config::Config::load();
    cfg.autostart = now;
    cfg.save().map_err(|e| format!("save config: {e}"))?;
    Ok(now)
}

/// 启动时前端读当前自启动状态，反映勾选框
#[tauri::command]
pub fn get_autostart(app: AppHandle) -> bool {
    use tauri_plugin_autostart::ManagerExt;
    app.autolaunch().is_enabled().unwrap_or_else(|_| config::Config::load().autostart)
}

// ───────────────────── v0.4.0 P1 · 术语库 (vocab) commands ─────────────────────

/// 在系统默认编辑器里打开 `~/.mouseclaw/vocab/user.txt` —— 托盘
/// 「📝 编辑术语表...」走这条。文件不存在会先创建一份带说明的模板。
#[tauri::command]
pub fn vocab_open_user_file() -> Result<(), String> {
    crate::vocab::ensure_user_file().map_err(|e| format!("ensure user file: {e}"))?;
    let path = crate::vocab::user_file_path().map_err(|e| format!("path: {e}"))?;
    std::process::Command::new("open")
        .arg(&path)
        .spawn()
        .map_err(|e| format!("open {}: {e}", path.display()))?;
    Ok(())
}

/// 重读用户词表 → 重新生成 active.txt → 失效 sherpa recognizer 让下次
/// transcribe 重新 init 注入 hotwords。
/// 托盘「🔄 刷新术语表」 + 设置 builtin enabled 时都走这条。
#[tauri::command]
pub fn vocab_reload() -> Result<usize, String> {
    let cfg = config::Config::load();
    let n = crate::vocab::regenerate_active(cfg.vocab_builtin_enabled)
        .map_err(|e| format!("regen: {e}"))?;
    crate::transcribe_stream::invalidate_recognizer();
    println!("[mouseclaw] 📝 vocab reloaded: {n} entries");
    Ok(n)
}

#[tauri::command]
pub fn vocab_get_builtin_enabled() -> bool {
    config::Config::load().vocab_builtin_enabled
}

#[tauri::command]
pub fn vocab_set_builtin_enabled(enabled: bool) -> Result<usize, String> {
    let mut cfg = config::Config::load();
    cfg.vocab_builtin_enabled = enabled;
    cfg.save().map_err(|e| format!("save config: {e}"))?;
    let n = crate::vocab::regenerate_active(enabled).map_err(|e| format!("regen: {e}"))?;
    crate::transcribe_stream::invalidate_recognizer();
    println!("[mouseclaw] 📝 builtin vocab → {enabled}, {n} entries active");
    Ok(n)
}
