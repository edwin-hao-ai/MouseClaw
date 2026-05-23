/**
 * v0.4.x · Onboarding 向导专用 —— 让桌宠在向导窗口里就"活"起来。
 *
 * 桌面桌宠的眼球追踪走 Rust `companion-tick`(companion.rs 30 FPS 推全局光标),
 * 但那个事件**只发给 mouse 窗口**;onboarding 是另一个窗口,收不到。
 * 这里用向导窗口自己的 `mousemove`(可聚焦窗口能拿到)合成一个 tick,
 * 复用 `deriveCompanionFrame` 同一套 tanh 眼球偏移 + 贴近反应数学 ——
 * 9 款皮肤自动适配(眼球偏移是 PixelMouse 平移 `<g.mc-eyes>`,palette 无关)。
 *
 * 设计依据:docs/prototypes/first-run-aha-20260522.html 场景①「第0秒·它活了」。
 */
import { useEffect, useRef, useState } from "react";
import { deriveCompanionFrame, type CompanionFrame, type CompanionState } from "./useCompanion";

const NEUTRAL: CompanionFrame = {
  state: "idle", eyeOffsetX: 0, eyeOffsetY: 0, distancePx: Infinity,
};

export interface LocalEyeFrame {
  companionState: CompanionState;
  eyeOffset: { x: number; y: number };
}

/**
 * @param petRef 桌宠根元素 ref —— 取 bounding rect 中心算光标方向
 * @returns 直接喂 PixelMouse 的 { companionState, eyeOffset }
 */
export function useLocalEyeTracking(
  petRef: { current: HTMLElement | null },
): LocalEyeFrame {
  const [frame, setFrame] = useState<CompanionFrame>(NEUTRAL);
  const rafRef = useRef(0);

  useEffect(() => {
    function onMove(e: MouseEvent) {
      const el = petRef.current;
      if (!el) return;
      // 节流到一帧,避免每个 mousemove 都 setState
      if (rafRef.current) return;
      const cx = e.clientX, cy = e.clientY;
      rafRef.current = requestAnimationFrame(() => {
        rafRef.current = 0;
        const node = petRef.current;
        if (!node) return;
        const r = node.getBoundingClientRect();
        const petCenter = { x: r.left + r.width / 2, y: r.top + r.height / 2 };
        // 合成 tick:刚移过鼠标(sinceMouse=0 → 永不进 sleep),无打字/点击。
        // nowHour 固定 12(白天)—— 向导无论几点都应该是醒着、迎接的样子,不犯困。
        const tick = { x: cx, y: cy, sinceKey: 999, sinceMouse: 0, sinceClick: 999 };
        setFrame(deriveCompanionFrame(tick, petCenter, { typingStormSecs: 0, nowHour: 12 }));
      });
    }
    window.addEventListener("mousemove", onMove);
    return () => {
      window.removeEventListener("mousemove", onMove);
      if (rafRef.current) cancelAnimationFrame(rafRef.current);
    };
  }, [petRef]);

  return {
    companionState: frame.state,
    eyeOffset: { x: frame.eyeOffsetX, y: frame.eyeOffsetY },
  };
}
