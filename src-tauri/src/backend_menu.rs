//! 「🤖 AI 后端 ▸」托盘子菜单 —— 运行时切换当前 AI 后端。
//!
//! 用户决策（2026-05-22）：**只列已安装的后端**，没装的不展示；一个都没装就给一条
//! 「去安装（推荐 Claude）」入口。理由：装了 13 个后端但用户机器上通常只有 1–3 个真能
//! 用，把没装的也铺出来只会让人困惑「选了它为什么不工作」。已安装 = `find_binary` 在
//! 拓宽 PATH 里找得到二进制（不查登录态 —— 那是更重的探针，归 onboarding 平滑化那条线）。
//!
//! 当前生效的后端打勾。切换由 [`crate::tray_handlers`] 派发到
//! [`crate::tray_actions::change_backend`]（同时改 config + 内存里的 AppState.backend）。

use tauri::menu::{CheckMenuItem, IsMenuItem, MenuItem, Submenu};
use tauri::{AppHandle, Wry};

use crate::backend::Backend;

/// 已安装的后端列表（在拓宽 PATH 里找得到二进制的）。
fn installed_backends() -> Vec<Backend> {
    Backend::all()
        .into_iter()
        .filter(|b| crate::claude_cli::find_binary(b.binary_name()).is_ok())
        .collect()
}

/// 构建「🤖 AI 后端」子菜单。只放已装的；都没装则放一条「去安装」入口。
pub fn build_backend_submenu(app: &AppHandle, en: bool) -> tauri::Result<Submenu<Wry>> {
    let current = crate::config::Config::load().backend;
    let installed = installed_backends();

    let root_label = if installed.is_empty() {
        if en { "🤖 AI backend: none installed".to_string() }
        else { "🤖 AI 后端：未安装".to_string() }
    } else if en {
        format!("🤖 AI backend: {}", current.display_name())
    } else {
        format!("🤖 AI 后端：{}", current.display_name())
    };

    // CheckMenuItem 实例要活到 submenu 构建完，先收进 vec 再借引用。
    let mut checks: Vec<CheckMenuItem<Wry>> = Vec::new();
    for b in &installed {
        checks.push(CheckMenuItem::with_id(
            app,
            format!("backend:{}", b.binary_name()),
            b.display_name(),
            true,
            *b == current,
            None::<&str>,
        )?);
    }

    // 一个都没装 → 推荐去安装（点了弹气泡给推荐命令，见 tray_actions）
    let install_item = if installed.is_empty() {
        let lbl = if en {
            "    ↳ Install one (recommend Claude Code CLI)…"
        } else {
            "    ↳ 去安装（推荐 Claude Code CLI）…"
        };
        Some(MenuItem::with_id(app, "backend-install", lbl, true, None::<&str>)?)
    } else {
        None
    };

    let mut refs: Vec<&dyn IsMenuItem<Wry>> =
        checks.iter().map(|c| c as &dyn IsMenuItem<Wry>).collect();
    if let Some(ref it) = install_item {
        refs.push(it as &dyn IsMenuItem<Wry>);
    }

    Submenu::with_id_and_items(app, "backend-submenu", root_label, true, &refs)
}
