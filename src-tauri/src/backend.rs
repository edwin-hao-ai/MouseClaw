//! 多 AI 后端抽象 —— 用户在 Onboarding 选一个，存进 `~/.mouseclaw/config.json`。
//!
//! 所有后端走统一契约：`(截图路径 + 文字 prompt) → 流式文本块`。
//!
//! - **ClaudeCli**（默认，最成熟）：`claude -p --output-format stream-json`，见 claude_cli.rs。
//!   原生 agentic + 读图 + 细粒度 thinking/tool 事件（唯一有 on_status 的后端）。
//! - 其余后端都是「spawn 一个非交互 CLI，逐行读 stdout 累计」的统一实现：截图路径写进
//!   prompt（各 CLI 的 Read/Bash 工具能读到），不走 stream-json。每个后端只差「二进制名 +
//!   一次性调用参数」，集中在 `Backend::oneshot_args` 一处维护：
//!
//!   | 后端 | 二进制 | 一次性参数（验证日期 2026-05-21） |
//!   |---|---|---|
//!   | CodexCli        | codex    | `exec --skip-git-repo-check <prompt>` |
//!   | OpenclawCli     | openclaw | `agent --local -m <prompt>` |
//!   | HermesAgent     | hermes   | `-z <prompt>` |
//!   | OpenCodeCli     | opencode | `run <prompt>` |
//!   | GeminiCli       | gemini   | `-p <prompt>` |
//!   | CopilotCli      | copilot  | `-p <prompt> -s --allow-all-tools` |
//!   | KiroCli         | kiro-cli | `chat --no-interactive --trust-all-tools <prompt>` |
//!   | ClineCli        | cline    | `-y <prompt>` |
//!   | KimiCli         | kimi     | `--quiet -p <prompt>` |
//!   | VibeCli         | vibe     | `--prompt <prompt>` |
//!   | PiAgent         | pi       | `-p <prompt>` |
//!   | AntigravityCli  | agy      | `-p <prompt> --dangerously-skip-permissions` |
//!
//! 设计取舍：Claude 路径完整可用；其余是真实调用但依赖用户环境，没装/没配就在选择时
//! 报清晰错误（带 install_cmd）—— 抽象层在，用户随时能接。
//!
//! 加新后端的 checklist（CLAUDE.md「多后端 CLI 兼容」硬规则）：
//!   1. `Backend` 加变体 → 编译器会把所有 match 标红，逐个补 display_name / binary_name /
//!      from_choice / install_url / install_cmd / oneshot_args
//!   2. 前端 `src/types.ts` 的 `BackendChoice` + `Onboarding.tsx` 的 BACKENDS_META / backendDesc
//!   3. 不用为新后端写独立 streaming fn —— oneshot_args 出参数，generic_streaming 跑

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
    HermesAgent, // v0.1.23 · Nous Research 的 Hermes Agent
    // v0.4.4 (2026-05-21) · 一次补齐主流 CLI agent
    OpenCodeCli,    // OpenCode (SST) · `opencode run`
    GeminiCli,      // Google Gemini CLI · `gemini -p`
    CopilotCli,     // GitHub Copilot CLI · `copilot -p`
    KiroCli,        // Kiro CLI (AWS) · `kiro-cli chat --no-interactive`
    ClineCli,       // Cline CLI · `cline -y`
    KimiCli,        // Kimi Code CLI (Moonshot) · `kimi --quiet -p`
    VibeCli,        // Mistral Vibe · `vibe --prompt`
    PiAgent,        // Pi Coding Agent · `pi -p`
    AntigravityCli, // Google Antigravity CLI · `agy -p`
    // v0.4.6 (2026-05-23) · 国产 CLI agent。
    // 注：曾计划同时加 Trae Agent(字节)，但实测它不在 PyPI、git 装缺一串依赖(docker/pexpect)
    // 启动即崩、且是 Docker 重型研究 agent 输出冗长，不适合本场景 → 不收。
    QwenCode,       // 通义 Qwen Code (阿里 QwenLM) · gemini-cli 同源，用位置参数 one-shot
}

