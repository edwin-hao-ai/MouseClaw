/**
 * 金字塔腰：PixelMouse 接 companion 帧后的 DOM 输出验证。
 *
 * 验证两件硬规则：
 *   1. companion class 只在 state="listen" 时挂（不污染任务态）
 *   2. eyeOffset 平移 `.mc-eyes` group —— 所有 9 款皮肤共用，自动适配
 */
import { describe, it, expect } from "vitest";
import { render } from "@testing-library/react";
import { PixelMouse } from "../PixelMouse";
import { SKINS } from "../../skins";

describe("PixelMouse · companion", () => {
  // v0.4+ 关键回归（2026-05-20 真 app 实测 bug 修复）:
  // idle 视图下 mouseStateFor → "sleep"，companion 必须在 state="sleep" 也挂 class
  // 否则用户日常静默状态下根本看不到陪伴动画
  it("REGRESSION: state=sleep + companionState=alert → 挂 companion-alert class", () => {
    const { container } = render(
      <PixelMouse state="sleep" companionState="alert" eyeOffset={{ x: 0.3, y: 0.1 }} />
    );
    expect(container.querySelector(".mouse-wrap")?.className).toContain("companion-alert");
  });

  it("REGRESSION: state=sleep + companionState=clicked/worried/excited 都挂 class", () => {
    for (const cs of ["clicked", "worried", "excited", "typing"] as const) {
      const { container } = render(<PixelMouse state="sleep" companionState={cs} />);
      expect(container.querySelector(".mouse-wrap")?.className).toContain(`companion-${cs}`);
    }
  });

  it("REGRESSION: state=sleep + companion 非 sleep/drowsy → 眼睛用 listen anatomy（睁眼可见瞳孔）", () => {
    const { container } = render(
      <PixelMouse state="sleep" companionState="alert" eyeOffset={{ x: 0.3, y: 0.1 }} />
    );
    // listen 状态的 eye rect 是 width=1 height=2（竖立瞳孔），sleep 状态是 1×1（横线闭眼）
    // 用 rect 数量 + 维度区分：listen=2 个 1×2 rect；sleep=2 个 1×1
    const rects = container.querySelectorAll(".mc-eyes rect");
    expect(rects.length).toBe(2);
    expect(rects[0].getAttribute("height")).toBe("2"); // 睁眼竖立瞳孔
  });

  it("REGRESSION: state=sleep + companionState=sleep → 保持闭眼 anatomy（10s 真睡着）", () => {
    const { container } = render(
      <PixelMouse state="sleep" companionState="sleep" />
    );
    const rects = container.querySelectorAll(".mc-eyes rect");
    expect(rects[0].getAttribute("height")).toBe("1"); // 闭眼横线
  });

  it("state=listen + companionState=alert → 挂 companion-alert class", () => {
    const { container } = render(
      <PixelMouse state="listen" companionState="alert" eyeOffset={{ x: 0.3, y: 0.1 }} />
    );
    const wrap = container.querySelector(".mouse-wrap");
    expect(wrap?.className).toContain("companion-alert");
  });

  it("state=listen + companionState=idle → 不挂 companion class（保持纯净）", () => {
    const { container } = render(
      <PixelMouse state="listen" companionState="idle" />
    );
    const wrap = container.querySelector(".mouse-wrap");
    expect(wrap?.className).not.toContain("companion-");
  });

  it("state=think（任务态）→ companion class 被忽略（任务态优先）", () => {
    const { container } = render(
      <PixelMouse state="think" companionState="excited" />
    );
    const wrap = container.querySelector(".mouse-wrap");
    expect(wrap?.className).not.toContain("companion-excited");
  });

  it("eyeOffset 平移 .mc-eyes group（SVG transform attribute · 所有皮肤生效）", () => {
    for (const skin of SKINS) {
      const { container } = render(
        <PixelMouse state="listen" skin={skin.id} companionState="alert" eyeOffset={{ x: 0.4, y: -0.2 }} />
      );
      const eyes = container.querySelector(".mc-eyes") as HTMLElement | null;
      expect(eyes, `skin=${skin.id} 缺 .mc-eyes group`).toBeTruthy();
      // v0.4+ · 用 SVG transform attribute（user 单位，WebKit 可靠），不是 CSS style
      const transform = eyes!.getAttribute("transform") || "";
      expect(transform, `skin=${skin.id} 未应用 eyeOffset transform`)
        .toMatch(/translate\(0\.400 -0\.200\)/);
    }
  });

  it("companionState=sleep → 即使有 eyeOffset 也不平移（闭眼）", () => {
    const { container } = render(
      <PixelMouse state="listen" companionState="sleep" eyeOffset={{ x: 0.5, y: 0.5 }} />
    );
    const eyes = container.querySelector(".mc-eyes") as HTMLElement | null;
    const transform = eyes?.getAttribute("transform") || "";
    // sleep 时 transform attribute 应缺省 —— 由 CSS 接管 scaleY 闭眼
    expect(transform).not.toMatch(/translate\(/);
  });

  // v0.4+ 扩展状态
  it("companionState=clicked → 挂 companion-clicked class", () => {
    const { container } = render(<PixelMouse state="listen" companionState="clicked" />);
    expect(container.querySelector(".mouse-wrap")?.className).toContain("companion-clicked");
  });

  it("companionState=worried → 挂 companion-worried class", () => {
    const { container } = render(<PixelMouse state="listen" companionState="worried" />);
    expect(container.querySelector(".mouse-wrap")?.className).toContain("companion-worried");
  });

  it("companionState=drowsy → 挂 companion-drowsy class（眼睛不平移，由 CSS 接管闭眼）", () => {
    const { container } = render(
      <PixelMouse state="listen" companionState="drowsy" eyeOffset={{ x: 0.5, y: 0.5 }} />
    );
    expect(container.querySelector(".mouse-wrap")?.className).toContain("companion-drowsy");
    const transform = container.querySelector(".mc-eyes")?.getAttribute("transform") || "";
    expect(transform).not.toMatch(/translate\(/);
  });

  // v0.4+ 全集补完状态
  it("waking / dizzy / hop / glance 各自挂对应 companion class", () => {
    for (const cs of ["waking", "dizzy", "hop", "glance"] as const) {
      const { container } = render(<PixelMouse state="sleep" companionState={cs} />);
      expect(container.querySelector(".mouse-wrap")?.className).toContain(`companion-${cs}`);
    }
  });

  it("dizzy → 眼睛不平移（转圈，由 CSS 接管）", () => {
    const { container } = render(
      <PixelMouse state="sleep" companionState="dizzy" eyeOffset={{ x: 0.5, y: 0.5 }} />
    );
    const transform = container.querySelector(".mc-eyes")?.getAttribute("transform") || "";
    expect(transform).not.toMatch(/translate\(/);
  });

  it("intimacyLevel → 挂 intimacy-N class；neglected → companion-neglected", () => {
    const { container: c1 } = render(<PixelMouse state="sleep" intimacyLevel={2} />);
    expect(c1.querySelector(".mouse-wrap")?.className).toContain("intimacy-2");
    const { container: c2 } = render(<PixelMouse state="sleep" neglected />);
    expect(c2.querySelector(".mouse-wrap")?.className).toContain("companion-neglected");
  });

  it("intimacy / neglected 在任务态（think）不挂（不分心）", () => {
    const { container } = render(<PixelMouse state="think" intimacyLevel={3} neglected />);
    const cls = container.querySelector(".mouse-wrap")?.className || "";
    expect(cls).not.toContain("intimacy-");
    expect(cls).not.toContain("neglected");
  });

  it("9 款皮肤都能渲染 companion 帧无异常", () => {
    for (const skin of SKINS) {
      const { container } = render(
        <PixelMouse state="listen" skin={skin.id} companionState="typing" eyeOffset={{ x: 0.1, y: 0.1 }} />
      );
      // 关键 anatomy 都在：耳/眼/身（body 形状随 variant 变，但 aria-label 一定有）
      const svg = container.querySelector("svg.mouse-svg");
      expect(svg, `skin=${skin.id} 缺 SVG`).toBeTruthy();
      expect(svg?.getAttribute("aria-label")).toContain(skin.name);
    }
  });
});
