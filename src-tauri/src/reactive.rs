//! Reactive 老鼠 · ambient 反应（v0.4+）
//!
//! 设计原型：`docs/prototypes/reactive-pet-20260519.html`
//!
//! 用户复制 / 选词 → 老鼠立刻有反应。99% 情况只是抖耳（T1，无 UI），
//! 内容"看着值得处理"时升级到 T2（头顶 ribbon 弹按钮）。
//!
//! ## 两条 source 共用同一管道
//! - `Source::Clipboard` —— `clipboard.rs` 的 500ms 轮询发现新 NSPasteboard
//! - `Source::Selection` —— `selection.rs` 的 500ms 轮询发现 AXSelectedText 变化
//!
//! 两者都调 `on_new_text()` → classify → 缓存原文 → emit `clipboard-reactive` 事件。
//! 前端 ribbon 在 idle 收到事件时弹按钮；点按钮 → invoke `process_reactive_action`
//! → 后端从 LAST_TEXT 缓存读最新一条 → 喂给 claude CLI → 写回剪贴板。
//!
//! ## Tier 决策
//! - Silent : 短复制 < 20 字 / 高熵密码串 / 敏感前台 → 不发事件
//! - T1     : 默认通过，前端表现 = 抖耳 ~400ms，零 UI
//! - T2     : 内容看着可处理（URL / 代码 / 乱格式 / 长文本）→ 抖耳 + 弹 ribbon

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Mutex, OnceLock};
use serde::Serialize;
use tauri::{AppHandle, Emitter};

pub const EV_CLIPBOARD_REACTIVE: &str = "clipboard-reactive";

/// process_reactive_action 完成时（成功 / 失败）emit 的事件 channel。
/// 让前端在 ribbon 已消失（用户切走 / 4s 超时）时也能看到一个 transient 反馈气泡。
pub const EV_REACTIVE_RESULT: &str = "reactive-result";

#[derive(Debug, Clone, Serialize)]
pub struct ReactiveResultPayload {
    pub action: String,
    pub ok: bool,
    /// 中文短句，前端直接 toast。失败时是错误摘要。
    pub summary: String,
}

/// 在 process_reactive_action 末尾调，告诉前端"我做完了"，
/// 这样即使 ribbon 已被 dismiss 用户也会从 transientAck 气泡看到反馈。
pub fn emit_action_result(action: &str, ok: bool, summary: &str) {
    let Some(app) = APP_HANDLE.get() else { return };
    let payload = ReactiveResultPayload {
        action: action.to_string(),
        ok,
        summary: summary.to_string(),
    };
    if let Err(e) = app.emit(EV_REACTIVE_RESULT, &payload) {
        eprintln!("[mouseclaw] 📋 reactive-result emit failed: {e}");
    }
}

// ───────────────────── 后台任务忙碌追踪 ─────────────────────
//
// 用户痛点（2026-05-20）：「点了纯文本然后切去做别的，不知道还在不在处理」。
// 解：任何后台 AI 任务（reactive action / 未来 selection action）开始时 +1、结束时 -1，
// 计数变化 emit bg-task-changed。前端据此在桌宠上显示"忙碌"指示，跨任何视图可见。
//
// 用 RAII guard 保证即使任务 panic / 早 return 也会 -1，不会卡在"永远忙碌"。

pub const EV_BG_TASK_CHANGED: &str = "bg-task-changed";

static BG_TASKS: AtomicU32 = AtomicU32::new(0);

#[derive(Debug, Clone, Serialize)]
pub struct BgTaskPayload {
    pub count: u32,
}

fn emit_bg_count() {
    let count = BG_TASKS.load(Ordering::SeqCst);
    if let Some(app) = APP_HANDLE.get() {
        let _ = app.emit(EV_BG_TASK_CHANGED, &BgTaskPayload { count });
    }
}

/// 后台任务计数 RAII guard —— `let _g = reactive::TaskGuard::start();` 放在任务起点，
/// drop（函数返回 / panic / await 取消）时自动 -1。
pub struct TaskGuard {
    _private: (),
}

impl TaskGuard {
    pub fn start() -> Self {
        BG_TASKS.fetch_add(1, Ordering::SeqCst);
        emit_bg_count();
        TaskGuard { _private: () }
    }
}

impl Drop for TaskGuard {
    fn drop(&mut self) {
        BG_TASKS.fetch_sub(1, Ordering::SeqCst);
        emit_bg_count();
    }
}

