//! MouseClaw main entry. See `/Users/edwinhao/MouseClaw/CLAUDE.md` for full architecture.

pub mod audio;
pub mod claude_cli;
pub mod config;
pub mod events;
pub mod mode_b;
pub mod screenshot;
pub mod sessions;
pub mod transcribe;

use std::str::FromStr;
use std::sync::{Arc, Mutex as StdMutex};
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
}

impl AppState {
    fn new() -> anyhow::Result<Self> {
        Ok(Self {
            sessions: Mutex::new(SessionStore::new()?),
            last_screenshot: Mutex::new(None),
            recorder: StdMutex::new(None),
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
fn show_mouse(window: &WebviewWindow) -> tauri::Result<()> {
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
    // Unregister all previous shortcuts (we only ever have one in V1)
    let _ = gs.unregister_all();
    gs.register(new_shortcut)
        .map_err(|e| format!("注册快捷键失败：{e}"))?;

    // Persist
    let cfg = config::Config { shortcut: new_str.clone() };
    cfg.save().map_err(|e| format!("保存配置失败：{e}"))?;

    println!("[mouseclaw] shortcut updated → {new_str} (choice: {choice})");
    Ok(())
}

#[tauri::command]
fn cancel_pipeline(app: AppHandle) -> Result<(), String> {
    emit_view(&app, &ViewKind::Idle);
    Ok(())
}

/// Manual stop from the recording bubble's ◼ button (equivalent to a 2nd shortcut press).
#[tauri::command]
async fn toggle_recording(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
) -> Result<(), String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn(async move { on_shortcut_pressed(app, state).await });
    Ok(())
}

// ────────────────── Pipeline ──────────────────

async fn run_pipeline(transcript: String, app: AppHandle, state: Arc<AppState>) {
    // 1. Use screenshot captured at shortcut-press time (recent + matches user intent)
    let img_path = match state.last_screenshot.lock().await.clone() {
        Some(p) if p.exists() => p,
        _ => match screenshot::capture_main_screen().await {
            Ok(p) => p,
            Err(e) => {
                emit_view(&app, &ViewKind::Blocked { reason: format!("截图失败：{e}") });
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
    let reply = match claude_cli::ask_claude(&prompt_with_context, &img_path, frontmost.as_deref()).await {
        Ok(r) => r,
        Err(e) => {
            emit_view(&app, &ViewKind::Blocked { reason: format!("Claude 调用失败：{e}") });
            return;
        }
    };

    {
        let mut store = state.sessions.lock().await;
        let _ = store.record_user(transcript.clone()).await;
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
        for remaining in (1..=3).rev() {
            emit_view(&app, &ViewKind::ModeBCountdown {
                insert_text: text.clone(), remaining,
            });
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        }
        emit_view(&app, &ViewKind::ModeBInserting { insert_text: text.clone() });
        if let Err(e) = mode_b::write_at_cursor(&text).await {
            emit_view(&app, &ViewKind::Blocked { reason: format!("写入失败：{e}") });
            return;
        }
        emit_view(&app, &ViewKind::Reply {
            transcript, reply: format!("✅ 已写入：{text}"),
            mode: ReplyMode::A, insert_text: None,
        });
    }
}

// ────────────────── Voice trigger (toggle recording on shortcut) ──────────────────

/// Called on each shortcut press. Toggles recording state:
///   1st press → start cpal capture, emit Listening with mic indicator
///   2nd press → stop capture, transcribe with Whisper, run pipeline with transcript
async fn on_shortcut_pressed(app: AppHandle, state: Arc<AppState>) {
    // Check current recorder state (under std mutex — no awaits while held)
    let was_recording = {
        let g = state.recorder.lock().unwrap();
        g.is_some()
    };

    if was_recording {
        // Stop + transcribe + pipeline
        let recorder = {
            let mut g = state.recorder.lock().unwrap();
            g.take()
        };
        let Some(recorder) = recorder else { return };

        emit_view(&app, &ViewKind::Thinking { transcript: "(转写中…)".into() });

        // Move CPU-bound work off the async thread
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
                    return;
                }
            };
            println!("[mouseclaw] transcript: {transcript:?}");
            if transcript.is_empty() {
                let _ = app_clone.emit(EV_VIEW_CHANGED, ViewKind::Idle);
                return;
            }
            // Run the pipeline on the async runtime
            tauri::async_runtime::spawn(async move {
                run_pipeline(transcript, app_clone, state_clone).await;
            });
        });
    } else {
        // Start recording. Also capture screenshot up front.
        if !transcribe::is_available() {
            emit_view(&app, &ViewKind::Blocked {
                reason: "Whisper 模型未找到（~/.mouseclaw/models/ggml-base-q5_1.bin）".into(),
            });
            return;
        }
        let recorder = match audio::Recorder::start() {
            Ok(r) => r,
            Err(e) => {
                eprintln!("[mouseclaw] start recording: {e:#}");
                emit_view(&app, &ViewKind::Blocked { reason: format!("录音启动失败：{e}") });
                return;
            }
        };
        *state.recorder.lock().unwrap() = Some(recorder);

        // Take screenshot in parallel
        let state_clone = state.clone();
        let app_clone = app.clone();
        tauri::async_runtime::spawn(async move {
            match screenshot::capture_main_screen().await {
                Ok(p) => { *state_clone.last_screenshot.lock().await = Some(p); }
                Err(e) => eprintln!("[mouseclaw] screenshot: {e:#}"),
            }
            emit_view(&app_clone, &ViewKind::Listening);
        });
    }
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
                    if event.state() != ShortcutState::Pressed { return; }
                    println!("[mouseclaw] 🦞 shortcut fired: {shortcut:?}");
                    if let Some(w) = app.get_webview_window("mouse") {
                        let _ = show_mouse(&w);
                    }
                    let app_handle = app.clone();
                    let state: Arc<AppState> = app.state::<Arc<AppState>>().inner().clone();
                    tauri::async_runtime::spawn(async move {
                        on_shortcut_pressed(app_handle, state).await;
                    });
                })
                .build(),
        )
        .invoke_handler(tauri::generate_handler![
            submit_query, follow_up, new_session, save_shortcut, cancel_pipeline, toggle_recording
        ])
        .setup(|app| {
            // Read user-saved shortcut (or fall back to Cmd+Shift+Space on first run)
            let cfg = config::Config::load();
            let shortcut = Shortcut::from_str(&cfg.shortcut)
                .unwrap_or_else(|_| Shortcut::from_str("Super+Shift+Space").unwrap());
            app.global_shortcut().register(shortcut)?;
            println!("[mouseclaw] registered global shortcut: {} (toggle record)", cfg.shortcut);
            set_accessory_activation_policy();
            println!("[mouseclaw] activation policy = Accessory (no dock icon)");
            emit_view(&app.handle(), &ViewKind::Idle);
            println!("[mouseclaw] 🦞 ready — press {} to start recording", cfg.shortcut);
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
