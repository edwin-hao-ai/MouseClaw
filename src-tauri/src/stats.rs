//! 听写统计 (v0.6) —— 时长 / 字数 / WPM / 估省打字时间。纯本地，零联网。
//!
//! 持久化在 `~/.mouseclaw/stats.json`（累计值，跨重启）。每次成功听写写入后累加。
//! Typeless 卖点之一："你已经省了 X 分钟打字"——这里用保守的打字基线估出来。
//!
//! 字数口径：CJK 每字算 1 词，ASCII 按空白切词（中英混排合理估）。
//! 省时估法：打字所需(words/TYPING_WPM) − 实际口述所需(total_minutes)，下限 0。

use std::path::PathBuf;
use std::sync::atomic::{AtomicI64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};

/// 普通键盘打字基线（WPM）—— 估"省了多少时间"用。40 是中英混排保守常见值。
const TYPING_WPM: f64 = 40.0;
/// 太短的（误触 / 空录）不计入。
const MIN_SECONDS: f64 = 0.3;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Stats {
    pub total_words: u64,
    pub total_seconds: f64,
    pub sessions: u64,
    #[serde(default)]
    pub updated_at: i64, // epoch secs
}

/// 给前端的派生视图（含 WPM / 省下分钟）。
#[derive(Debug, Clone, Serialize)]
pub struct StatsView {
    pub total_words: u64,
    pub total_minutes: f64,
    pub sessions: u64,
    pub avg_wpm: f64,
    pub saved_minutes: f64,
}

static START_US: AtomicI64 = AtomicI64::new(0);

fn now_us() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_micros() as i64).unwrap_or(0)
}

fn stats_path() -> Result<PathBuf> {
    let home = std::env::var_os("HOME").ok_or_else(|| anyhow!("HOME not set"))?;
    Ok(PathBuf::from(home).join(".mouseclaw/stats.json"))
}

pub fn load() -> Stats {
    stats_path()
        .ok()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn save(s: &Stats) -> Result<()> {
    let p = stats_path()?;
    if let Some(dir) = p.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(&p, serde_json::to_string_pretty(s)?)?;
    Ok(())
}

/// 录音开始时调（记起点）。
pub fn mark_start() {
    START_US.store(now_us(), Ordering::Relaxed);
}

/// 成功写入后调：算时长 + 字数，累加持久化。空 / 误触跳过。
pub fn record_finish(text: &str) {
    let start = START_US.swap(0, Ordering::Relaxed);
    if start == 0 {
        return;
    }
    let secs = (now_us() - start) as f64 / 1_000_000.0;
    let words = count_words(text);
    if words == 0 || secs < MIN_SECONDS {
        return;
    }
    let mut s = load();
    s.total_words += words;
    s.total_seconds += secs;
    s.sessions += 1;
    s.updated_at = now_us() / 1_000_000;
    if let Err(e) = save(&s) {
        eprintln!("[mouseclaw] stats save: {e}");
    }
}

/// CJK / 假名范围 —— 逐字算 1 词。（原在 vocab.rs，v0.7 vocab 模块删除后内联到此。）
fn is_cjk(c: char) -> bool {
    matches!(c as u32,
        0x3040..=0x30FF |   // 平假名 / 片假名
        0x3400..=0x4DBF |   // CJK 扩展 A
        0x4E00..=0x9FFF |   // CJK 基本
        0xF900..=0xFAFF |   // CJK 兼容
        0x20000..=0x2A6DF)  // CJK 扩展 B
}

/// CJK 每字算 1 词，ASCII 按空白切词。
pub fn count_words(text: &str) -> u64 {
    let mut n = 0u64;
    let mut in_ascii = false;
    for ch in text.chars() {
        if is_cjk(ch) {
            n += 1;
            in_ascii = false;
        } else if ch.is_alphanumeric() {
            if !in_ascii {
                n += 1;
                in_ascii = true;
            }
        } else {
            in_ascii = false;
        }
    }
    n
}

/// 派生视图（前端读）。
pub fn view() -> StatsView {
    derive(&load())
}

fn derive(s: &Stats) -> StatsView {
    let minutes = s.total_seconds / 60.0;
    let avg_wpm = if minutes > 0.0 { s.total_words as f64 / minutes } else { 0.0 };
    let typing_min = s.total_words as f64 / TYPING_WPM;
    let saved = (typing_min - minutes).max(0.0);
    StatsView {
        total_words: s.total_words,
        total_minutes: minutes,
        sessions: s.sessions,
        avg_wpm,
        saved_minutes: saved,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn count_words_chinese_per_char() {
        assert_eq!(count_words("今天天气很好"), 6);
        assert_eq!(count_words("你好"), 2);
    }

    #[test]
    fn count_words_english_by_token() {
        assert_eq!(count_words("hello world"), 2);
        assert_eq!(count_words("the useEffect hook"), 3);
    }

    #[test]
    fn count_words_mixed() {
        // 6 CJK + "hello" + "world" = 8
        assert_eq!(count_words("今天天气很好 hello world"), 8);
        // 标点不计
        assert_eq!(count_words("你好，世界！"), 4);
    }

    #[test]
    fn count_words_empty() {
        assert_eq!(count_words(""), 0);
        assert_eq!(count_words("，。！"), 0);
    }

    #[test]
    fn derive_computes_wpm_and_saved() {
        // 400 词、5 分钟口述 → 80 WPM；打字基线 40 WPM 要 10 分钟 → 省 5 分钟
        let s = Stats { total_words: 400, total_seconds: 300.0, sessions: 3, updated_at: 0 };
        let v = derive(&s);
        assert_eq!(v.total_words, 400);
        assert!((v.total_minutes - 5.0).abs() < 1e-6);
        assert!((v.avg_wpm - 80.0).abs() < 1e-6);
        assert!((v.saved_minutes - 5.0).abs() < 1e-6);
    }

    #[test]
    fn derive_zero_is_safe() {
        let v = derive(&Stats::default());
        assert_eq!(v.total_words, 0);
        assert_eq!(v.avg_wpm, 0.0);
        assert_eq!(v.saved_minutes, 0.0);
    }
}
