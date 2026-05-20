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

use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;
use anyhow::{anyhow, Context, Result};
use once_cell::sync::Lazy;

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
    // v0.4.1 · 词表变了 → 清大小写还原缓存，下次 recase 用新词表重建。
    // 放这儿覆盖所有重载路径（vocab_reload / set_builtin / 启动），无需改 commands.rs。
    invalidate_casing_map();
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

// ============================================================================
// 英文大小写还原 (v0.4.1) —— 修 "中英混合英文全是大写字母"
// ============================================================================
//
// 背景：sherpa zh-en zipformer 2023-02-20 的英文 BPE token 全是大写（▁OPEN ▁AI…），
// 所以英文必然输出成 `OPENAI` / `OPEN AI` / `OPEN CLAW`。后处理还原成自然大小写：
//   - 词表里的术语 → 用词表的正确大小写（OPENAI→OpenAI, USEEFFECT→useEffect, API→API）
//   - 相邻全大写词尝试合并匹配复合术语（OPEN AI→OpenAI, OPEN CLAW→OpenClaw）
//   - 代词 "I" 保持大写；其余未知英文 → 小写（比 ALLCAPS 自然太多）
//   - 非英文（中文 / 数字 / 标点）原样保留
//
// 词表是单一信源：用户往术语表加词，既 boost 识别（hotwords）又修大小写。

/// 缓存的还原表：casing_key(UPPERCASE 去非字母数字) → 正确大小写词。
static CASING_MAP: Lazy<Mutex<Option<HashMap<String, String>>>> = Lazy::new(|| Mutex::new(None));

/// 归一 key：取 ASCII 字母数字、转大写。"Next.js"→"NEXTJS"，"OpenAI"→"OPENAI"。
fn casing_key(term: &str) -> String {
    term.chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .flat_map(|c| c.to_uppercase())
        .collect()
}

/// 从内置 + 用户词表建还原表。内置永远纳入（大小写还原无害，不像 hotwords 影响解码）。
fn build_casing_map() -> HashMap<String, String> {
    let mut entries: Vec<(String, Option<f32>)> = Vec::new();
    collect_entries(BUILTIN_PROGRAMMER, &mut entries);
    if let Ok(user) = user_file_path() {
        if let Ok(text) = fs::read_to_string(&user) {
            collect_entries(&text, &mut entries);
        }
    }
    let mut map = HashMap::new();
    for (w, _) in entries {
        // 只收"含 ASCII 字母、不含空格"的英文词（中文术语 / 多词短语不参与英文 recase）
        if w.contains(' ') || !w.chars().any(|c| c.is_ascii_alphabetic()) {
            continue;
        }
        let key = casing_key(&w);
        if !key.is_empty() {
            // 用户词表后插 → 覆盖内置（同 regenerate_active 的"用户优先"）
            map.insert(key, w);
        }
    }
    map
}

/// 词表改了 → 清掉缓存，下次 recase 重建（跟 invalidate_recognizer 一起调）。
pub fn invalidate_casing_map() {
    *CASING_MAP.lock().unwrap() = None;
}

/// 把模型输出的全大写英文还原成自然大小写。中文 / 数字 / 标点原样保留。
/// 纯字符串处理，~µs 级，可在流式 partial 热循环里调。
pub fn recase_english(text: &str) -> String {
    {
        let mut g = CASING_MAP.lock().unwrap();
        if g.is_none() {
            *g = Some(build_casing_map());
        }
    }
    let g = CASING_MAP.lock().unwrap();
    recase_with_map(text, g.as_ref().unwrap())
}

enum Tok {
    Word(String),  // 连续 ASCII 字母数字
    Other(String), // 其余（空格 / 中文 / 标点）
}

fn tokenize(text: &str) -> Vec<Tok> {
    let mut toks = Vec::new();
    let mut cur = String::new();
    let mut cur_word = false;
    for c in text.chars() {
        let is_word = c.is_ascii_alphanumeric();
        if cur.is_empty() {
            cur.push(c);
            cur_word = is_word;
        } else if is_word == cur_word {
            cur.push(c);
        } else {
            toks.push(if cur_word { Tok::Word(std::mem::take(&mut cur)) } else { Tok::Other(std::mem::take(&mut cur)) });
            cur.push(c);
            cur_word = is_word;
        }
    }
    if !cur.is_empty() {
        toks.push(if cur_word { Tok::Word(cur) } else { Tok::Other(cur) });
    }
    toks
}

