/**
 * MouseClaw root — single overlay window, composes mouse + bubble + panel + onboarding.
 *
 * State machine driven by ViewKind events from Rust. For Day 2 dev, we expose
 * a hidden keyboard shortcut (?) to cycle states locally so the UI can be
 * inspected without firing the pipeline.
 */
import React, { useCallback, useEffect, useRef, useState } from "react";
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
import { useT, getCurrentLang } from "./i18n";
import { ReactiveOverlay, type ReactivePayload } from "./components/ReactiveOverlay";

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
    case "reply":            return view.streaming ? "talk"
                                  : view.mode === "B" ? "write" : "jump";
    case "panel":            return "think"; // panel 默认沉默；流式由内部组件单独驱动
    case "mode-b-countdown": return "write";
    case "mode-b-inserting": return "paste"; // v0.1.14: 拎剪贴板 sprite
    case "voice-confirm":    return "think"; // v0.4.0 转写完确认中
    case "tour-step":        return "listen"; // v0.4.0 引导桌宠 = 听 sprite，活泼
    case "blocked":          return "block";
  }
}

export default function App() {
  const t = useT();
  const [view, setView] = useState<ViewKind>({ kind: "idle" });
  // session continuation chip — true when current view is part of an ongoing session
  const [continuing, setContinuing] = useState(false);
  // v0.4.0 · 模型下载状态 —— idle 时模型未就绪自动显示常驻迷你气泡，
  //   让用户即使没按快捷键也能看到「在下载 / 完成 / 失败」。
  //   下完后这个气泡自动消失，桌宠回到正常 sleep 状态。
  const [modelProgress, setModelProgress] = useState<{
    pct: number; mb_done: number; mb_total: number; phase: string;
  } | null>(null);
  // 当前桌宠皮肤 —— 启动读 config，运行期托盘换皮可热切换（不重启）
  const [skin, setSkin] = useState<SkinId>(DEFAULT_SKIN);
  // v0.1.27 P2 · 点击桌宠 → 弹出菜单（只在 idle 状态触发）
  const [petMenuOpen, setPetMenuOpen] = useState(false);
  // 临时 ack 气泡（喂奶酪 / 休息了 等本地动作的反馈）
  const [transientAck, setTransientAck] = useState<string | null>(null);
  // v0.1.27 P3 · 主动提醒（presence + nudge 引擎触发）
  const [nudge, setNudge] = useState<NudgePayload | null>(null);
  // v0.4 · Reactive 桌宠：剪贴板变化时立刻反应。
  // - twitching：T1 抖耳一次，~500ms 后自动清掉
  // - reactive：T2 payload —— ribbon 在桌宠头顶弹一组按钮，5s 自动消失（或点了 action）
  const [twitching, setTwitching] = useState(false);
  const [reactive, setReactive] = useState<ReactivePayload | null>(null);

  // v0.3.12 · 在 idle 状态下显示 React-only UI（下载提示气泡 / petMenu / nudge / ack
  //   / v0.4 reactive ribbon）时主动通知 Rust 把窗口 hit-box 扩到全窗口；
  //   其余时间 Rust 自动按 view.kind 处理。
  //   这是为了让 idle 静默时透明区域真的穿透到底层 app，但 React 临时弹的气泡也可点。
  useEffect(() => {
    const hasReactUi = !!modelProgress || petMenuOpen || !!nudge || !!transientAck || !!reactive;
    if (view.kind !== "idle") return; // 非 idle 由 Rust emit_view 那侧管，前端不要干扰
    invoke("set_overlay_has_ui", { hasUi: hasReactUi }).catch(() => {});
  }, [view.kind, modelProgress, petMenuOpen, nudge, transientAck, reactive]);

  // 启动时从 Rust 读当前皮肤（避免闪一下默认 classic 再切换）
  useEffect(() => {
    invoke<string>("get_skin")
      .then((s) => setSkin((s as SkinId) ?? DEFAULT_SKIN))
      .catch(() => { /* 浏览器 dev 模式 invoke 不可用 */ });
  }, []);

  // v0.4.0 · 监听模型下载进度 —— 聚合 ASR + 标点两条，算总 % 给桌宠 idle 气泡用
  useEffect(() => {
    interface PEvent {
      model_id: string; phase: string;
      total_done: number; total_expected: number;
    }
    const progressMap: Record<string, PEvent> = {};
    const recompute = () => {
      const vals = Object.values(progressMap);
      if (vals.length === 0) { setModelProgress(null); return; }
      // 任一 phase=error → 显示错误状态
      // 全部 ok → 隐藏（null）
      // 否则取最小 % 显示（最慢的那个）
      const anyErr = vals.some(v => v.phase === "error");
      const allOk = vals.every(v => v.phase === "ok");
      if (allOk && !anyErr) { setModelProgress(null); return; }
      const totalDone = vals.reduce((s, v) => s + v.total_done, 0);
      const totalExpected = vals.reduce((s, v) => s + v.total_expected, 0);
      const pct = totalExpected > 0 ? (totalDone / totalExpected * 100) : 0;
      setModelProgress({
        pct,
        mb_done: totalDone / 1024 / 1024,
        mb_total: totalExpected / 1024 / 1024,
        phase: anyErr ? "error" : "downloading",
      });
    };
    // 初次拉一次（catch 模型已就绪 / 没事件可听的场景）
    invoke<PEvent[]>("get_model_status").then(list => {
      for (const e of list || []) progressMap[e.model_id] = e;
      recompute();
    }).catch(() => {});

    let unlisten: (() => void) | null = null;
    try {
      const p = listen<PEvent>("model-progress", (e) => {
        progressMap[e.payload.model_id] = e.payload;
        recompute();
      });
      p.then(fn => { unlisten = fn; }).catch(() => {});
    } catch {/* dev mode */}
    return () => { if (unlisten) unlisten(); };
  }, []);


  // v0.4 · Reactive 桌宠 —— Rust clipboard 捕获到新条目时按 tier 发事件
  //   - tier=acknowledge → 只触发抖耳（不弹 ribbon）
  //   - tier=hint        → 抖耳 + ribbon 弹按钮组
  // 注意：overlay 窗口 idle 时是 compact 80×80，ribbon 超出会被裁掉。
  //   set_overlay_has_ui 的调用由上面那个 useEffect 通过 `reactive` 依赖自动驱动 ——
  //   reactive 非空 ⇒ hasUi=true（窗口撑大）；reactive=null ⇒ hasUi=false（缩回去）。
  //   这里不要再重复 invoke，会跟那个 effect 抢状态。
  useEffect(() => {
    let unlisten: (() => void) | null = null;
    let twitchTimer: number | undefined;
    type Inbound = Omit<ReactivePayload, "receivedAt">;
    try {
      const p = listen<Inbound>("clipboard-reactive", (e) => {
        setTwitching(true);
        if (twitchTimer) window.clearTimeout(twitchTimer);
        twitchTimer = window.setTimeout(() => setTwitching(false), 600);
        if (e.payload.tier === "hint") {
          setReactive({ ...e.payload, receivedAt: Date.now() });
        }
      });
      p.then((fn) => { unlisten = fn; }).catch(() => {});
    } catch {/* dev mode */}
    return () => {
      if (unlisten) unlisten();
      if (twitchTimer) window.clearTimeout(twitchTimer);
    };
  }, []);

  const dismissReactive = useCallback(() => setReactive(null), []);


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
  // v0.4.0 · voice-confirm 状态下：Esc → voice_confirm_cancel; Enter → voice_confirm_send。
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const target = e.target as HTMLElement | null;
      const inInput = target && (target.tagName === "INPUT" || target.tagName === "TEXTAREA");
      if (inInput) return;
      if (view.kind === "voice-confirm") {
        if (e.key === "Escape") {
          invoke("voice_confirm_cancel").catch(() => {});
          e.preventDefault();
        } else if (e.key === "Enter") {
          invoke("voice_confirm_send").catch(() => {});
          e.preventDefault();
        }
        return;
      }
      if (e.key !== "Escape") return;
      invoke("dismiss").catch(() => {});
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [view.kind]);

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
    // v0.3.12 · 拖动期间锁定 has_ui=true（全窗口接收事件）。否则 pet_passthrough 在光标
    // 移出 hit-box 时会切到穿透 → 拖动断裂、桌宠定在半路不动。
    invoke("set_overlay_has_ui", { hasUi: true }).catch(() => {});
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
    // v0.3.12 · 拖动结束 —— 放开 has_ui 锁定。下一帧 useEffect 会按 idle 状态自动恢复
    // 正确值（idle 静默 = false / 有气泡 = true）。
    if (view.kind === "idle") {
      const hasReactUi = !!modelProgress || petMenuOpen || !!nudge || !!transientAck || !!reactive;
      invoke("set_overlay_has_ui", { hasUi: hasReactUi }).catch(() => {});
    }
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
  }, [view.kind, modelProgress, petMenuOpen, nudge, transientAck]);

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
      <BubbleFor view={view} continuing={continuing} onExpand={handleExpand} onNewSession={handleNewSession} modelProgress={modelProgress} />
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
          twitching={twitching}
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
        {/* v0.4 · Reactive ribbon —— idle 视图下浮在桌宠头顶。其他视图（listening / thinking
            / panel 等）有自己的气泡，让位 */}
        {view.kind === "idle" && !petMenuOpen && !nudge && (
          <ReactiveOverlay
            payload={reactive}
            lang={getCurrentLang() as "zh" | "en"}
            onDismiss={dismissReactive}
          />
        )}
      </div>
    </div>
  );
}

