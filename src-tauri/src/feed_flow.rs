//! v0.4 · "拖文档喂桌宠 → 语音提问" 编排层。
//!
//! 时序：
//!   1. macOS WindowEvent::DragDrop::Enter 文件命中"mouse"窗口 → emit FeedWaiting
//!   2. WindowEvent::DragDrop::Drop  files → 调 ingest（异步）→ 存进 state.fed_docs
//!   3. 立刻起一个 Recorder + sherpa StreamSession，每 150ms 喂 partial
//!   4. partial 停 2.5s 不变 (silence) 且 >= 1 字符 → finalize → run_pipeline
//!   5. 12s 兜底超时 → 同样 finalize
//!   6. run_pipeline 看到 state.fed_docs → 把 prompt_preamble 前置到 transcript
//!   7. 跑完 fed_docs 自动清空
//!
//! 不复用 voice_ime / on_shortcut_press —— 那两个有自己的 type-to-cursor 副作用。
//! feed 流要的是干净的"录音 → 转写 → 当问题送 AI"，跟 AI 召唤一样但跳过快捷键。

use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{Duration, Instant};

use tauri::{AppHandle, Manager};

use crate::events::ViewKind;
use crate::overlay::{emit_view, show_mouse};
use crate::{audio, feed, transcribe_stream, AppState};

/// 静默判定：用户**说话之后**多久没新字了就算说完了。
const SILENCE_TIMEOUT_MS: u64 = 2500;
/// 用户开口前的耐心 —— drop 之后这么久内必须出第一个字，否则当作他不想问。
/// 不能用 SILENCE_TIMEOUT_MS 共用一个计时器（那样的话用户走神 3 秒还没开口就被掐）。
const FIRST_WORD_GRACE_MS: u64 = 8_000;
/// 兜底：总录音不超过这个，避免用户走开后录到空气。
const MAX_RECORD_MS: u64 = 30_000;
/// 最低必须录到这么多字 partial 才允许 finalize（避免抓到一声"嗯"就发）。
const MIN_PARTIAL_CHARS: usize = 1;
/// 吞咽过渡：drop 之后先停 600ms 让 CSS gulp 动画跑完 + 给用户视觉确认"它真的吃下了"，
/// 再切到 FeedListening 开始录音。少了这一步用户感觉不到吃下，会以为 drop 失败。
const GULP_DURATION_MS: u64 = 600;

/// 入口：drop 命中后调用。立刻返回，重活全部 spawn 出去。
pub fn on_files_dropped(app: AppHandle, state: Arc<AppState>, paths: Vec<PathBuf>) {
    println!("[mouseclaw] 🍽️ drop {} files", paths.len());
    state.feed_drag_active.store(false, Ordering::SeqCst);

    // 录音前置检查 —— 已在录其它东西就拒收
    if state.recorder.lock().unwrap().is_some() {
        eprintln!("[mouseclaw] 🍽️ recorder busy, drop ignored");
        emit_view(&app, &ViewKind::Blocked {
            reason: "正在录其它语音，等一下再喂".into(),
        });
        return;
    }
    if !transcribe_stream::is_ready() {
        // v0.4.0 · 显示实时下载进度，复用 pipeline 的组装函数
        emit_view(&app, &ViewKind::Blocked {
            reason: crate::pipeline::build_model_blocked_msg(),
        });
        return;
    }

    show_mouse(&app);

    let app_bg = app.clone();
    let state_bg = state.clone();
    tauri::async_runtime::spawn(async move {
        // 1. ingest（图片秒回；PDF/docx 可能 1-3s）
        let bundle = feed::ingest(paths).await;
        if !bundle.has_accepted() {
            let reasons: Vec<String> = bundle
                .files
                .iter()
                .filter_map(|f| f.reject_reason.as_ref().map(|r| format!("{}: {}", f.name, r)))
                .collect();
            emit_view(&app_bg, &ViewKind::Blocked {
                reason: format!("没吃下任何文件 — {}", reasons.join("; ")),
            });
            return;
        }
        let names: Vec<String> = bundle
            .files
            .iter()
            .filter(|f| f.kind != feed::FeedKind::Reject)
            .map(|f| f.name.clone())
            .collect();
        println!("[mouseclaw] 🍽️ ingested: {:?}", names);

        *state_bg.fed_docs.lock().await = Some(bundle);

        // 2. 起录音 + 流式 session
        let recorder = match audio::Recorder::start() {
            Ok(r) => r,
            Err(e) => {
                eprintln!("[mouseclaw] 🍽️ recorder: {e:#}");
                state_bg.fed_docs.lock().await.take();
                let reason = if !crate::permissions::check_microphone() {
                    "麦克风权限未开 — 系统设置 → 隐私 → 麦克风 把 MouseClaw 打开".into()
                } else {
                    format!("录音启动失败：{e}")
                };
                emit_view(&app_bg, &ViewKind::Blocked { reason });
                return;
            }
        };
        *state_bg.recorder.lock().unwrap() = Some(recorder);
        match transcribe_stream::StreamSession::new() {
            Ok(s) => *state_bg.stream_session.lock().unwrap() = Some(s),
            Err(e) => eprintln!("[mouseclaw] 🍽️ StreamSession::new: {e:#}"),
        }
        state_bg.streaming_active.store(true, Ordering::SeqCst);

        // 2.5 · 视觉吞咽过渡 —— FeedWaiting 状态 + 600ms 让 CSS gulp 动画跑完。
        // 没这一帧用户感觉不到"它真的吃下了"，会以为 drop 失败。
        emit_view(&app_bg, &ViewKind::FeedWaiting);
        tokio::time::sleep(Duration::from_millis(GULP_DURATION_MS)).await;

        emit_view(&app_bg, &ViewKind::FeedListening {
            files: names.clone(),
            partial: String::new(),
        });

        // 3. 150ms poll：喂 sherpa + 监听静默
        record_until_silent(app_bg, state_bg, names).await;
    });
}

