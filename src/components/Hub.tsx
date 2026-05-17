/**
 * Hub —— 点击桌宠展开的快捷面板 (v0.1.10)
 *
 * 来自 docs/prototypes/voice-ime-and-clipboard-20260516.html 的设计：
 * - 顶部 tabs: 📋 剪贴板 / 💬 AI 历史
 * - 搜索框
 * - 列表项点击 → 关 Hub + 粘贴到原光标
 * - Esc / 点外面 → 关
 * - ⌘1..9 数字快选
 */
import { useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useT } from "../i18n";
import "./Hub.css";

interface ClipItem {
  id: number;
  kind: string;
  text: string;
  app_bundle: string;
  app_name: string;
  ts: number;
  pinned: boolean;
}

interface HistoryTurn { role: string; text: string; timestamp: string; }
interface HistorySession {
  session_id: number;
  started_at: string;
  ended_at: string;
  turns: HistoryTurn[];
}

type Tab = "clipboard" | "history";

interface HubProps {
  onClose: () => void;
}

function relTime(ts: number, lang: string): string {
  const sec = Math.max(0, Math.floor(Date.now() / 1000 - ts));
  if (sec < 60) return lang === "en" ? "just now" : "刚刚";
  if (sec < 3600) {
    const m = Math.floor(sec / 60);
    return lang === "en" ? `${m}m ago` : `${m} 分钟前`;
  }
  if (sec < 86400) {
    const h = Math.floor(sec / 3600);
    return lang === "en" ? `${h}h ago` : `${h} 小时前`;
  }
  const d = Math.floor(sec / 86400);
  return lang === "en" ? `${d}d ago` : `${d} 天前`;
}

