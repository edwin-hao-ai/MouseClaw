//! Session storage — append-only JSONL at `~/.mouseclaw/sessions.jsonl`.
//! Each line is a single turn record. Sessions are derived by grouping consecutive
//! lines with the same `session_id`.
//!
//! Boundary rules (v0.4.x · 2026-05-20 重做):
//!   • **默认永远连续** —— 删掉隐式断点（idle 超时 + 前台 app 切换）。
//!     用户报：每次召唤都像新会话；改 bug 跨 app、编辑 PPT 多轮都需要连续上下文。
//!   • 断会话只剩**显式**方式：双击快捷键 / 菜单「🆕 新对话」/ 语音"新对话"。
//!   • 「📌 钉住任务」模式：pinned=true 时连显式 force_new 也忽略，必须先解钉
//!     —— 防止长任务（编辑 PPT 十几轮）被误触双击清掉。
//!
//! 保留最近 N=20 轮作 prompt 上下文（长任务从 10 调到 20）。

use std::path::PathBuf;
use anyhow::{Context, Result};
use chrono::{DateTime, Local, Utc};
use serde::{Deserialize, Serialize};
use tokio::io::AsyncWriteExt;

use crate::events::{Turn, TurnRole};

/// 隔多久后给"软提示"（不断会话，只在气泡上提醒上下文可能旧了）。
pub const SOFT_HINT_AFTER_SECS: i64 = 30 * 60;
const MAX_HISTORY_TURNS: usize = 20;

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
    /// v0.4.x · 「📌 钉住任务」模式 —— true 时连显式 force_new 都忽略，
    /// 必须先 unpin。给编辑 PPT 这类长多轮任务防误触清空。
    pinned: bool,
    /// 钉住时显示的任务标签（如 "deck.pptx"）。None = 无标签。
    pinned_label: Option<String>,
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
            pinned: false,
            pinned_label: None,
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
    /// v0.4.x · **默认永远连续**。只有显式 `force_new`（双击快捷键 / 菜单「新对话」/
    /// 语音"新对话"）才开新 session。删掉了 idle 超时 + 前台 app 切换两个隐式断点
    /// —— 它们坑了改 bug / 编辑 PPT 这类跨 app、长间隔的连续工作流。
    ///
    /// 「📌 钉住任务」时连 force_new 都忽略（防误触清空长任务）。
    ///
    /// `frontmost` 参数保留只为 API 兼容（不再参与判断）。
    pub fn touch(&mut self, force_new: bool, _frontmost: Option<&str>) -> u64 {
        let should_new = force_new && !self.pinned;
        if should_new {
            self.current_session = Self::next_session_id();
            self.history.clear();
        }
        self.current_session
    }

    pub fn current_session(&self) -> u64 { self.current_session }

    pub fn turn_count(&self) -> usize { self.history.len() }

    /// 对话轮数（一问一答 = 1 轮）—— 给气泡 / 链条图标显示「第 N 轮」。
    pub fn round_count(&self) -> usize {
        self.history.iter().filter(|t| matches!(t.role, TurnRole::User)).count()
    }

    /// 当前是否有可续的上下文（history 非空）—— 决定要不要显示链条图标。
    pub fn has_context(&self) -> bool { !self.history.is_empty() }

    /// 距上次回复多少秒（None = 还没有过任何轮）。给"软提示"用。
    pub fn secs_since_last_turn(&self) -> Option<i64> {
        self.last_turn_at.map(|t| (Utc::now() - t).num_seconds())
    }

    /// 是否该显示"隔了很久"的软提示（有上下文 + 超过阈值）。
    pub fn should_soft_hint(&self) -> bool {
        self.has_context()
            && self.secs_since_last_turn().map(|s| s > SOFT_HINT_AFTER_SECS).unwrap_or(false)
    }

    /// 打包当前 session 状态给前端（链条图标 / 轮次 / 钉住 / 软提示）。
    pub fn state_snapshot(&self) -> crate::events::SessionState {
        crate::events::SessionState {
            continuing: self.has_context(),
            round: self.round_count() as u32,
            pinned: self.pinned,
            pinned_label: self.pinned_label.clone(),
            soft_hint: self.should_soft_hint(),
        }
    }

    // ── 「📌 钉住任务」 ────────────────────────────────────
    pub fn is_pinned(&self) -> bool { self.pinned }
    pub fn pinned_label(&self) -> Option<&str> { self.pinned_label.as_deref() }
    /// 钉住当前会话。label = 任务名（如 "deck.pptx"），None 也行。
    pub fn pin(&mut self, label: Option<String>) {
        self.pinned = true;
        self.pinned_label = label;
    }
    /// 解除钉住。
    pub fn unpin(&mut self) {
        self.pinned = false;
        self.pinned_label = None;
    }

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

    /// v0.3.11 · 从历史记录恢复一个 session —— 用户在 HistoryView 里点
    /// 「💬 继续这个话题」时调。把 in-memory state 切到指定 session：
    ///   - `current_session` = 指定 id（下次 append 会写到同一 session）
    ///   - `history` 重填为该 session 的最近 MAX_HISTORY_TURNS 轮（拼 prompt 用）
    ///   - `last_turn_at` 拨到现在 —— 软提示计时从 resume 重新开始
    pub fn resume(&mut self, session_id: u64, turns: Vec<Turn>) {
        self.current_session = session_id;
        self.history = turns;
        self.trim_history();
        self.last_turn_at = Some(Utc::now());
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

#[cfg(test)]
mod tests {
    use super::*;

    /// 构造一个临时 store（绕开 new() 的 ~/.mouseclaw 写入），只测纯逻辑。
    fn temp_store() -> SessionStore {
        let mut p = std::env::temp_dir();
        p.push(format!("mouseclaw-test-{}.jsonl", std::process::id()));
        let _ = std::fs::File::create(&p);
        SessionStore {
            file_path: p,
            current_session: 1,
            last_turn_at: None,
            pinned: false,
            pinned_label: None,
            history: Vec::new(),
        }
    }

    #[test]
    fn touch_default_continues_no_force() {
        let mut s = temp_store();
        let a = s.touch(false, Some("Code"));
        let b = s.touch(false, Some("Safari")); // 切 app 不再断
        assert_eq!(a, b, "默认应保持同一 session（删了 app_switched / idle）");
    }

    #[test]
    fn touch_force_new_starts_new_session() {
        let mut s = temp_store();
        let a = s.touch(false, None);
        let b = s.touch(true, None); // 显式 force_new
        assert_ne!(a, b, "force_new 应开新 session");
    }

    #[test]
    fn pinned_blocks_force_new() {
        let mut s = temp_store();
        let a = s.touch(false, None);
        s.pin(Some("deck.pptx".into()));
        let b = s.touch(true, None); // 钉住时 force_new 被忽略
        assert_eq!(a, b, "钉住时连 force_new 都不该开新 session");
        assert!(s.is_pinned());
        assert_eq!(s.pinned_label(), Some("deck.pptx"));
        s.unpin();
        let c = s.touch(true, None); // 解钉后恢复
        assert_ne!(b, c, "解钉后 force_new 应能开新 session");
    }
}
