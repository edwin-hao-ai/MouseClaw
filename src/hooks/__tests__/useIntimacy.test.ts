/**
 * 金字塔底：亲密度等级映射 + hook 行为。
 */
import { describe, it, expect, beforeEach, vi } from "vitest";
import { renderHook, act } from "@testing-library/react";
import { levelFor, useIntimacy, LEVEL_THRESHOLDS, NEGLECT_DAYS, INTERACT_DEBOUNCE_MS } from "../useIntimacy";

describe("levelFor", () => {
  it("阈值边界正确映射 0/1/2/3", () => {
    expect(levelFor(0)).toBe(0);
    expect(levelFor(LEVEL_THRESHOLDS[1] - 1)).toBe(0);
    expect(levelFor(LEVEL_THRESHOLDS[1])).toBe(1);
    expect(levelFor(LEVEL_THRESHOLDS[2])).toBe(2);
    expect(levelFor(LEVEL_THRESHOLDS[3])).toBe(3);
    expect(levelFor(999999)).toBe(3); // 封顶 3
  });
});

describe("useIntimacy", () => {
  beforeEach(() => {
    localStorage.clear();
    vi.useRealTimers();
  });

  it("初始 level 0 / count 0 / 不委屈", () => {
    const { result } = renderHook(() => useIntimacy());
    expect(result.current.level).toBe(0);
    expect(result.current.count).toBe(0);
    expect(result.current.neglected).toBe(false);
  });

  it("bumpInteract 累计 count（首次立即 +1）", () => {
    const { result } = renderHook(() => useIntimacy());
    act(() => { result.current.bumpInteract(); });
    expect(result.current.count).toBe(1);
  });

  it("debounce：连续 bump 在窗口内只 +1", () => {
    const { result } = renderHook(() => useIntimacy());
    act(() => {
      result.current.bumpInteract();
      result.current.bumpInteract();
      result.current.bumpInteract();
    });
    expect(result.current.count).toBe(1); // 同一窗口内多次只算 1
  });

  it("count 持久化到 localStorage（重挂载读回）", () => {
    const { result, unmount } = renderHook(() => useIntimacy());
    act(() => { result.current.bumpInteract(); });
    expect(result.current.count).toBe(1);
    unmount();
    const { result: r2 } = renderHook(() => useIntimacy());
    expect(r2.current.count).toBe(1);
  });

  it("冷落：lastInteract 超 NEGLECT_DAYS 天前 → neglected=true", () => {
    const longAgo = Date.now() - (NEGLECT_DAYS + 1) * 86400_000;
    localStorage.setItem("mouseclaw.intimacy.lastInteractMs", String(longAgo));
    const { result } = renderHook(() => useIntimacy());
    expect(result.current.neglected).toBe(true);
  });

  it("互动后清委屈", () => {
    const longAgo = Date.now() - (NEGLECT_DAYS + 1) * 86400_000;
    localStorage.setItem("mouseclaw.intimacy.lastInteractMs", String(longAgo));
    const { result } = renderHook(() => useIntimacy());
    expect(result.current.neglected).toBe(true);
    act(() => { result.current.bumpInteract(); });
    expect(result.current.neglected).toBe(false);
  });

  it("debounce 常量合理（≥ 1s 防刷）", () => {
    expect(INTERACT_DEBOUNCE_MS).toBeGreaterThanOrEqual(1000);
  });
});
