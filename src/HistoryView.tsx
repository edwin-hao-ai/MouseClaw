/**
 * History window — opened from the menubar tray.
 * Renders all sessions (most recent first) with collapsible turns.
 */
import { useEffect, useState } from "react";
import { invoke, convertFileSrc } from "@tauri-apps/api/core";
import { useT } from "./i18n";
import "./HistoryView.css";

interface DictationStats {
  total_words: number;
  total_minutes: number;
  sessions: number;
  avg_wpm: number;
  saved_minutes: number;
}

/** v0.6 · 听写统计 banner —— 纯本地累计，省下多少打字时间。total_words=0 时不显示。 */
function DictationBanner({ stats }: { stats: DictationStats | null }) {
  const t = useT();
  if (!stats || stats.total_words <= 0) return null;
  return (
    <div className="hv-dict-banner" data-testid="dictation-stats">
      {t("hist.dict.summary", {
        words: stats.total_words,
        min: stats.total_minutes.toFixed(1),
        wpm: Math.round(stats.avg_wpm),
        saved: Math.round(stats.saved_minutes),
      })}
    </div>
  );
}

interface HistoryTurn {
  role: "user" | "assistant" | string;
  text: string;
  timestamp: string;
  screenshot?: string;
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
  const [dictStats, setDictStats] = useState<DictationStats | null>(null);

  useEffect(() => {
    invoke<HistorySession[]>("read_history")
      .then((s) => setSessions(s))
      .catch((e) => setError(String(e)));
    invoke<DictationStats>("get_dictation_stats")
      .then(setDictStats)
      .catch(() => {});
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
        <DictationBanner stats={dictStats} />
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
      <DictationBanner stats={dictStats} />
      <div className="hv-list">
        {sessions.map((s) => (
          <Session key={s.session_id} session={s} />
        ))}
      </div>
    </div>
  );
}

function Thumbnail({ path }: { path: string }) {
  const [expanded, setExpanded] = useState(false);
  const [error, setError] = useState(false);
  const src = convertFileSrc(path);
  if (error) {
    return <div className="hv-thumb-missing">📷 截图已删除：<code>{path}</code></div>;
  }
  return (
    <div className={`hv-thumb ${expanded ? "expanded" : ""}`} onClick={() => setExpanded(v => !v)}>
      <img src={src} alt="screenshot" onError={() => setError(true)} />
      {!expanded && <span className="hv-thumb-hint">点击放大</span>}
    </div>
  );
}

function Session({ session }: { session: HistorySession }) {
  const [open, setOpen] = useState(false);
  const [resuming, setResuming] = useState(false);
  const userTurns = session.turns.filter((t) => t.role === "user").length;
  const firstUser = session.turns.find((t) => t.role === "user");

  // v0.3.11 · 继续这个话题 —— 把当前 session 切回这个 id + 打开 Panel 窗口续聊。
  // 用户痛点：不小心关掉 Panel 后想接着问没有入口；之前只能新开一段对话。
  const handleResume = async (e: React.MouseEvent) => {
    e.stopPropagation();
    if (resuming) return;
    setResuming(true);
    try {
      await invoke("resume_session", { sessionId: session.session_id });
    } catch (err) {
      console.warn("resume_session failed:", err);
      alert(`继续会话失败：${String(err)}`);
    } finally {
      setResuming(false);
    }
  };

  return (
    <div className={`hv-session ${open ? "open" : ""}`}>
      <div className="hv-session-row">
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
        <button
          type="button"
          className="hv-session-resume"
          onClick={handleResume}
          disabled={resuming}
          title="把这段对话接回继续追问窗口"
        >
          {resuming ? "打开中…" : "💬 继续这个话题"}
        </button>
      </div>
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
              {turn.screenshot && (
                <Thumbnail path={turn.screenshot} />
              )}
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
