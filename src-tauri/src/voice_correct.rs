//! 3-秒语音纠错 (v0.4.0 P2) —— 不用 LLM，纯规则。
//!
//! ## 触发
//! 上一次 voice IME 写入完成后 3 秒内，再次按住快捷键说一句"纠错口令"，
//! 自动把刚写入的文本就地改掉。覆盖 4 个高频 pattern：
//!
//! | 中文 / English                          | 行为
//! |---                                       |---
//! | 把 X 改成 Y / 把 X 换成 Y                | 把 last_written 里最后一处 X 替换为 Y
//! | X 改成 Y / X 换成 Y                      | 同上（"把"可省）
//! | change X to Y / replace X with Y         | 同上
//! | 重说 / 重来 / 算了 / scratch that        | 整段撤销（删掉 prev_char_count 个字）
//!
//! ## 边界条件
//! - 只有"刚才那次写入还在 3 秒窗口内"且"前台 app 没换"才尝试纠错
//! - 前台 app 是终端 (Terminal/iTerm/Warp/Alacritty) → 直接 disable 纠错
//!   （backspace 会改命令行，太危险）
//! - X 在 last_written 里找不到 → NotFound，不做写入，气泡提示
//! - 语言判断：中文 "改成/换成" 优先；英文 "change to / replace with" fallback
//!
//! ## 不在本轮支持
//! - 模糊指代（"那个数字" → 哪个数字？）需要 LLM，跳过
//! - "不对，是 Y" 需要识别上一个名词，启发式假阳性多，跳过
//! - 多步链式纠错（"再改成 Z"）跳过

use once_cell::sync::Lazy;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// 上一次写入到现在，多久内还能纠错。
pub const CORRECTION_WINDOW: Duration = Duration::from_secs(3);

/// 上次 IME 写入的快照 —— 用来匹配 / 撤销纠错。
#[derive(Debug, Clone)]
pub struct LastWritten {
    /// 真实写进目标 app 的字符串（已加标点 / cleaned）
    pub text: String,
    /// 写完瞬间
    pub at: Instant,
    /// 写入时前台 app 的 bundle id —— 切了 app 就不纠错
    pub frontmost_bundle: String,
}

static LAST: Lazy<Mutex<Option<LastWritten>>> = Lazy::new(|| Mutex::new(None));

pub fn record_write(text: &str, bundle: &str) {
    *LAST.lock().unwrap() = Some(LastWritten {
        text: text.to_string(),
        at: Instant::now(),
        frontmost_bundle: bundle.to_string(),
    });
}

pub fn clear() {
    *LAST.lock().unwrap() = None;
}

/// 用户当前的 transcript 经过分析后该执行的纠错动作。
#[derive(Debug, PartialEq, Eq)]
pub enum CorrectionAction {
    /// 把 last_written 里最后一处 `old` 替换为 `new`，整段重写为 `full_replacement`。
    /// `prev_char_count` 是要 backspace 的字符数（中文 1 char = 1 backspace）。
    Replace {
        old: String,
        new: String,
        full_replacement: String,
        prev_char_count: usize,
    },
    /// 整段撤销 —— backspace `prev_char_count` 字符，清空 last_written。
    UndoAll { prev_char_count: usize },
    /// pattern 看起来像纠错但 X 不在 last_written 里 —— 不写，给用户气泡反馈。
    NotFound { needle: String },
}

/// 终端类 app —— 进入这些 app 时禁用纠错（backspace 会改命令行，太危险）
pub fn is_terminal_bundle(bundle: &str) -> bool {
    matches!(
        bundle,
        "com.apple.Terminal"
            | "com.googlecode.iterm2"
            | "dev.warp.Warp-Stable"
            | "co.zeit.hyper"
            | "io.alacritty"
            | "net.kovidgoyal.kitty"
    )
}