/// 后台主循环：边录边推 partial，partial 停 SILENCE_TIMEOUT_MS 就 finalize。
///
/// 两层计时器（**这是修过一次的 bug**，原版只有一个 last_change，用户慢一拍开口就被掐）：
///   - first_word_time = None：还没出第一个字，用 FIRST_WORD_GRACE_MS（~8s）等用户
///   - first_word_time = Some(_)：已经出字了，进入 SILENCE_TIMEOUT_MS（~2.5s）静默检测
async fn record_until_silent(app: AppHandle, state: Arc<AppState>, files: Vec<String>) {
    let start = Instant::now();
    let mut first_word_time: Option<Instant> = None;
    let mut last_change = Instant::now();
    let mut last_partial = String::new();
    let mut ticker = tokio::time::interval(Duration::from_millis(150));
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        ticker.tick().await;
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
        if !samples.is_empty() {
            let partial = {
                let mut g = state.stream_session.lock().unwrap();
                if let Some(s) = g.as_mut() {
                    s.accept(&samples);
                    s.partial()
                } else {
                    break;
                }
            };
            if partial != last_partial {
                last_partial = partial.clone();
                last_change = Instant::now();
                if first_word_time.is_none()
                    && last_partial.chars().count() >= MIN_PARTIAL_CHARS
                {
                    first_word_time = Some(Instant::now());
                    println!("[mouseclaw] 🍽️ first word @ {}ms", start.elapsed().as_millis());
                }
                emit_view(&app, &ViewKind::FeedListening {
                    files: files.clone(),
                    partial,
                });
            }
        }

        let total = start.elapsed();
        match first_word_time {
            None => {
                // 还没出字 —— 用 grace 等用户
                if total >= Duration::from_millis(FIRST_WORD_GRACE_MS) {
                    println!("[mouseclaw] 🍽️ no first word in {}ms → give up",
                        FIRST_WORD_GRACE_MS);
                    break;
                }
            }
            Some(_) => {
                // 已经在说 —— 用 silence 判断说完
                let silent_for = last_change.elapsed();
                if silent_for >= Duration::from_millis(SILENCE_TIMEOUT_MS) {
                    println!("[mouseclaw] 🍽️ silence {}ms after speech → finalize",
                        silent_for.as_millis());
                    break;
                }
                if total >= Duration::from_millis(MAX_RECORD_MS) {
                    println!("[mouseclaw] 🍽️ max-record {}ms → finalize",
                        total.as_millis());
                    break;
                }
            }
        }
    }

    // 4. finalize
    state.streaming_active.store(false, Ordering::SeqCst);
    let recorder = state.recorder.lock().unwrap().take();
    let session = state.stream_session.lock().unwrap().take();

    let transcript = tokio::task::spawn_blocking(move || -> anyhow::Result<String> {
        let remaining = match recorder {
            Some(r) => r.stop_drain_remaining_16k()?,
            None => Vec::new(),
        };
        let Some(mut sess) = session else {
            return Ok(String::new());
        };
        if !remaining.is_empty() {
            sess.accept(&remaining);
        }
        sess.finalize()
    })
    .await
    .unwrap_or_else(|e| Err(anyhow::anyhow!("join: {e}")))
    .unwrap_or_else(|e| {
        eprintln!("[mouseclaw] 🍽️ finalize: {e:#}");
        String::new()
    });
    println!("[mouseclaw] 🍽️ transcript = {transcript:?}");

    let transcript = crate::tidy_up::light_clean(&transcript, &crate::config::Config::load().language);

    if transcript.trim().is_empty() {
        // 用户没说话 —— 清掉 fed_docs，气泡告知
        state.fed_docs.lock().await.take();
        emit_view(&app, &ViewKind::Blocked {
            reason: "我吃完啦，但没听到你说什么 🥲 重新拖一次再问吧".into(),
        });
        crate::overlay::schedule_auto_hide(&app, &state, 4000);
        return;
    }

    // v0.4.0 · 5. 跑 confirm 倒数（A 方案） —— 喂文件场景也容易语音误识别，
    // 给用户 3 秒 Esc 取消 / Enter 立即 / 点击编辑的机会。
    let final_text = match crate::pipeline::voice_confirm_countdown(&app, &state, &transcript).await {
        Some(t) => t,
        None => {
            state.fed_docs.lock().await.take();  // 取消了也清掉 fed_docs 不污染下次
            crate::overlay::hide_overlay(&app);
            return;
        }
    };
    // 6. 跑 pipeline —— 它会从 state.fed_docs 取走 bundle，注入 prompt 前置
    crate::pipeline::run_pipeline(final_text, app, state).await;
}

