/**
 * v0.4+ · 桌宠陪伴向动画 hook
 *
 * 设计原型：docs/prototypes/companion-animations-20260519.html
 * Rust 端：src-tauri/src/companion.rs（30 FPS 推 `companion-tick`）
 *
 * 订阅 `companion-tick` 事件 → 算桌宠 webview 中心到桌面光标的距离/方向 →
 * 输出 `{ companionState, eyeOffset }` 给 PixelMouse 驱动眼球追鼠标 + 状态切换。
 *
 * ## 状态全集（优先级从高到低）
 * - sleep    : 键鼠都超 idle 阈值 → 闭眼睡（深夜阈值减半）
 * - waking   : 刚从 sleep/drowsy 醒来 → 一次性伸懒腰（~700ms 过渡）
 * - dizzy    : 鼠标快速大幅甩动 → 转圈眼 ×_×（~800ms hold）
 * - hop      : 一段连续打字 burst 突然结束 → 小跳一下（"提交完成"近似）
 * - clicked  : 刚点鼠标 → 耳朵抽 + 微缩
 * - worried  : 打字风暴（连续狂敲/退格）→ 歪头担心
 * - typing   : 正在打字 → 节奏点头
 * - excited  : 光标 < EXCITED_PX → 兴奋上抬
 * - alert    : 光标 < ALERT_PX → 抬头看
 * - glance   : 过整点偶尔抬头看一眼（仅在本来 idle 时）
 * - drowsy   : 深夜默认态（眼皮半垂）
 * - idle     : 白天默认态
 *
 * ## 眼球偏移
 * tanh(d/falloff) 分量映射，X/Y 解耦，最大 ±MAX_EYE_OFFSET SVG 单位。
 * PixelMouse 用 SVG transform 平移整个 `<g.mc-eyes>` —— 9 款皮肤自动适配。
 */

import { useEffect, useRef, useState } from "react";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

// ─── 调参 ───────────────────────────────────────────────
export const IDLE_TO_SLEEP_SECS = 10;
export const TYPING_PULSE_SECS = 0.4;
export const ALERT_PX = 80;
export const EXCITED_PX = 30;
export const MAX_EYE_OFFSET = 1.4;       // SVG 单位（16×16 viewBox）
// tanh 软饱和距离（X/Y 各自）。2026-05-23 从 180→600：桌宠常蹲屏幕角落，光标通常
// 离它 >400px，180 时 tanh 一远就饱和(±1)钉死 → 用户感知"眼睛经常不追"。600 让眼睛
// 在整屏范围按比例追（playground 实测拍板，见 docs/prototypes/pet-naming-and-eye-tracking-20260523.html）。
export const EYE_FALLOFF_PX = 600;
export const CLICK_FLASH_SECS = 0.25;    // 点击后耳朵抽搐持续
export const TYPING_STORM_DURATION_SECS = 1.5;
export const TYPING_STORM_PULSE_THRESH = 0.15;
export const DROWSY_HOUR_START = 23;
export const DROWSY_HOUR_END = 6;        // exclusive
// v0.4+ 全集补完（2026-05-20）
export const WAKING_MS = 700;            // 醒来伸懒腰过渡时长
// dizzy 用"能量累积 + 冷却"判定（2026-05-20 修过敏 bug）：
//   - 单帧高速不算，必须**持续使劲甩** ≥ DIZZY_ENERGY_TRIGGER_SECS 才触发
//   - 触发后进冷却，避免一直晃 + 眼睛跟随被反复打断
export const DIZZY_SPEED_PX_S = 4200;    // "甩动"速度门槛（正常移鼠标到目标点不会持续超）
export const DIZZY_ENERGY_TRIGGER_SECS = 0.45; // 需累积这么久的持续高速才晕
export const DIZZY_ENERGY_DECAY = 2.0;   // 不再高速时能量衰减倍率（快速回落，防余量误触）
export const DIZZY_COOLDOWN_MS = 1600;   // 晕完冷却，期间不再触发
export const DIZZY_MAX_DT = 0.1;         // 单帧 dt 超这个视为节流跳变，跳过速度计算
export const DIZZY_HOLD_MS = 700;        // 晕眩 hold 时长（让转圈动画跑完）
export const HOP_MS = 480;               // burst 结束小跳时长
export const HOP_MIN_BURST_SECS = 1.0;   // 至少连打 1s 才算一个 burst（结束才跳）
export const GLANCE_MS = 2000;           // 整点抬头看一眼时长
export const GLANCE_HOURLY_PROB = 0.4;   // 每过整点 40% 概率触发（→ 每天 2-3 次）

