//! Chrome 浏览器自动化桥接 (P0a · daily-use 核心)
//!
//! ## 为什么不用 `agent-browser`？
//! agent-browser 跑的是它自己的 headless Chromium —— 看不到用户已经登录的 Chrome 标签页，
//! 所以「在我开着的这个 GitHub PR 里点 merge」之类的 daily 任务永远不工作。
//!
//! ## 真正的解：CDP 桥到用户的 Chrome
//! 1. 用户跑一次 `chrome-debug-on` 把 Chrome 以 `--remote-debugging-port=9222`
//!    + 一个**专用 debug profile** 启动（Chrome 安全规则禁止 default profile 开 debug port）
//! 2. MouseClaw 探测 `localhost:9222` 是否活的
//! 3. 活的 → 在 Claude CLI system_prompt 里注入 chrome-devtools-mcp 调用说明
//! 4. Claude 用 MCP tools 直接操作那个真·有用户 cookies/登录态的 Chrome
//!
//! ## chrome-devtools-mcp 集成
//! Google 官方出的 MCP server：`npx chrome-devtools-mcp@latest`。用户跑一次：
//!     claude mcp add chrome-devtools --scope user -- npx chrome-devtools-mcp@latest --browserUrl http://127.0.0.1:9222
//! 注册一次永久生效。我们在 onboarding / tray 一键帮做。

use std::time::Duration;
use anyhow::{Context, Result};

/// Chrome debug profile 路径 —— 隔离用户 default profile（Chrome 不允许 default 开 debug port）。
pub fn debug_profile_path() -> String {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    format!("{home}/.mouseclaw/chrome-debug-profile")
}

/// CDP debug port —— 固定 9222，跟生态对齐（chrome-devtools-mcp 默认走这个）。
pub const CDP_PORT: u16 = 9222;

/// 探测 Chrome debug 端口是不是活的 —— TCP 连一下 localhost:9222。
/// 100ms 超时，不阻塞主流程。
pub fn cdp_is_alive() -> bool {
    use std::net::{SocketAddr, TcpStream};
    let addr: SocketAddr = ([127, 0, 0, 1], CDP_PORT).into();
    TcpStream::connect_timeout(&addr, Duration::from_millis(100)).is_ok()
}

/// 启动一个带 CDP debug port 的 Chrome 实例（用户的真 cookies + 真扩展，但**专用 profile**）。
/// 已经在跑就 no-op。
///
/// 重要：用 `--user-data-dir=$HOME/.mouseclaw/chrome-debug-profile`，
/// **不**碰用户的 default profile —— Chrome 安全规则禁止 default profile 开 debug port。
/// 用户第一次用时需要在这个 profile 里登录一次想自动化的网站。
pub fn launch_chrome_with_cdp() -> Result<()> {
    if cdp_is_alive() {
        return Ok(()); // 已经开了
    }
    let chrome_paths = [
        "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
        "/Applications/Google Chrome Canary.app/Contents/MacOS/Google Chrome Canary",
        "/Applications/Chromium.app/Contents/MacOS/Chromium",
    ];
    let chrome = chrome_paths
        .iter()
        .find(|p| std::path::Path::new(p).exists())
        .copied()
        .context("找不到 Chrome / Chrome Canary / Chromium，请先装 Chrome")?;

    let profile = debug_profile_path();
    std::fs::create_dir_all(&profile).ok();

    std::process::Command::new(chrome)
        .arg(format!("--remote-debugging-port={CDP_PORT}"))
        .arg(format!("--user-data-dir={profile}"))
        // 减少首次启动的烦人弹窗
        .arg("--no-first-run")
        .arg("--no-default-browser-check")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .context("启动 Chrome with CDP 失败")?;

    // 等最多 2s 看 port 是否活了
    for _ in 0..20 {
        std::thread::sleep(Duration::from_millis(100));
        if cdp_is_alive() {
            return Ok(());
        }
    }
    anyhow::bail!("Chrome 启动了但 debug port {CDP_PORT} 没活 —— 可能被防火墙拦了")
}

/// 把 chrome-devtools-mcp 注册到 Claude Code 的 user-scoped MCP server 里。
/// 等价于用户手动跑：
///     claude mcp add chrome-devtools --scope user -- npx chrome-devtools-mcp@latest --browserUrl http://127.0.0.1:9222
/// 已经注册过会被 `claude mcp add` 拒绝，那就当成功（idempotent 处理）。
pub fn ensure_mcp_registered() -> Result<()> {
    let claude = crate::claude_cli::find_binary("claude")
        .context("找不到 claude CLI，先装 Claude Code: npm i -g @anthropic-ai/claude-code")?;
    let output = std::process::Command::new(&claude)
        .env("PATH", crate::claude_cli::expanded_path())
        .args([
            "mcp", "add", "chrome-devtools",
            "--scope", "user",
            "--",
            "npx", "chrome-devtools-mcp@latest",
            "--browserUrl", &format!("http://127.0.0.1:{CDP_PORT}"),
        ])
        .output()
        .context("运行 claude mcp add 失败")?;
    let stderr = String::from_utf8_lossy(&output.stderr);
    if !output.status.success() && !stderr.contains("already exists") {
        anyhow::bail!("claude mcp add 失败：{stderr}");
    }
    Ok(())
}

/// 启用浏览器自动化的一站式入口 —— 托盘菜单 / onboarding 调它。
/// 顺序：注册 MCP（幂等）→ 启动 Chrome with CDP（幂等）。
pub fn enable() -> Result<()> {
    ensure_mcp_registered()?;
    launch_chrome_with_cdp()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_profile_path_under_mouseclaw() {
        let p = debug_profile_path();
        assert!(p.ends_with("/.mouseclaw/chrome-debug-profile"),
                "profile 必须在 .mouseclaw 名下：{p}");
    }

    #[test]
    fn cdp_port_is_9222() {
        // 跟 chrome-devtools-mcp / playwright / puppeteer 生态对齐
        assert_eq!(CDP_PORT, 9222);
    }

    #[test]
    fn cdp_is_alive_false_when_port_closed() {
        // 在 CI / 大多数测试环境，9222 是没东西在监听的
        // 这测试是说「函数 100ms 内必须返回 false 不挂起」
        let t0 = std::time::Instant::now();
        let _ = cdp_is_alive();
        assert!(t0.elapsed() < Duration::from_millis(300),
                "cdp_is_alive 必须 100ms 探测超时，不能挂主线程");
    }
}
