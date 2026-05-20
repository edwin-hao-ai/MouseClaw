//! v0.4.x · 一键装 Browser Use / Office Use CLI（autonomous-install）。
//!
//! 设计目标见 docs/prototypes/auto-install-cli-20260520.html 八态拍板。
//!
//! 安装路径（两个 CLI）：
//!   - `agent-browser`：`npm i -g agent-browser` 然后 `agent-browser install`（首次下/检测浏览器）
//!                      —— 依赖本机有 Node + npm。⚠️ 包名是 `agent-browser`，**不是**
//!                      `@vercel/agent-browser`（后者 npm 上不存在 → E404 → exit 1）。
//!   - `officecli`：    `curl -fsSL https://raw.githubusercontent.com/iOfficeAI/OfficeCLI/main/install.sh | bash`
//!                      —— 单二进制，**不**依赖 Node
//!
//! 设计约束：
//!   1. 全程**流式**事件给前端（进度行 / log 一行一 emit），不堵 UI
//!   2. **不**静默装 —— 必须由前端按钮主动触发
//!   3. 没 `npm` 不假装能一键装 Node，返回 NoNpm 状态让前端引导到 nodejs.org
//!   4. 失败给具体下一步（重试 / 复制命令手动跑 / 打开 releases）

use std::process::Stdio;
use serde::Serialize;
use tauri::{AppHandle, Emitter};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

/// 安装目标 —— 前端 invoke 时传字符串，对应一个 [`InstallTarget`]。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstallTarget {
    AgentBrowser,
    OfficeCli,
}

impl InstallTarget {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "agent-browser" => Some(Self::AgentBrowser),
            "officecli"     => Some(Self::OfficeCli),
            _ => None,
        }
    }
    pub fn id(self) -> &'static str {
        match self {
            Self::AgentBrowser => "agent-browser",
            Self::OfficeCli    => "officecli",
        }
    }
}

/// 流式给前端的进度事件 —— 一个安装周期内会 emit 多条。
/// 前端按 `target` + `phase` 渲染对应行。
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "phase", rename_all = "kebab-case")]
pub enum InstallEvent {
    /// 开始装（前端切「安装中 · 黄点」）
    Started { target: String },
    /// 流式日志（log 行）
    Log { target: String, line: String },
    /// 完成 + 成功（前端切「绿点」+ refresh capability_status）
    Done { target: String },
    /// 失败 —— 给出明确原因码 + 一句人话
    Failed {
        target: String,
        /// 错误码：`no-npm` / `network` / `permission` / `unknown`
        code: String,
        message: String,
    },
}

pub const EV_INSTALL_PROGRESS: &str = "install-progress";

fn emit(app: &AppHandle, ev: InstallEvent) {
    // 失败也算事件之一，不要 println 吞掉
    if let Err(e) = app.emit(EV_INSTALL_PROGRESS, ev.clone()) {
        eprintln!("[cli_install] emit failed: {e}");
    }
    // 同时打日志便于排错（不带个人信息）
    println!("[cli_install] {:?}", ev);
}

/// 启动一个 install 任务（spawn 不阻塞 caller）。
/// 完成 / 失败时通过 `EV_INSTALL_PROGRESS` 事件通知前端。
pub fn start_install(app: AppHandle, target: InstallTarget) {
    tauri::async_runtime::spawn(async move {
        run(app, target).await;
    });
}

/// 升级提示 marker —— 弹过一次就写它，之后不再打扰。
fn hint_marker_path() -> std::path::PathBuf {
    let home = std::env::var("HOME").unwrap_or_default();
    std::path::PathBuf::from(home).join(".mouseclaw").join("upgrade_cli_hint_shown")
}

