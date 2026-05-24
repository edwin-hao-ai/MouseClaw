//! 同句内自我纠正 (v0.6 · B 方案) —— 不用 LLM，纯规则。
//!
//! ## 跟 voice_correct.rs 的区别
//! `voice_correct.rs` 处理**跨句**纠错：文字已经写进光标了，3 秒内再按一次说
//! "把 X 改成 Y"，靠 backspace 把已写入的改掉。
//!
//! 本模块处理**同一段话内部**的自我纠正 —— 用户在一次按住说话里先说正文、
//! 再用"哦/不对/等等 + 去掉/删掉/改成"纠正自己，松手前文字**还没写出去**，
//! 所以只需在 paste 之前把 transcript 改对，**无 backspace**。
//!
//! 覆盖用户原例：
//!   "请帮我记录一下今天的日记。我今天吃了一顿饭，然后喝了一碗汤。哦，把那个喝汤去掉。"
//!   → "请帮我记录一下今天的日记。我今天吃了一顿饭。"
//!
//! ## 覆盖的尾子句（必须带 marker，见下）
//! | 尾子句                                   | 行为
//! | 哦，把那个X去掉 / 不对，X删掉 / 不要X了   | 删掉正文里匹配 X 的子句
//! | 哦，把X改成Y / 不对，X换成Y               | 正文里 X → Y
//! | 算了重说 / 不对重来                       | 清空（不写）
//! | oh, delete the X / no wait, change X to Y | 同上（英文）
//!
//! ## 为什么必须带 marker（哦/不对/等等/算了…）
//! 不带 marker 无法区分"对输入法的纠正命令"和"正文里恰好出现删除字眼"：
//!   "我跟他说把这个文件删掉" —— 这是正文，绝不能删！
//! 自然语流里的自我纠正几乎总带 repair marker（哦/不对/等等/啊不是），所以要求
//! 尾子句以 marker 开头，能大幅压低误删正文的概率。代价：用户纠正前要带个语气词，
//! 自然且可教。这是 B 方案 precision-first 的核心取舍（用户已接受 B 的局限）。
//!
//! ## 保守原则（destructive edit 宁可不做也不做错）
//! - 只有"正文 + 末尾 marker 门控命令"同时成立才动；否则返回 None，按普通文本写。
//! - 命令识别置信但 X 在正文找不到 → 仍**剥掉命令子句**（绝不把命令字面打出来），
//!   保留正文，气泡提示没找到。删错正文比留个命令尾巴坏得多。

/// 自我纠正语气词 / repair marker。尾子句须以其中之一开头才识别。
/// 注意：按长度降序匹配（先试"不对"再试单字），避免 "不对" 被 "不" 截断。
const ZH_MARKERS: &[&str] = &[
    "等一下", "等下", "不对", "不是", "等等", "算了", "慢着", "稍等",
    "哦", "噢", "喔", "嗷", "啊", "呃", "欸", "诶", "唉",
];

/// 英文 repair marker（小写比较）。
const EN_MARKERS: &[&str] = &[
    "no wait", "scratch that", "hold on", "actually", "wait", "sorry", "oh", "no",
];

/// 删除目标里要剥掉的指示前缀（"那个喝汤" → "喝汤"）。
const TARGET_PREFIXES: &[&str] = &[
    "刚才说的", "刚说的", "刚才那个", "刚才", "那一个", "这一个", "那个", "这个", "那", "这", "说的",
];

/// 子句分隔符（中英）。
const DELIMS: &[char] = &['。', '！', '？', '，', '、', '；', '.', '!', '?', ',', ';'];

/// 同句内纠正结果。
#[derive(Debug, PartialEq, Eq)]
pub struct InlineResult {
    /// 纠正后要写入的正文。空串 = "重说"，调用方据此不写入。
    pub text: String,
    /// 气泡提示（中文）。
    pub note_zh: String,
    /// 气泡提示（英文）。
    pub note_en: String,
}

#[derive(Debug, Clone)]
struct Seg {
    text: String,
    delim: char, // '\0' = 末段无分隔符
}

#[derive(Debug, PartialEq, Eq)]
enum Command {
    Delete { target: String },
    Replace { old: String, new: String },
    UndoAll,
}