interface BubbleForProps {
  view: ViewKind; continuing: boolean;
  onExpand: () => void; onNewSession: () => void;
  modelProgress: { pct: number; mb_done: number; mb_total: number; phase: string } | null;
}
function BubbleFor({ view, continuing, onExpand, onNewSession, modelProgress }: BubbleForProps) {
  switch (view.kind) {
    case "idle":
      // v0.4.0 · idle 时模型未就绪 → 显示常驻迷你气泡 + 打开下载窗口入口
      if (modelProgress) {
        const isErr = modelProgress.phase === "error";
        const text = isErr
          ? `🦞 模型下载失败\n点击重试`
          : `🦞 下载语音模型…\n${modelProgress.mb_done.toFixed(0)}/${modelProgress.mb_total.toFixed(0)} MB · ${modelProgress.pct.toFixed(0)}%`;
        return (
          <div onClick={() => invoke("open_downloader_window").catch(() => {})}
               style={{ cursor: "pointer", pointerEvents: "auto" }}>
            <Bubble text={text} variant={isErr ? "danger" : "default"} />
          </div>
        );
      }
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
    case "voice-confirm":
      // v0.4.0 · 转写完 3 秒倒数确认。点气泡 = 进入编辑模式。
      return <VoiceConfirmBubble transcript={view.transcript} remaining={view.remaining} />;
    case "tour-step":
      // v0.4.0 · 首次使用引导 5 步流程
      return <TourBubble step={view.step} />;
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


// v0.4.0 → v0.3.11 重构 · 语音转写确认气泡
//   单一形态：转写文本一上来就装进可编辑 textarea，用户直接 ↵ 发 / Esc 取消 / 改字。
//   倒数 N 秒到了自动发送（Rust 端管，前端只显示数字）。
//   之前"先气泡 → 点一下进编辑"两段式被用户反馈"修改框很乱" → 砍掉。
interface VoiceConfirmBubbleProps { transcript: string; remaining: number; }
function VoiceConfirmBubble({ transcript, remaining }: VoiceConfirmBubbleProps) {
  const [text, setText] = useState(transcript);
  const dirtyRef = useRef(false); // 用户改过 → 不再被 Rust 周期 emit 覆盖
  const taRef = useRef<HTMLTextAreaElement>(null);
  // remaining === 99 是 Rust 端 HOLD 状态的哨兵 —— 显示"编辑中"，不再倒数
  const holding = remaining >= 99;

  useEffect(() => {
    if (!dirtyRef.current) setText(transcript);
  }, [transcript]);

  // mount 后自动 focus + 光标移到末尾（让用户能直接接着说补充）
  useEffect(() => {
    const el = taRef.current;
    if (!el) return;
    el.focus();
    el.setSelectionRange(el.value.length, el.value.length);
  }, []);

  const cancel = () => invoke("voice_confirm_cancel").catch(() => {});
  const sendNow = () => {
    const trimmed = text.trim();
    if (dirtyRef.current && trimmed.length > 0) {
      invoke("voice_confirm_edit", { text: trimmed }).catch(() => {});
    } else {
      invoke("voice_confirm_send").catch(() => {});
    }
  };
  const onChange = (val: string) => {
    if (!dirtyRef.current) {
      dirtyRef.current = true;
      // 第一次按键 → 通知 Rust 暂停倒数（HOLD），无限等用户改完
      invoke("voice_confirm_hold", { text: val }).catch(() => {});
    }
    setText(val);
  };

  return (
    <div
      style={{
        background: "#fff",
        border: "1px solid #e8d8dc",
        borderRadius: 18,
        padding: "10px 12px 8px",
        boxShadow: "0 8px 32px rgba(43,38,34,.18)",
        width: 282, // overlay 是 320 宽，留 ~38 边距给阴影 + 安全区
        boxSizing: "border-box",
        pointerEvents: "auto",
        fontFamily: "-apple-system, 'PingFang SC', system-ui, sans-serif",
      }}
    >
      <div style={{
        display: "flex", alignItems: "center", justifyContent: "space-between",
        fontSize: 11, color: "#8a8178", marginBottom: 6, gap: 8,
      }}>
        <span style={{ whiteSpace: "nowrap" }}>🎙 听到（可改）</span>
        <span style={{
          color: holding ? "#1a6b3a" : "#d63d6a", fontWeight: 600,
          whiteSpace: "nowrap",
        }}>
          {holding ? "✏️ 编辑中…" : `${remaining}s 自动发`}
        </span>
      </div>
      <textarea
        ref={taRef}
        value={text}
        onChange={(e) => onChange(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Enter" && !e.shiftKey) {
            e.preventDefault();
            sendNow();
          } else if (e.key === "Escape") {
            e.preventDefault();
            cancel();
          }
        }}
        rows={Math.min(5, Math.max(2, text.split("\n").length + 1))}
        style={{
          width: "100%",
          boxSizing: "border-box",
          border: "1px solid #ece6dd",
          borderRadius: 8,
          padding: "6px 8px",
          font: "inherit",
          fontSize: 13,
          lineHeight: 1.5,
          resize: "none",
          outline: "none",
          background: "#fafaf7",
          color: "#2b2622",
        }}
      />
      <div style={{
        display: "flex", alignItems: "center", justifyContent: "space-between",
        marginTop: 6, gap: 6,
      }}>
        <span style={{ fontSize: 10, color: "#a8a098" }}>↵ 发 · Esc 取消</span>
        <div style={{ display: "flex", gap: 6 }}>
          <button
            type="button"
            onClick={cancel}
            style={{
              border: "1px solid #ece6dd", background: "#f7f2e8", color: "#5a5249",
              padding: "3px 10px", borderRadius: 999, fontSize: 11, cursor: "pointer",
            }}
          >取消</button>
          <button
            type="button"
            onClick={sendNow}
            style={{
              border: 0, background: "#e8638c", color: "#fff",
              padding: "3px 12px", borderRadius: 999, fontSize: 11, fontWeight: 600, cursor: "pointer",
            }}
          >发送 ↵</button>
        </div>
      </div>
    </div>
  );
}

// v0.4.0 · 首次使用引导气泡 · 5 步流程
//   1 欢迎 · 2 准备网页 · 3 教 ⌘⇧Space · 4 教 fn · 5 庆祝
//   每步用户可以按按钮 advance 或 退出引导。Step 3/4 监听用户实际完成对话自动 advance。
interface TourBubbleProps { step: number; }
function TourBubble({ step }: TourBubbleProps) {
  const advance = (next: number) => invoke("tour_advance", { step: next }).catch(() => {});
  const skip = () => invoke("tour_skip").catch(() => {});

  const ChipHeader = ({ children }: { children: React.ReactNode }) => (
    <div style={{
      display: "inline-block",
      background: "#f3eee5", color: "#8a8178",
      fontSize: 11, padding: "2px 10px", borderRadius: 999,
      marginBottom: 8, letterSpacing: "0.03em",
    }}>{children}</div>
  );
  const Btn = ({ primary, onClick, children }: { primary?: boolean; onClick: () => void; children: React.ReactNode }) => (
    <button
      type="button"
      onClick={onClick}
      style={{
        font: "inherit", fontSize: 13, padding: "6px 14px",
        borderRadius: 999, cursor: "pointer",
        border: primary ? 0 : "1px solid #ece6dd",
        background: primary ? "#e8638c" : "#f0eae0",
        color: primary ? "#fff" : "#2b2622",
        fontWeight: primary ? 600 : 500,
      }}
    >{children}</button>
  );

  const wrapStyle = {
    background: "#fff", border: "2px solid #e8d8dc",
    borderRadius: 22, padding: "16px 22px",
    fontSize: 14, maxWidth: 360, lineHeight: 1.5,
    boxShadow: "0 8px 32px rgba(43,38,34,.18)",
    pointerEvents: "auto" as const,
    fontFamily: "-apple-system, 'PingFang SC', system-ui, sans-serif",
    color: "#2b2622",
  };
  const kbd = (k: string) => (
    <kbd style={{
      background: "#f0eae0", border: "1px solid #ddd", borderBottomWidth: 2,
      padding: "1px 6px", borderRadius: 4,
      fontFamily: "ui-monospace, Menlo, monospace", fontSize: 12,
    }}>{k}</kbd>
  );

  if (step === 1) {
    return (
      <div style={wrapStyle}>
        <ChipHeader>🎓 第 1 步 / 共 5 步</ChipHeader>
        <strong style={{ color: "#d63d6a" }}>下载完啦！</strong> 我能听见你说话，<br />
        帮你召唤本地 AI、做语音输入。<br />
        <span style={{ fontSize: 13, color: "#8a8178", display: "block", marginTop: 6 }}>
          花 60 秒教你怎么用？
        </span>
        <div style={{ marginTop: 12, display: "flex", gap: 10 }}>
          <Btn primary onClick={() => advance(2)}>✨ 好啊</Btn>
          <Btn onClick={skip}>下次再说</Btn>
        </div>
      </div>
    );
  }
  if (step === 2) {
    return (
      <div style={wrapStyle}>
        <ChipHeader>🎓 第 2 步 / 5 · 准备目标</ChipHeader>
        随便<strong style={{ color: "#d63d6a" }}>打开一个网页</strong>，比如<br />
        公众号文章 / 维基 / 新闻。<br />
        <span style={{ fontSize: 13, color: "#8a8178", display: "block", marginTop: 6 }}>
          打开后回来这里继续 ↓
        </span>
        <div style={{ marginTop: 12, display: "flex", gap: 10 }}>
          <Btn primary onClick={() => advance(3)}>✓ 已经打开了</Btn>
          <Btn onClick={skip}>退出引导</Btn>
        </div>
      </div>
    );
  }
  if (step === 3) {
    return (
      <div style={wrapStyle}>
        <ChipHeader>🎓 第 3 步 / 5 · 召唤 AI</ChipHeader>
        按住 {kbd("⌘")} {kbd("⇧")} {kbd("Space")} 然后说：<br />
        <em style={{
          background: "#e6f4ec", padding: "4px 10px", borderRadius: 6,
          display: "inline-block", margin: "6px 0",
        }}>"用三句话总结这个页面"</em><br />
        <span style={{ fontSize: 12, color: "#8a8178", display: "block", marginTop: 4 }}>
          松开快捷键 = 我开始干活
        </span>
        <div style={{ marginTop: 12, display: "flex", gap: 10 }}>
          <Btn primary onClick={() => advance(4)}>✓ 我学会了 → 下一步</Btn>
          <Btn onClick={skip}>退出</Btn>
        </div>
      </div>
    );
  }
  if (step === 4) {
    return (
      <div style={wrapStyle}>
        <ChipHeader>🎓 第 4 步 / 5 · 语音打字</ChipHeader>
        再教你<strong style={{ color: "#d63d6a" }}>一招</strong> —— 任何输入框里<br />
        长按 {kbd("fn")} 说话，字会打到光标位置。<br />
        <span style={{ fontSize: 12, color: "#8a8178", display: "block", marginTop: 6 }}>
          试试在 Spotlight ({kbd("⌘")}Space) 或微信里<br />
          长按 fn 说"今天天气真好"
        </span>
        <div style={{ marginTop: 12, display: "flex", gap: 10 }}>
          <Btn primary onClick={() => advance(5)}>✓ 学会了 → 完成</Btn>
          <Btn onClick={skip}>退出</Btn>
        </div>
      </div>
    );
  }
  // step >= 5 庆祝
  return (
    <div style={{ ...wrapStyle, background: "#e6f4ec", borderColor: "#b8e8c3" }}>
      <strong style={{ color: "#1a6b3a", fontSize: 16 }}>🎉 你学会了！</strong><br />
      以后任何时候按 {kbd("⌘")} {kbd("⇧")} {kbd("Space")} 就能召唤我。<br />
      <span style={{ fontSize: 12, color: "#5a5249", marginTop: 8, display: "block" }}>
        🍽 拖文件给我 = 喂我读 · 🦞 点我头开菜单
      </span>
      <span style={{ fontSize: 11, color: "#7a7167", marginTop: 6, display: "block" }}>
        （5 秒后自动消失 · 点桌宠菜单「📖 教我用」可重看）
      </span>
    </div>
  );
}
