/**
 * Long-form expanded panel — DESIGN.md §4.2.
 * Renders session header + scrolling conversation history + follow-up input row.
 */
import { useEffect, useRef, useState } from "react";
import "./Panel.css";

export interface Turn {
  /** "user" or "assistant" — drives styling. */
  role: "user" | "assistant";
  text: string;
  streaming?: boolean;
}

interface PanelProps {
  sessionId: number;
  turns: Turn[];
  /** Total turns budget (for "2/15 ⌬" display). */
  maxTurns?: number;
  onSend: (text: string) => void;
  onCollapse: () => void;
  onNewSession: () => void;
}

export function Panel({
  sessionId, turns, maxTurns = 15,
  onSend, onCollapse, onNewSession,
}: PanelProps) {
  const [input, setInput] = useState("");
  const bodyRef = useRef<HTMLDivElement>(null);

  // Auto-scroll to latest turn
  useEffect(() => {
    const el = bodyRef.current;
    if (el) el.scrollTop = el.scrollHeight;
  }, [turns]);

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    const trimmed = input.trim();
    if (!trimmed) return;
    onSend(trimmed);
    setInput("");
  };

  const handleKey = (e: React.KeyboardEvent) => {
    if (e.key === "Escape") {
      e.preventDefault();
      onCollapse();
    }
  };

  return (
    <div className="panel" onKeyDown={handleKey} tabIndex={-1}>
      <header className="panel-header">
        <span className="panel-chip">
          🔗 续 Session #{sessionId} · 第 {turns.filter(t => t.role === "user").length} 轮
        </span>
        <span className="panel-counter">
          {turns.length}/{maxTurns} ⌬
        </span>
        <div className="panel-actions">
          <button type="button" title="新 session" onClick={onNewSession}>＋</button>
          <button type="button" title="复制全部"
            onClick={() => navigator.clipboard.writeText(
              turns.map(t => `${t.role === "user" ? "我" : "🦞"}: ${t.text}`).join("\n\n")
            )}
          >⌘</button>
          <button type="button" title="折叠" onClick={onCollapse}>▾</button>
        </div>
      </header>

      <div className="panel-body" ref={bodyRef}>
        {turns.map((turn, i) => (
          <div key={i} className={`panel-turn turn-${turn.role}`}>
            {turn.role === "user" ? (
              <div className="turn-user-text">{turn.text}</div>
            ) : (
              <div className="turn-assistant-text">
                {turn.text}
                {turn.streaming && <span className="stream-cursor" aria-hidden>▮</span>}
              </div>
            )}
          </div>
        ))}
      </div>

      <form className="panel-input" onSubmit={handleSubmit}>
        <input
          type="text"
          value={input}
          onChange={(e) => setInput(e.target.value)}
          placeholder="接着问…（Enter 发送，Esc 折叠）"
          autoFocus
        />
        <button type="submit" className="panel-send" aria-label="发送">↑</button>
      </form>
    </div>
  );
}
