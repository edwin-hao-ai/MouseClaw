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

use tauri::AppHandle;

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
        emit_view(&app, &ViewKind::Blocked {
            reason: "语音模型未就绪 — 等几秒再喂".into(),
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

    // 5. 跑 pipeline —— 它会从 state.fed_docs 取走 bundle，注入 prompt 前置
    crate::pipeline::run_pipeline(transcript, app, state).await;
}

/// 用户在拖动期间把光标离开桌宠窗口 —— 重置等待态。
pub fn on_drag_leave(app: &AppHandle, state: &Arc<AppState>) {
    if state.feed_drag_active.swap(false, Ordering::SeqCst) {
        emit_view(app, &ViewKind::Idle);
    }
}

/// 文件 hover 在桌宠窗口上 —— 进 waiting 态。
pub fn on_drag_enter(app: &AppHandle, state: &Arc<AppState>) {
    state.feed_drag_active.store(true, Ordering::SeqCst);
    show_mouse(app);
    emit_view(app, &ViewKind::FeedWaiting);
}