/// 主入口：尝试把 transcript 解析为"正文 + 末尾自我纠正"。
/// 返回 None = 没有可信的同句纠正，调用方按普通文本写入（并仍可走跨句 voice_correct）。
pub fn try_parse_inline(transcript: &str) -> Option<InlineResult> {
    let t = transcript.trim();
    if t.is_empty() {
        return None;
    }
    let segs = split_segments(t);
    if segs.len() < 2 {
        return None; // 需要至少 body + 命令两段
    }

    // 末段视为命令候选。先试本段内联 marker，再试"前一段是纯 marker"。
    let last_idx = segs.len() - 1;
    let last_text = &segs[last_idx].text;

    // 情况 A：命令段自带内联 marker（"哦把那个喝汤去掉" / "oh delete the soup"）
    if let Some(stripped) = strip_leading_marker(last_text) {
        if let Some(cmd) = parse_command(&stripped) {
            let body = rejoin(&segs[..last_idx]);
            if !body.trim().is_empty() {
                return Some(apply(&body, cmd));
            }
        }
    }

    // 情况 B：前一段是纯 marker（"…。哦，把那个喝汤去掉。" → ["…","哦","把那个喝汤去掉"]）
    if last_idx >= 1 && is_pure_marker(&segs[last_idx - 1].text) {
        if let Some(cmd) = parse_command(last_text) {
            let body = rejoin(&segs[..last_idx - 1]);
            if !body.trim().is_empty() {
                return Some(apply(&body, cmd));
            }
        }
    }

    None
}

/// 把命令应用到正文，产出 InlineResult。
fn apply(body: &str, cmd: Command) -> InlineResult {
    match cmd {
        Command::UndoAll => InlineResult {
            text: String::new(),
            note_zh: "↶ 已重说".into(),
            note_en: "↶ Started over".into(),
        },
        Command::Delete { target } => {
            let clean = clean_target(&target);
            match apply_delete(body, &clean) {
                Some(new_body) => InlineResult {
                    text: new_body,
                    note_zh: format!("✂️ 已去掉「{clean}」"),
                    note_en: format!("✂️ Removed “{clean}”"),
                },
                None => InlineResult {
                    // 命令置信但没找到目标 → 剥命令、保正文
                    text: normalize_trailing(body.to_string()),
                    note_zh: format!("⚠️ 没找到「{clean}」，已保留原文", clean = clean),
                    note_en: format!("⚠️ Couldn't find “{clean}”, kept text"),
                },
            }
        }
        Command::Replace { old, new } => match apply_replace(body, &old, &new) {
            Some(new_body) => InlineResult {
                text: new_body,
                note_zh: format!("🔄 已替换「{old} → {new}」"),
                note_en: format!("🔄 Replaced “{old} → {new}”"),
            },
            None => InlineResult {
                text: normalize_trailing(body.to_string()),
                note_zh: format!("⚠️ 没找到「{old}」，已保留原文"),
                note_en: format!("⚠️ Couldn't find “{old}”, kept text"),
            },
        },
    }
}

/// 切成 (text, delim) 列表，丢掉纯标点空段。
fn split_segments(s: &str) -> Vec<Seg> {
    let mut out = Vec::new();
    let mut cur = String::new();
    for ch in s.chars() {
        if DELIMS.contains(&ch) {
            let text = cur.trim().to_string();
            if !text.is_empty() {
                out.push(Seg { text, delim: ch });
            }
            cur.clear();
        } else {
            cur.push(ch);
        }
    }
    let tail = cur.trim().to_string();
    if !tail.is_empty() {
        out.push(Seg { text: tail, delim: '\0' });
    }
    out
}

/// 把 seg 列表拼回字符串，规范化结尾标点。英文标点后补空格，中文标点不补。
fn rejoin(segs: &[Seg]) -> String {
    let mut s = String::new();
    for (i, seg) in segs.iter().enumerate() {
        s.push_str(&seg.text);
        if seg.delim != '\0' {
            s.push(seg.delim);
            // 英文标点（, . ; 等 ASCII）后补一个空格还原英文排版；CJK 标点不补。
            if seg.delim.is_ascii_punctuation() && i + 1 < segs.len() {
                s.push(' ');
            }
        }
    }
    normalize_trailing(s)
}

/// 结尾若是逗号类 → 换成句号（中英按上下文选 。/.）；首尾空白 trim。
fn normalize_trailing(s: String) -> String {
    let mut t = s.trim_end().to_string();
    if let Some(last) = t.chars().last() {
        if matches!(last, '，' | ',' | '、' | '；' | ';') {
            t.pop();
            while t.ends_with(' ') {
                t.pop();
            }
            // 末尾内容是 ASCII（英文/数字）→ 用英文句号，否则中文句号。
            let period = if t.chars().last().is_some_and(|c| c.is_ascii_alphanumeric()) {
                '.'
            } else {
                '。'
            };
            t.push(period);
        }
    }
    t.trim().to_string()
}

