/**
 * v0.4+ · 桌宠陪伴向动画 hook
 *
 * 设计原型：docs/prototypes/companion-animations-20260519.html
 * Rust 端：src-tauri/src/companion.rs（30 FPS 推 `companion-tick`）
 *
 * 订阅 `companion-tick` 事件 → 算桌宠 webview 中心到桌面光标的距离/方向 →
 * 输出 `{ companionState, eyeOffset }` 给 PixelMouse 驱动眼球追鼠标 +
 * 状态切换（idle / typing / alert / excited / sleep）。
 *
 * ## 状态判定（与 prototype state machine 一致）
 * - sleep    : sinceKey > IDLE_TO_SLEEP_SECS && sinceMouse > IDLE_TO_SLEEP_SECS
 * - typing   : sinceKey < TYPING_PULSE_SECS（最高优先，覆盖 idle）
 * - excited  : 光标距离 < EXCITED_PX
 * - alert    : 光标距离 < ALERT_PX
 * - idle     : 否则
 *
 * ## 眼球偏移
 * 单位：SVG 像素（16×16 viewBox 下），最大 ±0.6px。
 * 由 hook 算出方向 + 归一化幅度，PixelMouse 用 transform: translate 平移整个
 * `<g.mc-eyes>` —— 所有 9 款皮肤的眼睛 rect 一起平移，自动适配。
 */

import { useEffect, useState } from "react";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

// ─── 调参 ───────────────────────────────────────────────
export const IDLE_TO_SLEEP_SECS = 10;
export const TYPING_PULSE_SECS = 0.4;
export const ALERT_PX = 80;
export const EXCITED_PX = 30;
export const MAX_EYE_OFFSET = 0.6;       // SVG 单位（16×16 viewBox）
export const EYE_FALLOFF_PX = 200;       // 鼠标 200px 外瞳孔已偏到极限

export type CompanionState = "idle" | "typing" | "alert" | "excited" | "sleep";

export interface CompanionTick {
  x: number;
  y: number;
  sinceKey: number;
  sinceMouse: number;
}

export interface CompanionFrame {
  state: CompanionState;
  /** SVG-unit 眼球偏移（要 translate 到 .mc-eyes 上） */
  eyeOffsetX: number;
  eyeOffsetY: number;
  /** webview-local 鼠标距桌宠中心距离（px），调试用 */
  distancePx: number;
}

const NEUTRAL: CompanionFrame = {
  state: "idle",
  eyeOffsetX: 0,
  eyeOffsetY: 0,
  distancePx: Infinity,
};

/**
 * 纯函数 —— state machine 的核心，给单元测试用。
 *
 * @param tick      Rust 推过来的最新 tick
 * @param petCenter 桌宠在 webview 内的中心坐标（getBoundingClientRect 算）
 * @param windowScreenXY 当前 webview 窗口在桌面上的位置（window.screenX/Y）
 */
export function deriveCompanionFrame(
  tick: CompanionTick,
  petCenter: { x: number; y: number } | null,
  windowScreenXY: { x: number; y: number },
): CompanionFrame {
  // 1) sleep 优先（覆盖一切）
  if (tick.sinceKey > IDLE_TO_SLEEP_SECS && tick.sinceMouse > IDLE_TO_SLEEP_SECS) {
    return { ...NEUTRAL, state: "sleep" };
  }

  if (!petCenter) return NEUTRAL;

  // 桌面坐标 → webview-local
  const localCursorX = tick.x - windowScreenXY.x;
  const localCursorY = tick.y - windowScreenXY.y;
  const dx = localCursorX - petCenter.x;
  const dy = localCursorY - petCenter.y;
  const dist = Math.hypot(dx, dy);

  // 眼球偏移：归一化方向 × min(1, dist/200) × MAX_EYE_OFFSET
  let eyeOffsetX = 0, eyeOffsetY = 0;
  if (dist > 0.5 && Number.isFinite(dist)) {
    const norm = Math.min(1, dist / EYE_FALLOFF_PX);
    const ang = Math.atan2(dy, dx);
    eyeOffsetX = Math.cos(ang) * norm * MAX_EYE_OFFSET;
    eyeOffsetY = Math.sin(ang) * norm * MAX_EYE_OFFSET * 0.85; // y 稍收紧（眼眶矮）
  }

  // 2) typing — typing pulse 内点头节奏（但仍允许眼球追鼠标）
  if (tick.sinceKey < TYPING_PULSE_SECS) {
    return { state: "typing", eyeOffsetX, eyeOffsetY, distancePx: dist };
  }

  // 3) 贴近反应
  if (dist < EXCITED_PX) {
    return { state: "excited", eyeOffsetX, eyeOffsetY, distancePx: dist };
  }
  if (dist < ALERT_PX) {
    return { state: "alert", eyeOffsetX, eyeOffsetY, distancePx: dist };
  }

  return { state: "idle", eyeOffsetX, eyeOffsetY, distancePx: dist };
}

/**
 * Hook —— 订阅 Rust tick + 监听桌宠 DOM 元素位置，输出当前帧。
 *
 * @param petElRef 桌宠根元素的 ref（用来算 bounding rect 中心 + screenX/Y）
 */
export function useCompanion(petElRef: { current: HTMLElement | null }): CompanionFrame {
  const [frame, setFrame] = useState<CompanionFrame>(NEUTRAL);

  useEffect(() => {
    let unlisten: UnlistenFn | null = null;
    let alive = true;
    listen<CompanionTick>("companion-tick", (e) => {
      if (!alive) return;
      const el = petElRef.current;
      if (!el) {
        setFrame(NEUTRAL);
        return;
      }
      const rect = el.getBoundingClientRect();
      const petCenter = { x: rect.left + rect.width / 2, y: rect.top + rect.height / 2 };
      const screen = { x: window.screenX ?? 0, y: window.screenY ?? 0 };
      setFrame(deriveCompanionFrame(e.payload, petCenter, screen));
    }).then((u) => {
      if (alive) unlisten = u; else u();
    });
    return () => {
      alive = false;
      if (unlisten) unlisten();
    };
  }, [petElRef]);

  return frame;
}