/// v0.4.x · 老用户升级发现性。
///
/// 老用户 `onboarded=true`，升级后直接进 idle、永远看不到 Onboarding step 7/8。
/// 启动延迟几秒后检查：已 onboarded && 有 CLI 没装 && 没弹过 → 弹一次 nudge 气泡，
/// 文案只提**缺**的那个（已装 agent-browser 的人只会看到 OfficeCLI 提示）。
///
/// 一次性：弹完写 marker file，之后启动不再弹（用户也可在状态页随时装）。
pub fn maybe_hint_upgrade(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        // 让启动其它流程先跑（模型下载提示等），别开机就糊脸
        tokio::time::sleep(std::time::Duration::from_secs(8)).await;

        let cfg = crate::config::Config::load();
        if !cfg.onboarded {
            return; // 新用户走 Onboarding step 7/8，不需要这个
        }
        if hint_marker_path().exists() {
            return; // 弹过了
        }

        let has_ab  = crate::claude_cli::find_binary("agent-browser").is_ok();
        let has_off = crate::claude_cli::find_binary("officecli").is_ok();
        if has_ab && has_off {
            return; // 两个都装了，没什么可提示
        }

        let lang_en = cfg.language != "zh";
        // 缺哪个就只说哪个 —— 别让已经装好 agent-browser 的人看到莫名其妙的提示
        let what = match (has_ab, has_off) {
            (true, false)  => if lang_en { "edit Office files" } else { "操作 Office 文件" },
            (false, true)  => if lang_en { "drive a browser" } else { "操作浏览器" },
            _              => if lang_en { "drive a browser & edit Office files" } else { "操作浏览器 / Office 文件" },
        };
        let message = if lang_en {
            format!("🎉 New trick unlocked — install one tool and I can {what}")
        } else {
            format!("🎉 我有新本事了 · 装上工具我就能{what}")
        };
        let cta_label = if lang_en { "Set it up".into() } else { "去装上".into() };

        let payload = crate::events::NudgePayload {
            kind: crate::events::NudgeKind::LearnedCli,
            message,
            cta_label: Some(cta_label),
            cta_action: Some("open-status".into()),
        };
        if let Err(e) = app.emit(crate::events::EV_NUDGE, payload) {
            eprintln!("[cli_install] upgrade-hint emit failed: {e}");
            return;
        }
        // 写 marker —— 不论用户点不点，弹过一次就不再烦
        let p = hint_marker_path();
        if let Some(dir) = p.parent() { let _ = std::fs::create_dir_all(dir); }
        let _ = std::fs::write(&p, b"1");
        println!("[cli_install] upgrade hint shown (has_ab={has_ab} has_off={has_off})");
    });
}

async fn run(app: AppHandle, target: InstallTarget) {
    let id = target.id().to_string();
    emit(&app, InstallEvent::Started { target: id.clone() });

    let result = match target {
        InstallTarget::AgentBrowser => run_agent_browser(&app).await,
        InstallTarget::OfficeCli    => run_officecli(&app).await,
    };

    match result {
        Ok(()) => emit(&app, InstallEvent::Done { target: id }),
        Err(err) => emit(&app, InstallEvent::Failed {
            target: id,
            code: err.code.into(),
            message: err.message,
        }),
    }
}