/// s 以某个 marker 开头则剥掉（连同其后的逗号/空格），返回剩余；否则 None。
fn strip_leading_marker(s: &str) -> Option<String> {
    let t = s.trim_start();
    for m in ZH_MARKERS {
        if let Some(rest) = t.strip_prefix(m) {
            let rest = rest.trim_start_matches(|c| matches!(c, '，' | ',' | ' ' | '、'));
            return Some(rest.to_string());
        }
    }
    let lower = t.to_lowercase();
    for m in EN_MARKERS {
        if let Some(rest) = lower.strip_prefix(m) {
            // 用原文 case 还原（marker 是小写匹配，但剩余要保留原大小写）
            let consumed = t.len() - rest.len();
            let rest_orig = &t[consumed..];
            let rest_orig = rest_orig.trim_start_matches(|c| matches!(c, ',' | ' '));
            return Some(rest_orig.to_string());
        }
    }
    None
}

/// 整段是否就是一个纯 marker（"哦" / "不对" / "oh"）。
fn is_pure_marker(s: &str) -> bool {
    let t = s.trim();
    if ZH_MARKERS.contains(&t) {
        return true;
    }
    let lower = t.to_lowercase();
    EN_MARKERS.contains(&lower.as_str())
}

/// 把一段命令文本解析成 Command。已假定 marker 已剥。
fn parse_command(s: &str) -> Option<Command> {
    let t = s.trim();
    if t.is_empty() {
        return None;
    }
    // 撤销
    if is_undo_all(t) {
        return Some(Command::UndoAll);
    }
    // 替换（中英）
    if let Some((old, new)) = parse_replace(t) {
        return Some(Command::Replace { old, new });
    }
    // 删除（中英）
    if let Some(target) = parse_delete(t) {
        return Some(Command::Delete { target });
    }
    None
}

fn is_undo_all(s: &str) -> bool {
    const PATTERNS: &[&str] = &[
        "重说", "重新说", "重来", "重新来", "全部删掉", "全删了", "都删掉", "都不要了",
        "redo all", "delete all", "scratch all", "start over",
    ];
    let lower = s.to_lowercase();
    PATTERNS.iter().any(|p| s == *p || lower == *p)
}

/// "把 X 改成 Y" / "X 换成 Y" / "change X to Y" / "replace X with Y"
fn parse_replace(s: &str) -> Option<(String, String)> {
    let trimmed = s.trim();
    let no_ba = trimmed
        .strip_prefix("把")
        .or_else(|| trimmed.strip_prefix("将"))
        .unwrap_or(trimmed);
    for sep in ["改成", "改为", "换成", "换为"] {
        if let Some(idx) = no_ba.find(sep) {
            let x = no_ba[..idx].trim();
            let y = no_ba[idx + sep.len()..].trim();
            // 去掉 y 尾部的语气词
            let y = y.trim_end_matches(|c| matches!(c, '吧' | '了' | '啊' | '呀'));
            if !x.is_empty() && !y.is_empty() {
                return Some((x.to_string(), y.to_string()));
            }
        }
    }
    let lower = trimmed.to_lowercase();
    if let Some(rest) = lower.strip_prefix("change ") {
        if let Some(idx) = rest.find(" to ") {
            let x_start = "change ".len();
            let x_end = x_start + idx;
            let y_start = x_end + 4;
            if x_end <= trimmed.len() && y_start <= trimmed.len() {
                let x = trimmed[x_start..x_end].trim().to_string();
                let y = trimmed[y_start..].trim().to_string();
                if !x.is_empty() && !y.is_empty() {
                    return Some((x, y));
                }
            }
        }
    }
    if let Some(rest) = lower.strip_prefix("replace ") {
        if let Some(idx) = rest.find(" with ") {
            let x_start = "replace ".len();
            let x_end = x_start + idx;
            let y_start = x_end + 6;
            if x_end <= trimmed.len() && y_start <= trimmed.len() {
                let x = trimmed[x_start..x_end].trim().to_string();
                let y = trimmed[y_start..].trim().to_string();
                if !x.is_empty() && !y.is_empty() {
                    return Some((x, y));
                }
            }
        }
    }
    None
}

