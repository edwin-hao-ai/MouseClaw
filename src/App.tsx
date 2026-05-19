/**
 * MouseClaw root — single overlay window, composes mouse + bubble + panel + onboarding.
 *
 * State machine driven by ViewKind events from Rust. For Day 2 dev, we expose
 * a hidden keyboard shortcut (?) to cycle states locally so the UI can be
 * inspected without firing the pipeline.
 */
import { useCallback, useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";

import "./styles/tokens.css";
import { PixelMouse, type MouseState } from "./components/PixelMouse";
import { Bubble } from "./components/Bubble";
import { Panel } from "./components/Panel";
import { PetMenu } from "./components/PetMenu";
import { NudgeBubble } from "./components/NudgeBubble";
import { RecordingBubble } from "./components/RecordingBubble";
import {
  EV_VIEW_CHANGED, EV_SKIN_CHANGED, EV_NUDGE,
  type ViewKind, type SkinId, type NudgePayload,
} from "./types";
import { DEFAULT_SKIN } from "./skins";
import { useT } from "./i18n";

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
    case "feed-waiting":     return "feed-wait";    // v0.4 · 文件 hover 在桌宠上：张嘴
    case "feed-listening":   return "feed-digest";  // v0.4 · 已吞文件：消化 + 听问题
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
  const t = useT();
  const [view, setView] = useState<ViewKind>({ kind: "idle" });
  // session continuation chip — true when current view is part of an ongoing session
  const [continuing, setContinuing] = useState(false);
  // 当前桌宠皮肤 —— 启动读 config，运行期托盘换皮可热切换（不重启）
  const [skin, setSkin] = useState<SkinId>(DEFAULT_SKIN);
  // v0.1.27 P2 · 点击桌宠 → 弹出菜单（只在 idle 状态触发）
  const [petMenuOpen, setPetMenuOpen] = useState(false);
  // 临时 ack 气泡（喂奶酪 / 休息了 等本地动作的反馈）
  const [transientAck, setTransientAck] = useState<string | null>(null);
  // v0.1.27 P3 · 主动提醒（presence + nudge 引擎触发）
  const [nudge, setNudge] = useState<NudgePayload | null>(null);

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

  // v0.1.27 P3 · nudge event listener
  useEffect(() => {
    let unlisten: (() => void) | null = null;
    try {
      const p = listen<NudgePayload>(EV_NUDGE, (e) => setNudge(e.payload));
      p.then((fn) => { unlisten = fn; }).catch(() => {});
    } catch { /* browser-only mode */ }
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
      else if (k === "3") setView({ kind: "listening", partial: "" });
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
    // v0.3.11 · session_id 由后端读 state.sessions.current_session() 决定 ——
    // 之前 hardcode 1 让 Panel chip 永远显示 #000001。
    invoke("open_panel_window", {
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

  const showAck = useCallback((msg: string, ms: number = 2400) => {
    setTransientAck(msg);
    window.setTimeout(() => setTransientAck(null), ms);
  }, []);

  const handleFeed = useCallback(() => {
    showAck(t("petmenu.feed_ack"));
    // TODO P3: emit FedPet event so presence/nudge can reward streaks & swap animation
  }, [showAck, t]);

  const handleNap = useCallback((minutes: number) => {
    showAck(t("petmenu.nap_ack"), 3200);
    // v0.1.27 P3 · persist nap-until so nudge.rs suppresses reminders
    invoke("set_nap_until", { minutes }).catch((e) =>
      console.warn("set_nap_until failed:", e));
    // Also clear any currently-showing nudge so it goes away immediately
    setNudge(null);
  }, [showAck, t]);

  // v0.4.0 · 全 JS 手写 drag —— 之前 startDragging() 在 focus:false transparent
  // overlay 上不可靠，data-tauri-drag-region 同样不工作。这次自己算：
  //   pointerdown → setPointerCapture（事件追到 release 不丢，哪怕鼠标飞出窗口）
  //   pointermove → 跨过 5px 阈值 → 进 dragging 态 → setPosition 跟着鼠标走
  //   pointerup   → save 位置 + dragging=false（onClick 由阈值判断决定是否触发）
  //
  // 关键：onClick 永远会 fire（这是 DOM 标准），所以用 draggedRef 在 onClick 里判断
  // 「有没有真拖动过」，true 就 stopPropagation 让 click 静默。
  const dragStartRef = useRef<{
    pointerX: number; pointerY: number;
    winX: number; winY: number;
    scale: number;
  } | null>(null);
  const draggingRef = useRef(false);
  const draggedRef = useRef(false);
  const DRAG_THRESHOLD = 5;

  const handlePetPointerDown = useCallback(async (e: React.PointerEvent) => {
    // v0.3.11 · 拖动不再 gate 在 idle —— 回复 / thinking / 录音 状态下用户也常需要把桌宠
    // 挪开（比如挡住下面要看的内容）。只挡住非左键。
    if (e.button !== 0) return;
    try {
      const { getCurrentWindow } = await import("@tauri-apps/api/window");
      const pos = await getCurrentWindow().outerPosition();
      const scale = await getCurrentWindow().scaleFactor();
      dragStartRef.current = {
        pointerX: e.screenX,
        pointerY: e.screenY,
        winX: pos.x / scale,
        winY: pos.y / scale,
        scale,
      };
      draggingRef.current = false;
      draggedRef.current = false;
      (e.target as Element).setPointerCapture?.(e.pointerId);
    } catch (err) {
      console.debug("pointerdown init skipped:", err);
    }
  }, []);

  const handlePetPointerMove = useCallback(async (e: React.PointerEvent) => {
    if (!dragStartRef.current) return;
    const start = dragStartRef.current;
    const dx = e.screenX - start.pointerX;
    const dy = e.screenY - start.pointerY;
    if (!draggingRef.current) {
      // 还没进入 drag 态 —— 检查是否跨过阈值
      if (Math.abs(dx) < DRAG_THRESHOLD && Math.abs(dy) < DRAG_THRESHOLD) return;
      draggingRef.current = true;
      draggedRef.current = true;
    }
    // dragging 态：移动窗口跟着鼠标走
    try {
      const { getCurrentWindow, LogicalPosition } = await import("@tauri-apps/api/window");
      const newX = start.winX + dx;
      const newY = start.winY + dy;
      await getCurrentWindow().setPosition(new LogicalPosition(newX, newY));
    } catch (err) {
      console.debug("setPosition:", err);
    }
  }, []);

  const handlePetPointerUp = useCallback(async (e: React.PointerEvent) => {
    if (!dragStartRef.current) return;
    (e.target as Element).releasePointerCapture?.(e.pointerId);
    const wasDragged = draggingRef.current;
    dragStartRef.current = null;
    draggingRef.current = false;
    if (!wasDragged) return; // 轻点 → onClick 负责开菜单
    // 拖完了 → 持久化新位置
    try {
      const { getCurrentWindow } = await import("@tauri-apps/api/window");
      const pos = await getCurrentWindow().outerPosition();
      const scale = await getCurrentWindow().scaleFactor();
      await invoke("save_pet_custom_position", { x: pos.x / scale, y: pos.y / scale });
      console.debug(`[mouseclaw] pet dragged to (${pos.x / scale}, ${pos.y / scale}) saved`);
    } catch (err) {
      console.debug("save_pet_custom_position skipped:", err);
    }
  }, []);

  const handleMouseClick = useCallback((e: React.MouseEvent) => {
    e.stopPropagation();
    // v0.4.0 · 如果刚做完拖动，吃掉 click —— 不要顺便又开菜单
    if (draggedRef.current) {
      draggedRef.current = false;
      return;
    }
    if (view.kind === "listening" || view.kind === "voice-ime-listening") {
      invoke("toggle_recording").catch((err) =>
        console.warn("toggle_recording:", err));
      return;
    }
    if (view.kind !== "idle") return;
    setPetMenuOpen(prev => !prev);
  }, [view.kind]);

  return (
    <div className="stage stage-mouse-bubble">
      <BubbleFor view={view} continuing={continuing} onExpand={handleExpand} onNewSession={handleNewSession} />
      {/* Local ack bubble — visible above the pet without going through Rust */}
      {transientAck && (
        <div className="stage-bubble">
          <Bubble text={transientAck} variant="success" />
        </div>
      )}
      <div
        className="stage-mouse"
        onClick={handleMouseClick}
        onPointerDown={handlePetPointerDown}
        onPointerMove={handlePetPointerMove}
        onPointerUp={handlePetPointerUp}
        onPointerCancel={handlePetPointerUp}
        style={{ cursor: "grab" }}
      >
        <PixelMouse
          state={mouseStateFor(view)} skin={skin}
          size={view.kind === "idle" ? 64 : 96}
          continuing={continuing}
        />
        <PetMenu
          open={petMenuOpen}
          onClose={() => setPetMenuOpen(false)}
          onFeed={handleFeed}
          onNap={handleNap}
        />
        {nudge && !petMenuOpen && (
          <NudgeBubble payload={nudge} onDismiss={() => setNudge(null)} />
        )}
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
      // v0.2 · sherpa streaming —— partial transcript flows in as user speaks
      return <RecordingBubble partial={view.partial} />;
    case "voice-ime-listening":
      // v0.3.8 · Plan B —— 桌宠气泡实时显示 sherpa 流式 partial（视觉流式）。
      // 真正的"打字到光标"延后到 fn 松开后一次完成（避开 fn 抢焦点）。
      return <RecordingBubble partial={view.partial} />;
    case "feed-waiting":
      // v0.4 · 文件 hover 在桌宠上 —— 张嘴气泡
      return <Bubble text="🍽️ 喂我？拖到我嘴里" variant="warn" />;
    case "feed-listening": {
      // v0.4 · 已吞下文件 —— 显示文件名 + 实时 partial（边说边出字）
      const head = view.files.length === 1
        ? `🍽 已吃下：${view.files[0]}`
        : `🍽 已吃下 ${view.files.length} 份：${view.files.join("、")}`;
      const body = view.partial && view.partial.length > 0
        ? `\n🎙️ ${view.partial}`
        : "\n🎙️ 听着呢，对这些文件想问什么？";
      return <Bubble text={head + body} variant="default" streaming />;
    }
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
    case "blocked": {
      // v0.3.6 · [OPEN_AX_SETTINGS] 前缀 = voice IME 检测到 Accessibility 缺失
      // 渲染"🔓 去授权"按钮 + 引导文案，点击直接打开系统设置面板
      const AX_PREFIX = "[OPEN_AX_SETTINGS]";
      if (view.reason.startsWith(AX_PREFIX)) {
        const msg = view.reason.slice(AX_PREFIX.length);
        return (
          <Bubble
            text={`⛔ ${msg}`}
            variant="danger"
            action={{
              label: "🔓 去授权",
              onClick: () => {
                invoke("open_accessibility_settings").catch((e) =>
                  console.warn("open_accessibility_settings:", e));
              },
            }}
          />
        );
      }
      return <Bubble text={`⛔ ${view.reason}`} variant="danger" />;
    }
    default:
      return null;
  }
}
