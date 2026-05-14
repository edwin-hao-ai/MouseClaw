//! MouseClaw main entry. See `/Users/edwinhao/MouseClaw/CLAUDE.md` for full architecture.

pub mod audio;
pub mod claude_cli;
pub mod config;
pub mod events;
pub mod mode_b;
pub mod screenshot;
pub mod sessions;
pub mod transcribe;
pub mod tray;

use std::str::FromStr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;
use tauri::{AppHandle, Emitter, LogicalPosition, Manager, State, WebviewWindow};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};
use tokio::sync::Mutex;

use crate::events::{EV_VIEW_CHANGED, ReplyMode, ViewKind};
use crate::sessions::SessionStore;

/// App-wide state. Mutex split: tokio::Mutex for async-accessed bits,
/// std::Mutex for the cpal Recorder (cpal::Stream is !Send so we never await
/// while holding the recorder lock).
pub struct AppState {
    pub sessions: Mutex<SessionStore>,
    pub last_screenshot: Mutex<Option<std::path::PathBuf>>,
    pub recorder: StdMutex<Option<audio::Recorder>>,
    /// Monotonic counter — bumped on any new shortcut press / pipeline run /
    /// pin / dismiss. Auto-hide timers capture the gen at scheduling time and
    /// no-op if it changed by the time they fire, so a follow-up shortcut
    /// press doesn't get hidden by the previous reply's timer.
    pub gen: AtomicU64,
}

impl AppState {
    fn new() -> anyhow::Result<Self> {
        Ok(Self {
            sessions: Mutex::new(SessionStore::new()?),
            last_screenshot: Mutex::new(None),
            recorder: StdMutex::new(None),
            gen: AtomicU64::new(0),
        })
    }
}

#[cfg(target_os = "macos")]
fn set_accessory_activation_policy() {
    use cocoa::appkit::{NSApp, NSApplication, NSApplicationActivationPolicy};
    unsafe {
        let app = NSApp();
        app.setActivationPolicy_(NSApplicationActivationPolicy::NSApplicationActivationPolicyAccessory);
    }
}
#[cfg(not(target_os = "macos"))]
fn set_accessory_activation_policy() {}

/// Position the overlay window near the user's mouse cursor, then show it.
/// NSEvent::mouseLocation returns screen coords with bottom-left origin;
/// Tauri's set_position uses top-left origin, so we flip Y by screen height.
pub fn show_mouse(window: &WebviewWindow) -> tauri::Result<()> {
    if let Some((x, y)) = current_mouse_pos_top_left(window) {
        // Offset so the mouse character lands just below+right of the cursor
        let w_size = window.outer_size().ok();
        let (ww, wh) = match w_size {
            Some(s) => (s.width as f64, s.height as f64),
            None => (320.0, 320.0),
        };
        let scale = window.scale_factor().unwrap_or(1.0);
        let pos_x = x - (ww / scale) / 2.0;
        let pos_y = y - (wh / scale) + 32.0; // bubble sits above cursor; mouse char near cursor
        let _ = window.set_position(LogicalPosition::new(pos_x, pos_y));
    }
    window.show()?;
    window.set_always_on_top(true)?;
    Ok(())
}

/// Get cursor position in top-left-origin screen coordinates (matches Tauri API).
#[cfg(target_os = "macos")]
fn current_mouse_pos_top_left(window: &WebviewWindow) -> Option<(f64, f64)> {
    use cocoa::base::id;
    use cocoa::foundation::NSPoint;
    use objc::{class, msg_send, sel, sel_impl};
    unsafe {
        let event_class: id = msg_send![class!(NSEvent), class];
        let point: NSPoint = msg_send![event_class, mouseLocation];
        // mouseLocation is in screen coords with bottom-left origin.
        // To convert: y_top = primary_screen_height - point.y
        let screen_h = primary_screen_height_pts().unwrap_or(1080.0);
        let scale = window.scale_factor().unwrap_or(1.0);
        Some((point.x, (screen_h - point.y) * scale / scale))
    }
}

