/**
 * 金字塔底：useCompanion 的 state machine 纯函数测试。
 *
 * 不挂 React/Tauri runtime，只验 deriveCompanionFrame 的真值表。
 */
import { describe, it, expect } from "vitest";
import {
  deriveCompanionFrame,
  type CompanionCtx,
  IDLE_TO_SLEEP_SECS,
  TYPING_PULSE_SECS,
  ALERT_PX,
  EXCITED_PX,
  MAX_EYE_OFFSET,
  CLICK_FLASH_SECS,
  TYPING_STORM_DURATION_SECS,
} from "../useCompanion";

const PET_CENTER = { x: 100, y: 100 };

// helper：测试调用点保持简洁。waking/dizzy/hop/glance 缺省 false，需要时覆盖。
function ctx(partial: CompanionCtx): CompanionCtx {
  return { waking: false, dizzy: false, hop: false, glance: false, ...partial };
}

describe("deriveCompanionFrame", () => {
  it("sleep 优先 —— 键鼠都超 idle 阈值即入睡，覆盖一切", () => {
    const f = deriveCompanionFrame(
      { x: 105, y: 100, sinceKey: IDLE_TO_SLEEP_SECS + 1, sinceMouse: IDLE_TO_SLEEP_SECS + 1, sinceClick: 999 },
      PET_CENTER,
    );
    expect(f.state).toBe("sleep");
    expect(f.eyeOffsetX).toBe(0); // sleep 时眼睛闭合不偏移
    expect(f.eyeOffsetY).toBe(0);
  });

  it("没有 petCenter（DOM 还没挂）→ 中性帧", () => {
    const f = deriveCompanionFrame(
      { x: 0, y: 0, sinceKey: 0.1, sinceMouse: 0.1, sinceClick: 999 },
      null,
    );
    expect(f.state).toBe("idle");
    expect(f.distancePx).toBe(Infinity);
  });

  it("typing pulse 内 → typing 状态（但仍带眼球偏移）", () => {
    const f = deriveCompanionFrame(
      { x: 500, y: 500, sinceKey: TYPING_PULSE_SECS / 2, sinceMouse: 5, sinceClick: 999 },
      PET_CENTER,
    );
    expect(f.state).toBe("typing");
    expect(f.eyeOffsetX).toBeGreaterThan(0); // 鼠标在右下
    expect(f.eyeOffsetY).toBeGreaterThan(0);
  });

  it("光标距离 < EXCITED_PX → excited", () => {
    const f = deriveCompanionFrame(
      { x: PET_CENTER.x + EXCITED_PX - 5, y: PET_CENTER.y, sinceKey: 1, sinceMouse: 0, sinceClick: 999 },
      PET_CENTER,
    );
    expect(f.state).toBe("excited");
  });

  it("EXCITED_PX ≤ 距离 < ALERT_PX → alert", () => {
    const f = deriveCompanionFrame(
      { x: PET_CENTER.x + EXCITED_PX + 10, y: PET_CENTER.y, sinceKey: 1, sinceMouse: 0, sinceClick: 999 },
      PET_CENTER,
    );
    expect(f.state).toBe("alert");
  });

  it("距离 ≥ ALERT_PX → idle", () => {
    const f = deriveCompanionFrame(
      { x: PET_CENTER.x + ALERT_PX + 10, y: PET_CENTER.y, sinceKey: 1, sinceMouse: 0, sinceClick: 999 },
      PET_CENTER,
    );
    expect(f.state).toBe("idle");
  });

  it("eyeOffset 上限不超过 MAX_EYE_OFFSET（远距离 = tanh 饱和）", () => {
    const f = deriveCompanionFrame(
      { x: 10000, y: PET_CENTER.y, sinceKey: 1, sinceMouse: 0, sinceClick: 999 },
      PET_CENTER,
    );
    // 鼠标在正右方很远 → tanh→1 → eyeOffsetX ≈ MAX_EYE_OFFSET 饱和
    expect(Math.abs(f.eyeOffsetX)).toBeLessThanOrEqual(MAX_EYE_OFFSET + 1e-9);
    expect(Math.abs(f.eyeOffsetX)).toBeGreaterThan(MAX_EYE_OFFSET * 0.95);
    expect(f.eyeOffsetY).toBeCloseTo(0, 5); // 同一水平 → Y 分量为 0
  });

  it("REGRESSION: X 和 Y 解耦 —— 桌宠在角落、鼠标在屏幕中间（dy 大、dx 小）也能看见 X 偏移", () => {
    // 模拟真 app 场景：桌宠在屏幕右下角(1820, 1100)，鼠标在屏幕左上(200, 200)
    // 之前用极坐标 cos(atan2)*r → 因为 dy 远大于 dx，eyeOffsetX 接近 0
    // 现在用 tanh(dx/falloff) 分量映射 → dx 单独决定 X，dy 单独决定 Y
    const f = deriveCompanionFrame(
      // tick.x/y 是 mouse 窗口本地坐标（Rust 端已减过 outer_position）——
      // 模拟 mouse 窗口位置不重要，关键是 cursor 相对桌宠中心的 dx/dy
      { x: 200, y: 200, sinceKey: 1, sinceMouse: 0, sinceClick: 999 },
      { x: 1820, y: 1100 },
    );
    // dx = 200 - 1820 = -1620 → 远 → tanh 饱和负方向
    expect(f.eyeOffsetX).toBeLessThan(-MAX_EYE_OFFSET * 0.9);
    // dy = 200 - 1100 = -900 → 远 → Y 也饱和负方向（但 weight 0.75）
    expect(f.eyeOffsetY).toBeLessThan(-MAX_EYE_OFFSET * 0.6);
  });

  it("光标在桌宠正左 → eyeOffsetX 为负", () => {
    const f = deriveCompanionFrame(
      { x: -500, y: PET_CENTER.y, sinceKey: 1, sinceMouse: 0, sinceClick: 999 },
      PET_CENTER,
    );
    expect(f.eyeOffsetX).toBeLessThan(0);
  });

  it("tick.x/y 是 mouse 窗口本地坐标（Rust 端已减过 outer_position）", () => {
    // tick.x=105 直接就是 webview 内 X 坐标，无需前端再减 screenX
    const f = deriveCompanionFrame(
      { x: 105, y: 100, sinceKey: 1, sinceMouse: 0, sinceClick: 999 },
      PET_CENTER,
    );
    // dx = 105 - 100 = 5px → excited
    expect(f.state).toBe("excited");
  });

  it("typing + sleep 边界 —— 用户刚开始打字（sinceKey 微小）必须叫醒", () => {
    // 之前可能 sleep，但 sinceKey 重置成小值就立刻 typing
    const f = deriveCompanionFrame(
      { x: 200, y: 200, sinceKey: 0.05, sinceMouse: 100, sinceClick: 999 },
      PET_CENTER,
    );
    expect(f.state).toBe("typing");
  });

  it("只敲键不动鼠（写代码典型场景）→ 不睡 + typing", () => {
    const f = deriveCompanionFrame(
      { x: 1000, y: 1000, sinceKey: 0.1, sinceMouse: 60, sinceClick: 999 },
      PET_CENTER,
    );
    expect(f.state).toBe("typing");
  });

  // ─── v0.4+ 扩展：click / worried / drowsy ───────────────

  it("clicked —— sinceClick < CLICK_FLASH_SECS → clicked 状态（优先于 typing/excited）", () => {
    const f = deriveCompanionFrame(
      { x: 200, y: 200, sinceKey: 0.05, sinceMouse: 0.05, sinceClick: CLICK_FLASH_SECS / 2 },
      PET_CENTER, ctx({ typingStormSecs: 0, nowHour: 12 }),
    );
    expect(f.state).toBe("clicked");
  });

  it("clicked 过期 → 不再是 clicked", () => {
    const f = deriveCompanionFrame(
      { x: 1000, y: 1000, sinceKey: 5, sinceMouse: 5, sinceClick: CLICK_FLASH_SECS + 0.1 },
      PET_CENTER, ctx({ typingStormSecs: 0, nowHour: 12 }),
    );
    expect(f.state).not.toBe("clicked");
  });

  it("worried —— typingStormSecs 超过阈值 → worried（覆盖 typing）", () => {
    const f = deriveCompanionFrame(
      { x: 200, y: 200, sinceKey: 0.05, sinceMouse: 5, sinceClick: 999 },
      PET_CENTER,
      ctx({ typingStormSecs: TYPING_STORM_DURATION_SECS + 0.5, nowHour: 12 }),
    );
    expect(f.state).toBe("worried");
  });

  it("worried 仅在累计够秒数时触发；典型短暂打字 ≠ worried", () => {
    const f = deriveCompanionFrame(
      { x: 200, y: 200, sinceKey: 0.05, sinceMouse: 5, sinceClick: 999 },
      PET_CENTER, ctx({ typingStormSecs: 0.5, nowHour: 12 }),
    );
    expect(f.state).toBe("typing");
  });

  it("drowsy —— 深夜小时段（23 / 0 / 5）默认进 drowsy（替代 idle）", () => {
    for (const hour of [23, 0, 3, 5]) {
      const f = deriveCompanionFrame(
        { x: 1000, y: 1000, sinceKey: 1, sinceMouse: 1, sinceClick: 999 },
        PET_CENTER, ctx({ typingStormSecs: 0, nowHour: hour }),
      );
      expect(f.state, `hour=${hour}`).toBe("drowsy");
    }
  });

  it("drowsy 白天不触发", () => {
    for (const hour of [6, 12, 18, 22]) {
      const f = deriveCompanionFrame(
        { x: 1000, y: 1000, sinceKey: 1, sinceMouse: 1, sinceClick: 999 },
        PET_CENTER, ctx({ typingStormSecs: 0, nowHour: hour }),
      );
      expect(f.state, `hour=${hour}`).toBe("idle");
    }
  });

  it("drowsy 模式下 sleep 阈值减半（5s 而非 10s）", () => {
    const f = deriveCompanionFrame(
      { x: 1000, y: 1000, sinceKey: IDLE_TO_SLEEP_SECS / 2 + 0.5, sinceMouse: IDLE_TO_SLEEP_SECS / 2 + 0.5, sinceClick: 999 },
      PET_CENTER, ctx({ typingStormSecs: 0, nowHour: 2 }),
    );
    expect(f.state).toBe("sleep");
  });

  it("clicked / worried / drowsy 仍带 eyeOffset（眼球追鼠标 always-on）", () => {
    const f = deriveCompanionFrame(
      { x: 1000, y: PET_CENTER.y, sinceKey: 5, sinceMouse: 5, sinceClick: 0.1 },
      PET_CENTER, ctx({ typingStormSecs: 0, nowHour: 12 }),
    );
    expect(f.state).toBe("clicked");
    expect(f.eyeOffsetX).toBeGreaterThan(0); // 鼠标在右
  });

  // ─── v0.4+ 全集补完：waking / dizzy / hop / glance ───────────────

  it("waking —— ctx.waking 时进 waking（伸懒腰），仍追鼠标", () => {
    const f = deriveCompanionFrame(
      { x: 200, y: 200, sinceKey: 0.05, sinceMouse: 0.05, sinceClick: 0.05 },
      PET_CENTER, ctx({ typingStormSecs: 0, nowHour: 12, waking: true }),
    );
    expect(f.state).toBe("waking");
    expect(f.eyeOffsetX).not.toBe(0);
  });

  it("dizzy —— ctx.dizzy 时眼睛不追鼠标（转圈）", () => {
    const f = deriveCompanionFrame(
      { x: 1000, y: PET_CENTER.y, sinceKey: 5, sinceMouse: 0, sinceClick: 999 },
      PET_CENTER, ctx({ typingStormSecs: 0, nowHour: 12, dizzy: true }),
    );
    expect(f.state).toBe("dizzy");
    expect(f.eyeOffsetX).toBe(0);
    expect(f.eyeOffsetY).toBe(0);
  });

  it("hop —— ctx.hop（burst 结束小跳），优先于 clicked/typing", () => {
    const f = deriveCompanionFrame(
      { x: 200, y: 200, sinceKey: 0.6, sinceMouse: 5, sinceClick: 999 },
      PET_CENTER, ctx({ typingStormSecs: 0, nowHour: 12, hop: true }),
    );
    expect(f.state).toBe("hop");
  });

  it("glance —— 本来 idle + ctx.glance → glance，眼睛强制朝上", () => {
    const f = deriveCompanionFrame(
      { x: PET_CENTER.x + ALERT_PX + 50, y: PET_CENTER.y, sinceKey: 2, sinceMouse: 0, sinceClick: 999 },
      PET_CENTER, ctx({ typingStormSecs: 0, nowHour: 12, glance: true }),
    );
    expect(f.state).toBe("glance");
    expect(f.eyeOffsetY).toBeLessThan(0);
  });

  it("glance 让位给贴近反应（鼠标靠近不发呆抬头）", () => {
    const f = deriveCompanionFrame(
      { x: PET_CENTER.x + 10, y: PET_CENTER.y, sinceKey: 2, sinceMouse: 0, sinceClick: 999 },
      PET_CENTER, ctx({ typingStormSecs: 0, nowHour: 12, glance: true }),
    );
    expect(f.state).toBe("excited");
  });

  it("优先级链 waking > dizzy > hop > clicked", () => {
    const tick = { x: 200, y: 200, sinceKey: 0.05, sinceMouse: 0.05, sinceClick: 0.05 };
    expect(deriveCompanionFrame(tick, PET_CENTER, ctx({ typingStormSecs: 0, nowHour: 12, waking: true, dizzy: true, hop: true })).state).toBe("waking");
    expect(deriveCompanionFrame(tick, PET_CENTER, ctx({ typingStormSecs: 0, nowHour: 12, dizzy: true, hop: true })).state).toBe("dizzy");
    expect(deriveCompanionFrame(tick, PET_CENTER, ctx({ typingStormSecs: 0, nowHour: 12, hop: true })).state).toBe("hop");
    expect(deriveCompanionFrame(tick, PET_CENTER, ctx({ typingStormSecs: 0, nowHour: 12 })).state).toBe("clicked");
  });
});