fn recase_single(w: &str, map: &HashMap<String, String>) -> String {
    if let Some(term) = map.get(&casing_key(w)) {
        return term.clone();
    }
    if w == "I" {
        return "I".to_string(); // 代词保持大写
    }
    w.to_lowercase()
}

fn recase_with_map(text: &str, map: &HashMap<String, String>) -> String {
    let toks = tokenize(text);
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < toks.len() {
        match &toks[i] {
            Tok::Other(s) => {
                out.push_str(s);
                i += 1;
            }
            Tok::Word(_) => {
                // 贪心：先试把相邻"单空格分隔的全大写词"合并匹配复合术语（窗口 3→2→1）。
                let mut matched = false;
                for win in (1..=3usize).rev() {
                    let mut words: Vec<&str> = Vec::new();
                    let mut idx = i;
                    let mut ok = true;
                    for k in 0..win {
                        match toks.get(idx) {
                            Some(Tok::Word(w)) => words.push(w),
                            _ => { ok = false; break; }
                        }
                        if k < win - 1 {
                            match toks.get(idx + 1) {
                                Some(Tok::Other(s)) if s == " " => {}
                                _ => { ok = false; break; }
                            }
                            idx += 2;
                        }
                    }
                    if !ok || words.len() != win {
                        continue;
                    }
                    if win == 1 {
                        break; // 单词走下面 recase_single，不在这儿匹配
                    }
                    let key: String = words.iter().flat_map(|w| w.chars()).filter(|c| c.is_ascii_alphanumeric()).flat_map(|c| c.to_uppercase()).collect();
                    if let Some(term) = map.get(&key) {
                        out.push_str(term);
                        i = idx + 1; // 吃掉这些词 + 它们之间的空格
                        matched = true;
                        break;
                    }
                }
                if !matched {
                    if let Tok::Word(w) = &toks[i] {
                        out.push_str(&recase_single(w, map));
                    }
                    i += 1;
                }
            }
        }
    }
    out
}

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

    // ── recase 测试（用临时 map，不依赖磁盘 user.txt）──
    fn test_map() -> HashMap<String, String> {
        let mut m = HashMap::new();
        for t in ["OpenAI", "OpenClaw", "useEffect", "API", "GitHub", "TypeScript", "Next.js"] {
            m.insert(casing_key(t), t.to_string());
        }
        m
    }

    #[test]
    fn recase_maps_known_single_term() {
        assert_eq!(recase_with_map("OPENAI", &test_map()), "OpenAI");
        assert_eq!(recase_with_map("USEEFFECT", &test_map()), "useEffect");
        assert_eq!(recase_with_map("API", &test_map()), "API");
    }

    #[test]
    fn recase_merges_split_compound() {
        // 模型把 OpenAI / OpenClaw 拆成两词输出
        assert_eq!(recase_with_map("OPEN AI", &test_map()), "OpenAI");
        assert_eq!(recase_with_map("OPEN CLAW", &test_map()), "OpenClaw");
    }

    #[test]
    fn recase_lowercases_unknown_keeps_pronoun_I() {
        assert_eq!(recase_with_map("I LOVE CODING", &test_map()), "I love coding");
    }

    #[test]
    fn recase_preserves_chinese_and_punct() {
        // 中文原样，夹的英文术语还原，未知英文转小写
        assert_eq!(
            recase_with_map("帮我用 OPENAI 写一个 HELLO WORLD", &test_map()),
            "帮我用 OpenAI 写一个 hello world"
        );
    }

    #[test]
    fn recase_mixed_term_and_words() {
        assert_eq!(
            recase_with_map("THE USEEFFECT HOOK", &test_map()),
            "the useEffect hook"
        );
    }

    #[test]
    fn recase_dotted_term_via_collapsed_key() {
        // 模型不输出点：NEXT JS → Next.js（key 都归一成 NEXTJS）
        assert_eq!(recase_with_map("NEXT JS", &test_map()), "Next.js");
    }

    #[test]
    fn recase_empty_and_pure_chinese_noop() {
        assert_eq!(recase_with_map("", &test_map()), "");
        assert_eq!(recase_with_map("今天天气不错。", &test_map()), "今天天气不错。");
    }
}
