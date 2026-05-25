//! Reactive 桌宠 ribbon 动作执行（v0.4+）
//!
//! 用户 hover 桌宠头顶 ribbon → 点动作（清理 / 翻译 / 解释 / 写回信）
//! → 前端 invoke `process_reactive_action(action)`
//! → 这里从 `reactive::LAST_TEXT` 缓存读最新触发的原文（剪贴板或选词都行）
//! → 拼 prompt → 调后台 CLI → 写回剪贴板 + 返回结果给前端
//!
//! 命令命名故意不带 "clipboard"，因为来源可能是 selection。前端不需要传 source。

use anyhow::{bail, Result};

/// 前端 Tauri command 入口。
///
/// 返回最终处理后的纯文本。失败时返回错误字符串（友好可显示）。
/// 副作用：处理完会把结果写回系统剪贴板，用户 ⌘V 直接粘贴。
#[tauri::command]
pub async fn process_reactive_action(action: String) -> Result<String, String> {
    let outcome = run(&action).await;
    // v0.4 · 处理完通知 —— ribbon 可能已被 dismiss（用户切去做别的事 / 4s 超时），
    // 不管成败都 emit reactive-result 让前端在那种情况下也给一个 transient 气泡告知。
    match &outcome {
        Ok(_) => crate::reactive::emit_action_result(&action, true, &result_summary(&action)),
        Err(e) => crate::reactive::emit_action_result(&action, false, &format!("✗ {e}")),
    }
    outcome
}

async fn run(action: &str) -> Result<String, String> {
    let text = crate::reactive::take_last_text()
        .ok_or_else(|| "没有待处理的剪贴板 / 选词内容（可能已超时）".to_string())?;
    if text.trim().is_empty() {
        return Err("待处理内容为空".into());
    }
    // v0.6 · 清理 = 纯规则（去口头禅 + 收敛空白/标点），0 模型、即时，不走后端。
    if action == "clean" {
        let lang = crate::config::Config::load().language;
        let cleaned = crate::tidy_up::light_clean(&text, &lang);
        if cleaned.trim().is_empty() {
            return Err("清理后为空".into());
        }
        if let Err(e) = write_to_pasteboard(&cleaned) {
            eprintln!("[mouseclaw] 📋 write back failed: {e}");
        }
        return Ok(cleaned);
    }
    // 翻译 / 解释 / 写回信 = 走 CLI 后端（这些需要真模型；本地不做）。
    let prompt = build_prompt(action, &text)
        .map_err(|e| e.to_string())?;
    // v0.4 · AI 任务串行队列 —— 排队等轮到自己（桌宠在排队期间显示忙碌）。
    let _ticket = crate::ai_queue::acquire().await;
    // v0.4 · 走统一后端接口，自动适配用户在 Onboarding 选的 CLI（多后端硬规则）。
    let backend = crate::config::Config::load().backend;
    let result = crate::backend::ask_text_only(backend, &prompt).await
        .map_err(|e| format!("调用 {} 失败：{e}", backend.display_name()))?;
    let cleaned = strip_wrappers(result.trim());
    if cleaned.is_empty() {
        return Err("后台返回空结果".into());
    }
    if let Err(e) = write_to_pasteboard(&cleaned) {
        eprintln!("[mouseclaw] 📋 write back failed: {e}");
    }
    Ok(cleaned)
}

/// 给 reactive-result.summary 用的口语短句（前端直接 toast）。
fn result_summary(action: &str) -> String {
    match action {
        "clean"     => "📄 整理好了 · ⌘V 粘贴".to_string(),
        "translate" => "🌐 翻译好了 · ⌘V 粘贴".to_string(),
        "explain"   => "💡 解释好了 · ⌘V 粘贴".to_string(),
        "reply"     => "✉️ 回信草稿好了 · ⌘V 粘贴".to_string(),
        _           => format!("✓ {action} 完成 · ⌘V 粘贴"),
    }
}