function detectKind(text: string): { icon: string; bg: string } {
  if (/^https?:\/\//.test(text)) return { icon: "🌐", bg: "#e8f0fe" };
  if (/^#[0-9a-fA-F]{3,8}$/.test(text)) return { icon: "🎨", bg: "#fce4ec" };
  if (/^\/[^\s]+/.test(text) && text.length < 200) return { icon: "📄", bg: "#fff3e0" };
  if (/^\s*(function|class|const|let|var|def|fn|import)\s/.test(text)) return { icon: "💻", bg: "#e8eaf6" };
  return { icon: "📝", bg: "#f0ebe2" };
}

export function Hub({ onClose }: HubProps) {
  const t = useT();
  const [tab, setTab] = useState<Tab>("clipboard");
  const [clips, setClips] = useState<ClipItem[]>([]);
  const [sessions, setSessions] = useState<HistorySession[]>([]);
  const [q, setQ] = useState("");
  const [activeIdx, setActiveIdx] = useState(0);
  const [lang, setLang] = useState("zh");
  const searchRef = useRef<HTMLInputElement>(null);

  // Load data
  useEffect(() => {
    invoke<string>("get_language").then(l => setLang(l)).catch(() => {});
    invoke<ClipItem[]>("list_clipboard").then(setClips).catch(() => {});
    invoke<HistorySession[]>("read_history").then(setSessions).catch(() => {});
  }, []);

  // Refresh clipboard every 2s while open (catch new copies)
  useEffect(() => {
    if (tab !== "clipboard") return;
    const id = setInterval(() => {
      invoke<ClipItem[]>("list_clipboard").then(setClips).catch(() => {});
    }, 2000);
    return () => clearInterval(id);
  }, [tab]);

  // Focus search on mount + tab change
  useEffect(() => { searchRef.current?.focus(); }, [tab]);

  // v0.1.13 · 点 session 展开 turns
  const [expandedSessionId, setExpandedSessionId] = useState<number | null>(null);

  // Filtered list
  const filtered = useMemo(() => {
    if (tab === "clipboard") {
      const needle = q.toLowerCase();
      const items = needle
        ? clips.filter(c => c.text.toLowerCase().includes(needle) || c.app_name.toLowerCase().includes(needle))
        : clips;
      // 标星置顶
      return [...items].sort((a, b) => Number(b.pinned) - Number(a.pinned));
    }
    return q
      ? sessions.filter(s => s.turns.some(turn => turn.text.toLowerCase().includes(q.toLowerCase())))
      : sessions;
  }, [tab, clips, sessions, q]);

  // Keyboard nav
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") { e.preventDefault(); onClose(); return; }
      if (e.key === "ArrowDown") {
        e.preventDefault();
        setActiveIdx(i => Math.min(filtered.length - 1, i + 1));
      } else if (e.key === "ArrowUp") {
        e.preventDefault();
        setActiveIdx(i => Math.max(0, i - 1));
      } else if (e.key === "Enter") {
        e.preventDefault();
        if (tab === "clipboard" && filtered[activeIdx]) {
          handlePaste((filtered[activeIdx] as ClipItem).id);
        }
      } else if (e.key === "Tab") {
        e.preventDefault();
        setTab(t => t === "clipboard" ? "history" : "clipboard");
      } else if (/^[1-9]$/.test(e.key) && !e.metaKey && !e.ctrlKey && !e.altKey) {
        // v0.1.18 · 改用纯 1-9（不带 ⌘）—— 免冲突。
        // 但用户在搜索框打字时不能拦：只有搜索框为空时才接管
        if (q.trim().length === 0) {
          e.preventDefault();
          const idx = parseInt(e.key) - 1;
          if (tab === "clipboard" && filtered[idx]) {
            handlePaste((filtered[idx] as ClipItem).id);
          }
        }
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [filtered, activeIdx, tab, onClose]);

  // Reset active when filter changes
  useEffect(() => { setActiveIdx(0); }, [q, tab]);

  const handlePaste = async (id: number) => {
    onClose(); // 先隐 Hub 窗口 → 焦点回到用户原 app
    // macOS 窗口 hide() 异步 + 焦点切换有延时（实测 ~150ms），保险 200ms
    setTimeout(() => {
      invoke("paste_clipboard_item", { id }).catch(e => {
        console.warn("paste_clipboard_item failed:", e);
      });
    }, 200);
  };

  const handleDelete = async (id: number, e: React.MouseEvent) => {
    e.stopPropagation();
    await invoke("delete_clipboard_item", { id }).catch(() => {});
    setClips(c => c.filter(x => x.id !== id));
  };

  const handlePin = async (id: number, e: React.MouseEvent) => {
    e.stopPropagation();
    await invoke("toggle_clipboard_pin", { id }).catch(() => {});
    setClips(c => c.map(x => x.id === id ? { ...x, pinned: !x.pinned } : x));
  };

  // Hover 预览（>500ms 显示完整内容浮层）
  const [preview, setPreview] = useState<{ id: number; x: number; y: number } | null>(null);
  const previewTimerRef = useRef<number | null>(null);
  const handleEnterItem = (id: number, e: React.MouseEvent) => {
    if (previewTimerRef.current) window.clearTimeout(previewTimerRef.current);
    const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
    previewTimerRef.current = window.setTimeout(() => {
      setPreview({ id, x: rect.right + 8, y: rect.top });
    }, 500);
  };
  const handleLeaveItem = () => {
    if (previewTimerRef.current) window.clearTimeout(previewTimerRef.current);
    setPreview(null);
  };

  // 右键菜单：标星 / 复制回剪贴板 / 删除
  const [ctxMenu, setCtxMenu] = useState<{ id: number; x: number; y: number } | null>(null);
  useEffect(() => {
    const onClick = () => setCtxMenu(null);
    window.addEventListener("click", onClick);
    return () => window.removeEventListener("click", onClick);
  }, []);
  const handleContextMenu = (e: React.MouseEvent, id: number) => {
    e.preventDefault();
    setCtxMenu({ id, x: e.clientX, y: e.clientY });
  };
  const handleCopyOnly = async (id: number) => {
    // 把文本写回剪贴板（不粘贴），让用户自己 ⌘V 到任意地方
    const it = clips.find(c => c.id === id);
    if (!it) return;
    try {
      await navigator.clipboard.writeText(it.text);
      onClose();
    } catch (e) { console.warn(e); }
  };

  const handleClear = async () => {
    if (!confirm(lang === "en"
      ? "Clear all clipboard history? Pinned items will also be removed."
      : "清空全部剪贴板历史？包括已标星的项目。")) return;
    await invoke("clear_clipboard").catch(() => {});
    setClips([]);
  };

  return (
    <div className="hub" role="dialog" aria-label={t("hub.title")}>
      <div className="hub-tabs">
        <button className={`hub-tab ${tab === "clipboard" ? "active" : ""}`}
                onClick={() => setTab("clipboard")}>
          📋 {t("hub.tab.clipboard")}
          <span className="hub-tab-count">{clips.length}</span>
        </button>
        <button className={`hub-tab ${tab === "history" ? "active" : ""}`}
                onClick={() => setTab("history")}>
          💬 {t("hub.tab.history")}
          <span className="hub-tab-count">{sessions.length}</span>
        </button>
        <button className="hub-close" onClick={onClose} aria-label={t("common.close")}>✕</button>
      </div>

      <div className="hub-search">
        <span aria-hidden>🔍</span>
        <input ref={searchRef} value={q} onChange={e => setQ(e.target.value)}
               placeholder={tab === "clipboard" ? t("hub.search.clipboard") : t("hub.search.history")} />
        <kbd>{q ? "Esc" : "Tab"}</kbd>
      </div>

      <div className="hub-list">
        {filtered.length === 0 && (
          <div className="hub-empty">
            {tab === "clipboard" ? t("hub.empty.clipboard") : t("hub.empty.history")}
          </div>
        )}
        {tab === "clipboard" && (filtered as ClipItem[]).map((c, i) => {
          const k = detectKind(c.text);
          return (
            <div key={c.id}
                 className={`hub-item ${i === activeIdx ? "active" : ""}`}
                 onClick={() => handlePaste(c.id)}
                 onContextMenu={(e) => handleContextMenu(e, c.id)}
                 onMouseEnter={(e) => handleEnterItem(c.id, e)}
                 onMouseLeave={handleLeaveItem}
                 draggable
                 onDragStart={(e) => {
                   // 拖出 Hub → 拖到任意 app → 接收方收到 text/plain
                   // macOS Finder/任何文本字段/编辑器都能 drop（prototype Section ⑤）
                   e.dataTransfer.setData("text/plain", c.text);
                   e.dataTransfer.effectAllowed = "copy";
                 }}>
              <div className="hub-item-icon" style={{ background: k.bg }}>{k.icon}</div>
              <div className="hub-item-body">
                <div className="hub-item-text">{c.text.replace(/\n/g, " ⏎ ")}</div>
                <div className="hub-item-meta">
                  {c.pinned && <span className="hub-pin">⭐</span>}
                  {c.app_name || "?"} · {relTime(c.ts, lang)}
                </div>
              </div>
              <div className="hub-item-actions">
                {i < 9 && q.trim().length === 0 && <span className="hub-item-key">{i + 1}</span>}
                <button className="hub-act" onClick={e => handlePin(c.id, e)}
                        title={c.pinned ? t("hub.unpin") : t("hub.pin")}>
                  {c.pinned ? "★" : "☆"}
                </button>
                <button className="hub-act" onClick={e => handleDelete(c.id, e)}
                        title={t("common.close")}>✕</button>
              </div>
            </div>
          );
        })}
        {tab === "history" && (filtered as HistorySession[]).map((s, i) => {
          const isExpanded = expandedSessionId === s.session_id;
          return (
            <div key={s.session_id} className="hub-history-block">
              <div
                className={`hub-item ${i === activeIdx ? "active" : ""}`}
                onClick={() => setExpandedSessionId(isExpanded ? null : s.session_id)}
              >
                <div className="hub-item-icon">{isExpanded ? "▼" : "▶"}</div>
                <div className="hub-item-body">
                  <div className="hub-item-text">
                    {s.turns[0]?.text || t("hub.empty.history")}
                  </div>
                  <div className="hub-item-meta">
                    {t("history.turn_count", { count: s.turns.length })} · {new Date(s.started_at).toLocaleString()}
                  </div>
                </div>
              </div>
              {isExpanded && (
                <div className="hub-history-turns">
                  {s.turns.map((turn, ti) => (
                    <div key={ti} className={`hub-turn hub-turn-${turn.role}`}>
                      <span className="hub-turn-role">
                        {turn.role === "user" ? t("history.role.user") : t("history.role.assistant")}
                      </span>
                      <span className="hub-turn-text">{turn.text}</span>
                    </div>
                  ))}
                  <div className="hub-history-actions">
                    <button onClick={async () => {
                      // 复制最后一条 assistant 回答到剪贴板
                      const last = [...s.turns].reverse().find(t => t.role === "assistant");
                      if (last) {
                        try { await navigator.clipboard.writeText(last.text); } catch {}
                      }
                    }}>📋 {t("hub.copy_only")}</button>
                  </div>
                </div>
              )}
            </div>
          );
        })}
      </div>

      {preview && (() => {
        const item = clips.find(c => c.id === preview.id);
        if (!item) return null;
        // 自动 clamp 位置，避免超出右边屏幕
        const maxLeft = Math.min(preview.x, window.innerWidth - 320);
        return (
          <div className="hub-preview"
               style={{ position: "fixed", left: maxLeft, top: preview.y }}>
            <div className="hub-preview-text">{item.text}</div>
            <div className="hub-preview-meta">
              {item.app_name || "?"} · {item.text.length} {t("common.search").length > 3 ? "chars" : "字"}
            </div>
          </div>
        );
      })()}
      {ctxMenu && (
        <div className="hub-ctx" style={{ position: "fixed", left: ctxMenu.x, top: ctxMenu.y }}
             onClick={e => e.stopPropagation()}>
          <button onClick={() => { handlePaste(ctxMenu.id); setCtxMenu(null); }}>
            {t("hub.foot.paste")}
          </button>
          <button onClick={() => { handleCopyOnly(ctxMenu.id); setCtxMenu(null); }}>
            {t("hub.copy_only")}
          </button>
          <button onClick={(e) => { handlePin(ctxMenu.id, e); setCtxMenu(null); }}>
            {clips.find(x => x.id === ctxMenu.id)?.pinned ? t("hub.unpin") : t("hub.pin")}
          </button>
          <button onClick={(e) => { handleDelete(ctxMenu.id, e); setCtxMenu(null); }}>
            {t("hub.delete")}
          </button>
        </div>
      )}
      <div className="hub-footer">
        <span><kbd>↑↓</kbd> · <kbd>↵</kbd> {t("hub.foot.paste")} · <kbd>1..9</kbd> · <kbd>Esc</kbd></span>
        {tab === "clipboard" && clips.length > 0 && (
          <button className="hub-clear" onClick={handleClear}>{t("hub.clear")}</button>
        )}
      </div>
    </div>
  );
}