/// 用户在拖动期间把光标离开桌宠窗口 —— 重置等待态，停跑步动画。
/// 防抖：macOS 在拖动期间会反复 enter/leave，加 200ms debounce 避免动画闪烁。
pub fn on_drag_leave(app: &AppHandle, state: &Arc<AppState>) {
    // Schedule leave with a 200ms debounce —— 如果同一拖动 session 200ms 内 re-enter，cancel
    let state_clone = state.clone();
    let app_clone = app.clone();
    tauri::async_runtime::spawn(async move {
        // 记录"我尝试离开的时刻"
        let leave_time = std::time::Instant::now();
        *state_clone.feed_last_leave.lock().unwrap() = Some(leave_time);
        tokio::time::sleep(Duration::from_millis(200)).await;
        // 200ms 后：如果还是这次 leave_time（没有 re-enter 重置），才真的退场
        let last = *state_clone.feed_last_leave.lock().unwrap();
        if last == Some(leave_time) && state_clone.feed_drag_active.load(Ordering::SeqCst) {
            println!("[mouseclaw] 🍽️ drag leave confirmed (200ms debounce)");
            state_clone.feed_drag_active.store(false, Ordering::SeqCst);
            crate::cursor_follow::disable(&state_clone);
            // v0.4 fix (2026-05-20)：跟 hide_overlay 同一个顺序坑 ——
            // 先 emit_view(Idle) 触发 shrink 320→80，再 apply_idle_anchor 用 80 尺寸
            // 算右下角位置，pet 才会真正停到角落。反过来的话桌宠停在离边 120px 的
            // "假右下角"，用户反馈"取消喂食后桌宠没回到最初的地方"就是这个。
            emit_view(&app_clone, &ViewKind::Idle);
            let cfg = crate::config::Config::load();
            if cfg.pet_anchor.pin_visible_when_idle() {
                crate::anchor::apply_idle_anchor(&app_clone, cfg.pet_anchor);
            }
        }
    });
}