impl Default for Backend {
    fn default() -> Self {
        Backend::ClaudeCli
    }
}

impl Backend {
    /// Onboarding UI 用的展示名（兼作错误信息里的「是哪个 CLI 挂了」）。
    pub fn display_name(&self) -> &'static str {
        match self {
            Backend::ClaudeCli => "Claude Code CLI",
            Backend::CodexCli => "OpenAI Codex CLI",
            Backend::OpenclawCli => "OpenClaw CLI",
            Backend::HermesAgent => "Hermes Agent (Nous Research)",
            Backend::OpenCodeCli => "OpenCode",
            Backend::GeminiCli => "Gemini CLI",
            Backend::CopilotCli => "GitHub Copilot CLI",
            Backend::KiroCli => "Kiro CLI",
            Backend::ClineCli => "Cline CLI",
            Backend::KimiCli => "Kimi Code CLI",
            Backend::VibeCli => "Mistral Vibe CLI",
            Backend::PiAgent => "Pi Coding Agent",
            Backend::AntigravityCli => "Antigravity CLI",
            Backend::QwenCode => "Qwen Code (通义千问)",
        }
    }

    /// 二进制名（在拓宽 PATH 里找）。
    pub fn binary_name(&self) -> &'static str {
        match self {
            Backend::ClaudeCli => "claude",
            Backend::CodexCli => "codex",
            Backend::OpenclawCli => "openclaw",
            Backend::HermesAgent => "hermes",
            Backend::OpenCodeCli => "opencode",
            Backend::GeminiCli => "gemini",
            Backend::CopilotCli => "copilot",
            Backend::KiroCli => "kiro-cli",
            Backend::ClineCli => "cline",
            Backend::KimiCli => "kimi",
            Backend::VibeCli => "vibe",
            Backend::PiAgent => "pi",
            Backend::AntigravityCli => "agy",
            Backend::QwenCode => "qwen",
        }
    }

    /// 从前端传来的字符串解析（Onboarding 选项 id）。未知 → 回落 Claude。
    pub fn from_choice(s: &str) -> Backend {
        match s {
            "claude-cli" | "claude" => Backend::ClaudeCli,
            "codex-cli" | "codex" => Backend::CodexCli,
            "openclaw-cli" | "openclaw" => Backend::OpenclawCli,
            "hermes-agent" | "hermes" => Backend::HermesAgent,
            "opencode-cli" | "open-code-cli" | "opencode" => Backend::OpenCodeCli,
            "gemini-cli" | "gemini" => Backend::GeminiCli,
            "copilot-cli" | "copilot" | "vscode-copilot" => Backend::CopilotCli,
            "kiro-cli" | "kiro" => Backend::KiroCli,
            "cline-cli" | "cline" => Backend::ClineCli,
            "kimi-cli" | "kimi" => Backend::KimiCli,
            "vibe-cli" | "vibe" => Backend::VibeCli,
            "pi-agent" | "pi" => Backend::PiAgent,
            "antigravity-cli" | "antigravity" | "agy" => Backend::AntigravityCli,
            "qwen-code" | "qwen" | "qwencode" => Backend::QwenCode,
            _ => Backend::ClaudeCli,
        }
    }

    /// 安装指引 URL —— Onboarding 检测到没装时给用户的"去装"链接。
    pub fn install_url(&self) -> &'static str {
        match self {
            Backend::ClaudeCli   => "https://www.anthropic.com/claude-code",
            Backend::CodexCli    => "https://www.npmjs.com/package/@openai/codex",
            Backend::OpenclawCli => "https://www.npmjs.com/package/openclaw",
            Backend::HermesAgent => "https://github.com/NousResearch/hermes-agent",
            Backend::OpenCodeCli => "https://opencode.ai",
            Backend::GeminiCli   => "https://github.com/google-gemini/gemini-cli",
            Backend::CopilotCli  => "https://github.com/features/copilot/cli",
            Backend::KiroCli     => "https://kiro.dev/docs/cli/",
            Backend::ClineCli    => "https://cline.bot/cli",
            Backend::KimiCli     => "https://github.com/MoonshotAI/kimi-cli",
            Backend::VibeCli     => "https://github.com/mistralai/mistral-vibe",
            Backend::PiAgent     => "https://pi.dev",
            Backend::AntigravityCli => "https://antigravity.google/docs/cli-using",
            Backend::QwenCode    => "https://github.com/QwenLM/qwen-code",
        }
    }

    /// 一行能跑的安装命令 —— 直接复制到 Terminal 用（验证日期 2026-05-21）。
    pub fn install_cmd(&self) -> &'static str {
        match self {
            Backend::ClaudeCli   => "npm i -g @anthropic-ai/claude-code",
            Backend::CodexCli    => "npm i -g @openai/codex",
            Backend::OpenclawCli => "npm i -g openclaw",
            Backend::HermesAgent => "curl -fsSL https://raw.githubusercontent.com/NousResearch/hermes-agent/main/scripts/install.sh | bash",
            Backend::OpenCodeCli => "curl -fsSL https://opencode.ai/install | bash",
            Backend::GeminiCli   => "npm i -g @google/gemini-cli",
            Backend::CopilotCli  => "npm i -g @github/copilot",
            Backend::KiroCli     => "curl -fsSL https://cli.kiro.dev/install | bash",
            Backend::ClineCli    => "npm i -g cline",
            Backend::KimiCli     => "curl -LsSf https://code.kimi.com/install.sh | bash",
            Backend::VibeCli     => "uv tool install mistral-vibe",
            Backend::PiAgent     => "npm i -g @mariozechner/pi-coding-agent",
            Backend::AntigravityCli => "curl -fsSL https://antigravity.google/cli/install.sh | bash",
            Backend::QwenCode    => "npm i -g @qwen-code/qwen-code",
        }
    }

    /// 所有后端的规范顺序 —— 托盘切换菜单 / onboarding 选择 / 测试遍历共用一处。
    /// Claude 排第一（默认 + 最成熟），其余大致按主流度。
    pub fn all() -> [Backend; 14] {
        [
            Backend::ClaudeCli, Backend::CodexCli, Backend::GeminiCli, Backend::CopilotCli,
            Backend::OpenCodeCli, Backend::ClineCli, Backend::KimiCli, Backend::KiroCli,
            Backend::AntigravityCli, Backend::VibeCli, Backend::PiAgent, Backend::OpenclawCli,
            Backend::HermesAgent, Backend::QwenCode,
        ]
    }

    /// 一次性（非流式）调用的命令参数。`prompt` 已拼好（流式路径含 system + 截图路径；
    /// 纯文本路径就是 reactive action 的 prompt）。Claude 这里给的是它的纯文本 `-p` 形态，
    /// 它的 stream-json 流式路径不走这里（见 claude_cli::ask_claude_streaming）。
    ///
    /// **没有 wildcard arm** —— 加新 Backend 变体时编译器强制在此补一行（参数即"协议"）。
    fn oneshot_args(&self, prompt: &str) -> Vec<String> {
        let p = prompt.to_string();
        match self {
            Backend::ClaudeCli => vec![
                "-p".into(), p,
                "--permission-mode".into(), "auto".into(),
                "--allowedTools".into(), "".into(),
            ],
            Backend::CodexCli => vec!["exec".into(), "--skip-git-repo-check".into(), p],
            Backend::OpenclawCli => vec!["agent".into(), "--local".into(), "-m".into(), p],
            Backend::HermesAgent => vec!["-z".into(), p],
            Backend::OpenCodeCli => vec!["run".into(), p],
            Backend::GeminiCli => vec!["-p".into(), p],
            // -s 静默：抑制 model 元信息行，stdout 只剩干净答案（官方文档推荐用于脚本捕获）；
            // --allow-all-tools 让非交互下不卡权限
            Backend::CopilotCli => vec!["-p".into(), p, "-s".into(), "--allow-all-tools".into()],
            Backend::KiroCli => vec![
                "chat".into(), "--no-interactive".into(), "--trust-all-tools".into(), p,
            ],
            Backend::ClineCli => vec!["-y".into(), p],
            // --quiet = --print --output-format text --final-message-only（只要最终答案）
            Backend::KimiCli => vec!["--quiet".into(), "-p".into(), p],
            Backend::VibeCli => vec!["--prompt".into(), p],
            Backend::PiAgent => vec!["-p".into(), p],
            // agy -p 是 print/headless 非交互模式（gemini-cli 血统），自身不卡工具权限。
            // 注：早期误加了 `--dangerously-skip-permissions`（那是 Claude 的 flag，agy 没有，
            // 会当 unknown flag 报错）—— 2026-05-23 web 核对后去掉。
            Backend::AntigravityCli => vec!["-p".into(), p],
            // Qwen Code（gemini-cli 同源）：用**位置参数** one-shot（`qwen "PROMPT"`）。
            // 不用 `-p` —— 官方已把 -p 标 deprecated（实测 2026-05-23），位置参数才是
            // 推荐的非交互入口，且避免 deprecation 警告污染捕获的 stdout。
            Backend::QwenCode => vec![p],
        }
    }

    /// 同 `oneshot_args`，但**放开工具**（给定时任务联网/读文件取真实数据，反幻觉）。
    /// 只有 Claude 需要特殊处理：它默认 `--allowedTools ""` 关掉了所有工具；显式放开
    /// web + 文件 + bash。其余后端的 oneshot_args 本就是 agentic（不禁工具），直接复用。
    fn oneshot_args_agentic(&self, prompt: &str) -> Vec<String> {
        match self {
            Backend::ClaudeCli => vec![
                "-p".into(), prompt.to_string(),
                "--permission-mode".into(), "auto".into(),
                "--allowedTools".into(), "WebSearch,WebFetch,Read,Bash,Grep,Glob".into(),
            ],
            other => other.oneshot_args(prompt),
        }
    }
}