/// 装 agent-browser —— 两步：
///   1. `npm i -g agent-browser`（装 CLI 本体）
///   2. `agent-browser install`（首次下 Chrome for Testing / 检测已有 Chrome·Brave·Playwright）
///
/// ⚠️ 包名是 `agent-browser`，**不是** `@vercel/agent-browser`（后者 npm 上不存在，
///    会 E404 → exit 1，这正是 v0.4.0 用户实测装不上的根因）。
///
/// 没有 npm 直接报 NoNpm，让前端引导到 nodejs.org（不假装能装 Node）。
/// 第 2 步 best-effort：CLI 本体已装好就算成功，浏览器没就绪也不整体判失败
/// （agent-browser 运行时还能自动探测系统 Chrome），只把日志透给用户。
async fn run_agent_browser(app: &AppHandle) -> Result<(), InstallError> {
    let npm = crate::claude_cli::find_binary("npm").map_err(|_| InstallError {
        code: "no-npm",
        message: "本机没找到 npm。先装 Node.js（nodejs.org），装完老鼠自动检测。".into(),
    })?;

    // ── 第 1 步：装 CLI 本体 ──
    let mut child = Command::new(&npm)
        .args(["install", "-g", "agent-browser"])
        .env("PATH", crate::claude_cli::expanded_path())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| InstallError {
            code: "unknown",
            message: format!("启动 npm 失败：{e}"),
        })?;

    stream_lines(app, "agent-browser", &mut child).await?;

    let status = child.wait().await.map_err(|e| InstallError {
        code: "unknown",
        message: format!("等待 npm 退出失败：{e}"),
    })?;

    if !status.success() {
        return Err(classify_failure("agent-browser", status.code()));
    }

    // ── 第 2 步：备好浏览器引擎（best-effort）──
    provision_agent_browser_browser(app).await;
    Ok(())
}

/// `agent-browser install` —— 首次下 Chrome for Testing（已有 Chrome/Brave/Playwright 会自动探测）。
/// best-effort：失败不让整体安装判失败，只 emit 一条日志说明（CLI 本体已就绪，
/// 运行时仍可能探测到系统浏览器）。找不到刚装的二进制也只记日志。
async fn provision_agent_browser_browser(app: &AppHandle) {
    let Ok(bin) = crate::claude_cli::find_binary("agent-browser") else {
        emit(app, InstallEvent::Log {
            target: "agent-browser".into(),
            line: "（已装 CLI；未能定位二进制跑 `agent-browser install`，运行时将自动探测系统浏览器）".into(),
        });
        return;
    };

    emit(app, InstallEvent::Log {
        target: "agent-browser".into(),
        line: "→ agent-browser install（备好浏览器引擎，首次可能下载 Chrome for Testing）".into(),
    });

    let spawned = Command::new(&bin)
        .arg("install")
        .env("PATH", crate::claude_cli::expanded_path())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn();

    let mut child = match spawned {
        Ok(c) => c,
        Err(e) => {
            emit(app, InstallEvent::Log {
                target: "agent-browser".into(),
                line: format!("（`agent-browser install` 启动失败：{e}；运行时将自动探测系统浏览器）"),
            });
            return;
        }
    };

    let _ = stream_lines(app, "agent-browser", &mut child).await;
    match child.wait().await {
        Ok(s) if s.success() => emit(app, InstallEvent::Log {
            target: "agent-browser".into(),
            line: "✓ 浏览器引擎已就绪".into(),
        }),
        _ => emit(app, InstallEvent::Log {
            target: "agent-browser".into(),
            line: "（浏览器引擎未就绪；运行时将自动探测系统 Chrome/Brave，或之后在终端跑 `agent-browser install`）".into(),
        }),
    }
}

/// 装 OfficeCLI —— 跑官方 install.sh。
/// 注意：脚本本身会 curl 下二进制，所以网络断 / 权限不足两种都可能在脚本里出错。
async fn run_officecli(app: &AppHandle) -> Result<(), InstallError> {
    // 显式列出来要执行的命令 —— 用户在 prototype 里能看到全文，这里要保持一致。
    let script = "curl -fsSL https://raw.githubusercontent.com/iOfficeAI/OfficeCLI/main/install.sh | bash";

    let mut child = Command::new("/bin/bash")
        .arg("-c")
        .arg(script)
        .env("PATH", crate::claude_cli::expanded_path())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| InstallError {
            code: "unknown",
            message: format!("启动 bash 失败：{e}"),
        })?;

    stream_lines(app, "officecli", &mut child).await?;

    let status = child.wait().await.map_err(|e| InstallError {
        code: "unknown",
        message: format!("等待 bash 退出失败：{e}"),
    })?;

    if status.success() {
        Ok(())
    } else {
        Err(classify_failure("officecli", status.code()))
    }
}