export type CompanionState =
  | "idle" | "typing" | "alert" | "excited" | "sleep"
  | "clicked"   // 点鼠标 → 耳朵抽
  | "worried"   // 打字风暴 → 歪头担心
  | "drowsy"    // 深夜眼皮半垂
  | "waking"    // 醒来伸懒腰
  | "dizzy"     // 鼠标甩动晕眩
  | "hop"       // burst 结束小跳
  | "glance";   // 整点抬头

export interface CompanionTick {
  x: number;
  y: number;
  sinceKey: number;
  sinceMouse: number;
  sinceClick: number;
}

export interface CompanionFrame {
  state: CompanionState;
  eyeOffsetX: number;
  eyeOffsetY: number;
  distancePx: number;
}

/** hook 自管的帧间瞬时态 —— 纯函数测试时手动构造。
 *  waking/dizzy/hop/glance 可选（缺省 false）——调用点只需填关心的字段。 */
export interface CompanionCtx {
  typingStormSecs: number;
  nowHour: number;
  waking?: boolean;
  dizzy?: boolean;
  hop?: boolean;
  glance?: boolean;
}

const DEFAULT_CTX: CompanionCtx = {
  typingStormSecs: 0, nowHour: 12,
  waking: false, dizzy: false, hop: false, glance: false,
};

const NEUTRAL: CompanionFrame = {
  state: "idle", eyeOffsetX: 0, eyeOffsetY: 0, distancePx: Infinity,
};

/**
 * 纯函数 —— state machine 核心，给单元测试用。
 *
 * tick.x/y 已是 mouse 窗口本地坐标（Rust companion.rs 已减过 outer_position），
 * 前端不再做坐标减法。
 */
export function deriveCompanionFrame(
  tick: CompanionTick,
  petCenter: { x: number; y: number } | null,
  ctx: CompanionCtx = DEFAULT_CTX,
): CompanionFrame {
  const isDrowsyHour = ctx.nowHour >= DROWSY_HOUR_START || ctx.nowHour < DROWSY_HOUR_END;

  // 1) sleep 优先（深夜阈值减半）
  const sleepThreshold = isDrowsyHour ? IDLE_TO_SLEEP_SECS / 2 : IDLE_TO_SLEEP_SECS;
  if (tick.sinceKey > sleepThreshold && tick.sinceMouse > sleepThreshold) {
    return { ...NEUTRAL, state: "sleep" };
  }

  if (!petCenter) {
    return { ...NEUTRAL, state: isDrowsyHour ? "drowsy" : "idle" };
  }

  const dx = tick.x - petCenter.x;
  const dy = tick.y - petCenter.y;
  const dist = Math.hypot(dx, dy);

  // 眼球偏移：tanh 分量软饱和，X/Y 解耦
  const eyeOffsetX = Number.isFinite(dx) ? Math.tanh(dx / EYE_FALLOFF_PX) * MAX_EYE_OFFSET : 0;
  const eyeOffsetY = Number.isFinite(dy) ? Math.tanh(dy / EYE_FALLOFF_PX) * MAX_EYE_OFFSET * 0.75 : 0;

  // 2) waking —— 刚醒（hook 在 sleep→awake 跳变时点亮 ~700ms）
  if (ctx.waking) {
    return { state: "waking", eyeOffsetX, eyeOffsetY, distancePx: dist };
  }

  // 3) dizzy —— 鼠标快速甩动（眼睛转圈，不追鼠标）
  if (ctx.dizzy) {
    return { state: "dizzy", eyeOffsetX: 0, eyeOffsetY: 0, distancePx: dist };
  }

  // 4) hop —— 一段 burst 打字突然结束（"提交完成"近似，小跳）
  if (ctx.hop) {
    return { state: "hop", eyeOffsetX, eyeOffsetY, distancePx: dist };
  }

  // 5) clicked —— 刚点鼠标
  if (tick.sinceClick < CLICK_FLASH_SECS) {
    return { state: "clicked", eyeOffsetX, eyeOffsetY, distancePx: dist };
  }

  // 6) worried —— 打字风暴
  if (ctx.typingStormSecs >= TYPING_STORM_DURATION_SECS) {
    return { state: "worried", eyeOffsetX, eyeOffsetY, distancePx: dist };
  }

  // 7) typing
  if (tick.sinceKey < TYPING_PULSE_SECS) {
    return { state: "typing", eyeOffsetX, eyeOffsetY, distancePx: dist };
  }

  // 8) 贴近反应
  if (dist < EXCITED_PX) {
    return { state: "excited", eyeOffsetX, eyeOffsetY, distancePx: dist };
  }
  if (dist < ALERT_PX) {
    return { state: "alert", eyeOffsetX, eyeOffsetY, distancePx: dist };
  }

  // 9) glance —— 整点抬头看一眼（只在本来 idle/drowsy 时；眼睛强制朝上）
  if (ctx.glance) {
    return { state: "glance", eyeOffsetX, eyeOffsetY: -MAX_EYE_OFFSET * 0.6, distancePx: dist };
  }

  // 10) 默认
  return { state: isDrowsyHour ? "drowsy" : "idle", eyeOffsetX, eyeOffsetY, distancePx: dist };
}

