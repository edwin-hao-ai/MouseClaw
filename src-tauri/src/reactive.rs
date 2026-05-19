//! Reactive 老鼠 · ambient 反应（v0.4+）
//!
//! 设计原型：`docs/prototypes/reactive-pet-20260519.html`
//!
//! 用户复制 / 选词 → 老鼠立刻有反应，但 99% 情况只是纯像素动画（抖耳 / 瞟眼），
//! 不弹按钮、不出 UI。只在内容"看着值得处理"时升级到 tier 2（头顶飘小图标），
//! 用户主动 hover 桌宠才到 tier 3（动作面板，前端本地展开，不走后端事件）。
//!
//! ## Tier 决策
//! - Silent  : 短复制 < 20 字 / 高熵密码串 / 敏感前台 app —— 不发事件
//! - T1 Acknowledge : 默认通过，前端表现 = 抖耳 ~400ms，零 UI
//! - T2 Hint        : 内容看着可处理（URL / 代码 / 乱格式 / 长文本）
//!
//! ## 为什么不在 backend 里做 2s paste suppression
//! macOS 没有"用户按了 ⌘V"事件可监听（要 Accessibility 全权限）。
//! 替代设计：T2 头顶图标 2.5s 自动消失 —— 用户搬运完早就移走视线，
//! 那点像素动画属于"察觉到但没打扰"。explicit suppression 留到 v0.5。

use std::sync::OnceLock;
use serde::Serialize;
use tauri::{AppHandle, Emitter};

pub const EV_CLIPBOARD_REACTIVE: &str = "clipboard-reactive";

static APP_HANDLE: OnceLock<AppHandle> = OnceLock::new();

/// 启动时由 lib.rs 调用一次。OnceLock 保证只能 set 一次；重复调用静默忽略。
pub fn init(app: AppHandle) {
    let _ = APP_HANDLE.set(app);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ReactiveTier {
    /// T1 · 默默抖耳，没 UI。
    Acknowledge,
    /// T2 · 头顶飘小图标，2.5s 自动消失。
    Hint,
}

/// 头顶图标暗示 —— 给用户一个"老鼠看到了你大概想干嘛"的弱提示。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum HintIcon {
    /// 📝 看着像乱格式（多空格 / 多换行）
    Format,
    /// 🌐 看着像跨语言文本（V1 暂不启用，保留 enum 给 v0.5）
    Translate,
    /// 💻 看着像代码（花括号 / 关键字 / 多分号）
    Code,
    /// 🔗 看着像 URL
    Url,
    /// ✨ 单纯就是长文本
    Generic,
}

#[derive(Debug, Clone, Serialize)]
pub struct ClipboardReactivePayload {
    pub tier: ReactiveTier,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<HintIcon>,
    #[serde(rename = "clipId")]
    pub clip_id: u64,
    /// 80 字符以内的预览（换行压成空格，末尾省略号）
    pub preview: String,
    #[serde(rename = "charLen")]
    pub char_len: usize,
}

/// 比 clipboard.rs::EXCLUDED_BUNDLES 多覆盖一些 —— EXCLUDED 是"不记录历史"，
/// 这里是"reactive 通道也别响"（更激进一点，覆盖银行 / 邮件等）。
const SENSITIVE_BUNDLES: &[&str] = &[
    "com.agilebits.onepassword7",
    "com.agilebits.onepassword-osx",
    "com.bitwarden.desktop",
    "com.dashlane.dashlanephonefinal",
    "com.apple.keychainaccess",
    "com.lastpass.LastPass",
];

/// 由 clipboard.rs::on_clipboard_changed 在 push_back 之后调用。
/// text/bundle 取的是当前刚入历史的那条。
pub fn on_new_clip(clip_id: u64, text: &str, bundle: &str) {
    let Some((tier, icon)) = classify(text, bundle) else {
        return; // 静音
    };
    let Some(app) = APP_HANDLE.get() else {
        // init 还没跑（早期启动），直接放过
        return;
    };
    let payload = ClipboardReactivePayload {
        tier,
        icon,
        clip_id,
        preview: preview_of(text, 80),
        char_len: text.chars().count(),
    };
    if let Err(e) = app.emit(EV_CLIPBOARD_REACTIVE, &payload) {
        eprintln!("[mouseclaw] 📋 reactive emit failed: {e}");
    }
}

// ───────────────────────── 分类 ─────────────────────────

fn classify(text: &str, bundle: &str) -> Option<(ReactiveTier, Option<HintIcon>)> {
    let n = text.chars().count();
    if n < 20 { return None; } // 短复制 → silent
    if SENSITIVE_BUNDLES.iter().any(|b| b.eq_ignore_ascii_case(bundle)) {
        return None; // 敏感前台
    }
    if looks_like_secret(text) { return None; } // 高熵串

    // Tier 2 提升判定（优先级从特异到一般）
    let icon = if looks_like_url(text) {
        Some(HintIcon::Url)
    } else if looks_like_code(text) {
        Some(HintIcon::Code)
    } else if messy_whitespace(text) {
        Some(HintIcon::Format)
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

fn preview_of(text: &str, max_chars: usize) -> String {
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
    t.starts_with("http://") || t.starts_with("https://") || t.starts_with("ftp://")
}

fn looks_like_code(text: &str) -> bool {
    let braces = text.contains('{') && text.contains('}');
    let semis = text.matches(';').count();
    let kws = ["fn ", "const ", "let ", "var ", "function ", "def ", "import ", "export ",
              "return ", "class ", "public ", "private "];
    let has_kw = kws.iter().any(|k| text.contains(k));
    braces || semis >= 2 || has_kw
}

fn messy_whitespace(text: &str) -> bool {
    // 连续 2+ 空格 / Tab 后跟空格 / 空格紧贴换行
    text.contains("  ") || text.contains("\t ") || text.contains(" \n")
}

/// 看着像 token / API key / 密码 → 跳过整条 reactive 路径
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
    }

    #[test]
    fn long_text_t2_generic() {
        let s = "this is a fairly long piece of normal english text without any special markers but it keeps going past eighty characters";
        assert!(matches!(classify(s, ""), Some((ReactiveTier::Hint, Some(HintIcon::Generic)))));
    }

    #[test]
    fn url_t2_url() {
        let s = "https://example.com/foo/bar/baz?query=1";
        assert!(matches!(classify(s, ""), Some((ReactiveTier::Hint, Some(HintIcon::Url)))));
    }

    #[test]
    fn code_t2_code() {
        let s = "fn main() { let x = 1; let y = 2; return x + y; }";
        assert!(matches!(classify(s, ""), Some((ReactiveTier::Hint, Some(HintIcon::Code)))));
    }

    #[test]
    fn messy_t2_format() {
        let s = "Hello    world  this   is    a   messy    paragraph    with extra spaces";
        assert!(matches!(classify(s, ""), Some((ReactiveTier::Hint, Some(HintIcon::Format)))));
    }

    #[test]
    fn medium_clean_t1_acknowledge() {
        // 20-80 字符, 无特殊 → T1
        let s = "moderately normal short sentence ok";
        assert!(matches!(classify(s, ""), Some((ReactiveTier::Acknowledge, None))));
    }

    #[test]
    fn secret_silent() {
        let s = "sk-abc123XYZdef456GHI789jklMNO012pqrSTU345vwx";
        assert_eq!(classify(s, ""), None);
    }

    #[test]
    fn sensitive_bundle_silent() {
        let s = "this would normally be a t2 url https://example.com/a/long/path";
        assert_eq!(classify(s, "com.bitwarden.desktop"), None);
    }
}
