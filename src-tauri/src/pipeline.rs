//! 核心 pipeline：截屏 + Whisper 转写 + AI 调用 + 输出模式（A/B）。
//! Push-to-talk 语义：按住快捷键 = 录音 + 截图；松开 = 转写 + 跑 pipeline。
//!
//! 之前是 toggle press（按一下开始、再按一下结束），用户反馈录音状态容易卡住，
//! 改成 push-to-talk —— 没有"是否在录音"的歧义状态。

use std::sync::Arc;
use std::time::Duration;
use tauri::{AppHandle, Emitter};

use crate::events::{ReplyMode, ViewKind, EV_VIEW_CHANGED};
use crate::overlay::{bump_gen, emit_view, hide_overlay, schedule_auto_hide};
use crate::{audio, backend, mode_b, screenshot, transcribe};
use crate::AppState;

/// 完整 pipeline：截图 → session 记账 → AI 流式调用 → Mode A/B 输出。
pub async fn run_pipeline(transcript: String, app: AppHandle, state: Arc<AppState>) {
    println!("[mouseclaw] ▶ run_pipeline 开始, transcript={transcript:?}");
    // Bump generation so any pending auto-hide timer from a previous run no-ops
    bump_gen(&state);

    // 1. Use screenshot captured at shortcut-press time (recent + matches user intent)
    let (img_path, cursor_ctx) = match state.last_screenshot.lock().await.clone() {
        Some(p) if p.exists() => (p, None), // older snapshot, no cursor context preserved
        _ => match screenshot::capture_main_screen().await {
            Ok(r) => {
                let cursor = match (r.cursor, r.screen_size) {
                    (Some((x, y)), Some((w, h))) => Some(backend::CursorContext {
                        x,
                        y,
                        screen_w: w,
                        screen_h: h,
                    }),
                    _ => None,
                };
                (r.path, cursor)
            }
            Err(e) => {
                let reason = if !crate::permissions::check_screen_recording() {
                    "屏幕录制权限未授权 — 请前往「系统设置 → 隐私与安全性 → 屏幕录制」开启".into()
                } else {
                    format!("截图失败：{e}")
                };
                emit_view(&app, &ViewKind::Blocked { reason });
                schedule_auto_hide(&app, &state, 4000);
                return;
            }
        },
    };

    // 2. Session bookkeeping —— 前台 app 提前算好，touch() 用它判断「切 app = 新 session」
    let frontmost = mode_b::frontmost_app_name();
    let context_preamble = {
        let mut store = state.sessions.lock().await;
        store.touch(false, frontmost.as_deref());
        store.context_preamble()
    };
    let prompt_with_context = match &context_preamble {
        Some(pre) => format!("{pre}{transcript}"),
        None => transcript.clone(),
    };

    emit_view(&app, &ViewKind::Thinking { transcript: transcript.clone() });

    // v0.1.20 · 拿出已采样的鼠标轨迹（on_shortcut_release 烘到 screenshot 上的那批）
    let trail_summary = state.last_trail.lock().await.take().and_then(|trail| {
        if trail.is_empty() { return None; }
        let total = trail.len();
        let drawn: usize = trail.iter().filter(|p| p.left_button).count();
        let dt_ms = trail.last().map(|p| p.t_ms).unwrap_or(0);
        // 取 annotation 段的端点（按下→松开）
        let mut spans: Vec<(usize, usize)> = vec![];
        let mut span_start: Option<usize> = None;
        for (i, p) in trail.iter().enumerate() {
            match (p.left_button, span_start) {
                (true, None) => span_start = Some(i),
                (false, Some(s)) => { spans.push((s, i.saturating_sub(1))); span_start = None; }
                _ => {}
            }
        }
        if let Some(s) = span_start { spans.push((s, trail.len() - 1)); }
        Some(format!(
            "用户按住快捷键 {:.1}s，采了 {total} 个光标点，其中 {drawn} 个点是按住左键画的（{} 段标注）。",
            dt_ms as f32 / 1000.0, spans.len()
        ))
    });

    // 3. 流式调用当前后端 —— 边收边 emit Reply{streaming:true}，气泡实时长出来。
    //    节流：最多每 90ms emit 一次，避免 IPC 被 token 级事件刷爆。
    let active_backend = state.backend.lock().await.clone();
    let app_chunks = app.clone();
    let transcript_chunks = transcript.clone();
    let mut last_emit = std::time::Instant::now() - Duration::from_secs(1);
    let reply = match backend::ask_streaming(
        active_backend,
        &prompt_with_context,
        &img_path,
        frontmost.as_deref(),
        cursor_ctx.as_ref(),
        trail_summary.as_deref(),
        |accumulated| {
            let now = std::time::Instant::now();
            if now.duration_since(last_emit) >= Duration::from_millis(90) {
                last_emit = now;
                emit_view(
                    &app_chunks,
                    &ViewKind::Reply {
                        transcript: transcript_chunks.clone(),
                        reply: accumulated.to_string(),
                        mode: ReplyMode::A,
                        insert_text: None,
                        streaming: true,
                    },
                );
            }
        },
    )
    .await
    {
        Ok(r) => r,
        Err(e) => {
            eprintln!("[mouseclaw] ✘ backend streaming 失败: {e:#}");
            let reason = friendly_backend_error(&format!("{e:#}"), active_backend);
            emit_view(&app, &ViewKind::Blocked { reason });
            schedule_auto_hide(&app, &state, 6000);
            return;
        }
    };
    println!("[mouseclaw] ✓ AI 返回 {} chars", reply.chars().count());

    // 4. Record turns into session history
    {
        let mut store = state.sessions.lock().await;
        let screenshot_path = img_path.to_string_lossy().to_string();
        let _ = store.record_user(transcript.clone(), Some(screenshot_path)).await;
        let _ = store.record_assistant(reply.clone()).await;
    }

    // 5. Mode detection — [INSERT_AT_CURSOR] marker → Mode B (write at cursor)
    let insert_text = crate::claude_cli::parse_insert_directive(&reply);
    let (mode, final_insert) = match insert_text {
        Some(t) => match mode_b::assert_writable() {
            Ok(()) => (ReplyMode::B, Some(t)),
            Err(_) => (ReplyMode::A, None), // terminal foreground → degrade to A
        },
        None => (ReplyMode::A, None),
    };

    // 最终 Reply —— streaming:false，mode 已确定
    emit_view(
        &app,
        &ViewKind::Reply {
            transcript: transcript.clone(),
            reply: reply.clone(),
            mode,
            insert_text: final_insert.clone(),
            streaming: false,
        },
    );

    // 6. Mode B: 3-second countdown then write at cursor
    if let (ReplyMode::B, Some(text)) = (mode, final_insert) {
        for remaining in (1..=3).rev() {
            emit_view(
                &app,
                &ViewKind::ModeBCountdown {
                    insert_text: text.clone(),
                    remaining,
                },
            );
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
        emit_view(&app, &ViewKind::ModeBInserting { insert_text: text.clone() });
        if let Err(e) = mode_b::write_at_cursor(&text).await {
            emit_view(&app, &ViewKind::Blocked { reason: format!("写入失败：{e}") });
            schedule_auto_hide(&app, &state, 4000);
            return;
        }
        emit_view(
            &app,
            &ViewKind::Reply {
                transcript,
                reply: format!("✅ 已写入：{text}"),
                mode: ReplyMode::A,
                insert_text: None,
                streaming: false,
            },
        );
    }

    // ❌ V1 auto-hide 3s 太激进 —— 用户来不及看完就消失了。
    // v0.1.8 改成「不自动消失」：
    //   - 短回复（< 4 行）：保留 30s 让用户看清楚
    //   - 长回复：完全不自动消失（用户看完按 Esc / 点窗口外 / 切 app 关闭）
    //   - Mode B 有自己的倒计时 UI，不归这里管
    //   - 错误 / 阻塞：保留 6s 自动消失（错误信息没多重要，不需要长留）
    let is_long = reply.split('\n').count() >= 4 || reply.chars().count() > 140;
    if is_long {
        println!("[mouseclaw] reply 长回复 → 不自动消失，等用户 Esc / 点外 / 切 app");
        // 不调 schedule_auto_hide
    } else {
        println!("[mouseclaw] reply 短回复 → 30s 后自动消失");
        schedule_auto_hide(&app, &state, 30_000);
    }
}

// ────────────────── Push-to-talk shortcut handling ──────────────────

/// 按下快捷键：起手录音 + 截图。
pub async fn on_shortcut_press(app: AppHandle, state: Arc<AppState>) {
    bump_gen(&state);

    // 已经在录音中（hold + auto-repeat 会再次 fire Press）→ 跳过
    if state.recorder.lock().unwrap().is_some() {
        return;
    }

    if !transcribe::is_available() {
        let active = crate::config::Config::load().whisper_model;
        let msg = match transcribe::current_state() {
            Some(transcribe::ModelState::Downloading) => format!(
                "正在下载 Whisper {} 模型（~{}MB），下载完成后再试一次",
                active.display_name(), active.size_mb()
            ),
            Some(transcribe::ModelState::Failed(e)) => format!(
                "Whisper 模型下载失败：{e}（手动跑：curl -L -o ~/.mouseclaw/models/{} {}）",
                active.filename(), active.url()
            ),
            _ => format!("Whisper 模型未找到（~/.mouseclaw/models/{}）", active.filename()),
        };
        emit_view(&app, &ViewKind::Blocked { reason: msg });
        schedule_auto_hide(&app, &state, 6000);
        return;
    }

    let recorder = match audio::Recorder::start() {
        Ok(r) => r,
        Err(e) => {
            eprintln!("[mouseclaw] start recording: {e:#}");
            let reason = if !crate::permissions::check_microphone() {
                "麦克风权限未授权 — 请前往「系统设置 → 隐私与安全性 → 麦克风」开启".into()
            } else {
                format!("录音启动失败：{e}")
            };
            emit_view(&app, &ViewKind::Blocked { reason });
            schedule_auto_hide(&app, &state, 4000);
            return;
        }
    };
    *state.recorder.lock().unwrap() = Some(recorder);

    // v0.1.20 · 启动鼠标轨迹采样 + 实时 overlay（v0.1.21）
    #[cfg(target_os = "macos")]
    crate::cursor_trail::start(app.clone());

    // 截图并行（不阻塞录音）
    let state_clone = state.clone();
    let app_clone = app.clone();
    tauri::async_runtime::spawn(async move {
        match screenshot::capture_main_screen().await {
            Ok(r) => {
                *state_clone.last_screenshot.lock().await = Some(r.path);
            }
            Err(e) => eprintln!("[mouseclaw] screenshot: {e:#}"),
        }
        emit_view(&app_clone, &ViewKind::Listening);
    });
}

/// 松开快捷键：停止录音 + 收尾轨迹 + Whisper 转写 + 跑 pipeline。
pub async fn on_shortcut_release(app: AppHandle, state: Arc<AppState>) {
    let recorder = {
        let mut g = state.recorder.lock().unwrap();
        g.take()
    };
    let Some(recorder) = recorder else {
        return; // 没在录音 → 忽略
    };

    // v0.1.20 · 停轨迹采样，拿到点列。烘到 screenshot 上。
    #[cfg(target_os = "macos")]
    let trail = crate::cursor_trail::stop_and_take();
    #[cfg(not(target_os = "macos"))]
    let trail: Vec<crate::cursor_trail::TrailPoint> = Vec::new();

    if !trail.is_empty() {
        let annotated = state.last_screenshot.lock().await.clone();
        if let Some(path) = annotated {
            let trail_clone = trail.clone();
            let _ = tokio::task::spawn_blocking(move || {
                #[cfg(target_os = "macos")]
                if let Err(e) = crate::cursor_trail::render_onto_screenshot(&path, &trail_clone) {
                    eprintln!("[mouseclaw] 🐭 render trail failed: {e}");
                } else {
                    println!("[mouseclaw] 🐭 trail {} points 已烘到 {}",
                             trail_clone.len(), path.display());
                }
            }).await;
        }
        // 把 trail 也存进 AppState，给 run_pipeline 加进 prompt 上下文
        *state.last_trail.lock().await = Some(trail);
    }

    emit_view(&app, &ViewKind::Thinking { transcript: "(转写中…)".into() });

    let app_clone = app.clone();
    let state_clone = state.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let samples = match recorder.stop_and_take() {
            Ok(s) => s,
            Err(e) => {
                eprintln!("[mouseclaw] stop_and_take: {e:#}");
                let _ = app_clone.emit(
                    EV_VIEW_CHANGED,
                    ViewKind::Blocked { reason: format!("录音失败：{e}") },
                );
                schedule_auto_hide(&app_clone, &state_clone, 4000);
                return;
            }
        };
        println!(
            "[mouseclaw] captured {} samples @ 16kHz ({:.1}s)",
            samples.len(),
            samples.len() as f32 / 16_000.0
        );
        let transcript = match transcribe::transcribe(&samples) {
            Ok(t) => t,
            Err(e) => {
                eprintln!("[mouseclaw] transcribe: {e:#}");
                let _ = app_clone.emit(
                    EV_VIEW_CHANGED,
                    ViewKind::Blocked { reason: format!("Whisper 失败：{e}") },
                );
                schedule_auto_hide(&app_clone, &state_clone, 4000);
                return;
            }
        };
        println!("[mouseclaw] transcript (raw): {transcript:?}");
        if transcript.is_empty() {
            hide_overlay(&app_clone);
            return;
        }
        // v0.1.10 · 双层 tidy：
        //   Layer 1 light_clean (50ms, 永远开) —— regex 去口头禅 + 收敛标点
        //   Layer 2 LLM tidy (3-8s, opt-in)   —— config.tidy_up_enabled 才走
        // 默认只跑 Layer 1，保流畅；想要精修的用户开 Layer 2。
        tauri::async_runtime::spawn(async move {
            let cfg = crate::config::Config::load();

            // Layer 1: 即时 light clean，无感
            let light = crate::tidy_up::light_clean(&transcript, &cfg.language);
            println!("[mouseclaw] transcript (light): {light:?}");

            // Layer 2: opt-in LLM tidy
            let final_transcript = if cfg.tidy_up_enabled {
                let _ = app_clone.emit(EV_VIEW_CHANGED, ViewKind::Thinking {
                    transcript: format!("（{}…）",
                        if cfg.language == "en" { "polishing voice" } else { "精修语音" }),
                });
                match crate::tidy_up::tidy(&light, cfg.backend, &cfg.language).await {
                    Ok(cleaned) => {
                        println!("[mouseclaw] transcript (LLM tidied): {cleaned:?}");
                        cleaned
                    }
                    Err(e) => {
                        eprintln!("[mouseclaw] LLM tidy 失败，用 light 版兜底: {e}");
                        light
                    }
                }
            } else {
                light
            };
            run_pipeline(final_transcript, app_clone, state_clone).await;
        });
    });
}

