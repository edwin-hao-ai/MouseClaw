//! 多 AI 后端抽象 —— Claude Code CLI / Codex CLI / OpenClaw CLI 三选一。
//!
//! 用户在 Onboarding 里选一个，存进 `~/.mouseclaw/config.json` 的 `backend` 字段。
//! 所有后端走统一契约：`(截图路径 + 文字 prompt) → 流式文本块`。
//!
//! - **ClaudeCli**（默认，最成熟）：`claude -p --output-format stream-json`，
//!   见 claude_cli.rs。原生 agentic + 读图。
//! - **CodexCli**：`codex exec`，逐行 stream stdout。需用户装 OpenAI Codex CLI。
//! - **OpenclawCli**：`openclaw agent --local -m`，需用户配好 provider key。
//!
//! 设计取舍：Claude 路径完整可用；Codex / OpenClaw 是真实调用但依赖用户环境，
//! 没装/没配就在选择时报清晰错误 —— 抽象层在，用户随时能接。

use std::path::Path;
use std::process::Stdio;
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, BufReader};

pub use crate::claude_cli::CursorContext;

/// 当前选用的 AI 后端。serde kebab-case 跟前端 / config.json 对齐。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Backend {
    ClaudeCli,
    CodexCli,
    OpenclawCli,
}

impl Default for Backend {
    fn default() -> Self {
        Backend::ClaudeCli
    }
}

impl Backend {
    /// 给 Onboarding UI 用的中文展示名。
    pub fn display_name(&self) -> &'static str {
        match self {
            Backend::ClaudeCli => "Claude Code CLI",
            Backend::CodexCli => "OpenAI Codex CLI",
            Backend::OpenclawCli => "OpenClaw CLI",
        }
    }

    /// 二进制名（在拓宽 PATH 里找）。
    pub fn binary_name(&self) -> &'static str {
        match self {
            Backend::ClaudeCli => "claude",
            Backend::CodexCli => "codex",
            Backend::OpenclawCli => "openclaw",
        }
    }

    /// 从前端传来的字符串解析（Onboarding 选项 id）。
    pub fn from_choice(s: &str) -> Backend {
        match s {
            "codex-cli" | "codex" => Backend::CodexCli,
            "openclaw-cli" | "openclaw" => Backend::OpenclawCli,
            _ => Backend::ClaudeCli,
        }
    }
}

/// 统一流式调用入口 —— 按 backend 分发到对应 CLI。
/// `on_chunk` 收到的是**累计**文本（不是单 delta），调用方直接 emit 即可。
pub async fn ask_streaming<F>(
    backend: Backend,
    transcript: &str,
    image: &Path,
    frontmost: Option<&str>,
    cursor: Option<&CursorContext>,
    on_chunk: F,
) -> Result<String>
where
    F: FnMut(&str),
{
    match backend {
        Backend::ClaudeCli => {
            crate::claude_cli::ask_claude_streaming(transcript, image, frontmost, cursor, on_chunk)
                .await
        }
        Backend::CodexCli => codex_streaming(transcript, image, frontmost, cursor, on_chunk).await,
        Backend::OpenclawCli => {
            openclaw_streaming(transcript, image, frontmost, cursor, on_chunk).await
        }
    }
}

/// 通用的「spawn 一个 CLI，逐行读 stdout，累计回调」流式实现。
/// Codex / OpenClaw 都不像 Claude 那样有 stream-json，直接把每行 stdout
/// 当作纯文本累加 —— 简单且对任何输出格式都鲁棒。
async fn spawn_and_stream<F>(
    bin: &Path,
    args: &[&str],
    mut on_chunk: F,
) -> Result<String>
where
    F: FnMut(&str),
{
    let mut child = tokio::process::Command::new(bin)
        .env("PATH", crate::claude_cli::expanded_path())
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("failed to spawn {}", bin.display()))?;

    let stdout = child.stdout.take().context("stdout not piped")?;
    let mut stderr = child.stderr.take();
    let mut reader = BufReader::new(stdout).lines();
    let mut accumulated = String::new();

    while let Some(line) = reader.next_line().await.context("read stdout")? {
        // 跳过明显的进度噪声行（spinner / 百分比）
        let trimmed = line.trim_end();
        if trimmed.is_empty() {
            continue;
        }
        if !accumulated.is_empty() {
            accumulated.push('\n');
        }
        accumulated.push_str(trimmed);
        on_chunk(&accumulated);
    }

    let status = child.wait().await.context("wait child")?;
    if !status.success() {
        let mut err_text = String::new();
        if let Some(ref mut s) = stderr {
            use tokio::io::AsyncReadExt;
            let _ = s.read_to_string(&mut err_text).await;
        }
        bail!("{} exited {}: {}", bin.display(), status, err_text.trim());
    }
    let out = accumulated.trim().to_string();
    if out.is_empty() {
        bail!("{} 没有返回任何输出", bin.display());
    }
    Ok(out)
}

