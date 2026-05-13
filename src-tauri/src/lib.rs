//! MouseClaw main entry. See `/Users/edwinhao/MouseClaw/CLAUDE.md` for full architecture.

pub mod claude_cli;
pub mod events;
pub mod mode_b;
pub mod screenshot;
pub mod sessions;

use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager, State, WebviewWindow};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};
use tokio::sync::Mutex;

use crate::events::{EV_VIEW_CHANGED, ReplyMode, ViewKind};
use crate::sessions::SessionStore;

/// Global app state — wrapped in tokio Mutex so async pipelines can lock.
pub struct AppState {
    pub sessions: Mutex<SessionStore>,
    /// Path of the most recent screenshot, captured at shortcut press.
    pub last_screenshot: Mutex<Option<std::path::PathBuf>>,
}

impl AppState {
    fn new() -> anyhow::Result<Self> {
        Ok(Self {
            sessions: Mutex::new(SessionStore::new()?),
            last_screenshot: Mutex::new(None),
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

fn show_mouse(window: &WebviewWindow) -> tauri::Result<()> {
    window.show()?;
    window.set_always_on_top(true)?;
    Ok(())
}

fn emit_view(app: &AppHandle, view: &ViewKind) {
    if let Err(e) = app.emit(EV_VIEW_CHANGED, view) {
        eprintln!("[mouseclaw] failed to emit view-changed: {e}");
    }
}

// ────────────────── Tauri commands ──────────────────

/// User pressed Submit on the listening prompt bar with `text`.
/// Pipeline: ensure screenshot → emit Thinking → call Claude → emit Reply.
/// If Mode B detected: emit countdown for 3s → emit Inserting → write at cursor.
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

/// Follow-up from the expanded Panel (no new screenshot — re-uses last).
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
    let id = store.touch(true);
    Ok(id)
}

#[tauri::command]
fn save_shortcut(choice: String) -> Result<(), String> {
    // V1: Onboarding choice persisted on the frontend in localStorage.
    // V2 will re-register the global shortcut based on this choice.
    println!("[mouseclaw] user picked shortcut: {choice}");
    Ok(())
}

#[tauri::command]
fn cancel_pipeline(app: AppHandle) -> Result<(), String> {
    // V1 best-effort: just return to idle. In-flight Claude call still completes
    // but its result is discarded by the view state.
    emit_view(&app, &ViewKind::Idle);
    Ok(())
}

// ────────────────── Pipeline ──────────────────

async fn run_pipeline(transcript: String, app: AppHandle, state: Arc<AppState>) {
    // 1. Ensure we have a fresh screenshot (taken when shortcut fired; if not, grab now)
    let img_path = match state.last_screenshot.lock().await.clone() {
        Some(p) if p.exists() => p,
        _ => match screenshot::capture_main_screen().await {
            Ok(p) => p,
            Err(e) => {
                eprintln!("[mouseclaw] screenshot failed: {e:#}");
                emit_view(&app, &ViewKind::Blocked { reason: format!("截图失败：{e}") });
                return;
            }
        },
    };

    // 2. Session bookkeeping (touch resolves new vs continue based on idle window)
    let (session_id, context_preamble) = {
        let mut store = state.sessions.lock().await;
        let id = store.touch(false);
        let pre = store.context_preamble();
        (id, pre)
    };

    let prompt_with_context = match &context_preamble {
        Some(pre) => format!("{pre}{transcript}"),
        None => transcript.clone(),
    };

    // 3. Emit Thinking
    emit_view(&app, &ViewKind::Thinking { transcript: transcript.clone() });

    // 4. Foreground app context (best effort)
    let frontmost = mode_b::frontmost_app_name();

    // 5. Call Claude
    let reply = match claude_cli::ask_claude(&prompt_with_context, &img_path, frontmost.as_deref()).await {
        Ok(r) => r,
        Err(e) => {
            eprintln!("[mouseclaw] claude failed: {e:#}");
            emit_view(&app, &ViewKind::Blocked { reason: format!("Claude 调用失败：{e}") });
            return;
        }
    };

    // 6. Record turns
    {
        let mut store = state.sessions.lock().await;
        let _ = store.record_user(transcript.clone()).await;
        let _ = store.record_assistant(reply.clone()).await;
    }

    // 7. Mode detection
    let insert_text = claude_cli::parse_insert_directive(&reply);

    // Mode B but blocked (e.g., terminal foreground) → degrade to Mode A
    let (mode, final_insert) = match insert_text {
        Some(text) => match mode_b::assert_writable() {
            Ok(()) => (ReplyMode::B, Some(text)),
            Err(_) => (ReplyMode::A, None), // block the write but still show reply
        },
        None => (ReplyMode::A, None),
    };

    // 8. Emit Reply
    let _ = session_id; // (could pass into view for chip; UI uses placeholder for now)
    emit_view(&app, &ViewKind::Reply {
        transcript: transcript.clone(),
        reply: reply.clone(),
        mode,
        insert_text: final_insert.clone(),
    });

    // 9. If Mode B: 3-second countdown then insert
    if let (ReplyMode::B, Some(text)) = (mode, final_insert) {
        for remaining in (1..=3).rev() {
            emit_view(&app, &ViewKind::ModeBCountdown {
                insert_text: text.clone(),
                remaining,
            });
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        }
        emit_view(&app, &ViewKind::ModeBInserting { insert_text: text.clone() });
        if let Err(e) = mode_b::write_at_cursor(&text).await {
            eprintln!("[mouseclaw] mode B write failed: {e:#}");
            emit_view(&app, &ViewKind::Blocked { reason: format!("写入失败：{e}") });
            return;
        }
        // After write, fall back to Reply (success) for the auto-dismiss timer
        emit_view(&app, &ViewKind::Reply {
            transcript,
            reply: format!("✅ 已写入：{text}"),
            mode: ReplyMode::A,
            insert_text: None,
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
                    if event.state() != ShortcutState::Pressed {
                        return;
                    }
                    println!("\n[mouseclaw] 🦞 shortcut fired: {shortcut:?}");

                    // Show overlay window
                    if let Some(w) = app.get_webview_window("mouse") {
                        let _ = show_mouse(&w);
                    }

                    // Capture screenshot up-front (so the screen state when the
                    // user pressed the key is what gets sent, not what's visible
                    // by the time they finish typing).
                    let app_handle = app.clone();
                    let state_clone: Arc<AppState> = app.state::<Arc<AppState>>().inner().clone();
                    tauri::async_runtime::spawn(async move {
                        match screenshot::capture_main_screen().await {
                            Ok(p) => {
                                *state_clone.last_screenshot.lock().await = Some(p);
                            }
                            Err(e) => eprintln!("[mouseclaw] screenshot at trigger: {e:#}"),
                        }
                        emit_view(&app_handle, &ViewKind::Listening);
                    });
                })
                .build(),
        )
        .invoke_handler(tauri::generate_handler![
            submit_query, follow_up, new_session, save_shortcut, cancel_pipeline
        ])
        .setup(|app| {
            let shortcut = Shortcut::new(Some(Modifiers::SUPER | Modifiers::SHIFT), Code::Space);
            app.global_shortcut().register(shortcut)?;
            println!("[mouseclaw] registered global shortcut: Cmd+Shift+Space");

            set_accessory_activation_policy();
            println!("[mouseclaw] activation policy = Accessory (no dock icon)");

            // Ensure initial state is idle on startup
            emit_view(&app.handle(), &ViewKind::Idle);

            println!("[mouseclaw] 🦞 ready — press Cmd+Shift+Space to summon");
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
