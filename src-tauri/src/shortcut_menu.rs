//! 快捷键设置菜单 · v0.4.2
//!
//! 把「召唤 AI 快捷键」+「语音输入触发键」两组快捷键设置收拢到一处 —— 用户反馈
//! 「托盘里没有一个统一的去切换快捷键的地方」。两个子菜单相邻摆在托盘里，构成统一
//! 的快捷键设置区。从 tray.rs 抽出（tray.rs 已超 800 行硬上限）。
//!
//! - **召唤快捷键**：tauri global-shortcut 注册的组合键（默认 ⌘⇧Space）。切换要
//!   unregister 旧 + register 新（热切换，不重启），失败回滚到旧的避免用户没键可用。
//! - **语音触发键**：CGEventTap 自管的单 modifier 键（fn/⌥/⌃/右⇧/右⌘/右⌥）。切换
//!   只改 atomic 缓存（voice_ime::set_trigger），本来就是热的。
//!
//! 两者切换后 tray dispatch 都会 rebuild_tray_menu → 勾选实时同步。
//!
//! 跨平台（v0.5）：菜单构建 + 召唤快捷键热切换走 tauri 跨平台 API，全平台可用。
//! 语音触发键的标签（fn/⌥ 等）目前是 macOS 语义；Win/Linux 的听写触发方式待 §4
//! UX 决策（见 docs/design/cross-platform-port-20260522.md）。

use std::str::FromStr;
use tauri::menu::{CheckMenuItem, IsMenuItem, Submenu};
use tauri::{AppHandle, Emitter, Manager, Wry};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut};

/// 召唤 AI 快捷键预设 —— (global-shortcut 注册串, 显示符号)。
/// Super = ⌘。避开 ⌘⇧V（Hub 剪贴板已占）。中英显示都用符号，无需双语。
pub const SUMMON_PRESETS: &[(&str, &str)] = &[
    ("Super+Shift+Space", "⌘⇧Space"),
    ("Super+Shift+KeyM", "⌘⇧M"),
    ("Super+Shift+KeyD", "⌘⇧D"),
    ("Control+Shift+Space", "⌃⇧Space"),
];

/// 构建「召唤快捷键」子菜单（CheckMenuItem，当前生效项打勾）。
pub fn build_summon_submenu(app: &AppHandle, en: bool) -> tauri::Result<Submenu<Wry>> {
    let current = crate::config::Config::load().shortcut;
    let mut items: Vec<CheckMenuItem<Wry>> = Vec::new();
    for (sc, disp) in SUMMON_PRESETS {
        let item = CheckMenuItem::with_id(
            app,
            format!("summon-shortcut:{sc}"),
            *disp,
            true,
            *sc == current,
            None::<&str>,
        )?;
        items.push(item);
    }
    let refs: Vec<&dyn IsMenuItem<Wry>> =
        items.iter().map(|i| i as &dyn IsMenuItem<Wry>).collect();
    let label = if en { "    ↳ Summon shortcut" } else { "    ↳ 召唤快捷键" };
    Submenu::with_id_and_items(app, "summon-shortcut-submenu", label, true, &refs)
}

/// 构建「语音输入触发键」子菜单（从 tray.rs 搬出）。
pub fn build_trigger_submenu(app: &AppHandle, en: bool) -> tauri::Result<Submenu<Wry>> {
    let current = crate::voice_ime::ImeTrigger::from_str(
        &crate::config::Config::load().voice_ime_trigger,
    );
    let mut items: Vec<CheckMenuItem<Wry>> = Vec::new();
    for t in crate::voice_ime::ImeTrigger::all() {
        let label = if en { t.display_en() } else { t.display_zh() };
        let item = CheckMenuItem::with_id(
            app,
            format!("vime-trigger:{}", t.as_str()),
            label,
            true,
            *t == current,
            None::<&str>,
        )?;
        items.push(item);
    }
    let refs: Vec<&dyn IsMenuItem<Wry>> =
        items.iter().map(|i| i as &dyn IsMenuItem<Wry>).collect();
    let label = if en { "    ↳ IME trigger key" } else { "    ↳ 触发键" };
    Submenu::with_id_and_items(app, "vime-trigger-submenu", label, true, &refs)
}