/// 按 action 选择不同的系统提示词。
pub fn build_prompt(action: &str, text: &str) -> Result<String> {
    let body = match action {
        "clean" => format!(
            "请把下面这段文字的格式清理一下：\n\
             - 去掉多余空格 / Tab / 不规则换行\n\
             - 保持原意和标点\n\
             - 不要改写、不要总结、不要翻译\n\
             - 只输出清理后的纯文本，前后不要加任何解释 / 引号 / Markdown\n\n\
             ===== 原文 START =====\n{text}\n===== 原文 END ====="
        ),
        "translate" => format!(
            "请翻译下面这段文字：\n\
             - 自动判断语向：中文 → 英文；非中文（英 / 日 / 韩 / 法 / 德 / 西 / 俄 等）→ 中文\n\
             - 保持段落分隔和原意\n\
             - 不要解释、不要加注释\n\
             - 只输出翻译后的纯文本，前后不要加任何引号 / Markdown\n\n\
             ===== 原文 START =====\n{text}\n===== 原文 END ====="
        ),
        "explain" => format!(
            "请用中文简要解释下面这段内容（≤ 5 句话，能直接放进对话窗发出去的口语风格）：\n\
             - 如果是代码：说它做什么、关键点在哪、有没有明显坑\n\
             - 如果是文章 / 段落：用 2-3 句话总结核心\n\
             - 如果是 URL：根据 URL 路径猜内容是什么类型的页面（视频 / 文档 / repo / 新闻 等）\n\
             - 只输出解释本身，不要加引号 / 标题 / Markdown\n\n\
             ===== 原文 START =====\n{text}\n===== 原文 END ====="
        ),
        "reply" => format!(
            "请给下面这段消息写一段恰当的回复（语言与原消息一致）：\n\
             - 语气客气、专业、简洁，3-5 句话\n\
             - 如果原文是问题：先简短答复，再补 1 句澄清 / 后续\n\
             - 如果原文是请求：先回复是否能做 + 时间 / 条件\n\
             - 如果原文是邮件 / 长信：开头不要 'Dear X'，直接进正文（用户可能贴到自己的邮件 / IM 里）\n\
             - 只输出回复正文，不要加 'Reply:' / 引号 / Markdown / 签名\n\n\
             ===== 原文 START =====\n{text}\n===== 原文 END ====="
        ),
        _ => bail!("未知 action: {action}"),
    };
    Ok(body)
}

/// AI 可能不听话给加 ``` 或 "" 包裹 —— 这里兜底剥掉。
fn strip_wrappers(s: &str) -> String {
    let trimmed = s.trim();
    // 三反引号包裹（带 / 不带 lang 标签）
    if let Some(inner) = trimmed.strip_prefix("```") {
        if let Some(end) = inner.rfind("```") {
            // 去掉可能的语言标识（第一行短词）
            let body = &inner[..end];
            let body = body
                .split_once('\n')
                .map(|(_, rest)| rest)
                .unwrap_or(body);
            return body.trim().to_string();
        }
    }
    // 一对双引号 / 中文引号
    let pairs = [('"', '"'), ('"', '"'), ('\'', '\''), ('「', '」')];
    for (open, close) in pairs {
        if trimmed.starts_with(open) && trimmed.ends_with(close) && trimmed.chars().count() > 2 {
            let mut iter = trimmed.chars();
            iter.next();
            let mut s: String = iter.collect();
            s.pop();
            return s.trim().to_string();
        }
    }
    trimmed.to_string()
}

// v0.4 (2026-05-20) · run_claude_text_only 删除 —— 替换为 backend::ask_text_only
// 让 reactive action 跟用户在 Onboarding 选的 CLI 一致（Claude / Codex / OpenClaw / Hermes
// 都支持），避免「我选了 Codex 但 reactive ribbon 偷偷调 claude」的接线 bug。