#[cfg(target_os = "macos")]
fn primary_screen_height_pts() -> Option<f64> {
    use cocoa::base::id;
    use cocoa::foundation::{NSRect, NSSize};
    use objc::{class, msg_send, sel, sel_impl};
    unsafe {
        let screen: id = msg_send![class!(NSScreen), mainScreen];
        if screen as usize == 0 { return None; }
        let frame: NSRect = msg_send![screen, frame];
        let size: NSSize = frame.size;
        Some(size.height)
    }
}

#[cfg(not(target_os = "macos"))]
fn current_mouse_pos_top_left(_w: &WebviewWindow) -> Option<(f64, f64)> { None }

fn emit_view(app: &AppHandle, view: &ViewKind) {
    if let Err(e) = app.emit(EV_VIEW_CHANGED, view) {
        eprintln!("[mouseclaw] failed to emit view-changed: {e}");
    }
}

/// Hide the overlay window, emit Idle. Idempotent.
fn hide_overlay(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("mouse") {
        let _ = w.hide();
    }
    emit_view(app, &ViewKind::Idle);
}

/// Schedule `hide_overlay` after `after_ms` ms — but only fire if the app's
/// generation hasn't changed (i.e., no new pipeline/shortcut activity).
fn schedule_auto_hide(app: &AppHandle, state: &Arc<AppState>, after_ms: u64) {
    let my_gen = state.gen.load(Ordering::SeqCst);
    let app_clone = app.clone();
    let state_clone = state.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_millis(after_ms)).await;
        if state_clone.gen.load(Ordering::SeqCst) == my_gen {
            println!("[mouseclaw] auto-hide (gen {my_gen} still current after {after_ms}ms)");
            hide_overlay(&app_clone);
        } else {
            println!("[mouseclaw] auto-hide skipped (gen advanced, user did something)");
        }
    });
}

/// Bump the generation — invalidates any pending auto-hide timer.
fn bump_gen(state: &Arc<AppState>) -> u64 {
    state.gen.fetch_add(1, Ordering::SeqCst) + 1
}

// ────────────────── Tauri commands ──────────────────

#[tauri::command]
async fn submit_query(
    text: String,
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
) -> Result<(), String> {
    let state = state.inner().clone();
    let app2 = app.clone();
    tauri::async_runtime::spawn(async move { run_pipeline(text, app2, state).await });
    Ok(())
}

#[tauri::command]
async fn follow_up(
    text: String,
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
) -> Result<(), String> {
    let state = state.inner().clone();
    let app2 = app.clone();
    tauri::async_runtime::spawn(async move { run_pipeline(text, app2, state).await });
    Ok(())
}

#[tauri::command]
async fn new_session(state: State<'_, Arc<AppState>>) -> Result<u64, String> {
    let mut store = state.sessions.lock().await;
    Ok(store.touch(true))
}

#[tauri::command]
fn save_shortcut(choice: String, app: AppHandle) -> Result<(), String> {
    let new_str = config::choice_to_shortcut_str(&choice).to_string();
    let new_shortcut = Shortcut::from_str(&new_str)
        .map_err(|e| format!("解析快捷键 {new_str:?} 失败：{e}"))?;

    let gs = app.global_shortcut();
    let _ = gs.unregister_all();
    gs.register(new_shortcut)
        .map_err(|e| format!("注册快捷键失败：{e}"))?;

    // Persist with onboarded=true so we never bring up the onboarding window again
    let cfg = config::Config { shortcut: new_str.clone(), onboarded: true };
    cfg.save().map_err(|e| format!("保存配置失败：{e}"))?;

    // Close the onboarding window if open
    if let Some(w) = app.get_webview_window("onboarding") {
        let _ = w.close();
    }

    println!("[mouseclaw] shortcut updated → {new_str} (choice: {choice}, onboarded ✓)");
    Ok(())
}

