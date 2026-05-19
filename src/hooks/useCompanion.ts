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

import { useEffect, useRef, useState } from "react";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

// ─── 调参 ───────────────────────────────────────────────
export const IDLE_TO_SLEEP_SECS = 10;
export const TYPING_PULSE_SECS = 0.4;
export const ALERT_PX = 80;
export const EXCITED_PX = 30;
export const MAX_EYE_OFFSET = 1.4;       // SVG 单位（16×16 viewBox）
/**
 * 分量化"软饱和"距离：鼠标在 X 方向越过这个值就达到 X 方向最大偏移；Y 同理。
 *
 * **关键设计**（2026-05-20 fix）：早期版本用极坐标 `cos(atan2(dy,dx))*r`
 * 把 (dx,dy) 转成方向 + 距离，结果桌宠常驻屏幕右下角时 mouse 在屏幕大部
 * 分位置 dy 远大于 dx → 角度接近 -π/2 → eyeOffsetX ≈ 0，用户感受上眼
 * 球"只会上下不会左右"。改成 tanh(dx/falloff)*MAX 的分量映射后 X 和 Y
 * 解耦，鼠标 X 方向小幅变化也能看到瞳孔水平偏移。
 */
export const EYE_FALLOFF_PX = 180;
export const CLICK_FLASH_SECS = 0.25;    // 点击后桌宠耳朵抽搐持续时间
/** "打字风暴"判定：连续这么多秒 sinceKey 都 < 0.15 → 用户大概率在狂打 / 退格挣扎 */
export const TYPING_STORM_DURATION_SECS = 1.5;
export const TYPING_STORM_PULSE_THRESH = 0.15;
/** 深夜模式时间窗（24h 制）：23:00–05:59 桌宠昏昏欲睡 */
export const DROWSY_HOUR_START = 23;
export const DROWSY_HOUR_END = 6;        // exclusive

export type CompanionState =
  | "idle" | "typing" | "alert" | "excited" | "sleep"
  | "clicked"   // 用户点鼠标了 —— 一闪而过的耳朵抽搐
  | "worried"   // 打字风暴 —— 歪头担心"卡住了？"
  | "drowsy";   // 深夜：眼皮半垂 + 动作迟缓