function isDrowsyNow(): boolean {
  const h = new Date().getHours();
  return h >= DROWSY_HOUR_START || h < DROWSY_HOUR_END;
}

/**
 * dizzy 能量累积器（纯函数，可单测）。
 *
 * 每帧调一次：高速时能量按 dt 累加，否则按 DIZZY_ENERGY_DECAY 倍速衰减。
 * 能量越过 DIZZY_ENERGY_TRIGGER_SECS 即"触发"——调用方据此点亮 dizzy + 清零能量 + 进冷却。
 * dt 异常大（标签页节流跳变）时跳过本帧速度计算，避免一次大跳变误判为甩动。
 *
 * @returns { energy: 更新后的能量, trigger: 本帧是否达到触发阈值 }
 */
export function updateDizzyEnergy(
  energy: number, speedPxPerSec: number, dt: number,
): { energy: number; trigger: boolean } {
  if (!(dt > 0) || dt > DIZZY_MAX_DT) return { energy, trigger: false };
  let next = speedPxPerSec > DIZZY_SPEED_PX_S
    ? energy + dt
    : Math.max(0, energy - dt * DIZZY_ENERGY_DECAY);
  if (next >= DIZZY_ENERGY_TRIGGER_SECS) {
    return { energy: 0, trigger: true };  // 触发即清零
  }
  return { energy: next, trigger: false };
}

/**
 * Hook —— 订阅 Rust tick + 监听桌宠 DOM 位置，输出当前帧 + 维护瞬时态计时器。
 *
 * @param petElRef 桌宠根元素 ref（算 bounding rect 中心）
 * @param onInteract 可选 —— 任何"用户主动互动"信号（点击 / 打字 / 贴近）触发，
 *                   供亲密度系统累计（见 useIntimacy）
 */
