import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import { useState } from "react";
import { TourBubble } from "./TourBubble";

const invokeMock = vi.fn<(cmd: string, args?: unknown) => Promise<unknown>>(
  () => Promise.resolve(null),
);
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (cmd: string, args?: unknown) => invokeMock(cmd, args),
}));

describe("TourBubble", () => {
  beforeEach(() => { invokeMock.mockClear(); });

  it("step 1: 点「好啊」→ tour_advance(step:2)；点「下次再说」→ tour_skip", () => {
    render(<TourBubble step={1} />);
    fireEvent.click(screen.getByText("✨ 好啊"));
    expect(invokeMock).toHaveBeenCalledWith("tour_advance", { step: 2 });
    fireEvent.click(screen.getByText("下次再说"));
    expect(invokeMock).toHaveBeenCalledWith("tour_skip", undefined);
  });

  // 🎯 回归测试（2026-05-21）：tour 按钮点不动的根因是 helper 组件定义在渲染函数体内 →
  // 父（App）频繁重渲染（桌宠眼球追鼠标）时按钮被 remount，mousedown/mouseup 落在不同
  // 元素上 → onClick 不触发。修复 = helper 提到模块级。本测试断言"父重渲染后按钮 DOM
  // 节点保持同一个（未 remount）+ 重渲染后点击仍触发"。若回退成内联组件，本测试会失败。
  it("父组件重复重渲染后，按钮 DOM 节点稳定不 remount + 仍可点击", () => {
    function Harness() {
      const [, setTick] = useState(0);
      return (
        <div>
          <button data-testid="rerender" onClick={() => setTick((t) => t + 1)}>tick</button>
          <TourBubble step={1} />
        </div>
      );
    }
    render(<Harness />);
    const before = screen.getByText("✨ 好啊");
    // 模拟桌宠眼球追鼠标导致的高频父重渲染
    for (let i = 0; i < 5; i++) fireEvent.click(screen.getByTestId("rerender"));
    const after = screen.getByText("✨ 好啊");
    expect(after).toBe(before); // 同一个 DOM 节点 = 没被卸载重建
    fireEvent.click(after);
    expect(invokeMock).toHaveBeenCalledWith("tour_advance", { step: 2 });
  });

  it("step 5 庆祝页正常渲染（不崩）", () => {
    render(<TourBubble step={5} />);
    expect(screen.getByText("🎉 你学会了！")).toBeTruthy();
  });
});
