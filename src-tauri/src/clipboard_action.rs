//! Clipboard-driven actions（v0.4+）
//!
//! 用户 hover 桌宠 → 弹 T3 action panel → 点动作（清理 / 翻译 / 解释 / ...）
//! → 前端 invoke `process_clipboard_action(clip_id, action)`
//! → 这里取剪贴板内容 → 拼 prompt → 调后台 CLI → 写回剪贴板 + 返回结果给前端
//!
//! MVP 只 wire "clean"（清理格式），其他 action 走同一 prompt template 框架，
//! 加几行就能扩。

use anyhow::{bail, Context, Result};
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};

/// 前端 Tauri command 入口。
///
/// 返回最终清洗后的纯文本。失败时返回错误字符串（友好可显示）。
/// 副作用：处理完会把结果写回系统剪贴板，用户 ⌘V 直接粘贴。
#[tauri::command]
pub async fn process_clipboard_action(clip_id: u64, action: String) -> Result<String, String> {
    let text = crate::clipboard::get_text(clip_id)
        .ok_or_else(|| "未找到对应剪贴板条目（可能已被清理）".to_string())?;
    if text.trim().is_empty() {
        return Err("剪贴板条目为空".into());
    }
    let prompt = build_prompt(&action, &text)
        .map_err(|e| e.to_string())?;
    let result = run_claude_text_only(&prompt).await
        .map_err(|e| format!("调用后台失败：{e}"))?;
    let cleaned = result.trim().to_string();
    if cleaned.is_empty() {
        return Err("后台返回空结果".into());
    }
    if let Err(e) = write_to_pasteboard(&cleaned) {
        eprintln!("[mouseclaw] 📋 write back failed: {e}");
        // 不 fail —— 用户至少能看到结果
    }
    Ok(cleaned)
}

/// 按 action 选择不同的系统提示词。MVP 只填 "clean"，其他先占位（返回错误，前端能识别）。
fn build_prompt(action: &str, text: &str) -> Result<String> {
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
        "reply" => bail!("'reply' 还未实现，下个版本支持"),
        _ => bail!("未知 action: {action}"),
    };
    Ok(body)
}

/// 跑 claude CLI 拿一段纯文本结果。
/// 复用 claude_cli 的二进制定位 / PATH 拓宽 / workspace cwd，但用更简单的 stdin 注入 prompt
/// 而非走 stream-json（这是个单次纯文本回包动作，不需要 token-level streaming）。
async fn run_claude_text_only(prompt: &str) -> Result<String> {
    let bin = crate::claude_cli::find_binary("claude")
        .context("找不到 claude CLI")?;

    let mut cmd = tokio::process::Command::new(&bin);
    cmd.env("PATH", crate::claude_cli::expanded_path());
    crate::provider_env::apply_to(&mut cmd);
    if let Some(ws) = crate::config::Config::load().workspace_path {
        if std::path::Path::new(&ws).is_dir() {
            cmd.current_dir(&ws);
        }
    }

    cmd.arg("-p").arg(prompt)
        .arg("--permission-mode").arg("auto")
        .arg("--allowedTools").arg("");  // 纯文本任务，不需要任何 tool

    let mut child = cmd
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("spawn claude")?;

    let stdout = child.stdout.take().context("stdout not piped")?;
    let mut reader = BufReader::new(stdout).lines();
    let mut out = String::new();
    while let Some(line) = reader.next_line().await.context("read stdout")? {
        if !out.is_empty() { out.push('\n'); }
        out.push_str(&line);
    }

    let status = child.wait().await.context("wait claude")?;
    if !status.success() {
        bail!("claude 退出码非 0");
    }
    Ok(out)
}

/// 把结果写回系统剪贴板（macOS NSPasteboard）。
/// 故意不更新 clipboard.rs 的历史 —— 这条是"老鼠帮你处理过的结果"，
/// 不是用户主动复制的，混进历史会污染时间线。
#[cfg(target_os = "macos")]
fn write_to_pasteboard(text: &str) -> Result<()> {
    use cocoa::base::{id, nil, BOOL, YES};
    use cocoa::foundation::{NSAutoreleasePool, NSString};
    use objc::{class, msg_send, sel, sel_impl};

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