/// "把 X 去掉/删掉/删了" / "X 去掉" / "不要 X 了" / "去掉 X" / 英文 delete/remove X
fn parse_delete(s: &str) -> Option<String> {
    let t = s.trim();
    // 中文：尾部删除动词
    const ZH_DEL_VERBS: &[&str] = &["去掉", "删掉", "删了", "删除", "拿掉", "去了", "不要了"];
    let body = t.strip_prefix("把").or_else(|| t.strip_prefix("将")).unwrap_or(t);
    for v in ZH_DEL_VERBS {
        if let Some(target) = body.strip_suffix(v) {
            let target = target.trim();
            if !target.is_empty() {
                return Some(target.to_string());
            }
        }
    }
    // 中文："不要 X 了" / "不要 X"
    if let Some(rest) = body.strip_prefix("不要") {
        let rest = rest.trim().trim_end_matches('了').trim();
        if !rest.is_empty() {
            return Some(rest.to_string());
        }
    }
    // 中文：动词在前 "去掉 X" / "删掉 X"
    for v in ["去掉", "删掉", "删除", "去除"] {
        if let Some(rest) = body.strip_prefix(v) {
            let rest = rest.trim();
            if !rest.is_empty() {
                return Some(rest.to_string());
            }
        }
    }
    // 英文：delete/remove/take out X [part]
    let lower = t.to_lowercase();
    for v in ["delete ", "remove ", "take out ", "scratch "] {
        if let Some(rest) = lower.strip_prefix(v) {
            let consumed = t.len() - rest.len();
            let mut orig = t[consumed..].trim().to_string();
            // 去掉 "the ... part" 包装
            if let Some(r) = orig.to_lowercase().strip_prefix("the ") {
                orig = orig[orig.len() - r.len()..].to_string();
            }
            let orig = orig
                .trim_end_matches(|c: char| c == '.' || c.is_whitespace())
                .trim_end_matches(" part")
                .trim()
                .to_string();
            if !orig.is_empty() {
                return Some(orig);
            }
        }
    }
    None
}

/// 清理删除目标：剥指示前缀（那个/这个…）+ 尾部语气助词。
fn clean_target(target: &str) -> String {
    let mut t = target.trim();
    for p in TARGET_PREFIXES {
        if let Some(rest) = t.strip_prefix(p) {
            t = rest.trim();
            break;
        }
    }
    t.trim_end_matches(|c| matches!(c, '了' | '的' | '吧' | '啊' | '呀'))
        .trim()
        .to_string()
}

/// 在正文里删掉与 target 最匹配的子句（key 字符按子序列匹配），取**最后**一处。
fn apply_delete(body: &str, target: &str) -> Option<String> {
    let key: Vec<char> = target
        .chars()
        .filter(|c| !matches!(c, '的' | '了' | ' ' | '　'))
        .collect();
    if key.is_empty() {
        return None;
    }
    let segs = split_segments(body);
    if segs.is_empty() {
        return None;
    }
    let mut hit: Option<usize> = None;
    for (i, seg) in segs.iter().enumerate() {
        if contains_subseq(&seg.text, &key) {
            hit = Some(i); // 取最后一处匹配
        }
    }
    let idx = hit?;
    let kept: Vec<Seg> = segs
        .into_iter()
        .enumerate()
        .filter(|(i, _)| *i != idx)
        .map(|(_, s)| s)
        .collect();
    Some(rejoin(&kept))
}

/// haystack 是否按顺序包含 key 全部字符（子序列）。
fn contains_subseq(haystack: &str, key: &[char]) -> bool {
    let mut ki = 0;
    if key.is_empty() {
        return false;
    }
    for c in haystack.chars() {
        if c == key[ki] {
            ki += 1;
            if ki == key.len() {
                return true;
            }
        }
    }
    false
}