export function useCompanion(
  petElRef: { current: HTMLElement | null },
  onInteract?: () => void,
): CompanionFrame {
  const [frame, setFrame] = useState<CompanionFrame>(NEUTRAL);
  const stormSecsRef = useRef(0);
  const lastTickAtRef = useRef(performance.now());
  // 瞬时态计时器（performance.now() 时间戳，0=未激活）
  const wakingUntilRef = useRef(0);
  const dizzyUntilRef = useRef(0);
  const dizzyEnergyRef = useRef(0);       // 甩动能量累积
  const dizzyCooldownRef = useRef(0);     // 晕完冷却到的时间戳
  const hopUntilRef = useRef(0);
  const glanceUntilRef = useRef(0);
  // 跳变检测用
  const prevSleepyRef = useRef(false);
  const lastPosRef = useRef<{ x: number; y: number } | null>(null);
  const lastGlanceHourRef = useRef(-1);
  const interactedRef = useRef(false);

  useEffect(() => {
    let unlisten: UnlistenFn | null = null;
    let alive = true;

    listen<CompanionTick>("companion-tick", (e) => {
      if (!alive) return;
      const now = performance.now();
      const dt = (now - lastTickAtRef.current) / 1000;
      lastTickAtRef.current = now;
      const p = e.payload;
      const drowsyHour = isDrowsyNow();
      const sleepThreshold = drowsyHour ? IDLE_TO_SLEEP_SECS / 2 : IDLE_TO_SLEEP_SECS;
      const sleepyNow = p.sinceKey > sleepThreshold && p.sinceMouse > sleepThreshold;

      // ── waking：sleep → awake 跳变 ──
      if (prevSleepyRef.current && !sleepyNow) {
        wakingUntilRef.current = now + WAKING_MS;
      }
      prevSleepyRef.current = sleepyNow;

      // ── typing storm 累计 + burst 结束 → hop ──
      const prevStorm = stormSecsRef.current;
      if (p.sinceKey < TYPING_STORM_PULSE_THRESH) {
        stormSecsRef.current = Math.min(stormSecsRef.current + dt, 10);
      } else {
        // sinceKey 跳上来了 → burst 可能结束
        if (prevStorm >= HOP_MIN_BURST_SECS && p.sinceKey > 0.4) {
          hopUntilRef.current = now + HOP_MS;
        }
        stormSecsRef.current = 0;
      }

      // ── dizzy：能量累积 + 冷却（必须持续使劲甩才触发，不被正常移动误触）──
      const last = lastPosRef.current;
      if (last && Number.isFinite(p.x) && Number.isFinite(p.y)) {
        const speed = Math.hypot(p.x - last.x, p.y - last.y) / dt;
        const { energy, trigger } = updateDizzyEnergy(dizzyEnergyRef.current, speed, dt);
        dizzyEnergyRef.current = energy;
        if (trigger && now > dizzyCooldownRef.current) {
          dizzyUntilRef.current = now + DIZZY_HOLD_MS;
          dizzyCooldownRef.current = now + DIZZY_HOLD_MS + DIZZY_COOLDOWN_MS;
        }
      }
      lastPosRef.current = { x: p.x, y: p.y };

      // ── glance：过整点低概率触发 ──
      const hour = new Date().getHours();
      if (hour !== lastGlanceHourRef.current) {
        lastGlanceHourRef.current = hour;
        if (!sleepyNow && Math.random() < GLANCE_HOURLY_PROB) {
          glanceUntilRef.current = now + GLANCE_MS;
        }
      }

      // ── 亲密度互动信号：点击 / 打字 / 贴近（去抖：一次互动只回调一次）──
      const el = petElRef.current;
      let interacting = p.sinceClick < CLICK_FLASH_SECS || p.sinceKey < TYPING_PULSE_SECS;
      if (el) {
        const r = el.getBoundingClientRect();
        const cx = r.left + r.width / 2, cy = r.top + r.height / 2;
        if (Math.hypot(p.x - cx, p.y - cy) < ALERT_PX) interacting = true;
      }
      if (interacting && !interactedRef.current) {
        interactedRef.current = true;
        onInteract?.();
      } else if (!interacting) {
        interactedRef.current = false;
      }

      if (!el) {
        setFrame({ ...NEUTRAL, state: drowsyHour ? "drowsy" : "idle" });
        return;
      }
      const rect = el.getBoundingClientRect();
      const petCenter = { x: rect.left + rect.width / 2, y: rect.top + rect.height / 2 };
      setFrame(deriveCompanionFrame(p, petCenter, {
        typingStormSecs: stormSecsRef.current,
        nowHour: hour,
        waking: now < wakingUntilRef.current,
        dizzy: now < dizzyUntilRef.current,
        hop: now < hopUntilRef.current,
        glance: now < glanceUntilRef.current,
      }));
    }).then((u) => { if (alive) unlisten = u; else u(); });

    return () => { alive = false; if (unlisten) unlisten(); };
  }, [petElRef, onInteract]);

  return frame;
}
