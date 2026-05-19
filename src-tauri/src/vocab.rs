//! Sherpa-onnx hotwords (contextual biasing) —— v0.4.0 P1.
//!
//! "术语库" —— 把用户/项目相关词喂给 sherpa decoder，解码时这些词的概率被
//! boost，识别准确率明显提升（特别是程序员说 useEffect / Tauri / Claude
//! 这类词）。完全本地、零额外模型、零延迟成本。
//!
//! ## 数据来源
//!   1. **内置程序员词表**（`include_str!` 编译进二进制 · ~2 KB）—— 默认开启
//!   2. **用户词表** `~/.mouseclaw/vocab/user.txt` —— 用户在托盘点"📝 编辑术语表..."
//!      手动加（一行一个词，可选 `:score` 后缀）
//!
//! ## 合并产物
//!   `~/.mouseclaw/vocab/active.txt` —— sherpa hotwords_file 实际读这个。
//!   每次 onboarding 完 / 托盘开关 / 用户改完 user.txt → 重新生成。
//!
//! ## 格式（sherpa-onnx 约定）
//!   每行：`<token sequence> :<score>` —— score 可省略，省略时用全局
//!   `hotwords_score`（我们设 2.0）。
//!   注释（`#` 开头）和空行被忽略。
//!
//! ## 改完啥时生效？
//!   Sherpa 把 hotwords 在 `OnlineRecognizer::create` 时 init 注入，热加载
//!   不支持。改完词表 → 调 `transcribe_stream::reload_recognizer()` 让下次
//!   transcribe 重新 init。**当前用户已经按住的快捷键不受影响**（OK trade-off）。

use std::fs;
use std::io::Write;
use std::path::PathBuf;
use anyhow::{anyhow, Context, Result};

/// 内置程序员词表 —— 编译进二进制，~2 KB。
const BUILTIN_PROGRAMMER: &str = include_str!("../resources/vocab/programmer.txt");

/// 默认 hotwords_score —— 没指定 `:score` 后缀的词用这个值。
/// 2.0 是 sherpa-onnx 文档建议的"明显但不过 boost"区间。
pub const DEFAULT_HOTWORDS_SCORE: f32 = 2.0;

/// `~/.mouseclaw/vocab/` 目录
fn vocab_dir() -> Result<PathBuf> {
    let home = std::env::var_os("HOME").ok_or_else(|| anyhow!("HOME not set"))?;
    Ok(PathBuf::from(home).join(".mouseclaw/vocab"))
}

/// `~/.mouseclaw/vocab/user.txt` —— 用户手动编辑
pub fn user_file_path() -> Result<PathBuf> { Ok(vocab_dir()?.join("user.txt")) }

/// `~/.mouseclaw/vocab/active.txt` —— 合并产物，sherpa 实际读这个
pub fn active_file_path() -> Result<PathBuf> { Ok(vocab_dir()?.join("active.txt")) }

/// 首次启动调一次：建 vocab 目录 + 创建带说明的空 user.txt（若不存在）。
pub fn ensure_user_file() -> Result<()> {
    let dir = vocab_dir()?;
    fs::create_dir_all(&dir)?;
    let user = user_file_path()?;
    if user.exists() { return Ok(()); }
    let template = USER_FILE_TEMPLATE;
    fs::write(&user, template).with_context(|| format!("write {}", user.display()))?;
    println!("[mouseclaw] 📝 created empty vocab at {}", user.display());
    Ok(())
}

/// 合并内置 (若开启) + 用户词表 → 写到 active.txt。返回总词条数。
/// 去重、去空行、跳过注释。重复词以最后出现的 score 为准。
pub fn regenerate_active(builtin_enabled: bool) -> Result<usize> {
    let mut entries: Vec<(String, Option<f32>)> = Vec::new();

    if builtin_enabled {
        collect_entries(BUILTIN_PROGRAMMER, &mut entries);
    }
    let user = user_file_path()?;
    if user.exists() {
        let user_text = fs::read_to_string(&user)
            .with_context(|| format!("read {}", user.display()))?;
        collect_entries(&user_text, &mut entries);
    }

    // 去重：相同 word 取最后一次（用户覆盖内置）
    let mut seen = std::collections::HashMap::<String, Option<f32>>::new();
    for (w, s) in entries.into_iter() {
        seen.insert(w, s);
    }
    let count = seen.len();

    let active = active_file_path()?;
    let mut f = fs::File::create(&active)
        .with_context(|| format!("create {}", active.display()))?;
    let mut sorted: Vec<_> = seen.into_iter().collect();
    sorted.sort_by(|a, b| a.0.cmp(&b.0));
    for (word, score) in sorted {
        match score {
            Some(s) => writeln!(f, "{} :{}", word, s)?,
            None => writeln!(f, "{}", word)?,
        }
    }
    Ok(count)
}

