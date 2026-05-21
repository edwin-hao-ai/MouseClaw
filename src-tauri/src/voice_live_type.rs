//! 边说边写 · type-as-you-speak (v0.4.2)
//!
//! 背景：Plan B（voice_ime.rs）把所有字憋到 fn 松手后一次性写入，因为 macOS 的 fn
//! 键自带系统语义（拼音切换 / Spotlight / Dock），按住瞬间会把当前输入框失焦，
//! 录音期间 CGEvent 注入的字会被丢到错的 app。所以历史上（v0.3.8）改成"只在气泡
//! 显示 partial，松手后才统一 paste"。
//!
//! 但这个限制是 **fn 专属** 的。如果用户把触发键设成 option / control / 右 shift /
//! 右 cmd（voice_ime::ImeTrigger 已支持），按住它们 **不会偷焦点**，就可以让 sherpa
//! partial 实时写进光标 —— 体感从"松手才出字"变成"边说边冒字"，这是延迟体感上最大
//! 的一次跃迁。
//!
//! ## 何时启用（decide_live_typing：三个条件全满足才 live-type，否则降级回 Plan B）
//! 1. 触发键不是 fn —— fn 系统语义会失焦
//! 2. 目标 app 不是富文本/Electron —— 那些吃 direct unicode keystroke（mode_b 对它们
//!    走 clipboard-paste），边写会丢字，必须降级到松手后 paste
//! 3. 目标 app 不是终端 —— assert_writable 红线
//!
//! ## LCP 增量（apply_live_update）
//! sherpa 的 partial 会自我纠错（"你好吗" → "你好嘛"），所以每拿到一个新 partial 就
//! 跟"已经写进光标的文本"求最长公共前缀，backspace 掉发散的尾部，再 type 新增部分。
//! 已写文本镜像在 `state.ime_typed`，松手后 finalize 拿它跟 final（含标点重整）补 delta。
//!
//! ## 写入串行化（TYPE_LOCK）
//! poller 线程（150ms 一次）和松手后的 finish_live 都会注入键盘事件。用一把进程级
//! Mutex 串行化所有 live 键盘写入，避免两路并发 post 导致字符乱序。

#![cfg(target_os = "macos")]

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use once_cell::sync::Lazy;
use std::sync::Mutex;
use tauri::AppHandle;

use crate::AppState;

/// 本次录音是否处于"边说边写"模式 —— start 时由 decide_live_typing 决定并写入，
/// 松手后 stop_and_paste 读它选择补 delta（live）还是全量 paste（Plan B）。
static LIVE_ACTIVE: AtomicBool = AtomicBool::new(false);

/// 串行化所有 live 键盘注入（poller 与 finish_live 不会真并发，但加锁更安全）。
static TYPE_LOCK: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

pub fn set_live_active(v: bool) { LIVE_ACTIVE.store(v, Ordering::SeqCst); }
pub fn is_live_active() -> bool { LIVE_ACTIVE.load(Ordering::SeqCst) }

/// 决定本次录音能否边说边写。`trigger_is_fn` 由 voice_ime 缓存的 atomic 传入。
pub fn decide_live_typing(trigger_is_fn: bool) -> bool {
    if trigger_is_fn { return false; }
    // 终端红线：禁止写入
    if crate::mode_b::assert_writable().is_err() { return false; }
    // 富文本/Electron：吃 direct unicode keystroke，降级回松手后 clipboard-paste
    if crate::mode_b::should_use_clipboard_paste() { return false; }
    true
}

/// 求把 `typed`（已写进光标）变成 `target`（最新想要）需要的增量：
/// 返回 (要 backspace 的 char 数, 要新 type 的字符串)。按 char 计 —— 中文 1 char =
/// 1 backspace，跟 mode_b::delete_chars / type_unicode_sync 的 char 语义一致。
pub fn lcp_delta(typed: &str, target: &str) -> (usize, String) {
    let t: Vec<char> = typed.chars().collect();
    let g: Vec<char> = target.chars().collect();
    let mut p = 0usize;
    while p < t.len() && p < g.len() && t[p] == g[p] {
        p += 1;
    }
    let backspaces = t.len() - p;
    let to_type: String = g[p..].iter().collect();
    (backspaces, to_type)
}

/// 把光标里的内容从 `state.ime_typed` 增量更新成 `target`（LCP：回退发散尾部 + 写新增）。
/// 持 TYPE_LOCK 串行化键盘注入。失败静默（语音打字不该因一次注入失败而中断）。
fn apply_live_update(state: &AppState, target: &str) {
    let typed = state.ime_typed.lock().unwrap().clone();
    let (backspaces, to_type) = lcp_delta(&typed, target);
    if backspaces == 0 && to_type.is_empty() {
        return;
    }
    {
        let _g = TYPE_LOCK.lock().unwrap();
        if backspaces > 0 {
            let _ = crate::mode_b::delete_chars(backspaces);
        }
        if !to_type.is_empty() {
            let _ = crate::mode_b::type_unicode_sync(&to_type);
        }
    }
    *state.ime_typed.lock().unwrap() = target.to_string();
}

