//! 桌宠说话 · v0.4.0
//!
//! 用 macOS 自带 `say` 命令朗读 AI 回复。零依赖、零网络、零成本。
//! - 默认关（`config.tts_enabled = false`），用户在托盘里开
//! - 朗读前杀掉上一个 `say` 进程（避免回复叠着说）
//! - 长文本截断到 600 char（say 在超长文本上会卡几秒）
//! - 中文优先 Tingting / Sinji；英文走系统默认
//!
//! 不挡主流程：spawn 后立刻 return，主线程不阻塞。
//!
//! ### UX 决策（CLAUDE.md 头号硬规则 10 问对照）
//! - 入口可发现性：托盘菜单 `🔊 桌宠开口说话` CheckMenuItem
//! - 首次使用：默认关，用户主动开（声音会打扰，不能默认）
//! - 快慢路径：spawn 不阻塞；最长 600 char 朗读约 30s，超长截断
//! - 取消和后悔：新 AI 回复来了 → 老 `say` 被 SIGKILL 替换
//! - 失败模式：`say` 命令找不到 / 进程挂 → silent，不打扰主流程

//! 跨平台（v0.5）：macOS 用 `say`；Win/Linux 走 `crate::platform::speak`
//! （tts crate：SAPI / speech-dispatcher）。Linux 需 speech-dispatcher 在跑，否则静默降级。

#[cfg(target_os = "macos")]
use std::process::{Child, Command, Stdio};
#[cfg(target_os = "macos")]
use std::sync::Mutex;
#[cfg(target_os = "macos")]
use once_cell::sync::Lazy;

/// 最长朗读字符数 —— 超过截断 + 加省略号
const MAX_SPEAK_CHARS: usize = 600;

/// 上一个还在跑的 `say` 进程 —— 新 reply 来了就 kill 它，不要叠播
#[cfg(target_os = "macos")]
static CURRENT_SAY: Lazy<Mutex<Option<Child>>> = Lazy::new(|| Mutex::new(None));

/// 朗读 `text`，spawn 后立刻返回。Safe to call from any thread.
/// `lang` = "zh" / "en" 决定声音偏好。
#[cfg(target_os = "macos")]
pub fn speak(text: &str, lang: &str) {
    let trimmed = text.trim();
    if trimmed.is_empty() { return; }
    let to_say = truncate_chars(trimmed, MAX_SPEAK_CHARS);

    // 杀掉上一轮 say —— 防回复叠加（用户连续召唤）
    if let Ok(mut guard) = CURRENT_SAY.lock() {
        if let Some(mut child) = guard.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }

    // 选声音：中文优先 Tingting（默认系统中文），英文走 default
    let mut cmd = Command::new("say");
    if lang == "zh" {
        // -v 'Tingting' 在所有 macOS 都有
        cmd.arg("-v").arg("Tingting");
    }
    // 让 stdin/out/err 都 null 避免 pipe 阻塞
    cmd.arg(&to_say)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    match cmd.spawn() {
        Ok(child) => {
            if let Ok(mut guard) = CURRENT_SAY.lock() {
                *guard = Some(child);
            }
            println!("[mouseclaw] 🔊 speaking {} chars ({})", to_say.chars().count(), lang);
        }
        Err(e) => eprintln!("[mouseclaw] 🔊 say spawn failed: {e}"),
    }
}

/// 强制停止当前朗读（Esc / 切 view 时调）。
#[cfg(target_os = "macos")]
pub fn stop() {
    if let Ok(mut guard) = CURRENT_SAY.lock() {
        if let Some(mut child) = guard.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

/// 非 macOS：截断后交给平台 TTS（tts crate）。
#[cfg(not(target_os = "macos"))]
pub fn speak(text: &str, lang: &str) {
    let trimmed = text.trim();
    if trimmed.is_empty() { return; }
    let to_say = truncate_chars(trimmed, MAX_SPEAK_CHARS);
    crate::platform::speak(&to_say, lang);
}

#[cfg(not(target_os = "macos"))]
pub fn stop() {
    crate::platform::stop_speak();
}

fn truncate_chars(s: &str, max: usize) -> String {
    if s.chars().count() <= max { return s.to_string(); }
    let mut out: String = s.chars().take(max).collect();
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_short_is_unchanged() {
        assert_eq!(truncate_chars("hello", 10), "hello");
    }

    #[test]
    fn truncate_long_adds_ellipsis() {
        let s = "你好世界你好世界你好世界";
        let got = truncate_chars(s, 4);
        // 4 chars + 1 ellipsis = 5 chars
        assert_eq!(got.chars().count(), 5);
        assert!(got.ends_with('…'));
    }

    #[test]
    fn stop_when_nothing_to_stop_is_noop() {
        stop(); // should not panic
    }
}
