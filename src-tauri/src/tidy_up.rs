//! Tidy-up · 语音转写后的 LLM 清洗（v0.1.10）
//!
//! 灵感：Typeless / Wispr Flow / FreeFlow 的核心套路 ——
//! Whisper 出原文 → 一次性 fast LLM 调用 → 整理出干净文字 → 写到光标
//!
//! 整理内容：
//!   - 去口头禅 / 填充词（嗯/啊/那个/呃 / um / uh / like）
//!   - 加恰当的标点（中文逗号/英文句号）
//!   - 修自我修正（"我想买…不对…我想订机票" → "我想订机票"）
//!   - 修明显的错字 / 同音错
//!   - **不**改写、不发挥、不加内容（用户原意必须 100% 保留）
//!
//! 触发条件：config.tidy_up_enabled = true（默认开），且 backend 是 Claude/Codex
//! 跳过：纯文本 < 8 字（短到不值得跑 LLM）/ backend 不可用 / tidy_up 失败兜底原文

use anyhow::Result;
use std::time::Duration;

/// LLM 的清洗指令 —— 比一般 prompt 更狠地强调「100% 保留原意」
const TIDY_PROMPT_ZH: &str = r#"你是一个语音转写**整理器**。下面是用户口语转写的原始文本（可能有口头禅/重复/错字/没标点）。

任务：
1. 去掉填充词（嗯/啊/那个/呃/就是…）
2. 加合适的标点（中文用，。！？）
3. 修明显的同音错字 / 自我纠正（"我想去…不，我想留下" → "我想留下"）
4. **不要**改写、不要加内容、不要润色风格 —— 保留用户原本的措辞和语气

**只输出整理后的纯文本**，不要解释、不要前后缀、不要 markdown。文本如下：

"#;

const TIDY_PROMPT_EN: &str = r#"You are a voice transcription **cleaner**. Below is raw speech-to-text output (may contain filler words, repeats, typos, missing punctuation).

Task:
1. Remove filler words (um, uh, like, you know, well…)
2. Add appropriate punctuation
3. Fix self-corrections ("I want to buy... no, I want to book" → "I want to book")
4. **Do NOT** rewrite, add content, or change style — preserve the user's original wording and tone

**Output only the cleaned text**, no explanations, no prefixes, no markdown. Text:

"#;

/// 太短的文本不值得过 LLM —— 8 chars 是经验值（一两个词）
const MIN_LEN_TO_TIDY: usize = 8;
/// LLM 调用超时 —— tidy 必须快，超 5s 就放弃用原文
const TIDY_TIMEOUT_MS: u64 = 5_000;

/// 用当前后端跑一次 tidy。失败 / 超时 → Err，调用方应该用原文兜底。
///
/// 注意：这里**不**通过 backend.rs 走 streaming —— 直接最简单的 spawn + stdout 收集，
/// 因为我们要的是一段纯文本，不需要流式动画。也省了一次截图的成本。
pub async fn tidy(raw: &str, backend: crate::backend::Backend, ui_lang: &str) -> Result<String> {
    let trimmed = raw.trim();
    if trimmed.chars().count() < MIN_LEN_TO_TIDY {
        // 太短，直接返回原文
        return Ok(trimmed.to_string());
    }

    let prompt = format!(
        "{}{}",
        if ui_lang == "en" { TIDY_PROMPT_EN } else { TIDY_PROMPT_ZH },
        trimmed
    );

    let result = tokio::time::timeout(
        Duration::from_millis(TIDY_TIMEOUT_MS),
        run_quick_llm(backend, &prompt),
    ).await;

    match result {
        Ok(Ok(cleaned)) => Ok(post_strip(&cleaned)),
        Ok(Err(e)) => Err(anyhow::anyhow!("tidy LLM failed: {e}")),
        Err(_) => Err(anyhow::anyhow!("tidy timed out after {}ms", TIDY_TIMEOUT_MS)),
    }
}

/// 跑一次最轻量的 LLM 调用 —— 不传图、不带历史、纯文本进出
async fn run_quick_llm(backend: crate::backend::Backend, prompt: &str) -> Result<String> {
    use std::process::Stdio;
    use tokio::io::AsyncReadExt;

    let bin = match backend {
        crate::backend::Backend::ClaudeCli => "claude",
        crate::backend::Backend::CodexCli => "codex",
        crate::backend::Backend::OpenclawCli => "openclaw",
    };
    let bin_path = crate::claude_cli::find_binary(bin)?;

    // Claude: `claude -p "<prompt>"` 单次调用，无 streaming
    // Codex: `codex exec --skip-git-repo-check "<prompt>"`
    // OpenClaw: `openclaw agent --local -m "<prompt>"`
    let args: Vec<String> = match backend {
        crate::backend::Backend::ClaudeCli => {
            vec!["-p".into(), prompt.into(),
                 "--permission-mode".into(), "auto".into(),
                 "--output-format".into(), "text".into()]
        }
        crate::backend::Backend::CodexCli => {
            vec!["exec".into(), "--skip-git-repo-check".into(), prompt.into()]
        }
        crate::backend::Backend::OpenclawCli => {
            vec!["agent".into(), "--local".into(), "-m".into(), prompt.into()]
        }
    };

    let mut child = tokio::process::Command::new(&bin_path)
        .env("PATH", crate::claude_cli::expanded_path())
        .args(&args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    let mut stdout = child.stdout.take().ok_or_else(|| anyhow::anyhow!("stdout piped"))?;
    let mut buf = String::new();
    stdout.read_to_string(&mut buf).await?;
    let status = child.wait().await?;
    if !status.success() {
        let mut err = String::new();
        if let Some(mut s) = child.stderr.take() {
            let _ = s.read_to_string(&mut err).await;
        }
        anyhow::bail!("{bin} exit {status}: {}", err.trim());
    }
    Ok(buf)
}

/// LLM 偶尔会包一层 markdown / "整理后:" 之类的前缀 —— 去掉
fn post_strip(text: &str) -> String {
    let mut s = text.trim().to_string();
    // 去常见的开头模板
    for prefix in ["整理后：", "整理后:", "整理结果：", "整理结果:",
                   "Cleaned:", "Cleaned text:", "Result:"] {
        if let Some(rest) = s.strip_prefix(prefix) {
            s = rest.trim().to_string();
        }
    }
    // 去 markdown 代码围栏
    if s.starts_with("```") {
        if let Some(end) = s.rfind("```") {
            if end > 3 {
                let inner = &s[3..end];
                let inner = inner.trim_start_matches(|c: char| c.is_alphanumeric());
                s = inner.trim().to_string();
            }
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn post_strip_removes_common_prefixes() {
        assert_eq!(post_strip("整理后: 今天天气真好"), "今天天气真好");
        assert_eq!(post_strip("Cleaned: I want to go home"), "I want to go home");
        assert_eq!(post_strip("just clean text"), "just clean text");
    }

    #[test]
    fn post_strip_unwraps_fences() {
        assert_eq!(post_strip("```\n你好世界\n```"), "你好世界");
    }
}
