//! Claude Code CLI 集成层。
//!
//! Risk #1 已于 2026-05-13 验证：`claude -p` + `--allowedTools "Read"` 可以读本地 PNG。
//! 完整调用约定见 `/Users/edwinhao/MouseClaw/CLAUDE.md` 「Claude CLI 调用约定」一节。
//!
//! v0.1.5：用 `--output-format stream-json --include-partial-messages --verbose`
//! 做流式 —— 边收边显示，用户不用干等 10-30s。`ask_claude_streaming` 是主路径，
//! `ask_claude` 是它的非流式 wrapper（smoke test 用）。

use std::path::Path;
use std::process::Stdio;
use anyhow::{bail, Context, Result};
use tokio::io::{AsyncBufReadExt, BufReader};

/// 发给 AI 后端的 system prompt 追加内容（不替换默认）。pub —— backend.rs 的
/// codex / openclaw 后端也复用同一套指令。
/// 教 AI:
///   1. 用截图 + 用户语音回答问题
///   2. **优先关注光标周围的内容**（用户大概率指的是"这里"、"这段"、"这个网页"）
///   3. 用 `[INSERT_AT_CURSOR]...[/INSERT_AT_CURSOR]` 标记区分 Mode A / Mode B 输出
pub const APPEND_SYSTEM_PROMPT: &str = r#"你是 MouseClaw 桌面助手。用户通过语音 + 一张当前屏幕截图向你提问。

## 怎么看截图
截图是用户按下快捷键瞬间「光标所在那块显示器」的完整画面（多显示器场景下不是主屏）。
用户的问题通常是指着光标位置上的东西问的——"这段代码"、"这个网页"、"这里的报错"——
**优先关注光标坐标附近的内容**，光标位置会通过 "光标位置 (x, y)" 显式告知你。

如果用户问的是显然不依赖光标位置的话题（"我屏幕上有什么"、"总结一下"），就看整张图。

## 写回光标（特殊路径）
如果用户**明确要求**把内容写入当前光标位置（"续写"、"补全这里"、"写一段在这里"），
用以下标记包裹要写入的纯文本（除标记内文本外不要其他内容）：
[INSERT_AT_CURSOR]
要写入的内容
[/INSERT_AT_CURSOR]
否则正常回答即可。"#;

/// 当检测到 `agent-browser` CLI 已安装时，追加给后端的「compute use」能力说明。
/// 不自己实现浏览器自动化（那是 Mode C / V2 独立安全模型）——而是告诉后端：
/// 你的 Bash 工具里有 `agent-browser` 这个轻量 CLI，需要操作浏览器/填表时可以调它。
/// 这样 compute use 能力随后端 agentic 能力自然获得，零新增安全面。
pub const BROWSER_CAPABILITY_PROMPT: &str = r#"

## 浏览器操作能力（compute use）
本机已安装 `agent-browser` CLI（Vercel Labs 出品，Rust 原生、headless）。
当用户要求「在浏览器里填表 / 点按钮 / 抓取网页 / 自动操作网站」时，你可以用 Bash 调它：
  - `agent-browser open <url>`        打开页面
  - `agent-browser snapshot`          拿可访问性树（元素带 @e1/@e2 引用）
  - `agent-browser click @e2`         点击元素
  - `agent-browser fill @e3 "文本"`   填表
  - `agent-browser screenshot`        截图确认
先 snapshot 看清楚再操作。涉及提交订单、付款、发送消息等不可逆动作时，**先停下来在回答里
说明你打算做什么，让用户确认**，不要直接执行。"#;

/// 按本机已安装的能力拼出最终 system prompt。
/// 目前唯一的可选能力：`agent-browser`（compute use）。
pub fn system_prompt() -> String {
    let mut p = APPEND_SYSTEM_PROMPT.to_string();
    if find_binary("agent-browser").is_ok() {
        p.push_str(BROWSER_CAPABILITY_PROMPT);
    }
    p
}

/// 光标在截图坐标系里的位置 + 屏幕尺寸（logical points, top-left origin）。
pub struct CursorContext {
    pub x: i32,
    pub y: i32,
    pub screen_w: i32,
    pub screen_h: i32,
}

