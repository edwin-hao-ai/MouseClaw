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
import { RecordingBubble } from "./components/RecordingBubble";
import { EV_VIEW_CHANGED, EV_SKIN_CHANGED, type ViewKind, type SkinId } from "./types";
import { DEFAULT_SKIN } from "./skins";

const PREVIEW_LONG = "这篇 Nature 文章讨论 2026 年 AI 加速材料发现的三个突破：室温超导候选材料、新型电池电解液、碳捕获催化剂。核心机制是自动化实验室加大模型生成假设的迭代闭环。";

function isLongReply(text: string): boolean {
  return text.split("\n").length >= 4 || text.length > 140;
}

function mouseStateFor(view: ViewKind): MouseState {
  switch (view.kind) {
    case "idle":             return "sleep";
    case "onboarding":       return "listen";
    case "listening":        return "listen";
    case "voice-ime-listening": return "type"; // 长按触发键说话 → 笔状前爪 sprite
    case "thinking":         return "think";
    case "reply":            return view.streaming ? "think"
                                  : view.mode === "B" ? "write" : "jump";
    case "panel":            return "think";
    case "mode-b-countdown": return "write";
    case "mode-b-inserting": return "paste"; // v0.1.14: 拎剪贴板 sprite
    case "blocked":          return "block";
  }
}

export default function App() {
  const [view, setView] = useState<ViewKind>({ kind: "idle" });
  // session continuation chip — true when current view is part of an ongoing session
  const [continuing, setContinuing] = useState(false);
  // 当前桌宠皮肤 —— 启动读 config，运行期托盘换皮可热切换（不重启）
  const [skin, setSkin] = useState<SkinId>(DEFAULT_SKIN);

  // 启动时从 Rust 读当前皮肤（避免闪一下默认 classic 再切换）
  useEffect(() => {
    invoke<string>("get_skin")
      .then((s) => setSkin((s as SkinId) ?? DEFAULT_SKIN))
      .catch(() => { /* 浏览器 dev 模式 invoke 不可用 */ });
  }, []);


  // 托盘菜单换皮肤 → Rust 广播 skin-changed → 实时切换，不重启
  useEffect(() => {
    let unlisten: (() => void) | null = null;
    try {
      const p = listen<string>(EV_SKIN_CHANGED, (e) => {
        setSkin((e.payload as SkinId) ?? DEFAULT_SKIN);
      });
      p.then((fn) => { unlisten = fn; }).catch(() => {});
    } catch { /* browser-only mode */ }
    return () => { if (unlisten) unlisten(); };
  }, []);

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

  // v0.1.10：点「💬 继续追问」→ 开独立 720×560 Panel 窗口。
  // 之前 inline 渲染到 320×320 overlay 里完全装不下，UI 全乱（用户实测反馈）。
  const handleExpand = useCallback(() => {
    if (view.kind !== "reply") return;
    invoke("open_panel_window", {
      sessionId: 1,
      transcript: view.transcript,
      reply: view.reply,
    }).catch(e => console.warn("open_panel_window failed:", e));
  }, [view]);

  // ────────────────── Render ──────────────────
  // Onboarding lives in its OWN window opened by Rust on first launch
  // (see src-tauri/src/tray.rs::open_onboarding_window). The overlay window
  // only handles the actual mouse interaction states.

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
          <PixelMouse state="think" size={48} skin={skin} continuing={continuing} />
        </div>
      </div>
    );
  }

  return (
    <div className="stage stage-mouse-bubble">
      <BubbleFor view={view} continuing={continuing} onExpand={handleExpand} onNewSession={handleNewSession} />
      <div className="stage-mouse" onClick={() => {
        // 点桌宠开 Hub —— v0.1.16 改成开独立窗口（不再 inline）
        if (view.kind === "idle" || view.kind === "reply" || view.kind === "blocked") {
          invoke("open_hub_window").catch(e => console.warn("open_hub_window:", e));
        }
      }} style={{ cursor: "pointer" }}>
        <PixelMouse
          state={mouseStateFor(view)} skin={skin}
          size={view.kind === "idle" ? 64 : 96}
          continuing={continuing}
        />
      </div>
    </div>
  );
}

interface BubbleForProps { view: ViewKind; continuing: boolean; onExpand: () => void; onNewSession: () => void; }
function BubbleFor({ view, continuing, onExpand, onNewSession }: BubbleForProps) {
  switch (view.kind) {
    case "idle":
      return null;
    case "listening":
      // Whisper recording active. User presses shortcut again (or clicks ◼) to stop.
      return <RecordingBubble />;
    case "voice-ime-listening":
      // v0.1.14 · 语音输入法中（长按触发键），桌宠 sprite 用 "type" 态
      return <RecordingBubble />;
    case "thinking":
      // 慢 —— Claude 调用要 10-30s，给个动态 loading 让用户知道在干活
      return <Bubble text={view.transcript} loading />;
    case "reply": {
      const streaming = view.streaming ?? false;
      const long = isLongReply(view.reply);
      // 流式期间：default 样式 + 闪烁光标；流完了：success/warn 终态样式
      // v0.1.8：长回复气泡内部直接可滚动；按钮改成「↗ 在 Panel 里打开」（仍保留 Panel 入口）
      const variant = streaming ? "default" : view.mode === "B" ? "warn" : "success";
      return (
        <Bubble
          text={view.reply}
          variant={variant}
          streaming={streaming}
          scrollable={long}
          markdown
          expandable={!streaming && long}
          onExpand={onExpand}
          sessionChip={continuing ? { sessionId: 42, turn: 2 } : undefined}
          onNewSession={continuing ? onNewSession : undefined}
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