/// 正文里把**最后一处** old 替换成 new。
fn apply_replace(body: &str, old: &str, new: &str) -> Option<String> {
    let pos = body.rfind(old)?;
    let mut s = String::with_capacity(body.len() + new.len());
    s.push_str(&body[..pos]);
    s.push_str(new);
    s.push_str(&body[pos + old.len()..]);
    Some(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(t: &str) -> String {
        try_parse_inline(t).map(|r| r.text).unwrap_or_else(|| t.to_string())
    }

    // ── 用户原例 ──
    #[test]
    fn user_example_remove_soup() {
        let r = try_parse_inline(
            "请帮我记录一下今天的日记。我今天吃了一顿饭，然后喝了一碗汤。哦，把那个喝汤去掉。",
        )
        .expect("should detect inline correction");
        assert_eq!(r.text, "请帮我记录一下今天的日记。我今天吃了一顿饭。");
    }

    #[test]
    fn delete_with_marker_buhdui() {
        assert_eq!(
            text("我买了苹果，香蕉，橘子。不对，把橘子去掉。"),
            "我买了苹果，香蕉。"
        );
    }

    #[test]
    fn delete_inline_marker_no_comma() {
        // marker 直接贴命令、无逗号
        assert_eq!(
            text("今天开会，明天放假。哦把放假删掉。"),
            "今天开会。"
        );
    }

    #[test]
    fn delete_buyao_le() {
        assert_eq!(
            text("行程是北京，上海，广州。等等，上海不要了。"),
            "行程是北京，广州。"
        );
    }

    // ── 替换 ──
    #[test]
    fn replace_with_marker() {
        assert_eq!(
            text("参会的有张三和李四。不对，把张三改成王五。"),
            "参会的有王五和李四。"
        );
    }

    #[test]
    fn replace_strips_trailing_particle() {
        assert_eq!(
            text("会议在三楼。哦，三楼换成五楼吧。"),
            "会议在五楼。"
        );
    }

    // ── 撤销 ──
    #[test]
    fn undo_all_returns_empty() {
        let r = try_parse_inline("随便说点什么。算了，重说。").expect("undo");
        assert_eq!(r.text, "");
    }

    // ── 英文 ──
    #[test]
    fn english_delete() {
        assert_eq!(
            text("Buy milk, eggs, and bread. Oh, delete the bread."),
            "Buy milk, eggs."
        );
    }

    #[test]
    fn english_replace() {
        assert_eq!(
            text("The meeting is on Monday. No wait, change Monday to Friday."),
            "The meeting is on Friday."
        );
    }

    // ── 必须有 marker（精度门控）──
    #[test]
    fn no_marker_does_not_fire() {
        // "把这个文件删掉" 是正文，没有 marker → 不动
        assert!(try_parse_inline("我跟他说把这个文件删掉。").is_none());
    }

    #[test]
    fn no_marker_delete_clause_kept_as_content() {
        let t = "记得把垃圾倒掉，把碗洗掉。";
        assert!(try_parse_inline(t).is_none());
    }

    // ── 纯命令（无正文）→ 交给跨句 voice_correct，本模块不接 ──
    #[test]
    fn pure_command_no_body_defers() {
        assert!(try_parse_inline("把张三改成李四").is_none());
        assert!(try_parse_inline("不对，把张三改成李四").is_none());
    }

    #[test]
    fn single_segment_defers() {
        assert!(try_parse_inline("重说").is_none());
    }

    // ── 命令置信但目标找不到 → 剥命令、保正文 ──
    #[test]
    fn target_not_found_keeps_body_strips_command() {
        let r = try_parse_inline("今天天气不错。哦，把下雨去掉。").expect("fires");
        assert_eq!(r.text, "今天天气不错。");
        assert!(r.note_zh.contains("没找到"));
    }

    // ── 辅助函数单测 ──
    #[test]
    fn clean_target_strips_demonstrative() {
        assert_eq!(clean_target("那个喝汤"), "喝汤");
        assert_eq!(clean_target("这个张三"), "张三");
        assert_eq!(clean_target("喝汤了"), "喝汤");
    }

    #[test]
    fn contains_subseq_works() {
        let key: Vec<char> = "喝汤".chars().collect();
        assert!(contains_subseq("然后喝了一碗汤", &key));
        assert!(!contains_subseq("吃了一顿饭", &key));
    }

    #[test]
    fn normalize_trailing_comma_to_period() {
        assert_eq!(normalize_trailing("我今天吃了一顿饭，".into()), "我今天吃了一顿饭。");
        assert_eq!(normalize_trailing("已经是句号。".into()), "已经是句号。");
    }

    #[test]
    fn parse_delete_variants() {
        assert_eq!(parse_delete("把喝汤去掉"), Some("喝汤".into()));
        assert_eq!(parse_delete("喝汤删掉"), Some("喝汤".into()));
        assert_eq!(parse_delete("不要喝汤了"), Some("喝汤".into()));
        assert_eq!(parse_delete("去掉喝汤"), Some("喝汤".into()));
    }

    #[test]
    fn strip_leading_marker_zh_en() {
        assert_eq!(strip_leading_marker("哦，把X去掉").as_deref(), Some("把X去掉"));
        assert_eq!(strip_leading_marker("不对，改成Y").as_deref(), Some("改成Y"));
        assert_eq!(strip_leading_marker("oh, delete X").as_deref(), Some("delete X"));
        assert_eq!(strip_leading_marker("把X去掉"), None);
    }

    #[test]
    fn empty_and_whitespace_safe() {
        assert!(try_parse_inline("").is_none());
        assert!(try_parse_inline("   ").is_none());
        assert!(try_parse_inline("就一句普通的话没有纠正。").is_none());
    }
}