#[tauri::command]
fn cancel_pipeline(app: AppHandle, state: State<'_, Arc<AppState>>) -> Result<(), String> {
    bump_gen(&state.inner().clone());
    hide_overlay(&app);
    Ok(())
}

/// React calls this when entering Panel / sticky states to cancel the
/// pending 3s auto-hide and keep the overlay visible until the user dismisses.
#[tauri::command]
fn pin_window(state: State<'_, Arc<AppState>>) -> Result<(), String> {
    let g = bump_gen(&state.inner().clone());
    println!("[mouseclaw] window pinned (gen → {g})");
    Ok(())
}

/// User pressed Esc / clicked away → hide immediately.
#[tauri::command]
fn dismiss(app: AppHandle, state: State<'_, Arc<AppState>>) -> Result<(), String> {
    bump_gen(&state.inner().clone());
    hide_overlay(&app);
    Ok(())
}

/// Read session history from ~/.mouseclaw/sessions.jsonl and return grouped sessions
/// for the History window. Returns most recent sessions first.
#[tauri::command]
fn read_history() -> Result<Vec<HistorySession>, String> {
    use std::collections::BTreeMap;
    use crate::sessions::TurnRecord;

    let home = std::env::var_os("HOME").ok_or("HOME not set")?;
    let path = std::path::PathBuf::from(home).join(".mouseclaw/sessions.jsonl");
    if !path.exists() {
        return Ok(Vec::new());
    }
    let text = std::fs::read_to_string(&path).map_err(|e| format!("read: {e}"))?;
    let mut sessions: BTreeMap<u64, HistorySession> = BTreeMap::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() { continue; }
        let Ok(record) = serde_json::from_str::<TurnRecord>(line) else { continue };
        let entry = sessions.entry(record.session_id).or_insert_with(|| HistorySession {
            session_id: record.session_id,
            started_at: record.timestamp,
            ended_at: record.timestamp,
            turns: Vec::new(),
        });
        entry.ended_at = record.timestamp;
        entry.turns.push(HistoryTurn {
            role: format!("{:?}", record.role).to_lowercase(),
            text: record.text,
            timestamp: record.timestamp,
            screenshot: record.screenshot,
        });
    }
    // Reverse-chronological (most recent first)
    let mut list: Vec<HistorySession> = sessions.into_values().collect();
    list.sort_by(|a, b| b.started_at.cmp(&a.started_at));
    Ok(list)
}

#[derive(serde::Serialize)]
pub struct HistorySession {
    pub session_id: u64,
    pub started_at: chrono::DateTime<chrono::Utc>,
    pub ended_at: chrono::DateTime<chrono::Utc>,
    pub turns: Vec<HistoryTurn>,
}

#[derive(serde::Serialize)]
pub struct HistoryTurn {
    pub role: String,
    pub text: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub screenshot: Option<String>,
}

/// ◼ Stop button in the RecordingBubble — equivalent to releasing the shortcut.
/// In push-to-talk mode, this is the only way to send a recording without
/// the user having to release the shortcut keys (useful if user wants to
/// keep holding the keys mid-thought).
#[tauri::command]
async fn toggle_recording(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
) -> Result<(), String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn(async move { on_shortcut_release(app, state).await });
    Ok(())
}

// ────────────────── Pipeline ──────────────────

