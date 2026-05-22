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
import { TourBubble } from "./components/TourBubble";
import {
  EV_VIEW_CHANGED, EV_SKIN_CHANGED, EV_NUDGE, EV_SESSION_STATE, EV_ENTRANCE,
  EV_SCHEDULE_RESULT,
  type ViewKind, type SkinId, type NudgePayload, type SessionState,
  type EntrancePhase, type EntrancePayload, type ScheduleResultPayload,
  type Schedule,
} from "./types";
import { DEFAULT_SKIN } from "./skins";
import { useT, getCurrentLang } from "./i18n";
import { scheduleLabel, scheduleIcon, formatWhen } from "./lib/schedule-format";
import { ReactiveOverlay, type ReactivePayload } from "./components/ReactiveOverlay";
import { useCompanion } from "./hooks/useCompanion";
import { useIntimacy } from "./hooks/useIntimacy";
import { useAdaptiveOverlay } from "./hooks/useAdaptiveOverlay";
import { usePetSounds } from "./hooks/usePetSounds";
import * as petAudio from "./audio/petAudio";

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
    case "schedule-confirm": return "think"; // v0.5 定时任务确认中 = 好奇思考态
    case "blocked":          return "block";
  }
}

export default function App() {
  const t = useT();
  const [view, setView] = useState<ViewKind>({ kind: "idle" });
  // session continuation chip — true when current view is part of an ongoing session
  const [continuing, setContinuing] = useState(false);
  // v0.4.x · session 状态（链条图标 / 第 N 轮 / 钉住 / 软提示）
  const [sessionState, setSessionState] = useState<SessionState>({
    continuing: false, round: 0, pinned: false, softHint: false,
  });
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
  // v0.5 · 定时任务执行完成的轻气泡 payload（不抢焦点，几秒自动消失）
  const [scheduleResult, setScheduleResult] = useState<ScheduleResultPayload | null>(null);
  // v0.4 · Reactive 桌宠：剪贴板变化时立刻反应。
  // - twitching：T1 抖耳一次，~500ms 后自动清掉
  // - reactive：T2 payload —— ribbon 在桌宠头顶弹一组按钮，5s 自动消失（或点了 action）
  const [twitching, setTwitching] = useState(false);
  const [reactive, setReactive] = useState<ReactivePayload | null>(null);
  // v0.5 · 开场调皮入场动画当前 phase（null = 没在入场）。Rust entrance.rs 推 phase + 动窗口位置；
  // 这里只控制桌宠精灵的 mc-entrance-* class。"done" 收到时清空回 idle。
  const [entrancePhase, setEntrancePhase] = useState<EntrancePhase | null>(null);
  // view 优先于 entrance：用户一旦召唤，view 变 listening/thinking…，入场精灵立刻让位
  //（防止入场动画叠在非 idle 视图上渲染 + 抢尺寸 —— finder D#7）。只有 idle 视图入场才生效。
  const entranceActive: EntrancePhase | null =
    entrancePhase !== null && view.kind === "idle" ? entrancePhase : null;
  // 横穿期间（peek/run/skid/beat）窗口被 Rust 扩到 320 + 推位置 → 桌宠用 96px + listen 表情，
  // 且必须关掉 useAdaptiveOverlay（否则自适应把横穿窗口缩掉，跟 set_position 抢尺寸）。
  // "stretch"（subtle 档）在 compact 角落原地播，保持 idle 64px。
  const bigEntrance = entranceActive === "peek" || entranceActive === "run"
                   || entranceActive === "skid" || entranceActive === "beat";
  // v0.4+ · 陪伴向动画 —— hook 订阅 Rust companion-tick + 算桌宠当前帧
  const petStageRef = useRef<HTMLDivElement>(null);
  const stageRootRef = useRef<HTMLDivElement>(null);
  // v0.4+ · 亲密度（localStorage 持久化）—— companion 互动信号 → bumpInteract
  const intimacy = useIntimacy();
  const companion = useCompanion(petStageRef, intimacy.bumpInteract);
  // v0.4+ · 点桌宠 → 浮一颗爱心（prototype 彩蛋移植）。存一组飘心的 id 队列。
  const [hearts, setHearts] = useState<number[]>([]);
  const popHeart = useCallback(() => {
    const id = Date.now() + Math.random();
    setHearts((hs) => [...hs, id]);
    window.setTimeout(() => setHearts((hs) => hs.filter((h) => h !== id)), 700);
  }, []);
  // v0.4 · 内容驱动 overlay 尺寸 —— 见 hooks/useAdaptiveOverlay.ts 注释。
  // listening（AI 召唤说话）时**禁用**：那是唯一开 cursor_follow 的状态，窗口位置
  // 归 cursor_follow 管（30fps 跟光标走）。自适应若同时按"保持锚点"反算位置 → 跟
  // cursor_follow 抢同一个窗口 → 桌宠在光标和原锚点之间来回弹（用户报"乱飘"）。
  // 其余视图（idle / tour / voice-confirm / reply / voice-ime-listening 等）窗口不
  // 跟随，自适应是唯一尺寸权威。
  // v0.5 · 入场横穿期间也禁用 —— 窗口尺寸/位置由 Rust entrance.rs 全权管，
  //   自适应若同时按内容反算会把横穿窗口缩掉、跟 set_position 抢尺寸。
  useAdaptiveOverlay(stageRootRef, { enabled: view.kind !== "listening" && !entranceActive });

  // v0.3.12 · 在 idle 状态下显示 React-only UI（下载提示气泡 / petMenu / nudge / ack
  //   / v0.4 reactive ribbon）时主动通知 Rust 把窗口 hit-box 扩到全窗口；
  //   其余时间 Rust 自动按 view.kind 处理。
  //   这是为了让 idle 静默时透明区域真的穿透到底层 app，但 React 临时弹的气泡也可点。
  useEffect(() => {
    if (view.kind !== "idle") return; // 非 idle 由 Rust emit_view 那侧管，前端不要干扰
    // v0.5 · 入场横穿期间强制 hit-box=false —— 否则那个移动的 320 窗口会拦住桌面点击
    //   （入场没有可交互 UI，桌宠只是动画；finder B#4）。
    if (entranceActive) {
      invoke("set_overlay_has_ui", { hasUi: false }).catch(() => {});
      return;
    }
    // v0.4.x · session chip（idle + 有上下文时显示在头顶）也算 React UI ——
    // 否则窗口 hit-box 只在桌宠底部，chip 在上方会被穿透掉点不到（点击查看对话失效）。
    const chipVisible = sessionState.continuing && !petMenuOpen && !nudge && !transientAck;
    const hasReactUi = !!modelProgress || petMenuOpen || !!nudge || !!transientAck || !!reactive || !!scheduleResult || chipVisible;
    invoke("set_overlay_has_ui", { hasUi: hasReactUi }).catch(() => {});
  }, [view.kind, entranceActive, modelProgress, petMenuOpen, nudge, transientAck, reactive, scheduleResult, sessionState.continuing]);

  // 启动时从 Rust 读当前皮肤（避免闪一下默认 classic 再切换）
  useEffect(() => {
    invoke<string>("get_skin")
      .then((s) => setSkin((s as SkinId) ?? DEFAULT_SKIN))
      .catch(() => { /* 浏览器 dev 模式 invoke 不可用 */ });
  }, []);

  // v0.4.x · voice-confirm 默认**不抢焦点** —— 否则目标 app 光标丢失，续写功能失效。
  // 只在用户主动点确认框 / 按 Tab（editRequested）时才让 overlay 获焦进编辑。
  // 离开 voice-confirm 一律关掉，恢复非激活面板。
  const [editRequested, setEditRequested] = useState(false);
  useEffect(() => {
    if (view.kind !== "voice-confirm") { setEditRequested(false); return; }
    if (editRequested) {
      invoke("set_overlay_focusable", { focusable: true }).catch(() => {});
      return () => { invoke("set_overlay_focusable", { focusable: false }).catch(() => {}); };
    }
  }, [view.kind, editRequested]);

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

  // v0.4 fix (2026-05-20) · 视图离开 idle → 立即清掉 ribbon 状态。
  // 否则用户点了 action 然后按 fn 切到语音输入法时，ribbon 不渲染了（view≠idle）但
  // reactive 状态还在 → reactive-result 通知逻辑误判"ribbon 还在"跳过 transientAck →
  // 用户啥反馈都看不到。清掉之后 reactiveRef 变 null，结果通过 transientAck 必达。
  useEffect(() => {
    if (view.kind !== "idle" && reactive) setReactive(null);
  }, [view.kind, reactive]);

  // v0.4+ · 撞墙回弹 —— 桌宠精灵朝那面墙挤压 + 回弹一下（纯 CSS transform，不动窗口）。
  // 两个触发源共用：① Rust emit edge-bonk{dir}（listening 跟随 / resize 被屏幕边顶住）；
  //   ② 手动拖动桌宠撞到显示器边（前端 clamp，见 handlePetPointerMove）。
  type BonkDir = "left" | "right" | "top" | "bottom";
  const [bonkDir, setBonkDir] = useState<BonkDir | null>(null);
  const bonkClearRef = useRef<number | undefined>(undefined);
  const triggerBonk = useCallback((dir: BonkDir) => {
    if (bonkClearRef.current) window.clearTimeout(bonkClearRef.current);
    // 先清空再设，强制 React 重挂 class → 同方向连撞也能重播动画
    setBonkDir(null);
    requestAnimationFrame(() => setBonkDir(dir));
    bonkClearRef.current = window.setTimeout(() => setBonkDir(null), 480);
  }, []);
  useEffect(() => {
    let unlisten: (() => void) | null = null;
    try {
      const p = listen<{ dir: BonkDir }>("edge-bonk", (e) => triggerBonk(e.payload.dir));
      p.then((fn) => { unlisten = fn; }).catch(() => {});
    } catch {/* dev mode */}
    return () => { if (unlisten) unlisten(); if (bonkClearRef.current) window.clearTimeout(bonkClearRef.current); };
  }, [triggerBonk]);

  // 音效 —— 把桌宠状态机接到 petAudio（程序化合成，按皮肤物种换嗓音）。
  // 见 hooks/usePetSounds.ts + docs/prototypes/skin-system-audio-20260521.html。
  usePetSounds({
    skin,
    mouseState: mouseStateFor(view),
    companionState: companion.state,
    continuing,
    intimacyLevel: intimacy.level,
    bonkActive: bonkDir !== null,
    nudgeActive: nudge !== null,
  });

  // v0.4 · 后台任务忙碌计数 —— 任何 reactive action 在跑时 > 0。
  // 桌宠据此显示忙碌指示（跨任何视图可见），用户永远知道"还在处理"。
  const [bgTaskCount, setBgTaskCount] = useState(0);
  useEffect(() => {
    interface BgPayload { count: number; }
    let unlisten: (() => void) | null = null;
    try {
      const p = listen<BgPayload>("bg-task-changed", (e) => {
        setBgTaskCount(e.payload.count);
      });
      p.then((fn) => { unlisten = fn; }).catch(() => {});
    } catch {/* dev mode */}
    return () => { if (unlisten) unlisten(); };
  }, []);

  // v0.4 · Reactive 处理完通知 —— ribbon 可能已被 dismiss / 超时 / 切到别的视图，
  // 后端处理完时 emit reactive-result。这里：
  //   - 如果 ribbon 还在（reactive 非 null）→ ribbon 自己显示了 done/err，不重复打扰
  //   - 如果 ribbon 已消失 → 通过 transientAck 弹一个 3s 气泡告知（成功 / 失败都告知）
  //   - 如果 view.kind 不是 idle（用户已经切到 listening / thinking 等）→ 仍然显示，
  //     用户起码知道之前那个 action 跑完了；transientAck 会出现在桌宠上方一会儿
  const reactiveRef = useRef<ReactivePayload | null>(null);
  useEffect(() => { reactiveRef.current = reactive; }, [reactive]);
  useEffect(() => {
    interface ResultPayload { action: string; ok: boolean; summary: string; }
    let unlisten: (() => void) | null = null;
    try {
      const p = listen<ResultPayload>("reactive-result", (e) => {
        // ribbon 还在 → ribbon 自己已经显示了 done/err，跳过 transient 气泡
        if (reactiveRef.current) return;
        setTransientAck(e.payload.summary);
        window.setTimeout(() => setTransientAck(null), 3000);
      });
      p.then((fn) => { unlisten = fn; }).catch(() => {});
    } catch {/* dev mode */}
    return () => { if (unlisten) unlisten(); };
  }, []);

  // v0.4.x · CLI 装好 → 桌宠头顶弹个庆祝气泡 5s（学会了 browser use / office use）
  // 设计文档：docs/prototypes/auto-install-cli-20260520.html State 7
  useEffect(() => {
    let unlisten: (() => void) | null = null;
    let dismissTimer: number | undefined;
    type InstallEv =
      | { phase: "started"; target: string }
      | { phase: "log"; target: string; line: string }
      | { phase: "done"; target: string }
      | { phase: "failed"; target: string; code: string; message: string };
    try {
      const p = listen<InstallEv>("install-progress", (e) => {
        if (e.payload.phase !== "done") return;
        const t = e.payload.target;
        const lang = getCurrentLang();
        const msg = lang === "zh"
          ? (t === "officecli" ? "🎉 我学会读写 Office 文件啦"
                                : "🎉 我学会操作浏览器啦")
          : (t === "officecli" ? "🎉 I just learned to edit Office files"
                                : "🎉 I just learned to drive a browser");
        setTransientAck(msg);
        if (dismissTimer) window.clearTimeout(dismissTimer);
        dismissTimer = window.setTimeout(() => setTransientAck(null), 5000);
      });
      p.then((fn) => { unlisten = fn; }).catch(() => {});
    } catch { /* dev mode */ }
    return () => {
      if (unlisten) unlisten();
      if (dismissTimer) window.clearTimeout(dismissTimer);
    };
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

  // v0.5 · 开场调皮入场动画 phase 监听 —— Rust entrance.rs 在横穿/张望/伸懒腰各拍 emit。
  // "done" → 清空回 idle。窗口位置 Rust 推，这里只切桌宠精灵动画。
  useEffect(() => {
    let unlisten: (() => void) | null = null;
    try {
      const p = listen<EntrancePayload>(EV_ENTRANCE, (e) => {
        const ph = e.payload.phase;
        setEntrancePhase(ph === "done" ? null : ph);
      });
      p.then((fn) => { unlisten = fn; }).catch(() => {});
    } catch { /* browser-only mode */ }
    return () => { if (unlisten) unlisten(); };
  }, []);

  // v0.5 · 上报 prefers-reduced-motion 给 Rust —— 入场动画据此决定是否跳过横穿/蹦跶。
  // mount 时报一次 + 监听变化再报。
  useEffect(() => {
    try {
      const mq = window.matchMedia("(prefers-reduced-motion: reduce)");
      const report = () => invoke("report_reduced_motion", { reduced: mq.matches }).catch(() => {});
      report();
      mq.addEventListener?.("change", report);
      return () => mq.removeEventListener?.("change", report);
    } catch { /* browser-only mode */ }
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

  // v0.5 · 定时任务执行完成 → 浮一个不抢焦点的轻气泡（几秒自动消失，结果已存任务窗）
  useEffect(() => {
    let unlisten: (() => void) | null = null;
    try {
      const p = listen<ScheduleResultPayload>(EV_SCHEDULE_RESULT, (e) => setScheduleResult(e.payload));
      p.then((fn) => { unlisten = fn; }).catch(() => {});
    } catch { /* browser-only mode */ }
    return () => { if (unlisten) unlisten(); };
  }, []);

  // v0.4.x · session 状态监听 —— 驱动链条图标 + 第 N 轮 + 钉住 + 软提示
  useEffect(() => {
    let unlisten: (() => void) | null = null;
    try {
      const p = listen<SessionState>(EV_SESSION_STATE, (e) => {
        setSessionState(e.payload);
        setContinuing(e.payload.continuing);
      });
      p.then((fn) => { unlisten = fn; }).catch(() => {});
      // 初次挂载读一次当前状态
      invoke<SessionState>("get_session_state")
        .then((s) => { setSessionState(s); setContinuing(s.continuing); })
        .catch(() => {});
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
        } else if (e.key === "Tab") {
          // Tab → 主动进编辑（让 overlay 获焦 + 暂停倒数）
          e.preventDefault();
          setEditRequested(true);
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
    // v0.4+ · 拖动撞墙回弹用：显示器可放窗口范围（逻辑 px）。窗口尺寸 + 桌宠 inset
    // 在 move 里实时测（拖动期间窗口会被扩到 320，桌宠只占中间一小块）。
    monX: number; monY: number; monW: number; monH: number;
  } | null>(null);
  const draggingRef = useRef(false);
  const draggedRef = useRef(false);
  // v0.4+ · 拖动时当前正顶着哪面墙 —— 防贴墙连发，只在"首次接触"弹一次。
  const dragWallRef = useRef<BonkDir | null>(null);
  const DRAG_THRESHOLD = 5;

  const handlePetPointerDown = useCallback(async (e: React.PointerEvent) => {
    // v0.3.11 · 拖动不再 gate 在 idle —— 回复 / thinking / 录音 状态下用户也常需要把桌宠
    // 挪开（比如挡住下面要看的内容）。只挡住非左键。
    if (e.button !== 0) return;
    // v0.3.12 · 拖动期间锁定 has_ui=true（全窗口接收事件）。否则 pet_passthrough 在光标
    // 移出 hit-box 时会切到穿透 → 拖动断裂、桌宠定在半路不动。
    invoke("set_overlay_has_ui", { hasUi: true }).catch(() => {});
    try {
      const { getCurrentWindow, currentMonitor } = await import("@tauri-apps/api/window");
      const win = getCurrentWindow();
      const pos = await win.outerPosition();
      const scale = await win.scaleFactor();
      // v0.4+ · 抓当前显示器范围（物理 → 逻辑），拖动撞边时 clamp 用。
      const mon = await currentMonitor().catch(() => null);
      const msf = mon?.scaleFactor ?? scale;
      dragStartRef.current = {
        pointerX: e.screenX,
        pointerY: e.screenY,
        winX: pos.x / scale,
        winY: pos.y / scale,
        scale,
        monX: mon ? mon.position.x / msf : 0,
        monY: mon ? mon.position.y / msf : 0,
        monW: mon ? mon.size.width / msf : Number.POSITIVE_INFINITY,
        monH: mon ? mon.size.height / msf : Number.POSITIVE_INFINITY,
      };
      draggingRef.current = false;
      draggedRef.current = false;
      dragWallRef.current = null;
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
    // dragging 态：移动窗口跟着鼠标走 —— clamp **桌宠本体**留在显示器内（窗口四周
    // 透明边距可越界出屏），撞边回弹。窗口拖动时被扩到 320，桌宠只占中间一小块，
    // 所以必须按桌宠实际渲染 rect 算 inset，而不是按整窗 —— 否则桌宠离屏幕边还差
    // 一大截就被挡住（用户反馈）。
    try {
      const { getCurrentWindow, LogicalPosition } = await import("@tauri-apps/api/window");
      const wantX = start.winX + dx;
      const wantY = start.winY + dy;
      // 窗口逻辑尺寸 + 桌宠在窗口内的 inset（CSS px = 逻辑 px），实时测。
      const winW = window.innerWidth;
      const winH = window.innerHeight;
      let insetL = 0, insetR = 0, insetT = 0, insetB = 0;
      const petEl = petStageRef.current;
      if (petEl) {
        const r = petEl.getBoundingClientRect();
        insetL = r.left; insetR = winW - r.right;
        insetT = r.top;  insetB = winH - r.bottom;
      }
      // 桌宠本体留在屏内 → 允许窗口越界出屏（透明边距）。
      const loX = start.monX - insetL;
      const hiX = Math.max(loX, start.monX + start.monW - winW + insetR);
      const loY = start.monY - insetT;
      const hiY = Math.max(loY, start.monY + start.monH - winH + insetB);
      const cx = Math.min(hiX, Math.max(loX, wantX));
      const cy = Math.min(hiY, Math.max(loY, wantY));
      // 撞了哪面墙（被往里推 = 撞那面）
      let wall: BonkDir | null = null;
      if (cx > wantX + 0.5) wall = "left";
      else if (cx < wantX - 0.5) wall = "right";
      else if (cy > wantY + 0.5) wall = "top";
      else if (cy < wantY - 0.5) wall = "bottom";
      // 只在"首次接触"弹一次；离开墙后再撞才会重弹
      if (wall && wall !== dragWallRef.current) triggerBonk(wall);
      dragWallRef.current = wall;
      await getCurrentWindow().setPosition(new LogicalPosition(cx, cy));
    } catch (err) {
      console.debug("setPosition:", err);
    }
  }, [triggerBonk]);

  const handlePetPointerUp = useCallback(async (e: React.PointerEvent) => {
    if (!dragStartRef.current) return;
    (e.target as Element).releasePointerCapture?.(e.pointerId);
    const wasDragged = draggingRef.current;
    dragStartRef.current = null;
    draggingRef.current = false;
    dragWallRef.current = null;
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
    // v0.4+ · 点桌宠 → 浮爱心 + 记一次亲密互动（菜单仍照常开合）
    popHeart();
    intimacy.bumpInteract();
    petAudio.playEvent("squeak", skin); // 直接点桌宠才吱（不跟桌面点击 companion=clicked 混）
    setPetMenuOpen(prev => !prev);
  }, [view.kind, popHeart, intimacy, skin]);

  return (
    <div ref={stageRootRef} className="stage stage-mouse-bubble">
      <BubbleFor view={view} continuing={continuing} onExpand={handleExpand} onNewSession={handleNewSession} modelProgress={entrancePhase ? null : modelProgress} editRequested={editRequested} onRequestEdit={() => setEditRequested(true)} />
      {/* Local ack bubble — visible above the pet without going through Rust */}
      {transientAck && (
        <div className="stage-bubble">
          <Bubble text={transientAck} variant="success" />
        </div>
      )}
      {/* v0.4.x · session 状态 chip —— idle 静默时浮在桌宠头顶，让"连续/钉住/软提示"可见 */}
      {view.kind === "idle" && !entrancePhase && !transientAck && !petMenuOpen && !nudge && sessionState.continuing && (
        <div className="stage-bubble">
          <SessionChip s={sessionState} />
        </div>
      )}
      <div
        ref={petStageRef}
        className="stage-mouse"
        onClick={handleMouseClick}
        onPointerDown={handlePetPointerDown}
        onPointerMove={handlePetPointerMove}
        onPointerUp={handlePetPointerUp}
        onPointerCancel={handlePetPointerUp}
        style={{ cursor: "grab" }}
      >
        <PixelMouse
          state={bigEntrance ? "listen" : mouseStateFor(view)} skin={skin}
          size={bigEntrance ? 96 : (view.kind === "idle" ? 64 : 96)}
          continuing={continuing}
          twitching={twitching}
          companionState={entranceActive ? undefined : companion.state}
          eyeOffset={entranceActive ? { x: 0, y: 0 } : { x: companion.eyeOffsetX, y: companion.eyeOffsetY }}
          intimacyLevel={intimacy.level}
          neglected={intimacy.neglected}
          bonk={bonkDir}
          entrance={entranceActive}
        />
        {hearts.map((id) => (
          <span key={id} className="pet-heart" aria-hidden>❤️</span>
        ))}
        {/* v0.4 · 后台任务忙碌指示 —— 任何视图可见，让用户知道"还在处理"。
            三点跳跃 badge 贴在桌宠右下，不抢戏但能看到。 */}
        {bgTaskCount > 0 && (
          <span className="pet-busy-badge" aria-label="处理中" title="处理中…">
            <span className="pet-busy-dot" />
            <span className="pet-busy-dot" />
            <span className="pet-busy-dot" />
          </span>
        )}
        <PetMenu
          open={petMenuOpen}
          onClose={() => setPetMenuOpen(false)}
          onFeed={handleFeed}
          onNap={handleNap}
        />
        {nudge && !petMenuOpen && (
          <NudgeBubble payload={nudge} onDismiss={() => setNudge(null)} />
        )}
        {scheduleResult && !petMenuOpen && !nudge && (
          <ScheduleResultBubble payload={scheduleResult} onDismiss={() => setScheduleResult(null)} />
        )}
        {/* v0.4 · Reactive ribbon —— idle 视图下浮在桌宠头顶。其他视图（listening / thinking
            / panel 等）有自己的气泡，让位 */}
        {view.kind === "idle" && !entrancePhase && !petMenuOpen && !nudge && (
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

/** v0.4.x · 桌宠头顶的 session 状态 chip —— 钉住 / 软提示 / 第 N 轮，三选一显示。
 *  点击 → 打开 Panel 看当前 session 完整对话。 */
function SessionChip({ s }: { s: SessionState }) {
  const zh = getCurrentLang().startsWith("zh");
  let text: string;
  let cls = "session-chip session-chip-clickable";
  if (s.pinned) {
    cls += " session-chip-pin";
    const label = s.pinnedLabel ? ` ${s.pinnedLabel}` : "";
    text = zh ? `📌 钉住${label} · 第 ${s.round} 轮` : `📌 Pinned${label} · turn ${s.round}`;
  } else if (s.softHint) {
    cls += " session-chip-soft";
    text = zh ? "🔗 接着很久前的 · 点看 / 双击重置" : "🔗 Old chat · click to view / dbl-tap reset";
  } else {
    cls += " session-chip-chain";
    text = zh ? `🔗 第 ${s.round} 轮 · 点看对话` : `🔗 turn ${s.round} · click to view`;
  }
  const open = () => { invoke("open_session_panel").catch(() => {}); };
  return (
    <div
      className={cls}
      role="button"
      tabIndex={0}
      title={zh ? "点击查看完整对话" : "Click to view full conversation"}
      onClick={open}
      onKeyDown={(e) => { if (e.key === "Enter" || e.key === " ") { e.preventDefault(); open(); } }}
    >{text}</div>
  );
}

interface BubbleForProps {
  view: ViewKind; continuing: boolean;
  onExpand: () => void; onNewSession: () => void;
  modelProgress: { pct: number; mb_done: number; mb_total: number; phase: string } | null;
  editRequested: boolean; onRequestEdit: () => void;
}
function BubbleFor({ view, continuing, onExpand, onNewSession, modelProgress, editRequested, onRequestEdit }: BubbleForProps) {
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
      // 慢 —— Claude 调用要 10-30s，给个动态 loading 让用户知道在干活。
      // v0.4.x · 有实时活动（status：💭 思考 / 🔧 工具）就显示它，让用户看到"在动"
      // 不像卡死；没活动则显示用户问题本身。
      return <Bubble text={view.status || view.transcript} loading />;
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
      // v0.4.0 · 转写完 3 秒倒数确认。默认不抢焦点；点气泡 / Tab = 进入编辑模式。
      return <VoiceConfirmBubble transcript={view.transcript} remaining={view.remaining}
               editRequested={editRequested} onRequestEdit={onRequestEdit} />;
    case "tour-step":
      // v0.4.0 · 首次使用引导 5 步流程
      return <TourBubble step={view.step} />;
    case "schedule-confirm":
      // v0.5 · 定时任务确认卡 —— backend 解析出 [SCHEDULE] 后摊开给用户确认
      return (
        <ScheduleConfirmBubble
          title={view.title}
          action={view.action}
          schedule={view.schedule}
          nextRun={view.nextRun}
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


// v0.4.0 → v0.3.11 重构 · 语音转写确认气泡
//   单一形态：转写文本一上来就装进可编辑 textarea，用户直接 ↵ 发 / Esc 取消 / 改字。
//   倒数 N 秒到了自动发送（Rust 端管，前端只显示数字）。
//   之前"先气泡 → 点一下进编辑"两段式被用户反馈"修改框很乱" → 砍掉。
interface VoiceConfirmBubbleProps {
  transcript: string; remaining: number;
  editRequested: boolean; onRequestEdit: () => void;
}
function VoiceConfirmBubble({ transcript, remaining, editRequested, onRequestEdit }: VoiceConfirmBubbleProps) {
  const [text, setText] = useState(transcript);
  const dirtyRef = useRef(false); // 用户改过 → 不再被 Rust 周期 emit 覆盖
  const taRef = useRef<HTMLTextAreaElement>(null);
  // remaining === 99 是 Rust 端 HOLD 状态的哨兵 —— 显示"编辑中"，不再倒数
  const holding = remaining >= 99;

  useEffect(() => {
    if (!dirtyRef.current) setText(transcript);
  }, [transcript]);

  // v0.4.x · 只有用户主动进编辑（editRequested）后才 focus —— 默认不抢焦点，
  // 保住目标 app 光标让续写能插入。editRequested 翻 true 时 overlay 已被设为可获焦，
  // 这里再 focus textarea + 光标移末尾。同时通知 Rust 暂停倒数（HOLD）。
  useEffect(() => {
    if (!editRequested) return;
    const el = taRef.current;
    if (!el) return;
    el.focus();
    el.setSelectionRange(el.value.length, el.value.length);
    if (!dirtyRef.current) {
      dirtyRef.current = true;
      invoke("voice_confirm_hold", { text: el.value }).catch(() => {});
    }
  }, [editRequested]);

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
      // v0.4.x fix · 必须打 data-adaptive-measure —— useAdaptiveOverlay 只测带 .bubble/
      // .stage-bubble 等 class 的元素，这个确认框是自定义 inline-style div，不打标记
      // 的话自适应会把窗口缩到只剩桌宠，282 宽的确认框被裁出窗口 = 用户看不到确认/编辑。
      data-adaptive-measure=""
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
        <span style={{ whiteSpace: "nowrap" }}>{editRequested ? "🎙 听到（编辑中）" : "🎙 听到"}</span>
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
        // v0.4.x · 默认 readOnly（不抢焦点，保住目标 app 光标让续写能插入）；
        // 点一下 / Tab 才进编辑（onRequestEdit → overlay 获焦）。
        readOnly={!editRequested}
        onClick={() => { if (!editRequested) onRequestEdit(); }}
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
          background: editRequested ? "#fff" : "#fafaf7",
          color: "#2b2622",
          cursor: editRequested ? "text" : "pointer",
        }}
      />
      <div style={{
        display: "flex", alignItems: "center", justifyContent: "space-between",
        marginTop: 6, gap: 6,
      }}>
        <span style={{ fontSize: 10, color: "#a8a098" }}>
          {editRequested ? "↵ 发 · Esc 取消" : "点这 / Tab 改 · ↵ 发 · Esc 取消"}
        </span>
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

// v0.5 · 定时任务确认卡 —— backend 解析出 [SCHEDULE] 后摊开「我理解成什么」让用户确认。
// ✓ 就这么定 → 建任务 + 收起；改一下 → 建任务 + 打开任务窗去 ✎ 调整；Esc/点外 → 取消（不建）。
interface ScheduleConfirmProps {
  title: string;
  action: string;
  schedule: Schedule;
  nextRun?: string;
}
function ScheduleConfirmBubble({ title, action, schedule, nextRun }: ScheduleConfirmProps) {
  const t = useT();
  const [saved, setSaved] = useState(false);

  const create = async () => {
    await invoke("create_schedule", { input: { title, action, schedule } }).catch(() => {});
  };
  const confirm = async () => {
    setSaved(true);
    await create();
    window.setTimeout(() => invoke("dismiss").catch(() => {}), 1100);
  };
  const tweak = async () => {
    await create();
    invoke("open_tasks_window").catch(() => {});
    invoke("dismiss").catch(() => {});
  };

  const card: React.CSSProperties = {
    background: "var(--bubble-bg)",
    border: "1px solid var(--bubble-border)",
    borderRadius: "var(--radius-xl)",
    padding: "14px 14px 10px",
    width: 290,
    boxSizing: "border-box",
    boxShadow: "var(--shadow-bubble)",
    pointerEvents: "auto",
    fontFamily: "var(--font-system)",
    color: "var(--text-primary)",
  };
  const chip: React.CSSProperties = {
    display: "inline-flex", alignItems: "center", gap: 4,
    fontSize: "var(--text-meta)", fontWeight: 600, color: "var(--text-link)",
    background: "var(--accent-glow)", border: "1px solid rgba(255,107,157,0.28)",
    padding: "3px 9px", borderRadius: "var(--radius-pill)",
  };

  if (saved) {
    return (
      <div className="stage-bubble">
        <div data-adaptive-measure="" style={{ ...card, width: "auto" }}>
          <div style={{ fontSize: "var(--text-body)", fontWeight: 600 }}>
            {t("schedule.confirm.created")}
          </div>
        </div>
      </div>
    );
  }

  return (
    <div className="stage-bubble">
      <div data-adaptive-measure="" style={card}>
        <div style={{ display: "flex", alignItems: "center", gap: 8, marginBottom: 8, flexWrap: "wrap" }}>
          <span style={{
            fontSize: "var(--text-micro)", fontWeight: 700, letterSpacing: "0.06em",
            color: "var(--text-link)", background: "var(--accent-glow)",
            padding: "2px 8px", borderRadius: "var(--radius-pill)",
          }}>{t("schedule.confirm.title")}</span>
          <span style={chip}>{scheduleIcon(schedule)} {scheduleLabel(t, schedule)}</span>
        </div>
        <div style={{ fontSize: "var(--text-body)", marginBottom: 8 }}>
          <span style={{ color: "var(--text-bubble-dim)" }}>{t("schedule.confirm.what")}</span>
          <br />
          {action}
        </div>
        <div style={{ fontSize: "var(--text-meta)", color: "var(--text-bubble-dim)", marginBottom: 10 }}>
          {nextRun && <span>{t("tasks.next", { when: formatWhen(t, nextRun) })} · </span>}
          {t("schedule.confirm.delivery")}
        </div>
        <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
          <button onClick={confirm} style={{
            border: "none", borderRadius: "var(--radius-sm)", padding: "7px 14px",
            fontSize: "var(--text-body-sm)", fontWeight: 700, cursor: "pointer",
            background: "var(--accent-gradient)", color: "#fff", boxShadow: "var(--shadow-cta)",
          }}>{t("schedule.confirm.ok")}</button>
          <button onClick={tweak} style={{
            border: "none", borderRadius: "var(--radius-sm)", padding: "7px 14px",
            fontSize: "var(--text-body-sm)", fontWeight: 700, cursor: "pointer",
            background: "rgba(0,0,0,0.05)", color: "var(--text-primary)",
          }}>{t("schedule.confirm.edit")}</button>
          <span style={{ marginLeft: "auto", fontSize: "var(--text-micro)", color: "var(--text-bubble-dim)" }}>
            {t("schedule.confirm.esc")}
          </span>
        </div>
      </div>
    </div>
  );
}

// v0.5 · 定时任务结果轻气泡 —— 不抢焦点、几秒自动消失，「展开」打开任务窗看全文。
// taskId 为空 = 一次性"发现提示"。
function ScheduleResultBubble({ payload, onDismiss }: { payload: ScheduleResultPayload; onDismiss: () => void }) {
  const t = useT();
  const isHint = payload.taskId === "";
  useEffect(() => {
    const id = window.setTimeout(onDismiss, isHint ? 12_000 : 7_000);
    return () => window.clearTimeout(id);
  }, [payload, onDismiss, isHint]);
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => { if (e.key === "Escape") onDismiss(); };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [onDismiss]);

  const expand = () => {
    invoke("open_tasks_window").catch(() => {});
    onDismiss();
  };

  return (
    <div
      className="schedule-result-bubble"
      data-adaptive-measure=""
      role="status"
      aria-live="polite"
      onClick={(e) => e.stopPropagation()}
      style={{
        position: "absolute", left: "50%", bottom: "100%", transform: "translateX(-50%)",
        marginBottom: 10, width: 260, boxSizing: "border-box",
        background: "var(--bubble-success-bg)", border: "1px solid var(--bubble-border)",
        borderRadius: "var(--radius-xl)", padding: "12px 14px 10px",
        boxShadow: "var(--shadow-bubble)", pointerEvents: "auto",
        fontFamily: "var(--font-system)", color: "var(--text-primary)",
      }}
    >
      <div style={{
        fontSize: "var(--text-micro)", fontWeight: 700, letterSpacing: "0.06em",
        color: "var(--text-link)", background: "var(--accent-glow)",
        display: "inline-block", padding: "2px 8px", borderRadius: "var(--radius-pill)", marginBottom: 6,
      }}>{isHint ? t("schedule.hint.tag") : t("schedule.result.tag")}{!isHint && payload.title ? ` · ${payload.title}` : ""}</div>
      <div style={{ fontSize: "var(--text-body)", lineHeight: 1.5 }}>{payload.summary}</div>
      <div onClick={expand} style={{
        marginTop: 8, fontSize: "var(--text-meta)", fontWeight: 600,
        color: "var(--text-link)", cursor: "pointer",
      }}>{t("schedule.result.expand")}</div>
    </div>
  );
}

