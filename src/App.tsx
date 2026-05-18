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

  // v0.3.9 · 手动 drag —— 之前 data-tauri-drag-region 在我们 transparent+focus:false
  // 的 overlay 上不工作。改成显式调 `startDragging()` JS API。
  //
  // 区分 click vs drag：onMouseDown 时记录起点；如果 release 前移动 >= 5px →
  // 我们调 startDragging（Tauri 接管，原生 window 跟着鼠标走），onClick 不触发
  // （因为 startDragging 抢占事件流）。如果几乎没动，onClick 正常触发菜单。
  const dragStartRef = useRef<{ x: number; y: number } | null>(null);
  const draggedRef = useRef(false);
  const DRAG_THRESHOLD = 5;

  const handlePetMouseDown = useCallback((e: React.MouseEvent) => {
    if (view.kind !== "idle") return;
    dragStartRef.current = { x: e.screenX, y: e.screenY };
    draggedRef.current = false;
  }, [view.kind]);

  const handlePetMouseMove = useCallback(async (e: React.MouseEvent) => {
    if (view.kind !== "idle" || !dragStartRef.current) return;
    const dx = Math.abs(e.screenX - dragStartRef.current.x);
    const dy = Math.abs(e.screenY - dragStartRef.current.y);
    if (dx > DRAG_THRESHOLD || dy > DRAG_THRESHOLD) {
      // 跨过阈值 → 进入拖动模式，把控制权交给 Tauri 原生 drag
      // 之后 mouseup 不会再触发 onClick（被 startDragging 抢占）
      if (!draggedRef.current) {
        draggedRef.current = true;
        try {
          const { getCurrentWindow } = await import("@tauri-apps/api/window");
          await getCurrentWindow().startDragging();
        } catch (err) {
          console.warn("startDragging:", err);
        }
      }
    }
  }, [view.kind]);

  const handlePetMouseUp = useCallback(async () => {
    if (view.kind !== "idle") return;
    const wasDragged = draggedRef.current;
    dragStartRef.current = null;
    draggedRef.current = false;
    if (!wasDragged) return; // 没拖动 → onClick 会负责开菜单
    // 拖动结束 → 读窗口位置 → 持久化
    try {
      const { getCurrentWindow } = await import("@tauri-apps/api/window");
      const pos = await getCurrentWindow().outerPosition();
      const scale = await getCurrentWindow().scaleFactor();
      const lx = pos.x / scale;
      const ly = pos.y / scale;
      await invoke("save_pet_custom_position", { x: lx, y: ly });
      console.debug(`[mouseclaw] pet dragged to (${lx}, ${ly}) saved`);
    } catch (e) {
      console.debug("save_pet_custom_position skipped:", e);
    }
  }, [view.kind]);

  const handleMouseClick = useCallback((e: React.MouseEvent) => {
    e.stopPropagation();
    // v0.1.30 · listening 时点桌宠 = 停止 + 发送（mouse-only push-to-talk 闭环）
    // 这样从 PetMenu「开始说话」启动后，用户能用鼠标完成整个流程
    if (view.kind === "listening" || view.kind === "voice-ime-listening") {
      invoke("toggle_recording").catch((err) =>
        console.warn("toggle_recording:", err));
      return;
    }
    // idle 状态：toggle 弹出菜单。
    // 其它状态（thinking / reply / panel）不响应点击，避免干扰当前流程。
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
        onMouseDown={handlePetMouseDown}
        onMouseMove={handlePetMouseMove}
        onMouseUp={handlePetMouseUp}
        style={{ cursor: view.kind === "idle" ? "grab" : "pointer" }}
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
