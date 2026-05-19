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

  it("eyeOffset 平移 .mc-eyes group（所有皮肤生效）", () => {
    for (const skin of SKINS) {
      const { container } = render(
        <PixelMouse state="listen" skin={skin.id} companionState="alert" eyeOffset={{ x: 0.4, y: -0.2 }} />
      );
      const eyes = container.querySelector(".mc-eyes") as HTMLElement | null;
      expect(eyes, `skin=${skin.id} 缺 .mc-eyes group`).toBeTruthy();
      const transform = eyes!.getAttribute("style") || "";
      expect(transform, `skin=${skin.id} 未应用 eyeOffset transform`)
        .toMatch(/translate\(0\.400px,\s*-0\.200px\)/);
    }
  });

  it("companionState=sleep → 即使有 eyeOffset 也不平移（闭眼）", () => {
    const { container } = render(
      <PixelMouse state="listen" companionState="sleep" eyeOffset={{ x: 0.5, y: 0.5 }} />
    );
    const eyes = container.querySelector(".mc-eyes") as HTMLElement | null;
    const style = eyes?.getAttribute("style") || "";
    // sleep 时 transform 应该是 undefined / 空 —— 由 CSS 接管 scaleY 闭眼
    expect(style).not.toMatch(/translate\(/);
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
    const style = container.querySelector(".mc-eyes")?.getAttribute("style") || "";
    expect(style).not.toMatch(/translate\(/);
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