/// `.app` bundle 启动时 PATH 默认是 launchd 给的最小集（`/usr/bin:/bin:/usr/sbin:/sbin`）
/// 不会继承用户 shell 的 `.zshrc` 等。这导致 `claude` CLI 找不到。
///
/// `.env("PATH", ...)` 只影响**子进程**看到的 PATH，不影响 OS 找二进制 ——
/// OS 在 spawn 之前已经用**当前进程**的 PATH 找 `claude` 了。所以必须先
/// 自己用拓宽过的 PATH 找到二进制的绝对路径，再用绝对路径 spawn。
pub fn expanded_path() -> String {
    let current = std::env::var("PATH").unwrap_or_default();
    let home = std::env::var("HOME").unwrap_or_default();
    let extras = [
        format!("{home}/.npm-global/bin"),
        format!("{home}/.bun/bin"),
        format!("{home}/.cargo/bin"),
        format!("{home}/.local/bin"),
        "/opt/homebrew/bin".to_string(),
        "/opt/homebrew/sbin".to_string(),
        "/usr/local/bin".to_string(),
        "/usr/local/sbin".to_string(),
    ];
    let mut paths: Vec<String> = current.split(':').map(String::from).collect();
    for p in extras {
        if !p.is_empty() && !paths.contains(&p) {
            paths.push(p);
        }
    }
    paths.join(":")
}

/// 在拓宽过的 PATH 里查找任意 CLI 二进制，返回绝对路径。
/// pub —— backend.rs 的 codex / openclaw 后端也用它定位自己的二进制。
/// 找不到时报错文本里列出所有搜过的目录方便排查。
pub fn find_binary(name: &str) -> Result<std::path::PathBuf> {
    let path = expanded_path();
    let mut searched = Vec::new();
    for dir in path.split(':') {
        if dir.is_empty() {
            continue;
        }
        let candidate = std::path::PathBuf::from(dir).join(name);
        searched.push(candidate.display().to_string());
        if candidate.exists() && std::fs::metadata(&candidate).is_ok() {
            return Ok(candidate);
        }
    }
    anyhow::bail!(
        "找不到 `{name}` CLI。\n已搜索的路径：\n  {}",
        searched.join("\n  ")
    )
}

/// 在拓宽过的 PATH 里查找 `claude` 二进制。
fn find_claude_binary() -> Result<std::path::PathBuf> {
    find_binary("claude").map_err(|e| {
        anyhow::anyhow!("{e}\n装一下：npm install -g @anthropic-ai/claude-code")
    })
}

/// 拼接发给 AI 的最终 prompt —— pub，backend.rs 各后端共用同一套格式。
pub fn build_prompt_pub(
    transcript: &str,
    image_path: &Path,
    frontmost_app: Option<&str>,
    cursor: Option<&CursorContext>,
) -> String {
    build_prompt(transcript, image_path, frontmost_app, cursor)
}

/// 拼接发给 Claude 的最终 prompt（transcript + 截图路径 + 光标位置 + 前台 app）。
fn build_prompt(
    transcript: &str,
    image_path: &Path,
    frontmost_app: Option<&str>,
    cursor: Option<&CursorContext>,
) -> String {
    let context_line = frontmost_app
        .map(|t| format!("\n上下文窗口：{t}"))
        .unwrap_or_default();
    let cursor_line = cursor
        .map(|c| {
            let pct_x = (c.x as f32 / c.screen_w.max(1) as f32 * 100.0).round() as i32;
            let pct_y = (c.y as f32 / c.screen_h.max(1) as f32 * 100.0).round() as i32;
            format!(
                "\n光标位置（截图坐标系 top-left, 单位 logical points）：x={}, y={} \
                 — 即屏幕的横向 {}%、纵向 {}%。屏幕尺寸：{}×{}。",
                c.x, c.y, pct_x, pct_y, c.screen_w, c.screen_h
            )
        })
        .unwrap_or_default();
    format!(
        "{transcript}\n\n截图位置：{}{cursor_line}{context_line}",
        image_path.display()
    )
}