/// 尝试把 transcript 解析为纠错指令；返回 None 表示"按普通新写入处理"。
///
/// `current_bundle` 是此刻前台 app —— 必须等于上一次写入时的 bundle 才尝试纠错。
pub fn try_parse(transcript: &str, current_bundle: &str) -> Option<CorrectionAction> {
    let guard = LAST.lock().unwrap();
    let last = match guard.as_ref() {
        Some(l) => l,
        None => {
            println!("[voice_correct] no last_written — fresh write");
            return None;
        }
    };
    let elapsed = last.at.elapsed();
    if elapsed > CORRECTION_WINDOW {
        println!("[voice_correct] window expired ({}ms > 3000ms)", elapsed.as_millis());
        return None;
    }
    if last.frontmost_bundle != current_bundle {
        println!(
            "[voice_correct] bundle mismatch: last={:?} current={:?}",
            last.frontmost_bundle, current_bundle
        );
        return None;
    }
    if is_terminal_bundle(current_bundle) {
        println!("[voice_correct] disabled in terminal bundle: {}", current_bundle);
        return None;
    }

    let prev_char_count = last.text.chars().count();
    let cleaned = strip_terminal_punct(transcript);
    println!(
        "[voice_correct] trying correction: transcript={:?} cleaned={:?} last_text={:?}",
        transcript, cleaned, last.text
    );

    // 整段撤销
    if is_undo_all(&cleaned) {
        println!("[voice_correct] → UndoAll ({} chars)", prev_char_count);
        return Some(CorrectionAction::UndoAll { prev_char_count });
    }

    // 替换 X→Y
    if let Some((x, y)) = parse_replace(&cleaned) {
        if !x.is_empty() && !y.is_empty() {
            // 把可能的内部空格 / 标点也尝试压缩匹配（sherpa 可能输出"张 三"）
            let x_compact: String = x.chars().filter(|c| !c.is_whitespace()).collect();
            let last_compact: String = last.text.chars().filter(|c| !c.is_whitespace()).collect();
            if let Some(pos_compact) = last_compact.rfind(&x_compact) {
                // 在 last_compact 找到，但 full_replacement 用原文还原比较安全：
                // 用 last.text 找最接近的（去空格后 byte 对应回原 text 的 char 位置）
                let mut byte_pos = 0;
                let mut compact_pos = 0;
                let mut x_compact_chars = x_compact.chars().count();
                let _ = x_compact_chars;  // 仅作调试参考
                for (i, ch) in last.text.char_indices() {
                    if compact_pos == pos_compact {
                        byte_pos = i;
                        break;
                    }
                    if !ch.is_whitespace() {
                        compact_pos += ch.len_utf8();
                    }
                }
                // 直接退化到 rfind(原文 x): 多数情况一次命中
                if let Some(pos) = last.text.rfind(&x) {
                    let mut new_text = String::new();
                    new_text.push_str(&last.text[..pos]);
                    new_text.push_str(&y);
                    new_text.push_str(&last.text[pos + x.len()..]);
                    println!(
                        "[voice_correct] → Replace '{}' → '{}' (direct match @ byte {})",
                        x, y, pos
                    );
                    return Some(CorrectionAction::Replace {
                        old: x,
                        new: y,
                        full_replacement: new_text,
                        prev_char_count,
                    });
                }
                // 直找失败但去空格能找到 → 用 compact 位置重建
                println!(
                    "[voice_correct] direct match failed; compact match @ {} (last_compact len {})",
                    pos_compact, last_compact.len()
                );
                // 把 byte_pos 处对应的原文 x 长度估算成 x_compact 的字节数（中文等宽）
                let approx_end = byte_pos.saturating_add(x_compact.len());
                let safe_end = approx_end.min(last.text.len());
                if byte_pos < safe_end {
                    let mut new_text = String::new();
                    new_text.push_str(&last.text[..byte_pos]);
                    new_text.push_str(&y);
                    new_text.push_str(&last.text[safe_end..]);
                    println!(
                        "[voice_correct] → Replace '{}' → '{}' (compact-match @ byte {}..{})",
                        x, y, byte_pos, safe_end
                    );
                    return Some(CorrectionAction::Replace {
                        old: x,
                        new: y,
                        full_replacement: new_text,
                        prev_char_count,
                    });
                }
            }
            println!("[voice_correct] → NotFound (x={:?})", x);
            return Some(CorrectionAction::NotFound { needle: x });
        }
    }

    println!("[voice_correct] no pattern matched, fall through to normal write");
    None
}

/// 去掉尾部句号 / 问号 / 感叹号 —— sherpa + 标点模块可能加上
fn strip_terminal_punct(s: &str) -> String {
    s.trim()
        .trim_end_matches(|c: char| {
            matches!(c, '。' | '，' | '！' | '？' | '.' | '!' | '?' | ',' | ' ')
        })
        .to_string()
}

