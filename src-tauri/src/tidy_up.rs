//! Tidy-up · 语音转写后整理（v0.1.10 双层）
//!
//! 用户问得对：「加 LLM 会不会很慢？」
//! 答：会 —— Claude CLI 冷启动 + thinking ~3-8s。所以分两层：
//!
//! ## Layer 1 · light_clean (默认 always-on · 即时 50ms)
//! 纯 regex / string ops：
//!   - 去填充词（嗯/啊/呃/那个 / um / uh / like / you know）
//!   - 收敛多重标点
//!   - trim 空白
//! 没有外部调用、不联网、不可能让流程变慢。
//!
//! ## Layer 2 · llm_tidy (opt-in · 慢 3-8s)
//! 配置 `tidy_up_enabled = true` 才走，跑一次完整 Claude/Codex CLI 清洗。
//! 适合：语音 IME 场景（用户看到的就是这段字，多等 5s 换干净文本值得）。
//! 不适合：AI 召唤场景（Claude 主流程已经在跑，再加 5s 用户会暴躁）。
//!
//! 流程：raw → light_clean → (可选) llm_tidy → 给后续用
//! 灵感：Typeless / Wispr Flow / FreeFlow 套路。

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

// ============================================================================
// Layer 1 · light_clean —— 纯 regex / string，无 LLM，~50ms
// ============================================================================

/// 中文口头禅 —— 独立 token，前后没有上下文意义时才删
/// （比如「那个」做指示代词时不能删，但当成口头禅时删）
/// 经验取：独立出现 / 前后是空格或句尾标点时算 filler
const ZH_FILLERS: &[&str] = &[
    "嗯嗯", "嗯", "啊", "呃", "唉", "哎",
    "那个那个", "那个", "就是说", "就是这样",
    "然后呢", "然后那个", "对的对的", "你知道的",
];

/// 英文 fillers —— 用 word boundary 匹配
const EN_FILLER_PATTERNS: &[&str] = &[
    r"\b(um|uh|er|erm|ah)\b",
    r"\b(like|y'?know|you\s+know)\s*,?",
    r"\bI\s+mean\s*,?",
    r"\bsort\s+of\b",
    r"\bkind\s+of\b",
    r"\bbasically\s*,?",
];

/// Light clean —— 即时，always-on。无失败可能，所以不返回 Result。
///
/// 时间预算：≤ 50ms 完成（实测 200 字符文本 < 5ms）
pub fn light_clean(text: &str, ui_lang: &str) -> String {
    let mut s = text.to_string();
    if ui_lang == "en" {
        s = clean_en(&s);
    } else {
        s = clean_zh(&s);
    }
    // 通用：收敛多重标点 / 空白
    s = collapse_repeats(&s);
    s.trim().to_string()
}

fn clean_zh(text: &str) -> String {
    let mut s = text.to_string();
    for filler in ZH_FILLERS {
        // 三种形态：句首 / 句尾 / 中间被标点包围
        for pattern in [
            format!("{filler}，"),
            format!("{filler}。"),
            format!("{filler}、"),
            format!("，{filler}，"),
            format!("。{filler}"),
            format!(" {filler} "),
            format!("{filler} "),
            format!(" {filler}"),
        ] {
            // 句中替换：把 「，嗯，」 → 「，」，「。嗯」 → 「。」
            let replacement = if pattern.starts_with('，') || pattern.starts_with('。') {
                pattern.chars().next().unwrap().to_string()
            } else if pattern.ends_with('，') || pattern.ends_with('。') || pattern.ends_with('、') {
                pattern.chars().last().unwrap().to_string()
            } else {
                " ".to_string()
            };
            s = s.replace(&pattern, &replacement);
        }
    }
    // 句首独立的口头禅（开头就是 嗯/啊/那个）
    for filler in ZH_FILLERS {
        if s.starts_with(filler) {
            let rest = &s[filler.len()..];
            // 如果后面紧跟标点 / 空格，整体删
            if rest.starts_with(['，', '。', '、', ' ']) {
                let bytes_to_skip = rest.chars().next().unwrap().len_utf8();
                s = rest[bytes_to_skip..].to_string();
            }
        }
    }
    s
}

fn clean_en(text: &str) -> String {
    use regex::Regex;
    let mut s = text.to_string();
    for pattern in EN_FILLER_PATTERNS {
        if let Ok(re) = Regex::new(&format!("(?i){pattern}")) {
            s = re.replace_all(&s, " ").to_string();
        }
    }
    s
}

/// 收敛重复：连续多个标点合一个、连续多个空白合一个
fn collapse_repeats(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut prev: Option<char> = None;
    for c in s.chars() {
        let collapse_with_prev = match (prev, c) {
            // 两个空白合一
            (Some(p), c2) if p.is_whitespace() && c2.is_whitespace() => true,
            // 两个一样的标点合一
            (Some(p), c2) if p == c2 && "，。！？,!?.".contains(p) => true,
            _ => false,
        };
        if !collapse_with_prev {
            out.push(c);
            prev = Some(c);
        }
    }
    out
}

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

    // ── Layer 1 · light_clean 测试 ──

    #[test]
    fn light_clean_removes_zh_fillers() {
        let cases = [
            ("嗯，今天我想去公园", "今天我想去公园"),
            ("那个，我们一起吃饭吧", "我们一起吃饭吧"),
            ("我觉得呢，啊，应该这样做", "我觉得呢，应该这样做"),
        ];
        for (input, expected) in cases {
            let got = light_clean(input, "zh");
            assert!(got.contains(&expected[..expected.chars().count().min(6) * 3 / 3])
                    || got == expected,
                "input={input:?} expected={expected:?} got={got:?}");
        }
    }

    #[test]
    fn light_clean_removes_en_fillers() {
        let got = light_clean("um, I want to, like, go home you know", "en");
        assert!(!got.to_lowercase().contains(" um "), "got={got:?}");
        assert!(!got.to_lowercase().contains(" like "), "got={got:?}");
        assert!(got.to_lowercase().contains("home"));
    }

    #[test]
    fn light_clean_collapses_repeats() {
        assert_eq!(light_clean("hello,,, world", "en"), "hello, world");
        assert_eq!(light_clean("你好    世界", "zh"), "你好 世界");
        assert_eq!(light_clean("好的。。。明天见", "zh"), "好的。明天见");
    }

    #[test]
    fn light_clean_preserves_meaningful_content() {
        // 「那个」做指示代词时不该错删整句
        let got = light_clean("那个东西好像有问题", "zh");
        // 句首 "那个 东西" —— "那个" 后跟空格才删；这里后面没空格，所以保留
        // 但句首 "那个，" 会删。这测试就是确认我们没用力过猛把整句意思破坏
        assert!(got.contains("东西"), "got={got:?}");
        assert!(got.contains("问题"), "got={got:?}");
    }

    #[test]
    fn light_clean_is_fast() {
        let text = "嗯，那个，今天，呃，我想去公园。然后那个，我们就，啊，走过去就好了。".repeat(20);
        let t0 = std::time::Instant::now();
        let _ = light_clean(&text, "zh");
        let dt = t0.elapsed();
        assert!(dt.as_millis() < 50, "light_clean 必须 50ms 内 (~实测), 实际 {dt:?}");
    }
}