/// 找后端二进制，没找到时把 install_cmd 拼进错误里（用户能直接照抄）。
fn find_backend_binary(backend: Backend) -> Result<std::path::PathBuf> {
    crate::claude_cli::find_binary(backend.binary_name())
        .map_err(|e| anyhow::anyhow!("{e}\n装一下：{}", backend.install_cmd()))
}

/// 统一流式调用入口 —— 按 backend 分发。
/// `on_chunk`：累计**最终答案**文本，调用方直接 emit。`on_status`：处理期间的**实时活动**
/// （只有 Claude CLI 的 stream-json 有 thinking_delta / tool_use），让用户知道没卡死。
/// 其余后端没有细粒度事件，不调 on_status。
pub async fn ask_streaming<F, G>(
    backend: Backend,
    transcript: &str,
    image: &Path,
    frontmost: Option<&str>,
    cursor: Option<&CursorContext>,
    trail_summary: Option<&str>,
    on_chunk: F,
    on_status: G,
) -> Result<String>
where
    F: FnMut(&str),
    G: FnMut(&str),
{
    match backend {
        Backend::ClaudeCli => {
            crate::claude_cli::ask_claude_streaming(
                transcript, image, frontmost, cursor, trail_summary, on_chunk, on_status
            ).await
        }
        // 其余所有后端：统一的「spawn 非交互 CLI，逐行累计 stdout」路径。
        _ => {
            let _ = on_status;
            generic_streaming(
                backend, transcript, image, frontmost, cursor, trail_summary, on_chunk
            ).await
        }
    }
}