/// 流式 poller —— 每 150ms drain 录音 → sherpa accept → partial → recase + 标点 →
/// 更新桌宠气泡；若 `live_type` 则同时 LCP 增量写进光标。
///
/// 从 voice_ime::start_recording_for_ime 搬来（原 v0.3.8 Plan B 的内联 poller），
/// 加了 live 分支。仍用专用 std::thread（不是 tokio）—— sherpa accept + add_punctuation
/// 是 CPU 密集同步调用，放 tokio worker 会在 AI 子进程抢 runtime 时被饿死（v0.4 教训）。
pub fn spawn_streaming_poller(app: AppHandle, state: Arc<AppState>, live_type: bool) {
    set_live_active(live_type);
    std::thread::Builder::new()
        .name("mouseclaw-ime-poller".into())
        .spawn(move || {
            let mut last_partial = String::new();
            loop {
                std::thread::sleep(Duration::from_millis(150));
                if !state.streaming_active.load(Ordering::SeqCst) {
                    break;
                }
                let samples = {
                    let g = state.recorder.lock().unwrap();
                    match g.as_ref() {
                        Some(r) => r.drain_resampled_16k(),
                        None => break,
                    }
                };
                if samples.is_empty() {
                    continue;
                }
                let partial = {
                    let mut g = state.stream_session.lock().unwrap();
                    if let Some(s) = g.as_mut() {
                        s.accept(&samples);
                        s.partial()
                    } else {
                        continue;
                    }
                };
                if partial == last_partial {
                    continue;
                }
                last_partial = partial.clone();
                // 全大写英文还原成自然大小写（OPENAI→OpenAI）
                let recased = crate::vocab::recase_english(&partial);
                // 短片段（< 4 字符）模型加标点效果差，跳过
                let display = if recased.chars().count() >= 4 {
                    crate::punctuation::add_punctuation(&recased)
                } else {
                    recased
                };
                // 边说边写：先把光标更新到 display（借用），再 emit 气泡（移动 display）
                if live_type {
                    apply_live_update(&state, &display);
                }
                crate::overlay::emit_view(
                    &app,
                    &crate::events::ViewKind::VoiceImeListening { partial: display },
                );
            }
            println!("[mouseclaw] 🎙️ IME streaming poller exited (live_type={live_type})");
        })
        .expect("spawn ime poller thread");
}

/// 松手后收尾（仅 live 模式）—— partial 已实时写入，这里只把光标补到 final（含标点
/// 重整）的 delta，跳过 Plan B 的 fn 焦点恢复 sleep/activate（live 模式焦点没被偷）。
/// 写完记 last_written（给 3 秒纠错口令用）+ 气泡反馈 + 自动 hide。
pub async fn finish_live(app: &AppHandle, state: &Arc<AppState>, final_text: &str, bundle: &str) {
    {
        let typed = state.ime_typed.lock().unwrap().clone();
        let (backspaces, to_type) = lcp_delta(&typed, final_text);
        if backspaces > 0 || !to_type.is_empty() {
            let _g = TYPE_LOCK.lock().unwrap();
            if backspaces > 0 {
                let _ = crate::mode_b::delete_chars(backspaces);
            }
            if !to_type.is_empty() {
                let _ = crate::mode_b::type_unicode_sync(&to_type);
            }
        }
        *state.ime_typed.lock().unwrap() = final_text.to_string();
    }
    // 记录这次写入 —— 下一次 3 秒内的语音可能要纠错它
    crate::voice_correct::record_write(final_text, bundle);
    crate::overlay::emit_view(app, &crate::events::ViewKind::Reply {
        transcript: "voice IME".into(),
        reply: format!("✍️ {final_text}"),
        mode: crate::events::ReplyMode::A,
        insert_text: None,
        streaming: false,
    });
    tokio::time::sleep(Duration::from_millis(1500)).await;
    crate::overlay::hide_overlay(app);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lcp_delta_append_only() {
        // 纯追加：无回退，只 type 新增尾部
        let (bs, add) = lcp_delta("你好", "你好世界");
        assert_eq!(bs, 0);
        assert_eq!(add, "世界");
    }

    #[test]
    fn lcp_delta_self_correction() {
        // 模型自我纠错：尾字变了 → 回退 1 char 重写
        let (bs, add) = lcp_delta("你好吗", "你好嘛");
        assert_eq!(bs, 1);
        assert_eq!(add, "嘛");
    }

    #[test]
    fn lcp_delta_full_divergence() {
        let (bs, add) = lcp_delta("abc", "xyz");
        assert_eq!(bs, 3);
        assert_eq!(add, "xyz");
    }

    #[test]
    fn lcp_delta_noop_when_identical() {
        let (bs, add) = lcp_delta("hello", "hello");
        assert_eq!(bs, 0);
        assert_eq!(add, "");
    }

    #[test]
    fn lcp_delta_shrink() {
        // target 比 typed 短（标点收敛等）→ 回退多余尾部
        let (bs, add) = lcp_delta("你好。", "你好");
        assert_eq!(bs, 1);
        assert_eq!(add, "");
    }

    #[test]
    fn lcp_delta_counts_chars_not_bytes() {
        // 中文按 char 计：回退数应是 char 数不是字节数
        let (bs, _add) = lcp_delta("中文测试", "中文");
        assert_eq!(bs, 2);
    }

    #[test]
    fn decide_live_typing_false_for_fn() {
        // fn 触发永远不 live-type（焦点会被偷）
        assert!(!decide_live_typing(true));
    }
}