/// 解析一段文本里的 hotwords entry，追加到 out。
/// 容错：忽略空行、`#` 注释、无法解析的 score（fallback 到全局 default）。
fn collect_entries(text: &str, out: &mut Vec<(String, Option<f32>)>) {
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') { continue; }
        // 拆 ":<score>" 后缀
        if let Some(idx) = line.rfind(':') {
            let (word, rest) = line.split_at(idx);
            let word = word.trim();
            let score_str = rest.trim_start_matches(':').trim();
            if let Ok(s) = score_str.parse::<f32>() {
                if !word.is_empty() {
                    out.push((word.to_string(), Some(s)));
                    continue;
                }
            }
        }
        // 没冒号 / score 解析失败 → 整行当成 word，用 default score
        out.push((line.to_string(), None));
    }
}

const USER_FILE_TEMPLATE: &str = "\
# MouseClaw · 你的术语表 / Your custom vocabulary
#
# 一行一个词，可选 `:score` 后缀（默认 2.0 · 1.5-3.0 是合理范围）。
# Lines starting with `#` are comments and ignored.
#
# 中文：每个汉字之间用空格分开（sherpa 字级 tokenization）
#   例 / Example:
#   心 房 颤 动 :2.5
#   爱 德 文 :3.0
#
# 英文：直接写整词
#   useEffect :2.5
#   RuVector :2.5
#
# 改完后：MouseClaw 托盘 → 「🔄 刷新术语表」即可生效。
#
# 在下面写你的词：
";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collect_entries_parses_score_suffix() {
        let mut out = Vec::new();
        collect_entries("useEffect :2.5\nTauri\n# comment\n\n", &mut out);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].0, "useEffect");
        assert_eq!(out[0].1, Some(2.5));
        assert_eq!(out[1].0, "Tauri");
        assert_eq!(out[1].1, None);
    }

    #[test]
    fn collect_entries_skips_empty_and_comments() {
        let mut out = Vec::new();
        collect_entries("\n\n# nothing\n   \n", &mut out);
        assert!(out.is_empty());
    }

    #[test]
    fn collect_entries_handles_garbled_score() {
        // ":not-a-number" → 整行当 word
        let mut out = Vec::new();
        collect_entries("foo :bar\n", &mut out);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].0, "foo :bar");
        assert_eq!(out[0].1, None);
    }

    #[test]
    fn builtin_programmer_compiles_and_has_entries() {
        let mut out = Vec::new();
        collect_entries(BUILTIN_PROGRAMMER, &mut out);
        assert!(out.len() >= 40, "builtin should have ≥40 entries, got {}", out.len());
    }

    #[test]
    fn collect_entries_handles_chinese_space_split() {
        // sherpa 双语模型需要中文字符级分词；测合并器原样保留
        let mut out = Vec::new();
        collect_entries("心 房 颤 动 :2.5\n", &mut out);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].0, "心 房 颤 动");
        assert_eq!(out[0].1, Some(2.5));
    }

    #[test]
    fn collect_entries_handles_extra_whitespace() {
        let mut out = Vec::new();
        collect_entries("  useEffect  :  2.0  \n", &mut out);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].0, "useEffect");
        assert_eq!(out[0].1, Some(2.0));
    }

    #[test]
    fn builtin_contains_critical_terms() {
        // 内置词表必须包含的程序员高频词 —— 缺一漏的话 P1 卖点就少了一截
        let critical = ["useEffect", "Tauri", "Claude", "MouseClaw", "TypeScript"];
        for term in &critical {
            assert!(
                BUILTIN_PROGRAMMER.contains(term),
                "builtin missing critical term: {term}"
            );
        }
    }

    #[test]
    fn default_hotwords_score_is_reasonable() {
        // sherpa 文档建议 1.5-3.0；过低没效果，过高过 boost
        assert!(DEFAULT_HOTWORDS_SCORE >= 1.5);
        assert!(DEFAULT_HOTWORDS_SCORE <= 3.0);
    }

    #[test]
    fn user_template_is_bilingual() {
        // template 模板必须中英对照（用户文档）
        assert!(USER_FILE_TEMPLATE.contains("中文"));
        assert!(USER_FILE_TEMPLATE.contains("Example") || USER_FILE_TEMPLATE.contains("example"));
    }
}