/// 把后端 CLI 抛回来的吓人英文 stderr 翻译成用户能执行的中文一句话。
/// 常见 case：
///   - 找不到二进制 → 给出 npm install 命令
///   - 登录过期 / 没 API key → 提示 `claude login` / 检查 env
///   - rate limit / 5xx → 提示稍后重试
///   - 其它 → 原样 + 「按 ⌘空格 重试」尾巴
fn friendly_backend_error(raw: &str, backend: crate::backend::Backend) -> String {
    let lower = raw.to_lowercase();
    let bin = backend.binary_name();
    if lower.contains("找不到") || lower.contains("no such file") || lower.contains("not found")
        || lower.contains("command not found")
    {
        return match backend {
            crate::backend::Backend::ClaudeCli =>
                "找不到 claude CLI。装一下：`npm i -g @anthropic-ai/claude-code`，然后跑 `claude login` 登录".into(),
            crate::backend::Backend::CodexCli =>
                "找不到 codex CLI。装一下：`npm i -g @openai/codex`，并配好 OpenAI API key".into(),
            crate::backend::Backend::OpenclawCli =>
                "找不到 openclaw CLI。装一下：`npm i -g openclaw`，并在 shell 配 provider key".into(),
            crate::backend::Backend::HermesAgent =>
                "找不到 hermes CLI。装一下：`curl -fsSL https://raw.githubusercontent.com/NousResearch/hermes-agent/main/scripts/install.sh | bash`，然后跑 `hermes setup` 配 API key".into(),
        };
    }
    if lower.contains("login") || lower.contains("authentic") || lower.contains("unauthorized") || lower.contains("401") {
        return format!("{bin} 似乎没登录或 token 过期。跑 `{bin} login` 重新登录后再试");
    }
    if lower.contains("rate") && lower.contains("limit") {
        return "API rate limit 触发 — 等 30s 再试一次".into();
    }
    if lower.contains("500") || lower.contains("502") || lower.contains("503") || lower.contains("timeout") {
        return "AI 服务暂时抽风（5xx / 超时）— 重试一次试试".into();
    }
    // 兜底：原文截到 200 字 + 重试提示
    let trimmed: String = raw.chars().take(200).collect();
    format!("AI 调用失败：{trimmed}\n（按 ⌘⇧空格 重试 · 在托盘点「📊 系统状态」可诊断）")
}