export interface CompanionTick {
  x: number;
  y: number;
  sinceKey: number;
  sinceMouse: number;
  sinceClick: number;
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
/**
 * @param ctx 帧间累计状态 —— hook 自管，纯函数测试时手动构造
 *
 * **注意**：从 v0.4+ bug fix（2026-05-20）开始，tick.x/y 已经是 **mouse 窗口本地坐标**
 * （Rust 端 companion.rs 已经减过窗口 outer_position）。前端**不再做坐标减法**，
 * 直接用 tick.x/y 算 dx/dy。
 */
export function deriveCompanionFrame(
  tick: CompanionTick,
  petCenter: { x: number; y: number } | null,
  ctx: { typingStormSecs: number; nowHour: number } = { typingStormSecs: 0, nowHour: 12 },
): CompanionFrame {
  const isDrowsyHour = ctx.nowHour >= DROWSY_HOUR_START || ctx.nowHour < DROWSY_HOUR_END;

  // 1) sleep 优先 —— 但深夜时间放宽到 5s（更容易入睡）
  const sleepThreshold = isDrowsyHour ? IDLE_TO_SLEEP_SECS / 2 : IDLE_TO_SLEEP_SECS;
  if (tick.sinceKey > sleepThreshold && tick.sinceMouse > sleepThreshold) {
    return { ...NEUTRAL, state: "sleep" };
  }

  if (!petCenter) {
    return { ...NEUTRAL, state: isDrowsyHour ? "drowsy" : "idle" };
  }

  // tick.x/y 已经是 mouse 窗口本地坐标（Rust 端已减过 outer_position）
  const dx = tick.x - petCenter.x;
  const dy = tick.y - petCenter.y;
  const dist = Math.hypot(dx, dy);

  // 眼球偏移 —— 分量化软饱和（tanh），X / Y 解耦
  // tanh(dx / EYE_FALLOFF_PX) ∈ (-1, 1)：dx=0 时 0，dx=±falloff 时 ±0.76，
  // 远距离逐渐饱和到 ±1。比线性 clamp 更"自然"（小幅鼠标移动也有可见反应）。
  const eyeOffsetX = Number.isFinite(dx) ? Math.tanh(dx / EYE_FALLOFF_PX) * MAX_EYE_OFFSET : 0;
  const eyeOffsetY = Number.isFinite(dy) ? Math.tanh(dy / EYE_FALLOFF_PX) * MAX_EYE_OFFSET * 0.75 : 0;

  // 2) clicked —— 刚点过鼠标（最高优先级，覆盖 typing/excited 之类，瞬时反应）
  if (tick.sinceClick < CLICK_FLASH_SECS) {
    return { state: "clicked", eyeOffsetX, eyeOffsetY, distancePx: dist };
  }

  // 3) worried —— 打字风暴（连续 ≥ TYPING_STORM_DURATION_SECS 的快速敲击）
  //    比 typing 高优先级，因为 sinceKey 也满足 typing 条件
  if (ctx.typingStormSecs >= TYPING_STORM_DURATION_SECS) {
    return { state: "worried", eyeOffsetX, eyeOffsetY, distancePx: dist };
  }

  // 4) typing
  if (tick.sinceKey < TYPING_PULSE_SECS) {
    return { state: "typing", eyeOffsetX, eyeOffsetY, distancePx: dist };
  }

  // 5) 贴近反应
  if (dist < EXCITED_PX) {
    return { state: "excited", eyeOffsetX, eyeOffsetY, distancePx: dist };
  }
  if (dist < ALERT_PX) {
    return { state: "alert", eyeOffsetX, eyeOffsetY, distancePx: dist };
  }

  // 6) 默认：深夜 → drowsy，其他 → idle
  return { state: isDrowsyHour ? "drowsy" : "idle", eyeOffsetX, eyeOffsetY, distancePx: dist };
}

/**
 * Hook —— 订阅 Rust tick + 监听桌宠 DOM 元素位置，输出当前帧。
 *
 * @param petElRef 桌宠根元素的 ref（用来算 bounding rect 中心 + screenX/Y）
 */
export function useCompanion(petElRef: { current: HTMLElement | null }): CompanionFrame {
  const [frame, setFrame] = useState<CompanionFrame>(NEUTRAL);
  // 累计"打字风暴"秒数 —— 每 tick 33ms，连续 sinceKey<阈值 就累加，否则清零
  const stormSecsRef = useRef(0);
  const lastTickAtRef = useRef<number>(performance.now());

  useEffect(() => {
    let unlisten: UnlistenFn | null = null;
    let alive = true;
    listen<CompanionTick>("companion-tick", (e) => {
      if (!alive) return;
      const now = performance.now();
      const dt = (now - lastTickAtRef.current) / 1000;
      lastTickAtRef.current = now;
      // typing storm 累计 / 清零
      if (e.payload.sinceKey < TYPING_STORM_PULSE_THRESH) {
        stormSecsRef.current = Math.min(stormSecsRef.current + dt, 10);
      } else {
        stormSecsRef.current = 0;
      }

      const el = petElRef.current;
      if (!el) {
        setFrame({ ...NEUTRAL, state: new Date().getHours() >= DROWSY_HOUR_START || new Date().getHours() < DROWSY_HOUR_END ? "drowsy" : "idle" });
        return;
      }
      const rect = el.getBoundingClientRect();
      const petCenter = { x: rect.left + rect.width / 2, y: rect.top + rect.height / 2 };
      setFrame(deriveCompanionFrame(
        e.payload, petCenter,
        { typingStormSecs: stormSecsRef.current, nowHour: new Date().getHours() },
      ));
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