#[cfg(target_os = "macos")]
fn write_to_pasteboard(text: &str) -> Result<()> {
    use cocoa::base::{id, nil, BOOL, YES};
    use cocoa::foundation::{NSAutoreleasePool, NSString};
    use objc::{class, msg_send, sel, sel_impl};

    // 反馈环防护：标记接下来这段是我们自己写的，剪贴板监听别再弹 ribbon。
    crate::reactive::mark_self_write(text);

    unsafe {
        let pool: id = NSAutoreleasePool::new(nil);
        let pb: id = msg_send![class!(NSPasteboard), generalPasteboard];
        if pb == nil {
            let _: () = msg_send![pool, drain];
            bail!("generalPasteboard nil");
        }
        let _: i64 = msg_send![pb, clearContents];
        let ns_str: id = NSString::alloc(nil).init_str(text);
        let ns_type: id = NSString::alloc(nil).init_str("public.utf8-plain-text");
        let ok: BOOL = msg_send![pb, setString: ns_str forType: ns_type];
        let _: () = msg_send![pool, drain];
        if ok != YES {
            bail!("NSPasteboard setString failed");
        }
    }
    Ok(())
}

#[cfg(not(target_os = "macos"))]
fn write_to_pasteboard(_text: &str) -> Result<()> {
    bail!("write_to_pasteboard 仅支持 macOS");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_prompt_clean() {
        let p = build_prompt("clean", "Hello  world").unwrap();
        assert!(p.contains("清理"));
        assert!(p.contains("Hello  world"));
        // clean 必须明确"不要改写、不要翻译"否则 AI 会自作主张
        assert!(p.contains("不要翻译"));
        // clean 主指令是"清理一下"
        assert!(p.contains("格式清理"));
    }

    #[test]
    fn build_prompt_translate() {
        let p = build_prompt("translate", "Hello world").unwrap();
        assert!(p.contains("翻译"));
        assert!(p.contains("自动判断语向"));
    }

    #[test]
    fn build_prompt_explain() {
        let p = build_prompt("explain", "fn main() {}").unwrap();
        assert!(p.contains("解释"));
        assert!(p.contains("代码"));
    }

    #[test]
    fn build_prompt_reply() {
        let p = build_prompt("reply", "Hi can you confirm?").unwrap();
        assert!(p.contains("回复"));
        assert!(p.contains("Hi can you confirm?"));
    }

    #[test]
    fn build_prompt_unknown_fails() {
        assert!(build_prompt("hack-me", "x").is_err());
        assert!(build_prompt("", "x").is_err());
    }

    #[test]
    fn strip_backticks() {
        let s = "```rust\nlet x = 1;\n```";
        assert_eq!(strip_wrappers(s), "let x = 1;");
    }

    #[test]
    fn strip_no_lang_backticks() {
        let s = "```\nhello\nworld\n```";
        assert_eq!(strip_wrappers(s), "hello\nworld");
    }

    #[test]
    fn strip_quotes() {
        assert_eq!(strip_wrappers("\"hello\""), "hello");
        assert_eq!(strip_wrappers("「中文」"), "中文");
    }

    #[test]
    fn strip_passthrough() {
        assert_eq!(strip_wrappers("plain text"), "plain text");
    }

    #[test]
    fn result_summary_all_actions_have_emoji() {
        for action in ["clean", "translate", "explain", "reply"] {
            let s = result_summary(action);
            assert!(s.contains("⌘V"), "{action} summary 缺 paste 提示: {s}");
            // 确保不是 fallback 路径（fallback 是 "✓ {action}"）
            assert!(!s.starts_with("✓ "), "{action} 没有命中专属 summary, 走了 fallback");
        }
    }

    #[test]
    fn result_summary_unknown_action_fallback() {
        let s = result_summary("frobnicate");
        assert!(s.contains("frobnicate"));
        assert!(s.contains("⌘V"));
    }
}