/// 当前后台任务数（测试 / 诊断用）。
pub fn bg_task_count() -> u32 {
    BG_TASKS.load(Ordering::SeqCst)
}

static APP_HANDLE: OnceLock<AppHandle> = OnceLock::new();

/// 最近一次触发 reactive 的原文（供 process_reactive_action 取用）。
///
/// 设计取舍：原文最大 1MB，每次 emit 都塞进事件 payload 会冲爆 IPC；
/// 改成"缓存最近一条 + 前端 invoke 命令时后端读它"。一次只服务一个动作，
/// 用户在 ribbon 显示期间不会同时连发多个 action，竞争窗口 < 几秒，安全。
pub static LAST_TEXT: once_cell::sync::Lazy<Mutex<Option<String>>> =
    once_cell::sync::Lazy::new(|| Mutex::new(None));

pub fn init(app: AppHandle) {
    let _ = APP_HANDLE.set(app);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    /// 来自系统剪贴板的新条目
    Clipboard,
    /// 来自 Accessibility 选区（用户在某个 app 里选中了文字）
    Selection,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ReactiveTier {
    Acknowledge,
    Hint,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum HintIcon {
    Format,
    Translate,
    Code,
    Url,
    Generic,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReactivePayload {
    pub source: Source,
    pub tier: ReactiveTier,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<HintIcon>,
    pub preview: String,
    #[serde(rename = "charLen")]
    pub char_len: usize,
}

/// 比 clipboard.rs::EXCLUDED_BUNDLES 多覆盖一些 —— EXCLUDED 是"不记录历史"，
/// 这里是"reactive 通道也别响"（更激进，覆盖银行 / 邮件等）。
const SENSITIVE_BUNDLES: &[&str] = &[
    "com.agilebits.onepassword7",
    "com.agilebits.onepassword-osx",
    "com.bitwarden.desktop",
    "com.dashlane.dashlanephonefinal",
    "com.apple.keychainaccess",
    "com.lastpass.LastPass",
];

/// 统一入口 —— clipboard 和 selection 都调它。
pub fn on_new_text(source: Source, text: &str, bundle: &str) {
    let Some((tier, icon)) = classify(text, bundle) else {
        return; // 静音
    };
    // 缓存原文供 action 用（一定要在 emit 之前 set，否则前端立刻 invoke 拿到旧文本）
    if let Ok(mut g) = LAST_TEXT.lock() {
        *g = Some(text.to_string());
    }
    let Some(app) = APP_HANDLE.get() else {
        return;
    };
    let payload = ReactivePayload {
        source,
        tier,
        icon,
        preview: preview_of(text, 80),
        char_len: text.chars().count(),
    };
    if let Err(e) = app.emit(EV_CLIPBOARD_REACTIVE, &payload) {
        eprintln!("[mouseclaw] 📋 reactive emit failed: {e}");
    }
}

/// 把当前缓存的原文取走（拿走后清空，避免同一段被反复处理）。
/// process_reactive_action 用它。
pub fn take_last_text() -> Option<String> {
    LAST_TEXT.lock().ok()?.take()
}

/// 只读读取（测试用）。
#[cfg(test)]
pub fn peek_last_text() -> Option<String> {
    LAST_TEXT.lock().ok()?.clone()
}

// ───────────────────────── 分类 ─────────────────────────

pub fn classify(text: &str, bundle: &str) -> Option<(ReactiveTier, Option<HintIcon>)> {
    let n = text.chars().count();
    if n < 20 { return None; }
    if SENSITIVE_BUNDLES.iter().any(|b| b.eq_ignore_ascii_case(bundle)) {
        return None;
    }
    if looks_like_secret(text) { return None; }

    let icon = if looks_like_url(text) {
        Some(HintIcon::Url)
    } else if looks_like_code(text) {
        Some(HintIcon::Code)
    } else if messy_whitespace(text) {
        Some(HintIcon::Format)
    } else if cross_language(text) {
        Some(HintIcon::Translate)
    } else if n > 80 {
        Some(HintIcon::Generic)
    } else {
        None
    };

    Some(match icon {
        Some(i) => (ReactiveTier::Hint, Some(i)),
        None => (ReactiveTier::Acknowledge, None),
    })
}

pub fn preview_of(text: &str, max_chars: usize) -> String {
    let collapsed: String = text.chars().take(max_chars)
        .map(|c| if c == '\n' || c == '\t' { ' ' } else { c })
        .collect::<String>()
        .trim()
        .to_string();
    if text.chars().count() > max_chars {
        format!("{collapsed}…")
    } else {
        collapsed
    }
}

fn looks_like_url(text: &str) -> bool {
    let t = text.trim();
    (t.starts_with("http://") || t.starts_with("https://") || t.starts_with("ftp://"))
        && !t.contains(char::is_whitespace)
}

fn looks_like_code(text: &str) -> bool {
    let braces = text.contains('{') && text.contains('}');
    let semis = text.matches(';').count();
    let kws = ["fn ", "const ", "let ", "var ", "function ", "def ", "import ", "export ",
              "return ", "class ", "public ", "private ", "async ", "await "];
    let has_kw = kws.iter().any(|k| text.contains(k));
    let arrows = text.contains("=>") || text.contains("->");
    braces || semis >= 2 || has_kw || arrows
}

fn messy_whitespace(text: &str) -> bool {
    // 多空格 / Tab + 空格混用 / 行尾尾随空格 / 多于 1 个连续换行
    text.contains("  ") || text.contains("\t ") || text.contains(" \n") || text.contains("\n\n\n")
}

/// 判定是否跨语言文本（用户大概率想翻译）。
/// 简单启发：包含 CJK 字符 AND 包含 ASCII 字母（中英混排）→ false（不算"跨语言"，就是带技术词的中文）。
/// 全 ASCII 字母 + 当前用户配置语言 ≠ "en" → 视为外文（想翻成母语）。
/// 全 CJK + 用户配置 ≠ "zh" → 反之亦然。
fn cross_language(text: &str) -> bool {
    let has_cjk = text.chars().any(|c|
        ('\u{4E00}'..='\u{9FFF}').contains(&c) ||
        ('\u{3040}'..='\u{30FF}').contains(&c) ||
        ('\u{AC00}'..='\u{D7AF}').contains(&c)
    );
    let has_ascii_letter = text.chars().any(|c| c.is_ascii_alphabetic());
    // 必须有字母类内容，不能纯符号
    if !has_ascii_letter && !has_cjk { return false; }

    let user_lang = crate::config::Config::load().language;
    if user_lang == "zh" {
        // 用户母语中文 → 全 ASCII 字母 / 主要非中文 = 想翻译
        !has_cjk && has_ascii_letter
    } else {
        // 用户母语英文 → 主要 CJK = 想翻译
        has_cjk && !has_ascii_letter
    }
}

fn looks_like_secret(text: &str) -> bool {
    let t = text.trim();
    let n = t.chars().count();
    if !(16..=128).contains(&n) { return false; }
    if t.contains(' ') || t.contains('\n') { return false; }
    let alnum_only = t.chars().all(|c|
        c.is_ascii_alphanumeric() || matches!(c, '+' | '/' | '=' | '-' | '_'));
    if !alnum_only { return false; }
    let has_upper = t.chars().any(|c| c.is_ascii_uppercase());
    let has_lower = t.chars().any(|c| c.is_ascii_lowercase());
    let has_digit = t.chars().any(|c| c.is_ascii_digit());
    (has_upper as u8) + (has_lower as u8) + (has_digit as u8) >= 2
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_copy_silent() {
        assert_eq!(classify("OK", ""), None);
        assert_eq!(classify("yes please", ""), None);
        assert_eq!(classify("", ""), None);
        assert_eq!(classify("a".repeat(19).as_str(), ""), None);
    }

    #[test]
    fn long_text_t2_generic() {
        let s = "this is a fairly long piece of normal english text without any special markers but it keeps going past eighty characters";
        // 注意：纯 ASCII 字母在 zh 用户下会被 cross_language 抓为 Translate（默认母语 zh）。
        // 测试场景下 Config::load() 读不到 ~/.mouseclaw/config.json → 走默认 zh。
        // 这条样本会先命中 cross_language 然后给 Translate（不是 Generic）—— 改成 cjk 文本来命中 Generic。
        let cn = "这是一段相当长的纯中文文字内容用来测试 generic tier 触发当文字超过八十字符没有命中其他特殊判定的时候应该走 generic icon 而不是 translate 因为这里全是中文";
        let r = classify(cn, "");
        assert!(matches!(r, Some((ReactiveTier::Hint, Some(HintIcon::Generic)))), "got {r:?}");
        // 原英文 sample 走 Translate 也算 Hint：
        assert!(matches!(classify(s, ""), Some((ReactiveTier::Hint, Some(HintIcon::Translate)))));
    }

    #[test]
    fn url_t2_url() {
        let s = "https://example.com/foo/bar/baz?query=1";
        assert!(matches!(classify(s, ""), Some((ReactiveTier::Hint, Some(HintIcon::Url)))));
    }

    #[test]
    fn url_with_trailing_text_not_url() {
        // URL 后跟空格再有文字 → 不是单纯 URL
        let s = "see https://example.com for more info on the topic";
        let r = classify(s, "");
        // 应该走其他分支（generic/translate），但不是 Url
        match r {
            Some((_, Some(HintIcon::Url))) => panic!("误判为 URL"),
            _ => {}
        }
    }

    #[test]
    fn code_t2_code() {
        let s = "fn main() { let x = 1; let y = 2; return x + y; }";
        assert!(matches!(classify(s, ""), Some((ReactiveTier::Hint, Some(HintIcon::Code)))));
    }

    #[test]
    fn code_arrow_detected() {
        let s = "const add = (a, b) => a + b; const sub = (a, b) => a - b;";
        assert!(matches!(classify(s, ""), Some((ReactiveTier::Hint, Some(HintIcon::Code)))));
    }

    #[test]
    fn code_rust_arrow_detected() {
        let s = "let result: Result<i32, Error> = process_input(data).await?;";
        // -> 在返回类型上 + async/await 关键字
        assert!(matches!(classify(s, ""), Some((ReactiveTier::Hint, Some(HintIcon::Code)))));
    }

    #[test]
    fn messy_t2_format() {
        let s = "Hello    world  这是   一段     乱   格式    的    文字    需要    清理";
        assert!(matches!(classify(s, ""), Some((ReactiveTier::Hint, Some(HintIcon::Format)))));
    }

    #[test]
    fn messy_multi_newline_t2_format() {
        let s = "first line\n\n\n\nsecond line after big gap that exceeds 20 chars";
        assert!(matches!(classify(s, ""), Some((ReactiveTier::Hint, Some(HintIcon::Format)))));
    }

    #[test]
    fn medium_clean_t1_acknowledge() {
        let s = "这是一个中等长度的中文句子刚好够得上二十字符阈值但是又不到八十字符";
        // 33 字符，全 CJK，用户默认 zh → cross_language=false → 长度 < 80 → Acknowledge
        let r = classify(s, "");
        assert!(matches!(r, Some((ReactiveTier::Acknowledge, None))), "got {r:?}");
    }

    #[test]
    fn secret_silent_base64ish() {
        let s = "sk-abc123XYZdef456GHI789jklMNO012pqrSTU345vwx";
        assert_eq!(classify(s, ""), None);
    }

    #[test]
    fn secret_silent_hex() {
        let s = "deadbeef1234567890cafebabe98765432";
        assert_eq!(classify(s, ""), None);
    }

    #[test]
    fn secret_with_space_not_secret() {
        // 含空格的 → 是普通文本不是 token
        let s = "deadbeef 1234567890 cafebabe 98765432";
        assert_ne!(classify(s, ""), None);
    }

    #[test]
    fn sensitive_bundle_silent() {
        let s = "this would normally be a t2 url https://example.com/a/long/path";
        assert_eq!(classify(s, "com.bitwarden.desktop"), None);
        assert_eq!(classify(s, "com.agilebits.onepassword7"), None);
    }

    #[test]
    fn preview_truncates_long() {
        let s = "a".repeat(200);
        let p = preview_of(&s, 80);
        assert!(p.ends_with('…'));
        assert_eq!(p.chars().count(), 81); // 80 + …
    }

    #[test]
    fn preview_collapses_newlines() {
        let p = preview_of("line1\nline2\tline3", 100);
        assert!(!p.contains('\n'));
        assert!(!p.contains('\t'));
    }

    #[test]
    fn last_text_roundtrip() {
        // 直接调 on_new_text 没 AppHandle 也能 set LAST_TEXT
        on_new_text(Source::Clipboard, "hello world a long enough message", "");
        let got = peek_last_text();
        assert_eq!(got.as_deref(), Some("hello world a long enough message"));
        let taken = take_last_text();
        assert_eq!(taken.as_deref(), Some("hello world a long enough message"));
        assert_eq!(peek_last_text(), None); // 拿走后清空
    }

    #[test]
    fn unicode_safe_preview() {
        // 多字节 UTF-8 在边界 char_count > byte len/4 时不能崩
        let s = "🦞".repeat(100);
        let p = preview_of(&s, 10);
        assert!(p.chars().count() <= 11);
    }
}