async fn stream_lines(
    app: &AppHandle,
    target: &str,
    child: &mut tokio::process::Child,
) -> Result<(), InstallError> {
    // 把 stdout + stderr 都按行 emit 出去 —— 用户在「查看日志」展开里能看完整过程
    if let Some(out) = child.stdout.take() {
        let app = app.clone();
        let target = target.to_string();
        tokio::spawn(async move {
            let mut reader = BufReader::new(out).lines();
            while let Ok(Some(line)) = reader.next_line().await {
                emit(&app, InstallEvent::Log { target: target.clone(), line });
            }
        });
    }
    if let Some(err) = child.stderr.take() {
        let app = app.clone();
        let target = target.to_string();
        tokio::spawn(async move {
            let mut reader = BufReader::new(err).lines();
            while let Ok(Some(line)) = reader.next_line().await {
                emit(&app, InstallEvent::Log { target: target.clone(), line });
            }
        });
    }
    Ok(())
}

fn classify_failure(target: &str, exit_code: Option<i32>) -> InstallError {
    // 没法精准区分 network / permission（subshell 里 curl/npm 的 errno 拿不到），
    // 给一个对用户最有帮助的兜底建议。
    let code = match exit_code {
        Some(7)  => "network",   // curl: failed to connect
        Some(13) => "permission",// permission denied
        _        => "unknown",
    };
    let message = match (target, code) {
        (_, "network") =>
            "下载失败 —— 检查网络 / VPN 后重试，或去 releases 页手动下载二进制。".into(),
        (_, "permission") =>
            "权限不足 —— 试试在终端跑 `sudo` 版命令，或装到用户目录 (~/.local/bin)。".into(),
        ("agent-browser", _) =>
            format!("npm 安装失败（exit {:?}）。可以打开终端手动跑：npm i -g agent-browser && agent-browser install",
                    exit_code),
        ("officecli", _) =>
            format!("OfficeCLI 安装失败（exit {:?}）。可以去 GitHub releases 直接下载二进制。",
                    exit_code),
        _ => format!("安装失败（exit {:?}）。", exit_code),
    };
    InstallError { code, message }
}

#[derive(Debug)]
struct InstallError {
    code: &'static str,
    message: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_valid_targets() {
        assert_eq!(InstallTarget::parse("agent-browser"), Some(InstallTarget::AgentBrowser));
        assert_eq!(InstallTarget::parse("officecli"),     Some(InstallTarget::OfficeCli));
    }

    #[test]
    fn rejects_unknown_targets() {
        assert!(InstallTarget::parse("codex").is_none());
        assert!(InstallTarget::parse("").is_none());
        assert!(InstallTarget::parse("AGENT-BROWSER").is_none()); // case-sensitive 防止笔误
    }

    #[test]
    fn id_roundtrip() {
        for t in [InstallTarget::AgentBrowser, InstallTarget::OfficeCli] {
            assert_eq!(InstallTarget::parse(t.id()), Some(t));
        }
    }

    #[test]
    fn classify_failure_picks_helpful_message() {
        let e = classify_failure("officecli", Some(7));
        assert_eq!(e.code, "network");
        assert!(e.message.contains("网络") || e.message.contains("VPN"));

        let e = classify_failure("agent-browser", Some(13));
        assert_eq!(e.code, "permission");

        let e = classify_failure("agent-browser", Some(1));
        assert_eq!(e.code, "unknown");
        assert!(e.message.contains("npm"));
    }

    #[test]
    fn hint_marker_under_mouseclaw_dir() {
        // marker 必须落在 ~/.mouseclaw 下，跟其它状态文件同源
        let p = hint_marker_path();
        assert!(p.to_string_lossy().contains(".mouseclaw"));
        assert!(p.ends_with("upgrade_cli_hint_shown"));
    }
}
