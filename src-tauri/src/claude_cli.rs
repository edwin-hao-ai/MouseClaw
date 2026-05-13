//! Claude Code CLI 集成层。
//!
//! Risk #1 已于 2026-05-13 验证：`claude -p` + `--allowedTools "Read"` 可以读本地 PNG。
//! 完整调用约定见 `/Users/edwinhao/MouseClaw/CLAUDE.md` 「Claude CLI 调用约定」一节。
//!
//! Day 1 用最简单的 `--output-format text`（一次拿全文）。
//! Day 2+ 切到 `stream-json` 做流式气泡。

use std::path::Path;
use anyhow::{bail, Context, Result};

/// 发给 Claude 的 system prompt 追加内容（不替换默认）。
/// 教 Claude 用 `[INSERT_AT_CURSOR]...[/INSERT_AT_CURSOR]` 标记区分 Mode A 和 Mode B 输出。
const APPEND_SYSTEM_PROMPT: &str = r#"你是 MouseClaw 桌面助手。用户通过语音 + 截图向你提问。
如果用户**明确要求**把内容写入当前光标位置（"续写"、"补全这里"、"写一段在这里"），
用以下标记包裹要写入的纯文本（除标记内文本外不要其他内容）：
[INSERT_AT_CURSOR]
要写入的内容
[/INSERT_AT_CURSOR]
否则正常回答即可。"#;

/// Claude CLI 一次性调用（非流式），返回完整回复文本。
///
/// `transcript`：用户的语音转文字（Day 1 还没接 Whisper，先传死字符串测试）
/// `image_path`：截屏 PNG 路径
/// `frontmost_app`：前台 app 标题（V1 先传 None，Day 2 加 NSWorkspace 获取）
pub async fn ask_claude(
    transcript: &str,
    image_path: &Path,
    frontmost_app: Option<&str>,
) -> Result<String> {
    let context_line = frontmost_app
        .map(|t| format!("\n上下文窗口：{t}"))
        .unwrap_or_default();
    let prompt = format!(
        "{transcript}\n\n截图位置：{}{context_line}",
        image_path.display()
    );

    let output = tokio::process::Command::new("claude")
        .args([
            "-p",
            &prompt,
            "--allowedTools",
            "Read,Write,Edit,Bash,Grep,Glob",
            "--permission-mode",
            "auto",
            "--output-format",
            "text",
            "--append-system-prompt",
            APPEND_SYSTEM_PROMPT,
        ])
        .output()
        .await
        .context("failed to spawn `claude` CLI — is it on PATH?")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("claude exited {}: {}", output.status, stderr.trim());
    }

    let stdout = String::from_utf8(output.stdout).context("claude stdout was not UTF-8")?;
    Ok(stdout.trim().to_string())
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