/// 文件 hover 在桌宠窗口上 —— 进 waiting 态 + 跑步过去咬住光标。
///
/// v0.4 · 这是用户原型里强调的"跑过来"动画。实现：
///   1. **只 `window.show()`**（不 reposition、不 enable follow）—— 上一版踩坑：
///      原来调 `show_mouse(app)`，内部会 `cursor_follow::enable()` 抢前 100ms 让窗口
///      直接传送到光标，看起来跟"跑步"完全无关。
///   2. emit FeedWaiting（CSS sprite 张嘴动画起）
///   3. 400ms ease-out cubic 插值窗口位置 → 鼠标位置
///   4. 动画结束才启用 cursor_follow，30fps 跟着鼠标走（直到 drop / leave）
pub fn on_drag_enter(app: &AppHandle, state: &Arc<AppState>) {
    // v0.4 fix (2026-05-20)：清掉待处理的 leave debounce —— 否则 200ms 内
    // re-enter 时旧 leave 仍会在 commit 时把 feed_drag_active 设回 false，
    // 跟新启动的 run_to_cursor 任务打架（老任务下一帧看到 false 也退出，
    // 新任务跟着第二次 enter 启动，整体看就是"飘+死"）。
    *state.feed_last_leave.lock().unwrap() = None;
    // 重复 enter（macOS 会反复触发）：仅在第一次启动动画
    if state.feed_drag_active.swap(true, Ordering::SeqCst) {
        return;
    }
    // v0.5 · 拖文件来了 = 打断进行中的开场入场动画，让 feed 流接管窗口位置。
    crate::entrance::abort(state);
    // 关键：抢在 cursor_follow 之前把它关掉，让动画独占 set_position
    crate::cursor_follow::disable(state);
    // 仅显示窗口（不动位置，让动画从当前位置 lerp 到光标）
    let app_show = app.clone();
    let _ = app.run_on_main_thread(move || {
        if let Some(w) = app_show.get_webview_window("mouse") {
            let _ = w.show();
            let _ = w.set_always_on_top(true);
        }
    });
    emit_view(app, &ViewKind::FeedWaiting);

    let app_clone = app.clone();
    let state_clone = state.clone();
    tauri::async_runtime::spawn(async move {
        run_to_cursor_then_follow(app_clone, state_clone).await;
    });
}

