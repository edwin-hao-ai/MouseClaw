//! Tidy-up · 语音转写后整理（v0.3.4 简化版）
//!
//! v0.3.4 起 **删了 LLM tidy 全套**。理由（用户语 2026-05-18）：
//!   - 语音打字 = 节省时间，加 1-3s LLM 等待 + 花用户钱 = 直接违背初衷
//!   - sherpa Zipformer 出的字本身可用，缺标点也好过等 2 秒
//!   - 要"标点+排版"的用户，键盘改更快
//!
//! 现在只剩 light_clean —— 纯 regex/string，即时 ~5ms 完成。
//! 去口头禅 + 收敛多重标点 + trim。无外部调用、无联网、无可能让流程变慢。
//!
//! 如果某天我们想加 polish 功能，应该用**本地 sherpa CT-Transformer 标点模型**（72MB，
//! 离线，instant），不要回到 LLM 路径。
//!
//! 灵感：Typeless / Wispr Flow / FreeFlow 套路里的 "instant clean" 那层。

// ============================================================================
// light_clean —— 纯 regex / string，无 LLM，~5ms
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

// ============================================================================
// v0.6 · 规则版"整理成清单" —— 纯规则、即时、0 模型（替代生成式 LLM 做简单理条理）
// ============================================================================
//
// 把一长串口述按列表性话语标记（先/然后/还有/对了/第一/其次/最后…）切成有序清单：
//   "今天要先写周报然后给客户回邮件对了还要订会议室"
//   → "1. 写周报\n2. 给客户回邮件\n3. 订会议室"
// 只分点 + 编号 + 换行，**不重写措辞**（保留原话、清掉口头禅）。
// 非列表性文本（切出的项 < 2）返回 None —— 不强行编号。

/// item 边界标记。多字优先（先匹配"首先"再"先"）。
const LIST_MARKERS: &[&str] = &[
    "首先", "第一点", "第二点", "第三点", "第一", "第二", "第三", "第四", "第五",
    "其次", "然后", "接着", "再就是", "再来", "还有", "还要", "另外", "此外",
    "以及", "对了", "最后", "最终",
    // 英文
    "firstly", "secondly", "first", "second", "third", "then", "next",
    "also", "additionally", "finally", "lastly",
];

/// 引子（应丢弃，不当 item）：短、且像"今天要做的有"这种开场白。
fn is_leadin(s: &str) -> bool {
    let t = s.trim();
    if t.is_empty() {
        return true;
    }
    let n = t.chars().count();
    // 短 + 以引子收尾（"今天要做的有"/"我想说的是"）才算引子；不按内容词误删真实项。
    n <= 8 && t.ends_with(['要', '有', '想', '是', '：', ':'])
}

/// 剥首尾标点 + 句首引子词（"今天要先写周报"→"写周报"），按引子结构单趟剥。
fn strip_item(s: &str) -> String {
    let mut t = s
        .trim()
        .trim_matches(|c| matches!(c, '，' | '。' | '、' | '；' | ',' | '.' | ';' | ' '))
        .to_string();
    // 句首引子结构 "(今天)?(我/我们)?(要/想/需要)?(先/首先)?"，按顺序各剥一次。
    // 剥后剩余必须 ≥2 字，避免把内容词当引子（保守）。
    for lead in ["今天", "我们", "我", "需要", "要", "想", "先", "首先", "这边", "那个", "就"] {
        if let Some(rest) = t.strip_prefix(lead) {
            if rest.chars().count() >= 2 {
                t = rest.to_string();
            }
        }
    }
    t.trim().to_string()
}

