//! Session storage — append-only JSONL at `~/.mouseclaw/sessions.jsonl`.
//! Each line is a single turn record. Sessions are derived by grouping consecutive
//! lines with the same `session_id`.
//!
//! Boundary rules (per CLAUDE.md):
//!   • idle > 5min → next call starts new session
//!   • user explicit "新对话/clear/重新开始" → new session
//!   • double-tap shortcut → new session (caller decides; this module just stores)
//!   • foreground app changes (V2) — not detected yet
//!
//! V1 keeps last N=10 turns or 8K-token cap for prompt context.

use std::path::PathBuf;
use anyhow::{Context, Result};
use chrono::{DateTime, Local, Utc};
use serde::{Deserialize, Serialize};
use tokio::io::AsyncWriteExt;

use crate::events::{Turn, TurnRole};

const SESSION_IDLE_LIMIT_SECS: i64 = 5 * 60;
const MAX_HISTORY_TURNS: usize = 10;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TurnRecord {
    pub session_id: u64,
    pub timestamp: DateTime<Utc>,
    pub role: TurnRole,
    pub text: String,
    /// Path to the screenshot PNG taken when the user pressed the shortcut.
    /// Only present on `User` turns. `None` for backward-compat with
    /// pre-v0.1.4 lines that don't have the field.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub screenshot: Option<String>,
}

/// Session store — in-memory state + JSONL append.
pub struct SessionStore {
    file_path: PathBuf,
    current_session: u64,
    last_turn_at: Option<DateTime<Utc>>,
    /// Frontmost app name at the last turn — switching apps starts a new session
    /// (per design doc: "前台 app 切换 = 新 session"). None until first turn.
    last_frontmost: Option<String>,
    /// Last N turns of the current session (for prompt context). Capped at MAX_HISTORY_TURNS.
    history: Vec<Turn>,
}

impl SessionStore {
    pub fn new() -> Result<Self> {
        let dir = dirs_home_dir().context("no home dir")?.join(".mouseclaw");
        std::fs::create_dir_all(&dir).context("mkdir ~/.mouseclaw")?;
        let file_path = dir.join("sessions.jsonl");
        // Touch the file
        if !file_path.exists() {
            std::fs::File::create(&file_path)?;
        }
        Ok(Self {
            file_path,
            current_session: Self::next_session_id(),
            last_turn_at: None,
            last_frontmost: None,
            history: Vec::new(),
        })
    }

    fn next_session_id() -> u64 {
        // simple monotonic — millis since epoch
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
    }

    /// Decide whether to start a new session before recording a turn.
    /// Caller invokes this at the START of each shortcut press.
    ///
    /// 新 session 触发条件（design doc）：
    ///   - `force_new`（保留给"用户说新对话"等显式重置）
    ///   - 距上次回复 > 5 分钟（idle 超时）
    ///   - 前台 app 和上次不同（切了应用 = 换了话题）
    ///
    /// `frontmost`：当前前台 app 名（None = 拿不到，不参与判断）。
    pub fn touch(&mut self, force_new: bool, frontmost: Option<&str>) -> u64 {
        let now = Utc::now();
        let idle_expired = match self.last_turn_at {
            None => false, // first call uses initial session id
            Some(t) => (now - t).num_seconds() > SESSION_IDLE_LIMIT_SECS,
        };
        let app_switched = match (&self.last_frontmost, frontmost) {
            (Some(prev), Some(cur)) => prev != cur,
            _ => false, // 任一侧缺失就不算切换
        };
        let should_new = force_new || idle_expired || app_switched;
        if should_new {
            self.current_session = Self::next_session_id();
            self.history.clear();
        }
        // 记下这次的前台 app，供下次比较
        if let Some(cur) = frontmost {
            self.last_frontmost = Some(cur.to_string());
        }
        self.current_session
    }

    pub fn current_session(&self) -> u64 { self.current_session }

    pub fn turn_count(&self) -> usize { self.history.len() }

    /// Build context preamble to prepend to next Claude prompt.
    pub fn context_preamble(&self) -> Option<String> {
        if self.history.is_empty() { return None; }
        let mut s = String::from("[历史]\n");
        for t in &self.history {
            let who = match t.role { TurnRole::User => "用户", TurnRole::Assistant => "助手" };
            s.push_str(&format!("{who}: {}\n", t.text));
        }
        s.push_str("[/历史]\n\n");
        Some(s)
    }

    pub async fn record_user(&mut self, text: String, screenshot: Option<String>) -> Result<()> {
        self.append(TurnRole::User, text.clone(), screenshot).await?;
        self.history.push(Turn { role: TurnRole::User, text, streaming: None });
        self.trim_history();
        Ok(())
    }

    pub async fn record_assistant(&mut self, text: String) -> Result<()> {
        self.append(TurnRole::Assistant, text.clone(), None).await?;
        self.history.push(Turn { role: TurnRole::Assistant, text, streaming: None });
        self.trim_history();
        Ok(())
    }

    pub fn snapshot_turns(&self) -> Vec<Turn> { self.history.clone() }

    async fn append(&mut self, role: TurnRole, text: String, screenshot: Option<String>) -> Result<()> {
        let record = TurnRecord {
            session_id: self.current_session,
            timestamp: Utc::now(),
            role,
            text,
            screenshot,
        };
        let line = serde_json::to_string(&record)? + "\n";
        let mut file = tokio::fs::OpenOptions::new()
            .append(true)
            .open(&self.file_path)
            .await
            .context("open sessions.jsonl")?;
        file.write_all(line.as_bytes()).await?;
        file.flush().await?;
        self.last_turn_at = Some(Utc::now());
        Ok(())
    }

    fn trim_history(&mut self) {
        let n = self.history.len();
        if n > MAX_HISTORY_TURNS {
            self.history.drain(0..(n - MAX_HISTORY_TURNS));
        }
    }
}

fn dirs_home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

/// Format a Local timestamp for human display (e.g. session chip).
#[allow(dead_code)]
pub fn fmt_local(ts: DateTime<Utc>) -> String {
    ts.with_timezone(&Local).format("%H:%M").to_string()
}