/// Claude CLI 流式调用 —— 边收边把累计文本喂给 `on_chunk`，结束返回完整文本。
///
/// 用 `--output-format stream-json --include-partial-messages --verbose`，stdout 是
/// NDJSON：每行一个 JSON 事件。我们只关心
/// `stream_event` → `content_block_delta` → `text_delta` → `.text` 这种增量文本块，
/// 累加后每收到一块就回调 `on_chunk(累计文本)`，让前端气泡实时长出来。
///
/// `on_chunk` 收到的是**累计**文本（不是单个 delta），调用方直接拿去 emit 即可。
pub async fn ask_claude_streaming<F>(
    transcript: &str,
    image_path: &Path,
    frontmost_app: Option<&str>,
    cursor: Option<&CursorContext>,
    mut on_chunk: F,
) -> Result<String>
where
    F: FnMut(&str),
{
    let prompt = build_prompt(transcript, image_path, frontmost_app, cursor);
    let sys_prompt = system_prompt();
    let claude_bin = find_claude_binary()?;

    let mut child = tokio::process::Command::new(&claude_bin)
        .env("PATH", expanded_path())
        .args([
            "-p",
            &prompt,
            "--allowedTools",
            "Read,Write,Edit,Bash,Grep,Glob",
            "--permission-mode",
            "auto",
            "--output-format",
            "stream-json",
            "--include-partial-messages",
            "--verbose",
            "--append-system-prompt",
            &sys_prompt,
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("failed to spawn {}", claude_bin.display()))?;

    let stdout = child.stdout.take().context("claude stdout not piped")?;
    // stderr 先 take 出来，结束时如果失败再读 —— 用 tokio 的 AsyncRead，不碰 raw fd
    let mut stderr = child.stderr.take();
    let mut reader = BufReader::new(stdout).lines();
    let mut accumulated = String::new();

    while let Some(line) = reader.next_line().await.context("read claude stdout")? {
        if line.trim().is_empty() {
            continue;
        }
        // 每行是一个 JSON 事件；解析失败的行直接跳过（容错）
        let Ok(v) = serde_json::from_str::<serde_json::Value>(&line) else {
            continue;
        };
        // 只取 stream_event → content_block_delta → text_delta → .text
        if v.get("type").and_then(|t| t.as_str()) == Some("stream_event") {
            let ev = &v["event"];
            if ev.get("type").and_then(|t| t.as_str()) == Some("content_block_delta") {
                let delta = &ev["delta"];
                if delta.get("type").and_then(|t| t.as_str()) == Some("text_delta") {
                    if let Some(text) = delta.get("text").and_then(|t| t.as_str()) {
                        accumulated.push_str(text);
                        on_chunk(&accumulated);
                    }
                }
            }
        }
    }

    let status = child.wait().await.context("wait claude")?;
    if !status.success() {
        let mut err_text = String::new();
        if let Some(ref mut s) = stderr {
            use tokio::io::AsyncReadExt;
            let _ = s.read_to_string(&mut err_text).await;
        }
        bail!("claude exited {}: {}", status, err_text.trim());
    }

    let trimmed = accumulated.trim().to_string();
    if trimmed.is_empty() {
        bail!("claude 没有返回任何文本（可能 stream-json 格式变了或调用被拒）");
    }
    Ok(trimmed)
}

/// Claude CLI 非流式调用 —— `ask_claude_streaming` 的 wrapper，丢弃增量回调。
/// smoke test / 不需要流式的场景用。
pub async fn ask_claude(
    transcript: &str,
    image_path: &Path,
    frontmost_app: Option<&str>,
    cursor: Option<&CursorContext>,
) -> Result<String> {
    ask_claude_streaming(transcript, image_path, frontmost_app, cursor, |_| {}).await
}

/// 解析回复是否是 Mode B（含 `[INSERT_AT_CURSOR]` 标记），返回要写入光标的纯文本（去掉标记）。
/// 若是 Mode A，返回 `None`。
pub fn parse_insert_directive(reply: &str) -> Option<String> {
    const OPEN: &str = "[INSERT_AT_CURSOR]";
    const CLOSE: &str = "[/INSERT_AT_CURSOR]";
    let start = reply.find(OPEN)? + OPEN.len();
    let rest = &reply[start..];
    let end = rest.find(CLOSE)?;
    Some(rest[..end].trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mode_a_returns_none() {
        assert!(parse_insert_directive("这段代码的 bug 在第 13 行……").is_none());
    }

    #[test]
    fn mode_b_extracts_inner_text() {
        let reply = "我帮你续写。\n[INSERT_AT_CURSOR]\n第二天清晨，雪停了。\n[/INSERT_AT_CURSOR]";
        assert_eq!(
            parse_insert_directive(reply).as_deref(),
            Some("第二天清晨，雪停了。")
        );
    }

    #[test]
    fn mode_b_takes_first_block() {
        let reply = "[INSERT_AT_CURSOR]A[/INSERT_AT_CURSOR][INSERT_AT_CURSOR]B[/INSERT_AT_CURSOR]";
        assert_eq!(parse_insert_directive(reply).as_deref(), Some("A"));
    }
}
