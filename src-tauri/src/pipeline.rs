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
use crate::{audio, backend, mode_b, screenshot};
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

    // v0.4 · 看 state.fed_docs：有就把"喂"给桌宠的文件 preamble 前置到 transcript
    // 取走（take）→ 一次性消费，跑完就清空。
    let feed_preamble = state.fed_docs.lock().await.take().and_then(|b| b.prompt_preamble());

    let prompt_with_context = match (&context_preamble, &feed_preamble) {
        (Some(ctx), Some(feed)) => format!("{ctx}{feed}{transcript}"),
        (Some(ctx), None) => format!("{ctx}{transcript}"),
        (None, Some(feed)) => format!("{feed}{transcript}"),
        (None, None) => transcript.clone(),
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
    // v0.4 · AI 任务串行队列 —— 若有 reactive action / 另一次召唤在跑，这里 await 等它完成。
    //   ticket 持有到 ask_streaming 结束（drop 在本作用域末）。期间桌宠显示忙碌。
    //   听写（fn IME）不走队列，不受影响。见 CLAUDE.md "AI 任务串行 + 听写即时"。
    let _ai_ticket = crate::ai_queue::acquire().await;
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
    // AI 调用结束 → 立即放锁，让下一个排队任务能进来（Mode A/B 输出 / session 存盘
    // 不需要 AI 锁，不该让它们继续阻塞队列）。
    drop(_ai_ticket);

    // 4. Record turns into session history
    {
        let mut store = state.sessions.lock().await;
        // 软提示要看"这一轮之前"的间隔 —— record 会把 last_turn_at 拨到 now，所以先抓。
        let soft_hint = store.should_soft_hint();
        let screenshot_path = img_path.to_string_lossy().to_string();
        let _ = store.record_user(transcript.clone(), Some(screenshot_path)).await;
        let _ = store.record_assistant(reply.clone()).await;
        // v0.4.x · 广播 session 状态给前端（链条图标 + 第 N 轮 + 钉住 + 软提示）
        let mut snap = store.state_snapshot();
        snap.soft_hint = soft_hint;
        let _ = app.emit(crate::events::EV_SESSION_STATE, snap);
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

    // v0.4.0 · 桌宠开口说话 —— 用 macOS `say` 朗读 AI 最终回复。
    // 默认 OFF（声音打扰，用户主动从托盘开）。Mode B 跳过（writing 视觉本身就够，
    // 再叠语音反而吵）。
    #[cfg(target_os = "macos")]
    if mode != ReplyMode::B {
        let cfg_tts = crate::config::Config::load();
        if cfg_tts.tts_enabled {
            crate::tts::speak(&reply, &cfg_tts.language);
        }
    }

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

        // v0.4.x · 续写前先把光标焦点还原到召唤时那个 app —— 倒数 / AI 处理期间
        // 用户可能切走了。还原成功 = 光标回到原输入框；还原不了（app 关了 / 切到
        // 别处拿不回） = 光标丢了 → 落剪贴板让用户自己 ⌘V，绝不盲插到错的地方。
        let cursor_ok = restore_cursor_for_insert(&state).await;

        if !cursor_ok {
            // 光标丢了 → 剪贴板兜底
            let copied = mode_b::set_clipboard(&text).is_ok();
            let msg = if copied {
                format!("📋 原输入框失焦了，内容已复制 · ⌘V 粘贴：\n{text}")
            } else {
                format!("⚠️ 原输入框失焦、且复制失败：\n{text}")
            };
            emit_view(&app, &ViewKind::Reply {
                transcript, reply: msg, mode: ReplyMode::A, insert_text: None, streaming: false,
            });
            schedule_auto_hide(&app, &state, 8000);
            return;
        }

        if let Err(e) = mode_b::write_at_cursor(&text).await {
            // 写入失败（终端等不可写 app）→ 也走剪贴板兜底，别只甩错误
            let copied = mode_b::set_clipboard(&text).is_ok();
            let msg = if copied {
                format!("📋 没法直接写入（{e}），已复制 · ⌘V 粘贴：\n{text}")
            } else {
                format!("写入失败：{e}")
            };
            emit_view(&app, &ViewKind::Reply {
                transcript, reply: msg, mode: ReplyMode::A, insert_text: None, streaming: false,
            });
            schedule_auto_hide(&app, &state, 8000);
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

/// v0.4.x · Mode B 续写前还原光标焦点到召唤时的 app。
/// 返回 true = 光标可用（原 app 仍在前台 / 成功 activate 回来）；
/// false = 光标丢了（没抓到原 pid / activate 失败 / 还原后前台仍不对）→ 调用方走剪贴板兜底。
#[cfg(target_os = "macos")]
async fn restore_cursor_for_insert(state: &Arc<AppState>) -> bool {
    let prev_pid = *state.prev_frontmost_pid.lock().unwrap();
    let Some(pid) = prev_pid else { return false; };
    if crate::frontmost::current_frontmost_pid() == Some(pid) {
        return true; // 原 app 还在前台，光标没丢
    }
    // 用户切走了 → 尝试把原 app 拉回前台
    if !crate::frontmost::activate_pid(pid) {
        return false; // activate 失败（app 已关 / 拉不回）
    }
    tokio::time::sleep(Duration::from_millis(120)).await;
    crate::frontmost::current_frontmost_pid() == Some(pid) // 再确认真的回来了
}
#[cfg(not(target_os = "macos"))]
async fn restore_cursor_for_insert(_state: &Arc<AppState>) -> bool { true }

/// 按下快捷键：起手录音 + 截图。
pub async fn on_shortcut_press(app: AppHandle, state: Arc<AppState>) {
    bump_gen(&state);

    // 已经在录音中（hold + auto-repeat 会再次 fire Press）→ 跳过
    if state.recorder.lock().unwrap().is_some() {
        return;
    }

    // v0.4.x · 抓住召唤瞬间的前台 app pid —— Mode B 续写要靠它把光标焦点还原回去
    // （倒数 / AI 处理期间用户可能切走了）。拿不到也不致命，Mode B 会退回剪贴板。
    #[cfg(target_os = "macos")]
    {
        let pid = crate::frontmost::current_frontmost_pid();
        *state.prev_frontmost_pid.lock().unwrap() = pid;
    }

    // v0.2 · check sherpa streaming model
    // v0.4.0 · 模型走 lazy download，blocked 气泡显示**实时下载进度**让用户知道在干嘛
    if !crate::transcribe_stream::is_ready() {
        let msg = build_model_blocked_msg();
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

    // v0.2 · 同步创建 streaming session（lazy load 模型 ~500ms 第一次，之后 ~0）
    match crate::transcribe_stream::StreamSession::new() {
        Ok(s) => *state.stream_session.lock().unwrap() = Some(s),
        Err(e) => {
            eprintln!("[mouseclaw] StreamSession::new failed: {e:#}");
            // 没 streaming 也别完全 block —— 让录音继续，partial 不出来而已
        }
    }
    state.streaming_active.store(true, std::sync::atomic::Ordering::SeqCst);

    // v0.3.1 · 150ms 轮询任务 —— 把 recorder buffer 喂进 sherpa stream，emit 实时 partial
    // 之前 200ms 偏慢；150ms 给"边说边出"更紧凑的感觉
    let state_stream = state.clone();
    let app_stream = app.clone();
    tauri::async_runtime::spawn(async move {
        let mut ticker = tokio::time::interval(std::time::Duration::from_millis(150));
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut last_partial = String::new();
        let mut total_samples_fed: usize = 0;
        loop {
            ticker.tick().await;
            if !state_stream.streaming_active.load(std::sync::atomic::Ordering::SeqCst) {
                break;
            }
            let samples = {
                let g = state_stream.recorder.lock().unwrap();
                match g.as_ref() {
                    Some(r) => r.drain_resampled_16k(),
                    None => break,
                }
            };
            if samples.is_empty() { continue; }
            total_samples_fed += samples.len();
            let partial = {
                let mut g = state_stream.stream_session.lock().unwrap();
                if let Some(s) = g.as_mut() {
                    s.accept(&samples);
                    s.partial()
                } else {
                    continue;
                }
            };
            if partial != last_partial {
                println!("[mouseclaw] 🎤 partial ({} samples fed): {:?}",
                    total_samples_fed, partial);
                last_partial = partial.clone();
                // v0.4.0 · 流式 partial 加标点（短片段跳过，避免单字符 punct 误判）
                let display = if partial.chars().count() >= 4 {
                    crate::punctuation::add_punctuation(&partial)
                } else {
                    partial.clone()
                };
                emit_view(&app_stream, &ViewKind::Listening { partial: display });
            }
        }
        println!("[mouseclaw] 🎤 streaming poller exited (total {} samples fed)", total_samples_fed);
    });

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
        emit_view(&app_clone, &ViewKind::Listening { partial: String::new() });
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

    // v0.2 · 停 streaming polling 任务 → 让 200ms loop 看到 false 退出
    state.streaming_active.store(false, std::sync::atomic::Ordering::SeqCst);

    emit_view(&app, &ViewKind::Thinking { transcript: "(转写中…)".into() });

    let app_clone = app.clone();
    let state_clone = state.clone();
    tauri::async_runtime::spawn_blocking(move || {
        // v0.2 · 拿走剩余样本（不再走 channel）+ 喂 sherpa final + finalize
        let remaining = match recorder.stop_drain_remaining_16k() {
            Ok(s) => s,
            Err(e) => {
                eprintln!("[mouseclaw] stop_drain_remaining_16k: {e:#}");
                let _ = app_clone.emit(
                    EV_VIEW_CHANGED,
                    ViewKind::Blocked { reason: format!("录音失败：{e}") },
                );
                schedule_auto_hide(&app_clone, &state_clone, 4000);
                return;
            }
        };
        println!(
            "[mouseclaw] tail {} samples @ 16kHz ({:.2}s)",
            remaining.len(),
            remaining.len() as f32 / 16_000.0
        );
        let session = {
            let mut g = state_clone.stream_session.lock().unwrap();
            g.take()
        };
        let transcript = if let Some(mut sess) = session {
            if !remaining.is_empty() { sess.accept(&remaining); }
            match sess.finalize() {
                Ok(t) => t,
                Err(e) => {
                    eprintln!("[mouseclaw] sherpa finalize: {e:#}");
                    let _ = app_clone.emit(
                        EV_VIEW_CHANGED,
                        ViewKind::Blocked { reason: format!("转写失败：{e}") },
                    );
                    schedule_auto_hide(&app_clone, &state_clone, 4000);
                    return;
                }
            }
        } else {
            eprintln!("[mouseclaw] stream session missing on release — empty transcript");
            String::new()
        };
        println!("[mouseclaw] transcript (raw): {transcript:?}");
        if transcript.is_empty() {
            hide_overlay(&app_clone);
            return;
        }
        // v0.3.4 · 只跑 light_clean（regex, 5ms）—— LLM polish 删除
        // v0.3.6 · light_clean 后再过本地标点模型（sherpa CT-Transformer，~10ms）
        tauri::async_runtime::spawn(async move {
            let cfg = crate::config::Config::load();
            let light = crate::tidy_up::light_clean(&transcript, &cfg.language);
            println!("[mouseclaw] transcript (light): {light:?}");
            let punctuated = crate::punctuation::add_punctuation(&light);
            if punctuated != light {
                println!("[mouseclaw] transcript (punctuated): {punctuated:?}");
            }
            // v0.4.0 · A 方案 · 3 秒倒数确认 —— 防误识别浪费 token。
            // Esc 取消 / Enter 立即发 / 点气泡进 edit / 默认 3s 后自动发。
            let final_text = match voice_confirm_countdown(&app_clone, &state_clone, &punctuated).await {
                Some(t) => t,
                None => { hide_overlay(&app_clone); return; }
            };
            run_pipeline(final_text, app_clone, state_clone).await;
        });
    });
}

/// 把后端 CLI 抛回来的吓人英文 stderr 翻译成用户能执行的中文一句话。
/// 常见 case：
///   - 找不到二进制 → 给出 npm install 命令
///   - 登录过期 / 没 API key → 提示 `claude login` / 检查 env
///   - rate limit / 5xx → 提示稍后重试
///   - 其它 → 原样 + 「按 ⌘空格 重试」尾巴
/// v0.4.0 · 语音确认倒数 —— 转写完后 3 秒确认窗口。
/// 返回值：
///   - `Some(text)` = 用户确认（或倒数完毕） → 发给 AI；text 可能被用户编辑过
///   - `None` = 用户取消（Esc / 点取消）→ 调用方应 hide_overlay 不跑 pipeline
///
/// 协议：通过 `state.voice_confirm_action` AtomicU8 + commands 传递用户操作：
///   - VC_PENDING = 还在倒数
///   - VC_SEND_NOW = 用户按 Enter / 倒数完 → 立即发
///   - VC_CANCEL = 用户按 Esc → 取消
///
/// 编辑路径：用户点气泡 → 前端弹输入框 → 改完 → 调 `voice_confirm_edit(new_text)`
/// 命令把新文本回写 + 标记 SEND_NOW。这部分由 commands.rs 实现，本函数只需读结果。
pub async fn voice_confirm_countdown(
    app: &AppHandle,
    state: &Arc<AppState>,
    initial_text: &str,
) -> Option<String> {
    use std::str::FromStr;
    use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut};

    // ⚠️ overlay 是非激活 NSPanel（focus:false）—— textarea 的 el.focus() 只是 DOM 级，
    // 物理 Esc/Enter 键事件其实发给**前台 app**，到不了 webview，所以 onKeyDown 里的
    // Esc 取消根本不触发，倒数照样自动发送（用户报：「按了 esc 还是会去执行」）。
    // 解：倒数期间注册一个临时全局 Esc，由 lib.rs 的 shortcut handler 设 VC_CANCEL。
    // 注册失败（极少数系统不让注册裸 Esc）则退回「点取消按钮」的老路径，不致命。
    let esc = Shortcut::from_str("Escape").ok();
    if let Some(s) = &esc {
        match app.global_shortcut().register(s.clone()) {
            Ok(()) => println!("[mouseclaw] voice-confirm: 临时全局 Esc 已注册"),
            Err(e) => eprintln!("[mouseclaw] voice-confirm: Esc 注册失败（退回点取消）: {e}"),
        }
    }

    let result = voice_confirm_loop(app, state, initial_text).await;

    if let Some(s) = &esc {
        let _ = app.global_shortcut().unregister(s.clone());
    }
    result
}

async fn voice_confirm_loop(
    app: &AppHandle,
    state: &Arc<AppState>,
    initial_text: &str,
) -> Option<String> {
    use std::sync::atomic::Ordering;
    use crate::{VC_PENDING, VC_SEND_NOW, VC_CANCEL, VC_HOLD};

    // 写入初始文本到 state，供 commands::voice_confirm_edit/hold 读取 + 改写
    *state.voice_confirm_text.lock().await = Some(initial_text.to_string());
    state.voice_confirm_action.store(VC_PENDING, Ordering::SeqCst);

    // v0.3.11 · 6 秒倒数（之前 3 秒用户来不及改）+ 用户开始打字 HOLD 暂停倒数。
    //   HOLD 状态：remaining 锁在 99 表示"编辑中"，前端据此显示"✏️ 编辑中"。
    const COUNTDOWN_SECS: u32 = 6;
    let mut remaining = COUNTDOWN_SECS;
    let mut holding = false;

    loop {
        let cur = state.voice_confirm_text.lock().await.clone().unwrap_or_default();
        emit_view(app, &ViewKind::VoiceConfirm {
            transcript: cur,
            remaining: if holding { 99 } else { remaining },
        });
        // 1 秒切 10 片 × 100ms 让 Enter/Esc 快响应
        for _ in 0..10 {
            tokio::time::sleep(Duration::from_millis(100)).await;
            let action = state.voice_confirm_action.load(Ordering::SeqCst);
            match action {
                a if a == VC_SEND_NOW => {
                    let text = state.voice_confirm_text.lock().await.take()
                        .unwrap_or_else(|| initial_text.to_string());
                    state.voice_confirm_action.store(VC_PENDING, Ordering::SeqCst);
                    return Some(text);
                }
                a if a == VC_CANCEL => {
                    *state.voice_confirm_text.lock().await = None;
                    state.voice_confirm_action.store(VC_PENDING, Ordering::SeqCst);
                    return None;
                }
                a if a == VC_HOLD => {
                    // 进入 hold：reset flag 回 PENDING 但记 holding=true
                    holding = true;
                    state.voice_confirm_action.store(VC_PENDING, Ordering::SeqCst);
                }
                _ => {}
            }
        }
        if holding {
            // 保持显示"编辑中"，不递减 —— 等用户主动 Enter / Esc
            continue;
        }
        if remaining <= 1 {
            // 倒数完毕 = 默认发送
            let text = state.voice_confirm_text.lock().await.take()
                .unwrap_or_else(|| initial_text.to_string());
            state.voice_confirm_action.store(VC_PENDING, Ordering::SeqCst);
            return Some(text);
        }
        remaining -= 1;
    }
}

/// v0.4.0 · 组装 blocked 气泡里的"模型未就绪"消息。
/// 优先用 model_downloader 的实时进度快照（具体下到第几个文件 / 哪个镜像 / 多少 MB），
/// fallback 到老的 ModelState 文案。
pub fn build_model_blocked_msg() -> String {
    use crate::model_downloader::latest_progress;
    // 优先看 active 语言模型进度（用户最关心 —— 这是按快捷键被挡住时唯一关心的事）
    let active_id = crate::transcribe_stream::active_spec().id;
    if let Some(p) = latest_progress(active_id) {
        return format_progress(&p);
    }
    // 还没收到任何 progress event —— 用 active spec 算总 MB，不再 hardcode 199
    let spec = crate::transcribe_stream::active_spec();
    let total_mb = spec.files.iter().map(|f| f.bytes).sum::<u64>() as f64 / 1024.0 / 1024.0;
    match crate::transcribe_stream::current_state() {
        Some(crate::transcribe_stream::ModelState::Downloading) =>
            format!("🦞 正在准备语音模型（首次启动需下 ~{:.0}MB），完成后再试", total_mb),
        Some(crate::transcribe_stream::ModelState::Failed(e)) =>
            format!("🦞 语音模型下载失败：{e}\n托盘 → 📥 模型下载进度 → 🔁 重试"),
        _ => "🦞 语音模型未就绪，请稍候".into(),
    }
}

fn format_progress(p: &crate::model_downloader::ProgressEvent) -> String {
    let pct = if p.total_expected > 0 {
        (p.total_done as f64 / p.total_expected as f64 * 100.0).min(100.0) as u32
    } else { 0 };
    let done_mb = p.total_done as f64 / 1024.0 / 1024.0;
    let tot_mb = p.total_expected as f64 / 1024.0 / 1024.0;
    let host = p.mirror.strip_prefix("https://")
        .and_then(|s| s.split('/').next()).unwrap_or("");
    let footer = "\n（托盘 📥 模型下载进度 可查看详情）";
    match p.phase {
        "downloading" => format!(
            "🦞 正在下载语音模型…\n{:.0}/{:.0} MB · {}% · {}{}",
            done_mb, tot_mb, pct, host, footer
        ),
        "fallback" => format!(
            "🦞 镜像 {host} 不通，切换中…\n已下 {:.0}/{:.0} MB{}",
            done_mb, tot_mb, footer
        ),
        "verifying" => format!("🦞 解压并校验模型…{footer}"),
        "error" => {
            let detail = p.message.as_deref().unwrap_or("");
            format!("🦞 语音模型下载失败：{detail}\n托盘 → 📥 模型下载进度 → 🔁 重试")
        }
        _ => format!("🦞 准备语音模型中…{footer}"),
    }
}

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