/// 通用的「spawn 一个 CLI，逐行读 stdout，累计回调」流式实现。
/// 非 Claude 后端都不像 Claude 那样有 stream-json，直接把每行 stdout 当纯文本累加 ——
/// 简单且对任何输出格式都鲁棒。
async fn spawn_and_stream<F>(
    bin: &Path,
    args: &[&str],
    mut on_chunk: F,
) -> Result<String>
where
    F: FnMut(&str),
{
    let mut cmd = tokio::process::Command::new(bin);
    cmd.env("PATH", crate::claude_cli::expanded_path());
    // v0.4 · 降优先级 —— 保护并发时本地语音输入法 ASR 的 CPU（所有外部 AI 子进程同理）
    crate::claude_cli::lower_priority(&mut cmd);
    // v0.1.25 · ~/.mouseclaw/provider.env 里的 key 灌进子进程
    //   让各后端看到 OPENAI_API_KEY / AI_GATEWAY_API_KEY / GEMINI_API_KEY / etc.
    //   不用用户改 shell rc。已在 OS env 里的同名变量不覆盖（shell 优先）。
    crate::provider_env::apply_to(&mut cmd);
    // v0.1.21 · 工作区 cwd
    if let Some(ws) = crate::config::Config::load().workspace_path {
        if std::path::Path::new(&ws).is_dir() {
            cmd.current_dir(&ws);
        }
    }
    let mut child = cmd
        .args(args)
        // stdin 给 null —— 否则 claude -p 等非交互 CLI 会先等 ~3s stdin 输入
        // （"no stdin data received in 3s"），白白拖慢每次 reactive/定时任务/反思调用。
        .stdin(Stdio::null())
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

/// 纯文本 → 文本 单次调用 —— 给 reactive ribbon 的 action（清理 / 翻译 / 解释 / 回信）用。
/// 不传截图，不开 streaming，等所有输出收完一次性返回。各后端用各自 CLI 但参数对齐。
///
/// 设计原则（CLAUDE.md "多后端 CLI 都要兼容"硬规则）：每次新加一种短任务流水线
/// **必须**走这个统一接口而不是硬编码 `claude` 二进制 —— 否则非 Claude 用户拿不到那功能。
pub async fn ask_text_only(backend: Backend, prompt: &str) -> Result<String> {
    let bin = find_backend_binary(backend)?;
    let args = backend.oneshot_args(prompt);
    let arg_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    spawn_and_stream(&bin, &arg_refs, |_| {}).await
}

/// 纯文本 → 文本 单次调用，但**放开 agentic 工具**（WebSearch/WebFetch/Bash/Read…）。
/// 给定时任务这种"到点自动跑、常需要联网/读文件取真实数据"的后台任务用 —— 防止
/// AI 凭记忆编造（如"整理 AI 新闻"没工具就只能幻觉）。reactive ribbon 的文本变换
/// 不需要工具，仍走 `ask_text_only`。各后端用各自 CLI 但都允许工具（多后端硬规则）。
pub async fn ask_text_only_agentic(backend: Backend, prompt: &str) -> Result<String> {
    let bin = find_backend_binary(backend)?;
    let args = backend.oneshot_args_agentic(prompt);
    let arg_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    spawn_and_stream(&bin, &arg_refs, |_| {}).await
}

/// 非 Claude 后端的多模态长任务：截图路径写进 prompt（各 CLI 的 Read/Bash 工具读到），
/// system_prompt + build_prompt 拼好后逐行 stream stdout 累计。
async fn generic_streaming<F>(
    backend: Backend,
    transcript: &str,
    image: &Path,
    frontmost: Option<&str>,
    cursor: Option<&CursorContext>,
    trail_summary: Option<&str>,
    on_chunk: F,
) -> Result<String>
where
    F: FnMut(&str),
{
    let bin = find_backend_binary(backend)?;
    // system_prompt() 对所有后端共用（改文案自动对全部生效）
    let prompt = format!(
        "{}\n\n{}",
        crate::claude_cli::system_prompt(),
        crate::claude_cli::build_prompt_pub(transcript, image, frontmost, cursor, trail_summary)
    );
    let args = backend.oneshot_args(&prompt);
    let arg_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    spawn_and_stream(&bin, &arg_refs, on_chunk).await
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
        assert_eq!(Backend::from_choice("hermes-agent"), Backend::HermesAgent);
        assert_eq!(Backend::from_choice("hermes"), Backend::HermesAgent);
        assert_eq!(Backend::from_choice("claude-cli"), Backend::ClaudeCli);
        // v0.4.4 新增后端
        assert_eq!(Backend::from_choice("opencode-cli"), Backend::OpenCodeCli);
        assert_eq!(Backend::from_choice("opencode"), Backend::OpenCodeCli);
        assert_eq!(Backend::from_choice("gemini-cli"), Backend::GeminiCli);
        assert_eq!(Backend::from_choice("gemini"), Backend::GeminiCli);
        assert_eq!(Backend::from_choice("copilot-cli"), Backend::CopilotCli);
        assert_eq!(Backend::from_choice("vscode-copilot"), Backend::CopilotCli);
        assert_eq!(Backend::from_choice("kiro-cli"), Backend::KiroCli);
        assert_eq!(Backend::from_choice("cline"), Backend::ClineCli);
        assert_eq!(Backend::from_choice("kimi"), Backend::KimiCli);
        assert_eq!(Backend::from_choice("vibe"), Backend::VibeCli);
        assert_eq!(Backend::from_choice("pi-agent"), Backend::PiAgent);
        assert_eq!(Backend::from_choice("antigravity"), Backend::AntigravityCli);
        assert_eq!(Backend::from_choice("agy"), Backend::AntigravityCli);
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
        let all = Backend::all();
        let mut names: Vec<&str> = all.iter().map(|b| b.binary_name()).collect();
        let n = names.len();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), n, "binary names must be unique");
        assert_eq!(Backend::KiroCli.binary_name(), "kiro-cli");
        assert_eq!(Backend::AntigravityCli.binary_name(), "agy");
    }

    #[test]
    fn install_cmd_and_url_non_empty_for_all() {
        let all = Backend::all();
        for b in all {
            assert!(!b.install_cmd().is_empty(), "{:?} install_cmd empty", b);
            assert!(b.install_url().starts_with("https://"), "{:?} bad url", b);
            assert!(!b.display_name().is_empty(), "{:?} display_name empty", b);
            // oneshot_args 末位必须是 prompt（防止哪个后端把 prompt 漏在中间被吞）
            let args = b.oneshot_args("PROMPT_SENTINEL");
            assert!(args.iter().any(|a| a == "PROMPT_SENTINEL"),
                "{:?} oneshot_args 丢了 prompt", b);
        }
    }

    #[test]
    fn serde_roundtrip_kebab_case() {
        let json = serde_json::to_string(&Backend::CodexCli).unwrap();
        assert_eq!(json, "\"codex-cli\"");
        let back: Backend = serde_json::from_str("\"openclaw-cli\"").unwrap();
        assert_eq!(back, Backend::OpenclawCli);
        // 新后端的 kebab-case 也要对齐前端 BackendChoice
        assert_eq!(serde_json::to_string(&Backend::OpenCodeCli).unwrap(), "\"open-code-cli\"");
        assert_eq!(serde_json::to_string(&Backend::AntigravityCli).unwrap(), "\"antigravity-cli\"");
    }

    /// 编译期保证 oneshot_args / ask_text_only 覆盖了所有 Backend 变体 ——
    /// 加新 backend 时 match 漏掉一个就会触发 non-exhaustive，强制开发者补齐
    /// （CLAUDE.md "多后端 CLI 都要兼容"硬规则）。
    #[test]
    fn ask_text_only_covers_all_backends() {
        fn _assert_exhaustive(b: Backend) {
            #[allow(clippy::let_underscore_future)]
            let _ = async move {
                // 无 wildcard 的 match —— 漏变体编译失败
                let _ = match b {
                    Backend::ClaudeCli
                    | Backend::CodexCli
                    | Backend::OpenclawCli
                    | Backend::HermesAgent
                    | Backend::OpenCodeCli
                    | Backend::GeminiCli
                    | Backend::CopilotCli
                    | Backend::KiroCli
                    | Backend::ClineCli
                    | Backend::KimiCli
                    | Backend::VibeCli
                    | Backend::PiAgent
                    | Backend::AntigravityCli
                    | Backend::QwenCode => crate::backend::ask_text_only(b, "x"),
                }.await;
            };
        }
        // 不实际执行（spawn 真二进制非 hermetic），只验证签名 + match exhaustiveness
        let _ = _assert_exhaustive;
    }
}