async fn run_pipeline(transcript: String, app: AppHandle, state: Arc<AppState>) {
    // Bump generation so any pending auto-hide timer from a previous run no-ops
    bump_gen(&state);

    // 1. Use screenshot captured at shortcut-press time (recent + matches user intent)
    let (img_path, cursor_ctx) = match state.last_screenshot.lock().await.clone() {
        Some(p) if p.exists() => (p, None), // older snapshot, no cursor context preserved
        _ => match screenshot::capture_main_screen().await {
            Ok(r) => {
                let cursor = match (r.cursor, r.screen_size) {
                    (Some((x, y)), Some((w, h))) => Some(claude_cli::CursorContext {
                        x, y, screen_w: w, screen_h: h,
                    }),
                    _ => None,
                };
                (r.path, cursor)
            }
            Err(e) => {
                emit_view(&app, &ViewKind::Blocked { reason: format!("截图失败：{e}") });
                schedule_auto_hide(&app, &state, 4000);
                return;
            }
        },
    };

    // 2. Session bookkeeping
    let (_session_id, context_preamble) = {
        let mut store = state.sessions.lock().await;
        let id = store.touch(false);
        (id, store.context_preamble())
    };
    let prompt_with_context = match &context_preamble {
        Some(pre) => format!("{pre}{transcript}"),
        None => transcript.clone(),
    };

    emit_view(&app, &ViewKind::Thinking { transcript: transcript.clone() });

    let frontmost = mode_b::frontmost_app_name();
    let reply = match claude_cli::ask_claude(
        &prompt_with_context,
        &img_path,
        frontmost.as_deref(),
        cursor_ctx.as_ref(),
    ).await {
        Ok(r) => r,
        Err(e) => {
            emit_view(&app, &ViewKind::Blocked { reason: format!("Claude 调用失败：{e}") });
            schedule_auto_hide(&app, &state, 4000);
            return;
        }
    };

    {
        let mut store = state.sessions.lock().await;
        let screenshot_path = img_path.to_string_lossy().to_string();
        let _ = store.record_user(transcript.clone(), Some(screenshot_path)).await;
        let _ = store.record_assistant(reply.clone()).await;
    }

    let insert_text = claude_cli::parse_insert_directive(&reply);
    let (mode, final_insert) = match insert_text {
        Some(t) => match mode_b::assert_writable() {
            Ok(()) => (ReplyMode::B, Some(t)),
            Err(_) => (ReplyMode::A, None),
        },
        None => (ReplyMode::A, None),
    };

    emit_view(&app, &ViewKind::Reply {
        transcript: transcript.clone(),
        reply: reply.clone(),
        mode,
        insert_text: final_insert.clone(),
    });

    if let (ReplyMode::B, Some(text)) = (mode, final_insert) {
        // Note: countdown frames don't bump gen — they're part of the same
        // logical pipeline. Auto-hide is scheduled after the final Reply emit.
        for remaining in (1..=3).rev() {
            emit_view(&app, &ViewKind::ModeBCountdown {
                insert_text: text.clone(), remaining,
            });
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
        emit_view(&app, &ViewKind::ModeBInserting { insert_text: text.clone() });
        if let Err(e) = mode_b::write_at_cursor(&text).await {
            emit_view(&app, &ViewKind::Blocked { reason: format!("写入失败：{e}") });
            schedule_auto_hide(&app, &state, 4000);
            return;
        }
        emit_view(&app, &ViewKind::Reply {
            transcript, reply: format!("✅ 已写入：{text}"),
            mode: ReplyMode::A, insert_text: None,
        });
    }

    // Auto-hide 3s after the final Reply (PRD §IV "3 秒后小老鼠跑回角落").
    // Cancelled if the user presses the shortcut again, expands to Panel
    // (which calls pin_window), or presses Esc (dismiss).
    schedule_auto_hide(&app, &state, 3000);
}

// ────────────────── Push-to-talk shortcut handling ──────────────────
//
// Press-and-hold semantics (v0.1.5+):
//   - 按住 = 开始录音 + 抓截图
//   - 松开 = 停止录音 + Whisper 转写 + 跑 pipeline
//
// 之前是 toggle press (按一下开始、再按一下结束)，但用户反馈说录音状态
// 容易卡住。push-to-talk 更符合直觉，没有"是否在录音"的歧义状态。

/// 按下快捷键：起手录音 + 截图。
pub async fn on_shortcut_press(app: AppHandle, state: Arc<AppState>) {
    bump_gen(&state);

    // 已经在录音中（理论上 hold + auto-repeat 会再次 fire Press）→ 跳过
    if state.recorder.lock().unwrap().is_some() {
        return;
    }

    if !transcribe::is_available() {
        let msg = match transcribe::current_state() {
            Some(transcribe::ModelState::Downloading) =>
                "正在下载 Whisper 模型（57MB），下载完成后再试一次".into(),
            Some(transcribe::ModelState::Failed(e)) =>
                format!("Whisper 模型下载失败：{e}（手动跑：curl -L -o ~/.mouseclaw/models/{} {}）",
                    transcribe::MODEL_FILENAME, transcribe::MODEL_URL),
            _ =>
                "Whisper 模型未找到（~/.mouseclaw/models/ggml-base-q5_1.bin）".into(),
        };
        emit_view(&app, &ViewKind::Blocked { reason: msg });
        schedule_auto_hide(&app, &state, 6000);
        return;
    }

    let recorder = match audio::Recorder::start() {
        Ok(r) => r,
        Err(e) => {
            eprintln!("[mouseclaw] start recording: {e:#}");
            emit_view(&app, &ViewKind::Blocked { reason: format!("录音启动失败：{e}") });
            schedule_auto_hide(&app, &state, 4000);
            return;
        }
    };
    *state.recorder.lock().unwrap() = Some(recorder);

    // 截图并行（不阻塞录音）
    let state_clone = state.clone();
    let app_clone = app.clone();
    tauri::async_runtime::spawn(async move {
        match screenshot::capture_main_screen().await {
            Ok(r) => { *state_clone.last_screenshot.lock().await = Some(r.path); }
            Err(e) => eprintln!("[mouseclaw] screenshot: {e:#}"),
        }
        emit_view(&app_clone, &ViewKind::Listening);
    });
}

/// 松开快捷键：停止录音 + Whisper 转写 + 跑 pipeline。
pub async fn on_shortcut_release(app: AppHandle, state: Arc<AppState>) {
    let recorder = {
        let mut g = state.recorder.lock().unwrap();
        g.take()
    };
    let Some(recorder) = recorder else {
        // 没在录音 → 可能是 Release 先于 Press（不常见），忽略
        return;
    };

    emit_view(&app, &ViewKind::Thinking { transcript: "(转写中…)".into() });

    let app_clone = app.clone();
    let state_clone = state.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let samples = match recorder.stop_and_take() {
            Ok(s) => s,
            Err(e) => {
                eprintln!("[mouseclaw] stop_and_take: {e:#}");
                let _ = app_clone.emit(EV_VIEW_CHANGED, ViewKind::Blocked {
                    reason: format!("录音失败：{e}"),
                });
                schedule_auto_hide(&app_clone, &state_clone, 4000);
                return;
            }
        };
        println!("[mouseclaw] captured {} samples @ 16kHz ({:.1}s)",
            samples.len(), samples.len() as f32 / 16_000.0);
        let transcript = match transcribe::transcribe(&samples) {
            Ok(t) => t,
            Err(e) => {
                eprintln!("[mouseclaw] transcribe: {e:#}");
                let _ = app_clone.emit(EV_VIEW_CHANGED, ViewKind::Blocked {
                    reason: format!("Whisper 失败：{e}"),
                });
                schedule_auto_hide(&app_clone, &state_clone, 4000);
                return;
            }
        };
        println!("[mouseclaw] transcript: {transcript:?}");
        if transcript.is_empty() {
            hide_overlay(&app_clone);
            return;
        }
        tauri::async_runtime::spawn(async move {
            run_pipeline(transcript, app_clone, state_clone).await;
        });
    });
}

