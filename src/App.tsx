/**
 * MouseClaw root — single overlay window, composes mouse + bubble + panel + onboarding.
 *
 * State machine driven by ViewKind events from Rust. For Day 2 dev, we expose
 * a hidden keyboard shortcut (?) to cycle states locally so the UI can be
 * inspected without firing the pipeline.
 */
import { useCallback, useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";

import "./styles/tokens.css";
import { PixelMouse, type MouseState } from "./components/PixelMouse";
import { Bubble } from "./components/Bubble";
import { Panel } from "./components/Panel";
import { Onboarding } from "./components/Onboarding";
import { RecordingBubble } from "./components/RecordingBubble";
import { EV_VIEW_CHANGED, type ViewKind, type ShortcutChoice } from "./types";

const PREVIEW_LONG = "这篇 Nature 文章讨论 2026 年 AI 加速材料发现的三个突破：室温超导候选材料、新型电池电解液、碳捕获催化剂。核心机制是自动化实验室加大模型生成假设的迭代闭环。";

function isLongReply(text: string): boolean {
  return text.split("\n").length >= 4 || text.length > 140;
}

function mouseStateFor(view: ViewKind): MouseState {
  switch (view.kind) {
    case "idle":             return "sleep";
    case "onboarding":       return "listen";
    case "listening":        return "listen";
    case "thinking":         return "think";
    case "reply":            return view.mode === "B" ? "write" : "jump";
    case "panel":            return "think";
    case "mode-b-countdown": return "write";
    case "mode-b-inserting": return "write";
    case "blocked":          return "block";
  }
}

export default function App() {
  const [view, setView] = useState<ViewKind>({ kind: "idle" });
  const [onboarded, setOnboarded] = useState<boolean>(
    () => localStorage.getItem("mouseclaw.onboarded") === "1"
  );
  // session continuation chip — true when current view is part of an ongoing session
  const [continuing, setContinuing] = useState(false);

  // Listen for state changes from Rust.
  // Wrapped in try/catch because @tauri-apps/api/event.listen() throws when run
  // outside a Tauri shell (e.g. browser-only Vite dev mode for UI testing).
  useEffect(() => {
    let unlisten: (() => void) | null = null;
    try {
      const p = listen<ViewKind>(EV_VIEW_CHANGED, (e) => {
        setView(e.payload);
      });
      p.then((fn) => { unlisten = fn; }).catch(() => {});
    } catch (e) {
      console.warn("Tauri event listen unavailable (browser-only mode):", e);
    }
    return () => { if (unlisten) unlisten(); };
  }, []);

  // Global Esc → dismiss immediately (hide window, cancel pending pipeline timers).
  // Skip when typing inside the Panel input (Panel handles its own Esc).
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Escape") return;
      const target = e.target as HTMLElement | null;
      if (target && (target.tagName === "INPUT" || target.tagName === "TEXTAREA")) return;
      invoke("dismiss").catch(() => {});
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  // When view transitions to Panel, tell Rust to pin (cancel pending auto-hide).
  useEffect(() => {
    if (view.kind === "panel") {
      invoke("pin_window").catch(() => {});
    }
  }, [view.kind]);

  // Dev-only: cycle states by pressing 1-9 on the keyboard
  useEffect(() => {
    if (!import.meta.env.DEV) return;
    const onKey = (e: KeyboardEvent) => {
      const k = e.key;
      if (k === "1") setView({ kind: "idle" });
      else if (k === "2") setView({ kind: "onboarding" });
      else if (k === "3") setView({ kind: "listening" });
      else if (k === "4") setView({ kind: "thinking", transcript: "总结这个网页" });
      else if (k === "5") setView({ kind: "reply", transcript: "总结这个网页", reply: PREVIEW_LONG, mode: "A" });
      else if (k === "6") setView({ kind: "reply", transcript: "看一眼屏幕", reply: "✅ 已发送邮件", mode: "A" });
      else if (k === "7") setView({
        kind: "panel", sessionId: 42, turns: [
          { role: "user", text: "总结这个网页" },
          { role: "assistant", text: PREVIEW_LONG },
          { role: "user", text: "那 Berkeley 用的是什么模型" },
          { role: "assistant", text: "等离子扩散模型，源代码在 github.com/Berkeley/...", streaming: true },
        ]
      });
      else if (k === "8") setView({ kind: "mode-b-countdown", insertText: "他望着窗外，第一片雪正缓缓落下。", remaining: 3 });
      else if (k === "9") setView({ kind: "blocked", reason: "终端窗口禁止写入" });
      else return;
      setContinuing(k === "7");
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  const handleOnboard = useCallback(async (choice: ShortcutChoice) => {
    try {
      await invoke("save_shortcut", { choice });
    } catch (e) {
      console.warn("save_shortcut not yet registered:", e);
    }
    localStorage.setItem("mouseclaw.onboarded", "1");
    setOnboarded(true);
    setView({ kind: "idle" });
  }, []);

  const handlePanelSend = useCallback(async (text: string) => {
    try { await invoke("follow_up", { text }); }
    catch (e) { console.warn("follow_up not yet registered:", e); }
  }, []);

  const handleCollapse = useCallback(() => {
    // Folding the panel back = dismiss the whole overlay (PRD §IV "回角落").
    invoke("dismiss").catch(() => {});
  }, []);

  const handleNewSession = useCallback(async () => {
    try { await invoke("new_session"); }
    catch (e) { console.warn("new_session not yet registered:", e); }
    setContinuing(false);
  }, []);

  // Must be declared BEFORE any early-return JSX below — React hooks rules
  // require the same number of hook calls on every render, but early returns
  // for panel/onboarding branches would have skipped this previously.
  const handleExpand = useCallback(() => {
    if (view.kind === "reply") {
      setView({
        kind: "panel",
        sessionId: 1,
        turns: [
          { role: "user", text: view.transcript },
          { role: "assistant", text: view.reply },
        ],
      });
    }
  }, [view]);

  // ────────────────── Render ──────────────────

  // Onboarding screen takes over the whole window
  if (!onboarded && view.kind !== "idle") {
    return <Onboarding onComplete={handleOnboard} />;
  }
  if (view.kind === "onboarding") {
    return <Onboarding onComplete={handleOnboard} />;
  }

  // Panel takes precedence over mouse-only views
  if (view.kind === "panel") {
    return (
      <div className="stage stage-panel">
        <Panel
          sessionId={view.sessionId}
          turns={view.turns}
          onSend={handlePanelSend}
          onCollapse={handleCollapse}
          onNewSession={handleNewSession}
        />
        <div className="stage-mouse">
          <PixelMouse state="think" size={48} continuing={continuing} />
        </div>
      </div>
    );
  }

  return (
    <div className="stage stage-mouse-bubble">
      <BubbleFor view={view} continuing={continuing} onExpand={handleExpand} />
      <div className="stage-mouse">
        <PixelMouse
          state={mouseStateFor(view)}
          size={view.kind === "idle" ? 64 : 96}
          continuing={continuing}
        />
      </div>
    </div>
  );
}

interface BubbleForProps { view: ViewKind; continuing: boolean; onExpand: () => void; }
function BubbleFor({ view, continuing, onExpand }: BubbleForProps) {
  switch (view.kind) {
    case "idle":
      return null;
    case "listening":
      // Whisper recording active. User presses shortcut again (or clicks ◼) to stop.
      return <RecordingBubble />;
    case "thinking":
      return <Bubble text={view.transcript} />;
    case "reply": {
      const long = isLongReply(view.reply);
      const variant = view.mode === "B" ? "warn" : "success";
      return (
        <Bubble
          text={view.reply}
          variant={variant}
          expandable={long}
          onExpand={onExpand}
          sessionChip={continuing ? { sessionId: 42, turn: 2 } : undefined}
        />
      );
    }
    case "mode-b-countdown":
      return (
        <Bubble
          text={`⌨ 即将写入：${view.insertText}\n(${view.remaining}s · Esc 取消 / ↵ 立即)`}
          variant="warn"
        />
      );
    case "mode-b-inserting":
      return <Bubble text={`正在写入「${view.insertText}」`} variant="warn" />;
    case "blocked":
      return <Bubble text={`⛔ ${view.reason}`} variant="danger" />;
    default:
      return null;
  }
}
