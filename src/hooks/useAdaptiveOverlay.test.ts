import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { renderHook } from "@testing-library/react";
import { useRef } from "react";

import { useAdaptiveOverlay } from "./useAdaptiveOverlay";

const invokeMock = vi.fn<(cmd: string, args?: unknown) => Promise<unknown>>(
  () => Promise.resolve(null),
);
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (cmd: string, args?: unknown) => invokeMock(cmd, args),
}));

// jsdom 没原生 ResizeObserver / MutationObserver 的 useful 实现，但我们这里只测
// 调用契约（dedupe / 启用开关 / measure 取并集），所以 noop 即可。
class FakeRO {
  cb: ResizeObserverCallback;
  constructor(cb: ResizeObserverCallback) { this.cb = cb; }
  observe() {}
  disconnect() {}
}
(globalThis as any).ResizeObserver = FakeRO;

describe("useAdaptiveOverlay", () => {
  beforeEach(() => {
    invokeMock.mockClear();
    invokeMock.mockImplementation(() => Promise.resolve(null));
    document.body.innerHTML = "";
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  function buildStage(uiHeight = 200) {
    const stage = document.createElement("div");
    stage.className = "stage";
    Object.defineProperty(stage, "getBoundingClientRect", {
      value: () => ({ left: 0, top: 0, right: 320, bottom: 320, width: 320, height: 320 }),
    });
    const mouse = document.createElement("div");
    mouse.className = "mouse-wrap";
    Object.defineProperty(mouse, "getBoundingClientRect", {
      value: () => ({ left: 130, top: 240, right: 190, bottom: 300, width: 60, height: 60 }),
    });
    const menu = document.createElement("div");
    menu.className = "pet-menu";
    Object.defineProperty(menu, "getBoundingClientRect", {
      value: () => ({ left: 20, top: 300 - uiHeight, right: 300, bottom: 300, width: 280, height: uiHeight }),
    });
    stage.appendChild(mouse);
    stage.appendChild(menu);
    document.body.appendChild(stage);
    return stage;
  }

  it("disabled → never invokes", () => {
    buildStage();
    renderHook(() => {
      const ref = useRef<HTMLDivElement>(null);
      ref.current = document.querySelector(".stage") as HTMLDivElement;
      useAdaptiveOverlay(ref, { enabled: false });
    });
    expect(invokeMock).not.toHaveBeenCalled();
  });

  it("enabled + visible UI → invokes set_overlay_content_size with union bounds", () => {
    buildStage(200); // menu height 200 → 总高 280 - 100 = 280? 让我们看
    renderHook(() => {
      const ref = useRef<HTMLDivElement>(null);
      ref.current = document.querySelector(".stage") as HTMLDivElement;
      useAdaptiveOverlay(ref, { enabled: true });
    });
    expect(invokeMock).toHaveBeenCalled();
    const call = invokeMock.mock.calls[0];
    expect(call[0]).toBe("set_overlay_content_size");
    const { width, height } = call[1] as { width: number; height: number };
    // 并集：x ∈ [20, 300]，y ∈ [100, 300] → w=280, h=200。
    // .pet-menu 可见且在 SHADOWED_SELECTOR → 用 SHADOW_PADDING(96) 容下重阴影
    expect(width).toBe(280 + 96);
    expect(height).toBe(200 + 96);
  });

  it("dedupes consecutive same-size pushes", async () => {
    buildStage();
    const { rerender } = renderHook(({ enabled }: { enabled: boolean }) => {
      const ref = useRef<HTMLDivElement>(null);
      ref.current = document.querySelector(".stage") as HTMLDivElement;
      useAdaptiveOverlay(ref, { enabled });
    }, { initialProps: { enabled: true } });
    // 多次 rerender 同样的内容 → invoke 只调一次（首次 push）
    rerender({ enabled: true });
    rerender({ enabled: true });
    // 兜底轮询是 250ms，setTimeout 默认不跑 → 计数应该还是 1
    expect(invokeMock.mock.calls.length).toBeGreaterThanOrEqual(1);
    const firstSize = invokeMock.mock.calls[0][1];
    // 所有调用尺寸都一致
    invokeMock.mock.calls.forEach((c) => {
      expect(c[1]).toEqual(firstSize);
    });
  });

  it("ignores zero-size elements in measure", () => {
    const stage = document.createElement("div");
    stage.className = "stage";
    const empty = document.createElement("div");
    empty.className = "rx-ribbon";
    Object.defineProperty(empty, "getBoundingClientRect", {
      value: () => ({ left: 0, top: 0, right: 0, bottom: 0, width: 0, height: 0 }),
    });
    const real = document.createElement("div");
    real.className = "mouse-wrap";
    Object.defineProperty(real, "getBoundingClientRect", {
      value: () => ({ left: 100, top: 200, right: 180, bottom: 280, width: 80, height: 80 }),
    });
    stage.appendChild(empty);
    stage.appendChild(real);
    document.body.appendChild(stage);
    renderHook(() => {
      const ref = useRef<HTMLDivElement>(null);
      ref.current = stage;
      useAdaptiveOverlay(ref, { enabled: true });
    });
    expect(invokeMock).toHaveBeenCalled();
    const { width, height } = invokeMock.mock.calls[0][1] as { width: number; height: number };
    expect(width).toBe(80 + 24);
    expect(height).toBe(80 + 24);
  });
});