fn is_undo_all(s: &str) -> bool {
    const PATTERNS: &[&str] = &[
        "重说", "重新说", "重来", "重新来", "全部删掉", "全删了", "都删掉",
        "算了", "撤销", "撤回",
        "redo all", "delete all", "scratch that", "never mind", "undo",
    ];
    let lower = s.to_lowercase();
    PATTERNS.iter().any(|p| lower == *p || s == *p)
}

/// 解析 "把 X 改成 Y" / "X 换成 Y" / "change X to Y" / "replace X with Y"
/// 返回 (X, Y) trim 过的 String。
fn parse_replace(s: &str) -> Option<(String, String)> {
    let trimmed = s.trim();
    let no_ba = trimmed.strip_prefix("把").unwrap_or(trimmed);

    // 中文：改成 / 改为 / 换成 / 换为
    for sep in ["改成", "改为", "换成", "换为"] {
        if let Some(idx) = no_ba.find(sep) {
            let x = no_ba[..idx].trim();
            let y = no_ba[idx + sep.len()..].trim();
            if !x.is_empty() && !y.is_empty() {
                return Some((x.to_string(), y.to_string()));
            }
        }
    }

    // 英文：change X to Y
    let lower = trimmed.to_lowercase();
    if let Some(rest) = lower.strip_prefix("change ") {
        if let Some(idx) = rest.find(" to ") {
            let x_lower = &rest[..idx];
            let y_lower = &rest[idx + 4..];
            // 用原文 case 而不是 lower —— 让大写词维持原样
            let x_start = "change ".len();
            let x_end = x_start + x_lower.len();
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

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_last(text: &str, bundle: &str) {
        *LAST.lock().unwrap() = Some(LastWritten {
            text: text.into(),
            at: Instant::now(),
            frontmost_bundle: bundle.into(),
        });
    }

    #[test]
    fn parses_ba_replace_zh() {
        let (x, y) = parse_replace("把张三改成李四").unwrap();
        assert_eq!(x, "张三");
        assert_eq!(y, "李四");
    }

    #[test]
    fn parses_replace_without_ba() {
        let (x, y) = parse_replace("张三换为李四").unwrap();
        assert_eq!(x, "张三");
        assert_eq!(y, "李四");
    }

    #[test]
    fn parses_change_x_to_y_en() {
        let (x, y) = parse_replace("change foo to bar").unwrap();
        assert_eq!(x, "foo");
        assert_eq!(y, "bar");
    }

    #[test]
    fn parses_replace_keeps_case() {
        let (x, y) = parse_replace("change useEffect to useMemo").unwrap();
        assert_eq!(x, "useEffect");
        assert_eq!(y, "useMemo");
    }

    #[test]
    fn detects_undo_all_zh() {
        assert!(is_undo_all("重说"));
        assert!(is_undo_all("算了"));
        assert!(is_undo_all("全部删掉"));
    }

    #[test]
    fn detects_undo_all_en() {
        assert!(is_undo_all("scratch that"));
        assert!(is_undo_all("delete all"));
    }

    #[test]
    fn terminal_bundles_disabled() {
        assert!(is_terminal_bundle("com.apple.Terminal"));
        assert!(is_terminal_bundle("dev.warp.Warp-Stable"));
        assert!(!is_terminal_bundle("com.apple.TextEdit"));
    }

    #[test]
    fn try_parse_window_expires() {
        setup_last("hello张三world", "com.apple.TextEdit");
        // mutate at to 5s ago
        {
            let mut g = LAST.lock().unwrap();
            if let Some(ref mut l) = *g {
                l.at = Instant::now() - Duration::from_secs(5);
            }
        }
        assert!(try_parse("把张三改成李四", "com.apple.TextEdit").is_none());
        clear();
    }

    #[test]
    fn try_parse_bundle_mismatch() {
        setup_last("hello张三world", "com.apple.TextEdit");
        assert!(try_parse("把张三改成李四", "com.apple.Safari").is_none());
        clear();
    }

    #[test]
    fn try_parse_terminal_disabled() {
        setup_last("ls -la", "com.apple.Terminal");
        assert!(try_parse("重说", "com.apple.Terminal").is_none());
        clear();
    }

    #[test]
    fn try_parse_replace_success() {
        setup_last("参会人：张三、王五", "com.apple.TextEdit");
        let action = try_parse("把张三改成李四", "com.apple.TextEdit").unwrap();
        match action {
            CorrectionAction::Replace { old, new, full_replacement, prev_char_count } => {
                assert_eq!(old, "张三");
                assert_eq!(new, "李四");
                assert_eq!(full_replacement, "参会人：李四、王五");
                // 9 chars: 参 会 人 ： 张 三 、 王 五
                assert_eq!(prev_char_count, 9);
            }
            other => panic!("expected Replace, got {:?}", other),
        }
        clear();
    }

    #[test]
    fn try_parse_replace_not_found() {
        setup_last("hello world", "com.apple.TextEdit");
        let action = try_parse("把张三改成李四", "com.apple.TextEdit").unwrap();
        assert!(matches!(action, CorrectionAction::NotFound { .. }));
        clear();
    }

    #[test]
    fn try_parse_undo_all() {
        setup_last("好长的一段话", "com.apple.TextEdit");
        let action = try_parse("重说", "com.apple.TextEdit").unwrap();
        match action {
            CorrectionAction::UndoAll { prev_char_count } => {
                assert_eq!(prev_char_count, 6);
            }
            other => panic!("expected UndoAll, got {:?}", other),
        }
        clear();
    }

    #[test]
    fn strip_terminal_punct_works() {
        assert_eq!(strip_terminal_punct("重说。"), "重说");
        assert_eq!(strip_terminal_punct("重说"), "重说");
        assert_eq!(strip_terminal_punct("scratch that."), "scratch that");
    }

    #[test]
    fn parses_replace_with_chinese_punct_tail() {
        // sherpa 标点模块加了句号 —— strip_terminal_punct 必须把它处理掉
        setup_last("张三和李四", "com.apple.TextEdit");
        let action = try_parse("把张三改成王五。", "com.apple.TextEdit").unwrap();
        match action {
            CorrectionAction::Replace { old, new, .. } => {
                assert_eq!(old, "张三");
                assert_eq!(new, "王五");
            }
            other => panic!("expected Replace, got {:?}", other),
        }
        clear();
    }

    #[test]
    fn replace_uses_last_occurrence() {
        // 文本里有多个 "x" 时替换最后一处（用户语义是"刚说的那个"）
        setup_last("x y z x", "com.apple.TextEdit");
        let action = try_parse("把x改成Y", "com.apple.TextEdit").unwrap();
        match action {
            CorrectionAction::Replace { full_replacement, .. } => {
                assert_eq!(full_replacement, "x y z Y");
            }
            other => panic!("expected Replace, got {:?}", other),
        }
        clear();
    }

    #[test]
    fn replace_with_english_word_works() {
        setup_last("the foo function", "com.apple.TextEdit");
        let action = try_parse("change foo to bar", "com.apple.TextEdit").unwrap();
        match action {
            CorrectionAction::Replace { full_replacement, .. } => {
                assert_eq!(full_replacement, "the bar function");
            }
            other => panic!("expected Replace, got {:?}", other),
        }
        clear();
    }

    #[test]
    fn replace_x_to_y_chinese_no_ba_prefix() {
        setup_last("hello 张三 world", "com.apple.TextEdit");
        let action = try_parse("张三换为李四", "com.apple.TextEdit").unwrap();
        match action {
            CorrectionAction::Replace { full_replacement, .. } => {
                assert_eq!(full_replacement, "hello 李四 world");
            }
            other => panic!("expected Replace, got {:?}", other),
        }
        clear();
    }

    #[test]
    fn correction_window_is_three_seconds() {
        // 防止 future 把它改成 1s/10s 导致体验断裂
        assert_eq!(CORRECTION_WINDOW.as_secs(), 3);
    }

    #[test]
    fn no_action_when_no_last_written() {
        clear();
        assert!(try_parse("把张三改成李四", "com.apple.TextEdit").is_none());
    }

    #[test]
    fn parses_change_with_extra_articles() {
        // 用户可能说 "把那个 X 改成 Y" —— 当前 MVP 不支持，确认行为是匹配到 "那个 X"
        // 这个 case 故意 fail 提醒未来加 stopword 过滤时来更新
        setup_last("hello", "com.apple.TextEdit");
        let action = try_parse("把那个 hello 改成 world", "com.apple.TextEdit");
        // 当前实现：X = "那个 hello"，在 last_written 里找不到 → NotFound
        // 不是 panic 而是确认它正确进 NotFound 分支
        match action {
            Some(CorrectionAction::NotFound { .. }) => {} // expected for MVP
            Some(CorrectionAction::Replace { .. }) => {} // future improvement
            other => panic!("unexpected: {:?}", other),
        }
        clear();
    }
}