/// 热切换召唤 AI 快捷键 —— unregister 旧 + register 新，失败回滚到旧的。
pub fn change_summon_shortcut(app: &AppHandle, new_str: &str) {
    let mut cfg = crate::config::Config::load();
    if cfg.shortcut == new_str {
        return;
    }
    let new_sc = match Shortcut::from_str(new_str) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("[mouseclaw] change_summon_shortcut parse {new_str:?}: {e}");
            return;
        }
    };
    // 注销旧的（没注册成功过也无妨，忽略错误）
    if let Ok(old_sc) = Shortcut::from_str(&cfg.shortcut) {
        let _ = app.global_shortcut().unregister(old_sc);
    }
    // 注册新的；失败 → 把旧的注册回来，别让用户落到「没有召唤键」的状态
    if let Err(e) = app.global_shortcut().register(new_sc) {
        eprintln!("[mouseclaw] change_summon_shortcut register {new_str:?}: {e}");
        if let Ok(old_sc) = Shortcut::from_str(&cfg.shortcut) {
            let _ = app.global_shortcut().register(old_sc);
        }
        let msg = if cfg.language == "en" {
            format!("⚠️ Shortcut {new_str} couldn't be registered (taken by another app?). Kept the old one.")
        } else {
            format!("⚠️ 快捷键 {new_str} 注册失败（可能被其它 app 占用），已保留原快捷键。")
        };
        emit_reply(app, "summon shortcut change", &msg);
        return;
    }
    cfg.shortcut = new_str.to_string();
    if let Err(e) = cfg.save() {
        eprintln!("[mouseclaw] change_summon_shortcut save: {e}");
        return;
    }
    let disp = SUMMON_PRESETS
        .iter()
        .find(|(s, _)| *s == new_str)
        .map(|(_, d)| *d)
        .unwrap_or(new_str);
    let msg = if cfg.language == "en" {
        format!("🦞 Summon shortcut → {disp}. Re-bound and active now.")
    } else {
        format!("🦞 召唤快捷键 → {disp}。已重新绑定，立即生效。")
    };
    emit_reply(app, "summon shortcut change", &msg);
}

/// 热切换语音输入触发键（从 tray.rs 搬出）—— 只改 CGEventTap 的 atomic 缓存。
pub fn change_ime_trigger(app: &AppHandle, trigger_str: &str) {
    let trigger = crate::voice_ime::ImeTrigger::from_str(trigger_str);
    let mut cfg = crate::config::Config::load();
    if cfg.voice_ime_trigger == trigger_str {
        return;
    }
    cfg.voice_ime_trigger = trigger.as_str().to_string();
    if let Err(e) = cfg.save() {
        eprintln!("[mouseclaw] change_ime_trigger save: {e}");
        return;
    }
    crate::voice_ime::set_trigger(trigger);
    let msg = if cfg.language == "en" {
        format!("🎙️ IME trigger → {}. Voice IME re-bound.", trigger.display_en())
    } else {
        format!("🎙️ 语音输入触发键 → {}。已重新绑定。", trigger.display_zh())
    };
    emit_reply(app, "IME trigger change", &msg);
}

/// 给所有 webview 发一条 reply 气泡（托盘操作的反馈）。
fn emit_reply(app: &AppHandle, transcript: &str, msg: &str) {
    for (_, w) in app.webview_windows() {
        let _ = w.emit(crate::events::EV_VIEW_CHANGED, serde_json::json!({
            "kind": "reply",
            "transcript": transcript,
            "reply": msg,
            "mode": "A",
            "streaming": false,
        }));
    }
}