/// Legacy alias — tray summon uses this. Maps to "tap": press+immediate release.
/// 不太理想但 tray 点一下没法 hold，只能模拟一个最短录音（500ms）。
/// 后续 v2 改 tray 触发独立路径（让用户用文字输入）。
pub async fn on_shortcut_pressed(app: AppHandle, state: Arc<AppState>) {
    on_shortcut_press(app.clone(), state.clone()).await;
    tokio::time::sleep(Duration::from_millis(800)).await;
    on_shortcut_release(app, state).await;
}

// ────────────────── Entry ──────────────────

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app_state = Arc::new(AppState::new().expect("init AppState"));

    tauri::Builder::default()
        .manage(app_state.clone())
        .plugin(tauri_plugin_opener::init())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(move |app, shortcut, event| {
                    let app_handle = app.clone();
                    let state: Arc<AppState> = app.state::<Arc<AppState>>().inner().clone();
                    match event.state() {
                        ShortcutState::Pressed => {
                            println!("[mouseclaw] 🦞 shortcut PRESS: {shortcut:?}");
                            if let Some(w) = app.get_webview_window("mouse") {
                                let _ = show_mouse(&w);
                            }
                            tauri::async_runtime::spawn(async move {
                                on_shortcut_press(app_handle, state).await;
                            });
                        }
                        ShortcutState::Released => {
                            println!("[mouseclaw] 🦞 shortcut RELEASE: {shortcut:?}");
                            tauri::async_runtime::spawn(async move {
                                on_shortcut_release(app_handle, state).await;
                            });
                        }
                    }
                })
                .build(),
        )
        .invoke_handler(tauri::generate_handler![
            submit_query, follow_up, new_session, save_shortcut,
            cancel_pipeline, toggle_recording, pin_window, dismiss, read_history
        ])
        .setup(|app| {
            let cfg = config::Config::load();
            set_accessory_activation_policy();
            println!("[mouseclaw] activation policy = Accessory (no dock icon)");
            emit_view(&app.handle(), &ViewKind::Idle);

            // Tray always available (gives user an escape valve before/during onboarding)
            if let Err(e) = tray::setup(&app.handle()) {
                eprintln!("[mouseclaw] tray setup failed: {e:#}");
            } else {
                println!("[mouseclaw] tray icon registered");
            }

            // Diagnostic: log PATH + claude CLI location so users can see in
            // ~/Library/Logs/MouseClaw or `bun tauri dev` console why a shortcut
            // press might not invoke Claude in a .app bundle (PATH inheritance issue).
            println!("[mouseclaw] inherited PATH = {}", std::env::var("PATH").unwrap_or_default());
            match std::process::Command::new("sh").arg("-c").arg("which claude").output() {
                Ok(out) if out.status.success() => {
                    println!("[mouseclaw] claude CLI found at: {}", String::from_utf8_lossy(&out.stdout).trim());
                }
                _ => {
                    eprintln!("[mouseclaw] ⚠️  claude CLI NOT on PATH at startup — will retry at invocation with expanded PATH (~/.npm-global/bin etc.)");
                }
            }

            // Kick off Whisper model download in background if missing.
            // First-launch users get a "downloading…" bubble instead of a
            // cryptic error when they press the shortcut.
            transcribe::kick_off_download_if_missing();

            if cfg.onboarded {
                // Existing user → register shortcut + ready
                let shortcut = Shortcut::from_str(&cfg.shortcut)
                    .unwrap_or_else(|_| Shortcut::from_str("Super+Shift+Space").unwrap());
                app.global_shortcut().register(shortcut)?;
                println!("[mouseclaw] registered global shortcut: {} (toggle record)", cfg.shortcut);
                println!("[mouseclaw] 🦞 ready — press {} to start recording", cfg.shortcut);
            } else {
                // First launch → no shortcut registered yet; open Onboarding window
                println!("[mouseclaw] first launch → opening Onboarding window");
                tray::open_onboarding_window(&app.handle());
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