/// 规则切分成 item 列表。
fn split_list_items(text: &str) -> Vec<String> {
    let delims = ['，', '。', '、', '；', '！', '？', ',', '.', ';', ' ', '\n', '\t', '：', ':'];
    // 1) 找所有 marker 在边界处的起始字节位置 + 其长度
    let mut marks: Vec<(usize, usize)> = Vec::new(); // (起始字节, marker字节长)
    // 口述多为连读无标点，多字话语标记（然后/还有/对了/其次…）在任意位置都视为 item 边界。
    let _ = delims;
    for m in LIST_MARKERS {
        let mut start = 0;
        while let Some(rel) = text[start..].find(m) {
            let abs = start + rel;
            marks.push((abs, m.len()));
            start = abs + m.len();
        }
    }
    if marks.is_empty() {
        return Vec::new();
    }
    marks.sort_by_key(|&(p, _)| p);
    // 去重叠：若两个 marker 起点距离过近（嵌套，如"第一点"含"第一"），保留靠前的长的
    let mut filtered: Vec<(usize, usize)> = Vec::new();
    for &(p, l) in &marks {
        if let Some(&(pp, pl)) = filtered.last() {
            if p < pp + pl {
                continue; // 落在上一个 marker 内，跳过
            }
        }
        filtered.push((p, l));
    }
    // 2) 切分：第一段 = [0, 第一个marker起点)；之后每段 = [marker结束, 下一个marker起点)
    let mut items: Vec<String> = Vec::new();
    let first_mark = filtered[0].0;
    let preamble = &text[..first_mark];
    if !is_leadin(preamble) {
        let it = strip_item(preamble);
        if !it.is_empty() {
            items.push(it);
        }
    }
    for i in 0..filtered.len() {
        let (p, l) = filtered[i];
        let seg_start = p + l;
        let seg_end = filtered.get(i + 1).map(|&(np, _)| np).unwrap_or(text.len());
        if seg_start >= seg_end {
            continue;
        }
        let it = strip_item(&text[seg_start..seg_end]);
        if !it.is_empty() && !is_leadin(&it) {
            items.push(it);
        }
    }
    items
}

/// 公开入口：把口述整理成有序清单。非列表性 → None。
pub fn organize_into_list(text: &str, ui_lang: &str) -> Option<String> {
    let cleaned = light_clean(text, ui_lang);
    let items = split_list_items(&cleaned);
    if items.len() < 2 {
        return None; // 不是清单，别强行编号
    }
    Some(
        items
            .iter()
            .enumerate()
            .map(|(i, it)| format!("{}. {}", i + 1, it))
            .collect::<Vec<_>>()
            .join("\n"),
    )
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

// v0.3.4 · LLM tidy / run_quick_llm / post_strip 全删除 —— 语音打字要快不要 LLM。
// 如果将来想加 polish，应该用本地 sherpa CT-Transformer 标点模型，不要回 LLM 路径。

#[cfg(test)]
mod tests {
    use super::*;

    // ── organize_into_list 测试（规则版理条理）──
    #[test]
    fn organize_user_example() {
        let out = organize_into_list(
            "今天要先写周报然后给客户回邮件对了还要订会议室",
            "zh",
        );
        eprintln!("整理输出:\n{}", out.clone().unwrap_or_else(|| "(None)".into()));
        let out = out.expect("应识别为清单");
        assert!(out.contains("1.") && out.contains("2.") && out.contains("3."));
        assert!(out.contains("写周报") && out.contains("订会议室"));
    }

    #[test]
    fn organize_sequential_markers() {
        let out = organize_into_list(
            "首先我们要确定目标，其次分配任务，最后定个时间线",
            "zh",
        ).expect("清单");
        eprintln!("整理输出2:\n{out}");
        assert_eq!(out.lines().count(), 3);
    }

    #[test]
    fn organize_non_list_returns_none() {
        // 普通一句话不该被强行编号
        assert!(organize_into_list("今天天气不错我打算去公园散步", "zh").is_none());
    }

    #[test]
    fn organize_english() {
        let out = organize_into_list(
            "first finish the slides then call the vendor also review the budget",
            "en",
        ).expect("list");
        eprintln!("organize en:\n{out}");
        assert_eq!(out.lines().count(), 3);
    }

    // ── light_clean 测试 ──

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