/// Legacy alias — tray summon uses this. Maps to "tap": press + 800ms + release.
/// tray 点一下没法 hold，只能模拟一个最短录音。
pub async fn on_shortcut_pressed(app: AppHandle, state: Arc<AppState>) {
    on_shortcut_press(app.clone(), state.clone()).await;
    tokio::time::sleep(Duration::from_millis(800)).await;
    on_shortcut_release(app, state).await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::Backend;

    #[test]
    fn missing_binary_maps_to_install_command() {
        let msg = friendly_backend_error("找不到 `claude` CLI", Backend::ClaudeCli);
        assert!(msg.contains("npm i -g @anthropic-ai/claude-code"));
        let msg = friendly_backend_error("command not found: codex", Backend::CodexCli);
        assert!(msg.contains("@openai/codex"));
    }

    #[test]
    fn auth_errors_suggest_login() {
        let msg = friendly_backend_error("401 Unauthorized", Backend::ClaudeCli);
        assert!(msg.contains("login"));
    }

    #[test]
    fn rate_limit_known() {
        let msg = friendly_backend_error("Error 429: rate limit exceeded", Backend::ClaudeCli);
        assert!(msg.contains("rate limit"));
    }

    #[test]
    fn unknown_errors_truncate_and_add_retry_hint() {
        let long_err = "a".repeat(500);
        let msg = friendly_backend_error(&long_err, Backend::ClaudeCli);
        assert!(msg.chars().count() < 350); // 200 截断 + 重试提示
        assert!(msg.contains("⌘⇧空格"));
    }
}