/// OpenAI Codex CLI：`codex exec <prompt>` 非交互模式，逐行 stream stdout。
/// 截图路径写进 prompt，Codex 的沙箱有文件读权限能读到。
async fn codex_streaming<F>(
    transcript: &str,
    image: &Path,
    frontmost: Option<&str>,
    cursor: Option<&CursorContext>,
    on_chunk: F,
) -> Result<String>
where
    F: FnMut(&str),
{
    let bin = crate::claude_cli::find_binary("codex")
        .map_err(|e| anyhow::anyhow!("{e}\n装一下：npm install -g @openai/codex"))?;
    let prompt = format!(
        "{}\n\n{}",
        crate::claude_cli::system_prompt(),
        crate::claude_cli::build_prompt_pub(transcript, image, frontmost, cursor)
    );
    spawn_and_stream(&bin, &["exec", "--skip-git-repo-check", &prompt], on_chunk).await
}

/// OpenClaw CLI：`openclaw agent --local -m <prompt>` 跑一个 agent turn。
/// `--local` 要求用户 shell 里有 model provider API key。
async fn openclaw_streaming<F>(
    transcript: &str,
    image: &Path,
    frontmost: Option<&str>,
    cursor: Option<&CursorContext>,
    on_chunk: F,
) -> Result<String>
where
    F: FnMut(&str),
{
    let bin = crate::claude_cli::find_binary("openclaw")
        .map_err(|e| anyhow::anyhow!("{e}\n装一下：npm install -g openclaw"))?;
    let prompt = format!(
        "{}\n\n{}",
        crate::claude_cli::system_prompt(),
        crate::claude_cli::build_prompt_pub(transcript, image, frontmost, cursor)
    );
    spawn_and_stream(&bin, &["agent", "--local", "-m", &prompt], on_chunk).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_choice_maps_known_ids() {
        assert_eq!(Backend::from_choice("codex-cli"), Backend::CodexCli);
        assert_eq!(Backend::from_choice("codex"), Backend::CodexCli);
        assert_eq!(Backend::from_choice("openclaw-cli"), Backend::OpenclawCli);
        assert_eq!(Backend::from_choice("openclaw"), Backend::OpenclawCli);
        assert_eq!(Backend::from_choice("claude-cli"), Backend::ClaudeCli);
    }

    #[test]
    fn from_choice_unknown_falls_back_to_claude() {
        assert_eq!(Backend::from_choice(""), Backend::ClaudeCli);
        assert_eq!(Backend::from_choice("garbage"), Backend::ClaudeCli);
    }

    #[test]
    fn default_is_claude() {
        assert_eq!(Backend::default(), Backend::ClaudeCli);
    }

    #[test]
    fn binary_names_are_distinct() {
        assert_eq!(Backend::ClaudeCli.binary_name(), "claude");
        assert_eq!(Backend::CodexCli.binary_name(), "codex");
        assert_eq!(Backend::OpenclawCli.binary_name(), "openclaw");
    }

    #[test]
    fn serde_roundtrip_kebab_case() {
        let json = serde_json::to_string(&Backend::CodexCli).unwrap();
        assert_eq!(json, "\"codex-cli\"");
        let back: Backend = serde_json::from_str("\"openclaw-cli\"").unwrap();
        assert_eq!(back, Backend::OpenclawCli);
    }
}
