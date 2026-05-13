/**
 * History window — opened from the menubar tray.
 * Renders all sessions (most recent first) with collapsible turns.
 */
import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import "./HistoryView.css";

interface HistoryTurn {
  role: "user" | "assistant" | string;
  text: string;
  timestamp: string;
}
interface HistorySession {
  session_id: number;
  started_at: string;
  ended_at: string;
  turns: HistoryTurn[];
}

function fmtTime(iso: string) {
  try {
    const d = new Date(iso);
    return d.toLocaleString("zh-CN", { hour12: false });
  } catch {
    return iso;
  }
}

function fmtSession(id: number) {
  // session_id is unix seconds — show short ref
  return `#${id.toString().slice(-6)}`;
}

export default function HistoryView() {
  const [sessions, setSessions] = useState<HistorySession[] | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    invoke<HistorySession[]>("read_history")
      .then((s) => setSessions(s))
      .catch((e) => setError(String(e)));
  }, []);

  if (error) {
    return (
      <div className="hv-root">
        <div className="hv-empty">⛔ 读取历史失败：{error}</div>
      </div>
    );
  }
  if (!sessions) {
    return (
      <div className="hv-root">
        <div className="hv-empty">加载中…</div>
      </div>
    );
  }
  if (sessions.length === 0) {
    return (
      <div className="hv-root">
        <div className="hv-empty">
          <div className="hv-empty-icon">📜</div>
          <div>还没有历史对话</div>
          <div className="hv-empty-sub">按 Cmd+Shift+Space 召唤老鼠开始第一次对话</div>
        </div>
      </div>
    );
  }

  const totalTurns = sessions.reduce((n, s) => n + s.turns.length, 0);

  return (
    <div className="hv-root">
      <header className="hv-header">
        <h1>历史记录</h1>
        <span className="hv-stat">
          {sessions.length} 个 session · {totalTurns} 轮对话
        </span>
      </header>
      <div className="hv-list">
        {sessions.map((s) => (
          <Session key={s.session_id} session={s} />
        ))}
      </div>
    </div>
  );
}

function Session({ session }: { session: HistorySession }) {
  const [open, setOpen] = useState(false);
  const userTurns = session.turns.filter((t) => t.role === "user").length;
  const firstUser = session.turns.find((t) => t.role === "user");

  return (
    <div className={`hv-session ${open ? "open" : ""}`}>
      <button
        type="button"
        className="hv-session-head"
        onClick={() => setOpen((v) => !v)}
        aria-expanded={open}
      >
        <span className="hv-session-chip">{fmtSession(session.session_id)}</span>
        <span className="hv-session-time">{fmtTime(session.started_at)}</span>
        <span className="hv-session-turns">{userTurns} 轮</span>
        <span className="hv-session-preview">{firstUser?.text ?? "(空)"}</span>
        <span className="hv-session-caret">{open ? "▾" : "▸"}</span>
      </button>
      {open && (
        <div className="hv-session-body">
          {session.turns.map((turn, i) => (
            <div key={i} className={`hv-turn turn-${turn.role}`}>
              <div className="hv-turn-meta">
                <span className="hv-turn-role">
                  {turn.role === "user" ? "🧑 我" : "🦞 鼠"}
                </span>
                <span className="hv-turn-time">{fmtTime(turn.timestamp)}</span>
              </div>
              <div className="hv-turn-text">{turn.text}</div>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
