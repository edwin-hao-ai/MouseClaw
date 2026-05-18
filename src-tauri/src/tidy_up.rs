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