/// 第一阶段：400ms 插值 dash 到光标；第二阶段：cursor_follow 跟随。
async fn run_to_cursor_then_follow(app: AppHandle, state: Arc<AppState>) {
    use tauri::{LogicalPosition, Manager};

    let frames = 12u32;
    let frame_ms = 32u64; // 12 × 32 ≈ 384ms 总跑步时长

    let Some(window) = app.get_webview_window("mouse") else { return };
    let (ww, wh) = match window.outer_size().ok() {
        Some(s) => (s.width as f64, s.height as f64),
        None => (320.0, 320.0),
    };
    let scale = window.scale_factor().unwrap_or(1.0);

    // 起点：当前窗口逻辑坐标（顶左）
    let start_logical = match window.outer_position().ok() {
        Some(p) => (p.x as f64 / scale, p.y as f64 / scale),
        None => (0.0, 0.0),
    };

    // 主线程读初始光标位置，决定终点
    let app_t = app.clone();
    let target_logical = tokio::task::spawn_blocking(move || {
        let (tx, rx) = std::sync::mpsc::sync_channel(1);
        let app_inner = app_t.clone();
        let _ = app_t.run_on_main_thread(move || {
            let pos = app_inner
                .get_webview_window("mouse")
                .and_then(|w| crate::overlay::cursor_screen_pos_unchecked(&w));
            let _ = tx.send(pos);
        });
        rx.recv_timeout(std::time::Duration::from_millis(80)).ok().flatten()
    })
    .await
    .ok()
    .flatten();

    if let Some((cx, cy)) = target_logical {
        let scale = scale.max(0.5);
        let ww_l = ww / scale;
        let wh_l = wh / scale;
        // v0.4 fix (2026-05-20)：桌宠**不再贴到光标**，停在离光标 KEEP_DISTANCE 的地方。
        // 用户反馈：之前桌宠扑得太近、还紧跟随，"文件必须喂给它才罢休"。多数拖文件
        // 场景用户根本不想喂。改成：从桌宠 home 方向接近、停在光标侧旁 ~120px，
        // 用户真想喂只需再拖一点点到桌宠身上（mouse 窗口 hit-box 触发 drop）。
        const KEEP_DISTANCE: f64 = 120.0;
        // 桌宠视觉中心（窗口底部居中，距底 ~40px）
        let pet_half_from_bottom = 40.0;
        let start_cx = start_logical.0 + ww_l / 2.0;
        let start_cy = start_logical.1 + wh_l - pet_half_from_bottom;
        // 方向：从光标指回桌宠起点（让它从"家"那侧靠近，不穿过光标）
        let mut dir_x = start_cx - cx;
        let mut dir_y = start_cy - cy;
        let dist = (dir_x * dir_x + dir_y * dir_y).sqrt();
        if dist < 1.0 {
            // 起点几乎重合（罕见）→ 默认停在光标正下方一点
            dir_x = 0.0;
            dir_y = 1.0;
        } else {
            dir_x /= dist;
            dir_y /= dist;
        }
        // 目标桌宠中心 = 光标 + 单位方向 × KEEP_DISTANCE
        let pet_target_cx = cx + dir_x * KEEP_DISTANCE;
        let pet_target_cy = cy + dir_y * KEEP_DISTANCE;
        // 转成窗口左上角坐标
        let tx = pet_target_cx - ww_l / 2.0;
        let ty = pet_target_cy - (wh_l - pet_half_from_bottom);
        for i in 1..=frames {
            if !state.feed_drag_active.load(Ordering::SeqCst) {
                return; // 用户中途 leave / drop
            }
            let t = i as f64 / frames as f64;
            // ease-out cubic：起步快、靠近时减速，像真的"刹车"
            let eased = 1.0 - (1.0 - t).powi(3);
            let nx = start_logical.0 + (tx - start_logical.0) * eased;
            let ny = start_logical.1 + (ty - start_logical.1) * eased;
            let app_step = app.clone();
            let _ = app.run_on_main_thread(move || {
                if let Some(w) = app_step.get_webview_window("mouse") {
                    let _ = w.set_position(LogicalPosition::new(nx, ny));
                }
            });
            tokio::time::sleep(Duration::from_millis(frame_ms)).await;
        }
    }

    // v0.4 fix (2026-05-20)：**不再开 cursor_follow**。
    // 之前跑到位后开 30fps 紧跟随，桌宠死死黏着光标 → 用户报"文件必须喂给它
    // 才罢休"。现在桌宠跑到光标侧旁 ~120px 就停下静候（FeedWaiting 张嘴动画
    // 继续）。用户真想喂只需再拖一点到桌宠身上触发 mouse 窗口的 drop hit-box；
    // 不想喂就正常拖走、松手 → drag-detector 检测到 mouseUp → on_drag_leave
    // 把桌宠送回 anchor。保证"拖文件 ≠ 被桌宠纠缠"。
}

/// 用户按 Esc：取消正在进行的 feed flow（feed-waiting 期间 / feed-listening 期间都管）。
/// 已经进入 run_pipeline 的 reply 不归这里管 —— 那部分由 cancel_pipeline 命令处理。
pub async fn cancel(app: AppHandle, state: Arc<AppState>) {
    let was_waiting = state.feed_drag_active.swap(false, Ordering::SeqCst);
    let was_listening = state.streaming_active.swap(false, Ordering::SeqCst);
    if !was_waiting && !was_listening {
        return;
    }
    println!("[mouseclaw] 🍽️ feed cancel (waiting={was_waiting}, listening={was_listening})");
    // 清掉 fed_docs（如果有）
    state.fed_docs.lock().await.take();
    // 停录音 + session（如果在录）
    let _ = state.recorder.lock().unwrap().take();
    let _ = state.stream_session.lock().unwrap().take();
    crate::cursor_follow::disable(&state);
    // v0.4 fix (2026-05-20)：同 hide_overlay / on_drag_leave —— 先 emit_view(Idle)
    // 让 shrink 320→80 跑完，再 apply_idle_anchor 用正确尺寸算角落。
    emit_view(&app, &ViewKind::Idle);
    let cfg = crate::config::Config::load();
    if cfg.pet_anchor.pin_visible_when_idle() {
        crate::anchor::apply_idle_anchor(&app, cfg.pet_anchor);
    }
}
